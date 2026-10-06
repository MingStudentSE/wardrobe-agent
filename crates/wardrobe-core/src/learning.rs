use super::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
type LearningWeights = std::collections::BTreeMap<(String, String), i64>;
const ALGORITHM: &str = "explicit-and-events-v1";
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS learning_state(member_id TEXT PRIMARY KEY,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS learning_history(id TEXT PRIMARY KEY,member_id TEXT NOT NULL,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS learning_profiles(id TEXT PRIMARY KEY,member_id TEXT NOT NULL,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS learning_memories(id TEXT PRIMARY KEY,member_id TEXT NOT NULL,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS learning_receipts(member_id TEXT NOT NULL,operation_key TEXT NOT NULL,request TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(member_id,operation_key));")?;
    tx.execute_batch("CREATE INDEX IF NOT EXISTS learning_memory_root ON learning_memories(member_id,json_extract(document,'$.root_id'),json_extract(document,'$.revision')); CREATE INDEX IF NOT EXISTS learning_profile_member ON learning_profiles(member_id,id); CREATE INDEX IF NOT EXISTS learning_history_member ON learning_history(member_id,id);")?;
    Ok(())
}
fn parse(s: String) -> Result<Value, ApiError> {
    serde_json::from_str(&s).map_err(|_| ApiError::new("storage_error", "偏好数据损坏"))
}
fn invalid(s: &str) -> ApiError {
    ApiError::new("invalid_input", s)
}
impl Application {
    fn learning_state(&self, member: &str) -> Result<Value, ApiError> {
        let s: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM learning_state WHERE member_id=?1",
                [member],
                |r| r.get(0),
            )
            .optional()?;
        s.map(parse)
            .transpose()
            .map(|v| v.unwrap_or_else(|| json!({"cutoff":null,"suppressed":[]})))
    }
    fn learning_manage(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let key = string(&input, "operation_key")?;
        if key.trim().is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        let allowed = match op {
            "learning.edit" => vec![
                "member_id",
                "operation_key",
                "root_id",
                "expected_revision",
                "weight",
                "note",
            ],
            "learning.delete" => vec!["member_id", "operation_key", "dimension", "value"],
            _ => vec!["member_id", "operation_key", "scope"],
        };
        if input
            .as_object()
            .ok_or_else(|| invalid("须为对象"))?
            .keys()
            .any(|k| !allowed.contains(&k.as_str()))
        {
            return Err(invalid("记忆管理包含未知字段"));
        }
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let request = json!({"operation":op,"input":input}).to_string();
        let old:Option<(String,String)>=tx.query_row("SELECT request,result FROM learning_receipts WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((prior, result)) = old {
            if prior != request {
                return Err(ApiError::new("idempotency_conflict", "操作标识已使用"));
            }
            return parse(result);
        }
        let id = uuid::Uuid::now_v7().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let mut state = self.learning_state(&member)?;
        let result = if op == "learning.edit" {
            let root = string(&input, "root_id")?;
            let old:Option<String>=tx.query_row("SELECT document FROM learning_memories WHERE member_id=?1 AND json_extract(document,'$.root_id')=?2 ORDER BY json_extract(document,'$.revision') DESC LIMIT 1",params![member,root],|r|r.get(0)).optional()?;
            let mut doc = parse(old.ok_or_else(|| ApiError::new("not_found", "找不到显式偏好"))?)?;
            if doc["deleted"] == true {
                return Err(ApiError::new("not_found", "偏好已删除"));
            }
            if input["expected_revision"].as_u64() != doc["revision"].as_u64() {
                return Err(ApiError::new("stale_revision", "偏好已更新，请刷新"));
            }
            let weight = input["weight"]
                .as_i64()
                .filter(|v| (-5..=5).contains(v))
                .ok_or_else(|| invalid("权重须为-5至5整数"))?;
            if let Some(note) = input.get("note") {
                if note.as_str().is_none_or(|s| s.chars().count() > 2000) {
                    return Err(invalid("说明须为至多2000字文字"));
                }
                doc["note"] = note.clone();
            }
            doc["id"] = json!(id);
            doc["revision"] = json!(doc["revision"].as_u64().unwrap() + 1);
            doc["weight"] = json!(weight);
            doc["created_at"] = json!(now);
            tx.execute(
                "INSERT INTO learning_memories VALUES(?1,?2,?3)",
                params![id, member, doc.to_string()],
            )?;
            doc
        } else {
            if op == "learning.delete" {
                let dimension = string(&input, "dimension")?;
                let value = string(&input, "value")?;
                if ![
                    "color", "style", "material", "pattern", "category", "occasion",
                ]
                .contains(&dimension)
                    || value.is_empty()
                    || value.chars().count() > 100
                {
                    return Err(invalid("记忆特征无效"));
                }
                let feature = json!({"dimension":dimension,"value":value});
                if !state["suppressed"].as_array().unwrap().contains(&feature) {
                    state["suppressed"].as_array_mut().unwrap().push(feature);
                }
                let profile = self.learning_execute("learning.get", json!({"member_id":member}))?;
                for current in profile["explicit"].as_array().unwrap() {
                    if current["dimension"] == dimension && current["value"] == value {
                        let mut tomb = current.clone();
                        tomb["id"] = json!(uuid::Uuid::now_v7().to_string());
                        tomb["revision"] = json!(current["revision"].as_u64().unwrap() + 1);
                        tomb["deleted"] = json!(true);
                        tomb["created_at"] = json!(now);
                        tx.execute(
                            "INSERT INTO learning_memories VALUES(?1,?2,?3)",
                            params![tomb["id"].as_str(), member, tomb.to_string()],
                        )?;
                    }
                }
            } else {
                let scope = string(&input, "scope")?;
                if !["inferred", "all"].contains(&scope) {
                    return Err(invalid("重置范围须为inferred或all"));
                }
                state = json!({"cutoff":now,"suppressed":[]});
                if scope == "all" {
                    let profile =
                        self.learning_execute("learning.get", json!({"member_id":member}))?;
                    for current in profile["explicit"].as_array().unwrap() {
                        let mut tomb = current.clone();
                        tomb["id"] = json!(uuid::Uuid::now_v7().to_string());
                        tomb["revision"] = json!(current["revision"].as_u64().unwrap() + 1);
                        tomb["deleted"] = json!(true);
                        tomb["created_at"] = json!(now);
                        tx.execute(
                            "INSERT INTO learning_memories VALUES(?1,?2,?3)",
                            params![tomb["id"].as_str(), member, tomb.to_string()],
                        )?;
                    }
                }
            }
            tx.execute("INSERT INTO learning_state VALUES(?1,?2) ON CONFLICT(member_id) DO UPDATE SET document=excluded.document",params![member,state.to_string()])?;
            // Publish immediately so recommendation ranking cannot retain forgotten signals.
            let mut profile = self.learning_execute("learning.get", json!({"member_id":member}))?;
            profile["id"] = json!(id);
            profile["created_at"] = json!(now);
            profile["reset_boundary"] = state["cutoff"].clone();
            if op == "learning.reset" {
                profile["inferred"] = json!([]);
            } else {
                profile["inferred"] = json!(profile["inferred"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|v| !state["suppressed"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|f| f["dimension"] == v["dimension"] && f["value"] == v["value"]))
                    .cloned()
                    .collect::<Vec<_>>());
            }
            tx.execute(
                "INSERT INTO learning_profiles VALUES(?1,?2,?3)",
                params![id, member, profile.to_string()],
            )?;
            json!({"schema_version":1,"id":id,"member_id":member,"operation":op,"state":state,"created_at":now})
        };
        tx.execute(
            "INSERT INTO learning_history VALUES(?1,?2,?3)",
            params![id, member, result.to_string()],
        )?;
        tx.execute(
            "INSERT INTO learning_receipts VALUES(?1,?2,?3,?4)",
            params![member, key, request, result.to_string()],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,?2,?3)",
            params![id, op, result.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![id, result.to_string()],
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub(super) fn learning_weights(
        &self,
        member: &str,
    ) -> Result<(Value, LearningWeights), ApiError> {
        let profile = self.learning_execute("learning.get", json!({"member_id":member}))?;
        let mut weights = std::collections::BTreeMap::new();
        for signal in profile["inferred"].as_array().into_iter().flatten() {
            weights.insert(
                (
                    signal["dimension"].as_str().unwrap().to_owned(),
                    signal["value"].as_str().unwrap().to_owned(),
                ),
                signal["weight"].as_i64().unwrap(),
            );
        }
        // An explicit preference replaces a learned value for the same feature.
        for signal in profile["explicit"].as_array().into_iter().flatten() {
            weights.insert(
                (
                    signal["dimension"].as_str().unwrap().to_owned(),
                    signal["value"].as_str().unwrap().to_owned(),
                ),
                signal["weight"].as_i64().unwrap(),
            );
        }
        let context = json!({"algorithm_version":ALGORITHM,"profile_id":profile["id"],"explicit_count":profile["explicit"].as_array().map(Vec::len).unwrap_or(0),"inferred_count":profile["inferred"].as_array().map(Vec::len).unwrap_or(0),"source":"managed_member_memory","hard_constraints_override":true});
        Ok((context, weights))
    }
    fn learning_recompute(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        if input
            .as_object()
            .ok_or_else(|| invalid("须为对象"))?
            .keys()
            .any(|k| !["member_id", "operation_key", "include_prior"].contains(&k.as_str()))
        {
            return Err(invalid("重算包含未知字段"));
        }
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let old:Option<(String,String)>=tx.query_row("SELECT request,result FROM learning_receipts WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((request, result)) = old {
            if parse(request)? != input {
                return Err(ApiError::new("idempotency_conflict", "操作标识已使用"));
            }
            return parse(result);
        }
        let include_prior = input
            .get("include_prior")
            .map(|v| {
                v.as_bool()
                    .ok_or_else(|| invalid("include_prior须为布尔值"))
            })
            .transpose()?
            .unwrap_or(false);
        let mut state = self.learning_state(&member)?;
        if include_prior {
            state = json!({"cutoff":null,"suppressed":[]});
            tx.execute("INSERT INTO learning_state VALUES(?1,?2) ON CONFLICT(member_id) DO UPDATE SET document=excluded.document",params![member,state.to_string()])?;
        }
        let cutoff = state["cutoff"].as_str();
        type Signal = (
            i64,
            std::collections::BTreeSet<String>,
            std::collections::BTreeSet<String>,
        );
        let mut signals = std::collections::BTreeMap::<(String, String), Signal>::new();
        let mut absorb =
            |snapshots: Vec<Value>, base_weight: i64, id: &str, wear: bool, scores: &Value| {
                let mut features = std::collections::BTreeSet::new();
                for snapshot in snapshots {
                    for dimension in [
                        "color", "style", "material", "pattern", "category", "occasion",
                    ] {
                        let field = &snapshot[dimension];
                        if let Some(value) = field.as_str() {
                            features.insert((dimension.to_owned(), value.to_owned()));
                        }
                        for value in field
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                        {
                            features.insert((dimension.to_owned(), value.to_owned()));
                        }
                    }
                }
                for feature in features {
                    let mut weight =
                        base_weight + scores["overall"].as_i64().map(|n| n - 3).unwrap_or(0);
                    if feature.0 == "material" {
                        weight += scores["comfort"].as_i64().map(|n| n - 3).unwrap_or(0);
                    }
                    if ["color", "pattern", "style"].contains(&feature.0.as_str()) {
                        weight += scores["style"].as_i64().map(|n| n - 3).unwrap_or(0);
                    }
                    if weight == 0 {
                        continue;
                    }
                    let entry = signals.entry(feature).or_default();
                    entry.0 += weight;
                    if wear {
                        entry.2.insert(id.to_owned());
                    } else {
                        entry.1.insert(id.to_owned());
                    }
                }
            };
        {
            let mut st=tx.prepare("SELECT e.document FROM feedback_events e WHERE member_id=?1 AND revision=(SELECT MAX(revision) FROM feedback_events WHERE root_id=e.root_id) AND json_extract(document,'$.revoked')=0 ORDER BY root_id")?;
            for row in st.query_map([&member], |r| r.get::<_, String>(0))? {
                let event = parse(row?)?;
                if cutoff.is_some_and(|c| event["created_at"].as_str().is_some_and(|t| t <= c)) {
                    continue;
                }
                let weight = match event["reaction"].as_str() {
                    Some("like") => 2,
                    Some("dislike") => -2,
                    _ => 0,
                };
                absorb(
                    event["inventory_snapshots"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default(),
                    weight,
                    event["id"].as_str().unwrap(),
                    false,
                    &event["scores"],
                );
            }
        }
        for event in self.effective_wears(&member)? {
            if cutoff.is_some_and(|c| event["created_at"].as_str().is_some_and(|t| t <= c)) {
                continue;
            }
            absorb(
                event["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v["snapshot"].clone())
                    .collect(),
                1,
                event["id"].as_str().unwrap(),
                true,
                &Value::Null,
            );
        }
        let inferred:Vec<Value>=signals.into_iter().filter(|((dimension,value),_)|!state["suppressed"].as_array().into_iter().flatten().any(|v|v["dimension"]==*dimension&&v["value"]==*value)).map(|((dimension,value),(weight,feedback_ids,wear_ids))|json!({"dimension":dimension,"value":value,"weight":weight.clamp(-5,5),"source":{"kind":"inferred_events","feedback_ids":feedback_ids,"wear_ids":wear_ids},"algorithm_version":ALGORITHM})).collect();
        let id = uuid::Uuid::now_v7().to_string();
        let doc = json!({"schema_version":1,"id":id,"member_id":member,"algorithm_version":ALGORITHM,"inferred":inferred,"reset_boundary":state["cutoff"],"include_prior":include_prior,"created_at":chrono::Utc::now().to_rfc3339()});
        tx.execute(
            "INSERT INTO learning_profiles VALUES(?1,?2,?3)",
            params![id, member, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO learning_receipts VALUES(?1,?2,?3,?4)",
            params![member, key, input.to_string(), doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO learning_history VALUES(?1,?2,?3)",
            params![id, member, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'learning.recompute',?2)",
            params![id, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![id, doc.to_string()],
        )?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn learning_execute(&self, op: &str, mut input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        input["member_id"] = json!(member);
        if op == "learning.history" {
            if input
                .as_object()
                .ok_or_else(|| invalid("须为对象"))?
                .keys()
                .any(|k| !["member_id", "limit", "cursor"].contains(&k.as_str()))
            {
                return Err(invalid("历史查询包含未知字段"));
            }
            let limit = input
                .get("limit")
                .map(|v| {
                    v.as_u64()
                        .filter(|n| (1..=100).contains(n))
                        .ok_or_else(|| invalid("每页须为1–100"))
                })
                .transpose()?
                .unwrap_or(20);
            let cursor = input
                .get("cursor")
                .map(|v| v.as_str().ok_or_else(|| invalid("游标须为文字")))
                .transpose()?
                .unwrap_or("");
            if !cursor.is_empty()
                && !self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM learning_history WHERE member_id=?1 AND id=?2)",
                    params![member, cursor],
                    |r| r.get::<_, bool>(0),
                )?
            {
                return Err(invalid("游标不属于当前成员"));
            }
            let mut st=self.connection.prepare("SELECT document FROM learning_history WHERE member_id=?1 AND id>?2 ORDER BY id LIMIT ?3")?;
            let mut items = st
                .query_map(params![member, cursor, limit + 1], |r| {
                    r.get::<_, String>(0)
                })?
                .map(|r| parse(r?))
                .collect::<Result<Vec<_>, ApiError>>()?;
            let more = items.len() > limit as usize;
            items.truncate(limit as usize);
            let next = if more {
                items.last().map(|v| v["id"].clone())
            } else {
                None
            };
            return Ok(
                json!({"schema_version":1,"member_id":member,"items":items,"next_cursor":next}),
            );
        }
        if ["learning.edit", "learning.delete", "learning.reset"].contains(&op) {
            return self.learning_manage(op, input);
        }
        if op == "learning.recompute" {
            return self.learning_recompute(input);
        }
        if op == "learning.get" {
            if input
                .as_object()
                .ok_or_else(|| invalid("须为对象"))?
                .keys()
                .any(|k| k != "member_id")
            {
                return Err(invalid("画像查询包含未知字段"));
            }
            let mut st = self
                .connection
                .prepare("SELECT e.document FROM learning_memories e WHERE member_id=?1 AND COALESCE(json_extract(document,'$.deleted'),0)=0 AND json_extract(document,'$.revision')=(SELECT MAX(json_extract(document,'$.revision')) FROM learning_memories WHERE json_extract(document,'$.root_id')=json_extract(e.document,'$.root_id')) ORDER BY id")?;
            let explicit = st
                .query_map([&member], |r| r.get::<_, String>(0))?
                .map(|r| parse(r?))
                .collect::<Result<Vec<_>, ApiError>>()?;
            let saved:Option<String>=self.connection.query_row("SELECT document FROM learning_profiles WHERE member_id=?1 ORDER BY id DESC LIMIT 1",[&member],|r|r.get(0)).optional()?;
            let mut profile=saved.map(parse).transpose()?.unwrap_or_else(||json!({"schema_version":1,"member_id":member,"inferred":[],"algorithm_version":ALGORITHM}));
            profile["explicit"] = json!(explicit);
            return Ok(profile);
        }
        if op != "learning.set" {
            return Err(ApiError::new("unsupported_operation", "学习操作尚不可用"));
        }
        let obj = input.as_object().ok_or_else(|| invalid("须为对象"))?;
        if obj.keys().any(|k| {
            ![
                "member_id",
                "operation_key",
                "dimension",
                "value",
                "weight",
                "note",
            ]
            .contains(&k.as_str())
        }) {
            return Err(invalid("偏好包含未知字段"));
        }
        let key = string(&input, "operation_key")?;
        if key.trim().is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        let dimension = string(&input, "dimension")?;
        if ![
            "color", "style", "material", "pattern", "category", "occasion",
        ]
        .contains(&dimension)
        {
            return Err(invalid("偏好维度不支持尺寸或事实"));
        }
        let value = string(&input, "value")?;
        if value.trim().is_empty() || value.chars().count() > 100 {
            return Err(invalid("偏好值须为1–100字"));
        }
        let weight = input["weight"]
            .as_i64()
            .filter(|n| (-5..=5).contains(n))
            .ok_or_else(|| invalid("权重须为-5至5整数"))?;
        let note = input
            .get("note")
            .map(|v| {
                v.as_str()
                    .filter(|v| v.chars().count() <= 2000)
                    .ok_or_else(|| invalid("说明须为至多2000字文字"))
            })
            .transpose()?
            .unwrap_or("");
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let receipt:Option<(String,String)>=tx.query_row("SELECT request,result FROM learning_receipts WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((request, result)) = receipt {
            if parse(request)? != input {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "操作标识已用于其他偏好",
                ));
            }
            return parse(result);
        }
        let id = uuid::Uuid::now_v7().to_string();
        let current = self.learning_execute("learning.get", json!({"member_id":member}))?;
        if current["explicit"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["dimension"] == dimension && m["value"] == value)
        {
            return Err(ApiError::new(
                "preference_exists",
                "该特征已有明确偏好，请编辑现有修订",
            ));
        }
        let doc = json!({"schema_version":1,"id":id,"member_id":member,"dimension":dimension,"value":value,"weight":weight,"note":note,"source":{"kind":"explicit_user"},"algorithm_version":ALGORITHM,"root_id":id,"deleted":false,"revision":1,"created_at":chrono::Utc::now().to_rfc3339()});
        tx.execute(
            "INSERT INTO learning_memories VALUES(?1,?2,?3)",
            params![id, member, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO learning_receipts VALUES(?1,?2,?3,?4)",
            params![member, key, input.to_string(), doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO learning_history VALUES(?1,?2,?3)",
            params![id, member, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,?2,?3)",
            params![id, op, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![id, doc.to_string()],
        )?;
        tx.commit()?;
        Ok(doc)
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    ["get","set","edit","delete","reset","recompute","history"].into_iter().map(|op|{
 let mut props=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];
 if !["get","history"].contains(&op){props["operation_key"]=json!({"type":"string","minLength":1,"maxLength":200});required.push("operation_key");}
 if ["set","delete"].contains(&op){props["dimension"]=json!({"enum":["color","style","material","pattern","category","occasion"]});props["value"]=json!({"type":"string","minLength":1,"maxLength":100});required.extend(["dimension","value"]);}
 if ["set","edit"].contains(&op){props["weight"]=json!({"type":"integer","minimum":-5,"maximum":5});props["note"]=json!({"type":"string","maxLength":2000});required.push("weight");}
 if op=="edit"{props["root_id"]=json!({"type":"string"});props["expected_revision"]=json!({"type":"integer","minimum":1});required.extend(["root_id","expected_revision"]);}
 if op=="reset"{props["scope"]=json!({"enum":["inferred","all"]});required.push("scope");}
 if op=="recompute"{props["include_prior"]=json!({"type":"boolean","description":"明确重新纳入删除/重置前全部事实；否则保留删除和重置边界"});}
 if op=="history"{props["cursor"]=json!({"type":"string"});props["limit"]=json!({"type":"integer","minimum":1,"maximum":100});}
 json!({"name":format!("learning.{op}"),"description":"成员偏好与推断记忆管理，不改变事实","inputSchema":{"type":"object","properties":props,"required":required,"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id"]}})
 }).collect()
}
