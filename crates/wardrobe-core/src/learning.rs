use super::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
type LearningWeights = std::collections::BTreeMap<(String, String), i64>;
const ALGORITHM: &str = "explicit-and-events-v1";
const PAIR_ALGORITHM: &str = "effective-pairs-v1";
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS learning_state(member_id TEXT PRIMARY KEY,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS learning_history(id TEXT PRIMARY KEY,member_id TEXT NOT NULL,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS learning_profiles(id TEXT PRIMARY KEY,member_id TEXT NOT NULL,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS learning_memories(id TEXT PRIMARY KEY,member_id TEXT NOT NULL,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS learning_receipts(member_id TEXT NOT NULL,operation_key TEXT NOT NULL,request TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(member_id,operation_key));")?;
    tx.execute_batch("CREATE INDEX IF NOT EXISTS learning_memory_root ON learning_memories(member_id,json_extract(document,'$.root_id'),json_extract(document,'$.revision')); CREATE INDEX IF NOT EXISTS learning_profile_member ON learning_profiles(member_id,id); CREATE INDEX IF NOT EXISTS learning_history_member ON learning_history(member_id,id);")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS learning_source_versions(member_id TEXT PRIMARY KEY,version INTEGER NOT NULL);")?;
    let has_generation = tx
        .prepare("PRAGMA table_info(learning_source_versions)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|c| c == "generation");
    if !has_generation {
        tx.execute_batch("ALTER TABLE learning_source_versions ADD COLUMN generation TEXT;")?;
    }
    tx.execute("INSERT OR IGNORE INTO learning_source_versions(member_id,version,generation) VALUES('',0,?1)", [uuid::Uuid::now_v7().to_string()])?;
    for table in ["feedback_events", "wear_events"] {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            let row = if action == "DELETE" { "OLD" } else { "NEW" };
            tx.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS learning_{table}_{action} AFTER {action} ON {table} BEGIN INSERT INTO learning_source_versions(member_id,version) VALUES({row}.member_id,1) ON CONFLICT(member_id) DO UPDATE SET version=version+1; END;"))?;
        }
    }
    Ok(())
}
fn parse(s: String) -> Result<Value, ApiError> {
    serde_json::from_str(&s).map_err(|_| ApiError::new("storage_error", "偏好数据损坏"))
}
fn invalid(s: &str) -> ApiError {
    ApiError::new("invalid_input", s)
}
fn pair_event_weight(event: &Value, wear: bool) -> i64 {
    if wear {
        return 1;
    }
    (match event["reaction"].as_str() {
        Some("like") => 2,
        Some("dislike") => -2,
        _ => 0,
    }) + event["scores"]["overall"]
        .as_i64()
        .map(|n| n - 3)
        .unwrap_or(0)
}
// Pure deterministic projection from supplied effective events. Shared verbatim
// by recompute and portable archive validation so the two cannot diverge.
fn pair_projection_from_events(feedback: &[Value], wears: &[Value]) -> Value {
    let mut edges: std::collections::BTreeMap<(String, String), i64> = Default::default();
    let mut feedback_ids = std::collections::BTreeSet::new();
    let mut wear_ids = std::collections::BTreeSet::new();
    let mut teach = |members: &[String], weight: i64, anchor: Option<&str>| {
        match anchor {
            // Anchored feedback teaches only anchor-to-member edges; an anchor
            // outside the selected facts teaches nothing rather than inventing
            // unrelated pairs.
            Some(anchor) => {
                if members.iter().any(|member| member == anchor) {
                    for member in members {
                        if member.as_str() != anchor {
                            let key = if anchor < member.as_str() {
                                (anchor.to_owned(), member.clone())
                            } else {
                                (member.clone(), anchor.to_owned())
                            };
                            *edges.entry(key).or_default() += weight;
                        }
                    }
                }
            }
            // Ordinary feedback and wear teach every unordered distinct pair.
            None => {
                for (index, member) in members.iter().enumerate() {
                    for other in &members[index + 1..] {
                        *edges.entry((member.clone(), other.clone())).or_default() += weight;
                    }
                }
            }
        }
    };
    for event in feedback {
        feedback_ids.insert(event["id"].as_str().unwrap_or_default().to_owned());
        let mut members: Vec<String> = event["inventory_snapshots"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|snapshot| snapshot["id"].as_str())
            .map(str::to_owned)
            .collect();
        members.sort();
        members.dedup();
        teach(
            &members,
            pair_event_weight(event, false),
            event["controls"]["pairing_anchor_id"].as_str(),
        );
    }
    for event in wears {
        wear_ids.insert(event["id"].as_str().unwrap_or_default().to_owned());
        let mut members: Vec<String> = event["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["clothing_id"].as_str())
            .map(str::to_owned)
            .collect();
        members.sort();
        members.dedup();
        teach(&members, pair_event_weight(event, true), None);
    }
    // Source lists stay shared and occur once per profile regardless of entry
    // count; entries are sorted, unique and omit zero-weight pairs.
    json!({
        "schema_version": 1,
        "algorithm_version": PAIR_ALGORITHM,
        "entries": edges
            .into_iter()
            .filter(|(_, weight)| *weight != 0)
            .map(|((a, b), weight)| json!({"clothing_ids":[a,b],"weight":weight.clamp(-5,5)}))
            .collect::<Vec<_>>(),
        "source": {
            "feedback_ids": feedback_ids.into_iter().collect::<Vec<_>>(),
            "wear_ids": wear_ids.into_iter().collect::<Vec<_>>(),
        },
    })
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
            // A managed publication rewrites created_at, so a stored pair
            // projection could no longer re-derive exactly as of that instant.
            // Reset must additionally forget learned pairs; deletion publishes
            // without pairs until the next explicit recompute restores them.
            profile
                .as_object_mut()
                .ok_or_else(|| ApiError::new("storage_error", "画像数据损坏"))?
                .remove("pair_projection");
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
        for signal in profile["inferred"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|_| profile["source_stale"] != true)
        {
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
        let context = json!({"algorithm_version":ALGORITHM,"profile_id":profile["id"],"explicit_count":profile["explicit"].as_array().map(Vec::len).unwrap_or(0),"inferred_count":profile["inferred"].as_array().map(Vec::len).unwrap_or(0),"source":"managed_member_memory","hard_constraints_override":true,"inferred_source_stale":profile["source_stale"]});
        Ok((context, weights))
    }
    // Pair rewards keyed (lexicographically smaller clothing ID, larger ID).
    // Empty unless a fresh profile carries a well-formed projection: stale
    // sources or an absent field must rank nothing rather than linger.
    pub(super) fn learning_pair_weights(&self, member: &str) -> Result<LearningWeights, ApiError> {
        let profile = self.learning_execute("learning.get", json!({"member_id":member}))?;
        let mut weights = std::collections::BTreeMap::new();
        if profile["source_stale"] == true {
            return Ok(weights);
        }
        let projection = match profile.get("pair_projection") {
            None | Some(Value::Null) => return Ok(weights),
            Some(projection) => projection,
        };
        let damaged = || ApiError::new("storage_error", "学习配对投影损坏");
        if projection["schema_version"] != 1
            || projection["algorithm_version"] != PAIR_ALGORITHM
            || !projection["entries"].is_array()
            || !projection["source"].is_object()
        {
            return Err(damaged());
        }
        for entry in projection["entries"].as_array().unwrap() {
            let ids = entry["clothing_ids"].as_array().ok_or_else(damaged)?;
            if ids.len() != 2 {
                return Err(damaged());
            }
            let a = ids[0].as_str().ok_or_else(damaged)?;
            let b = ids[1].as_str().ok_or_else(damaged)?;
            if a.is_empty() || a >= b {
                return Err(damaged());
            }
            let weight = entry["weight"]
                .as_i64()
                .filter(|weight| (-5..=5).contains(weight))
                .ok_or_else(damaged)?;
            weights.insert((a.to_owned(), b.to_owned()), weight);
        }
        Ok(weights)
    }
    fn learning_enqueue(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let key = string(&input, "operation_key")?;
        if key.trim().is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        if input
            .as_object()
            .ok_or_else(|| invalid("须为对象"))?
            .keys()
            .any(|k| !["member_id", "operation_key", "include_prior"].contains(&k.as_str()))
            || input.get("include_prior").is_some_and(|v| !v.is_boolean())
        {
            return Err(invalid("重算包含未知字段或无效完整重建选择"));
        }
        let tx = self.begin_write()?;
        let existing:Option<String>=tx.query_row("SELECT document FROM jobs WHERE member_id=?1 AND kind='learning_recompute' AND operation_key=?2",params![member,key],|r|r.get(0)).optional()?;
        let job = if let Some(stored) = existing {
            let job = parse(stored)?;
            if job["payload"]["request"] != input {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "重算标识不能改变输入",
                ));
            }
            job
        } else {
            super::jobs::insert(
                &tx,
                &member,
                "learning_recompute",
                key,
                json!({"request":input,"expected_state":self.learning_state(&member)?}),
            )?
        };
        tx.commit()?;
        Ok(
            json!({"schema_version":1,"member_id":member,"job_id":job["id"],"state":job["state"],"result":job["result"]}),
        )
    }
    fn learning_worker(&self, input: Value) -> Result<Value, ApiError> {
        if input
            .as_object()
            .ok_or_else(|| invalid("须为对象"))?
            .keys()
            .any(|k| !["member_id", "job_id"].contains(&k.as_str()))
        {
            return Err(invalid("worker包含未知字段"));
        }
        let member = self.member(&input)?;
        let id = string(&input, "job_id")?;
        let existing = self.job_get(json!({"member_id":member,"id":id}))?;
        if existing["kind"] != "learning_recompute" {
            return Err(invalid("不是画像重算任务"));
        }
        if ["succeeded", "failed", "cancelled", "waiting_input"]
            .contains(&existing["state"].as_str().unwrap_or(""))
        {
            return Ok(existing);
        }
        let claimed = self.job_claim(&member, id, 300)?;
        let lease = claimed["lease_token"].as_str().unwrap();
        let result = self.learning_recompute(
            claimed["payload"]["request"].clone(),
            id,
            lease,
            &claimed["payload"]["expected_state"],
        );
        match result {
            Ok(profile) => {
                let job = self.job_get(json!({"member_id":member,"id":id}))?;
                if job["state"] == "succeeded" {
                    Ok(job)
                } else {
                    self.job_finish(
                        &member,
                        id,
                        lease,
                        "succeeded",
                        json!({"profile_id":profile["id"]}),
                        Value::Null,
                    )
                }
            }
            Err(error) => {
                let job = self.job_get(json!({"member_id":member,"id":id}))?;
                if error.code == "lease_lost" {
                    return Ok(job);
                }
                self.job_finish(
                    &member,
                    id,
                    lease,
                    "failed",
                    Value::Null,
                    json!({"code":error.code,"message":error.message,"retryable":false}),
                )
            }
        }
    }
    fn learning_generation(&self) -> Result<String, ApiError> {
        Ok(self.connection.query_row(
            "SELECT generation FROM learning_source_versions WHERE member_id=''",
            [],
            |r| r.get(0),
        )?)
    }
    fn learning_source_version(&self, member: &str) -> Result<i64, ApiError> {
        Ok(self
            .connection
            .query_row(
                "SELECT version FROM learning_source_versions WHERE member_id=?1",
                [member],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
    fn learning_recompute(
        &self,
        input: Value,
        job_id: &str,
        lease: &str,
        expected_state: &Value,
    ) -> Result<Value, ApiError> {
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
            rusqlite::TransactionBehavior::Deferred,
        )?;
        let old:Option<(String,String)>=tx.query_row("SELECT request,result FROM learning_receipts WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((request, result)) = old {
            if parse(request)? != input {
                return Err(ApiError::new("idempotency_conflict", "操作标识已使用"));
            }
            let cached = parse(result)?;
            if let Some(id) = cached["profile_id"].as_str() {
                let saved: Option<String> = tx
                    .query_row(
                        "SELECT document FROM learning_profiles WHERE member_id=?1 AND id=?2",
                        params![member, id],
                        |r| r.get(0),
                    )
                    .optional()?;
                return parse(
                    saved.ok_or_else(|| {
                        ApiError::new("storage_error", "重算收据引用的画像不可用")
                    })?,
                );
            }
            return Ok(cached);
        }
        let include_prior = input
            .get("include_prior")
            .map(|v| {
                v.as_bool()
                    .ok_or_else(|| invalid("include_prior须为布尔值"))
            })
            .transpose()?
            .unwrap_or(false);
        let observed_state = self.learning_state(&member)?;
        if observed_state != *expected_state {
            return Err(ApiError::new(
                "memory_changed",
                "记忆边界已更新，请明确发起新的重算",
            ));
        }
        let source_version = self.learning_source_version(&member)?;
        let mut state = observed_state.clone();
        if include_prior {
            state = json!({"cutoff":null,"suppressed":[]});
        }
        // RFC3339 offsets describe instants, not a sortable textual representation.
        let cutoff = state["cutoff"]
            .as_str()
            .map(|value| {
                chrono::DateTime::parse_from_rfc3339(value)
                    .map_err(|_| ApiError::new("storage_error", "学习重置时间损坏"))
            })
            .transpose()?;
        let predates_reset = |event: &Value| -> Result<bool, ApiError> {
            let Some(cutoff) = cutoff.as_ref() else {
                return Ok(false);
            };
            let created = chrono::DateTime::parse_from_rfc3339(string(event, "created_at")?)
                .map_err(|_| ApiError::new("storage_error", "学习来源记录时间损坏"))?;
            Ok(created <= *cutoff)
        };
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
        let mut pair_feedback = Vec::new();
        {
            let mut st=tx.prepare("SELECT e.document FROM feedback_events e WHERE member_id=?1 AND revision=(SELECT MAX(revision) FROM feedback_events WHERE root_id=e.root_id) AND json_extract(document,'$.revoked')=0 ORDER BY root_id")?;
            for row in st.query_map([&member], |r| r.get::<_, String>(0))? {
                let event = parse(row?)?;
                if predates_reset(&event)? {
                    continue;
                }
                pair_feedback.push(event.clone());
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
        let mut pair_wears = Vec::new();
        for event in self.effective_wears(&member)? {
            if predates_reset(&event)? {
                continue;
            }
            pair_wears.push(event.clone());
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
        let pair_projection = pair_projection_from_events(&pair_feedback, &pair_wears);
        let inferred:Vec<Value>=signals.into_iter().filter(|((dimension,value),_)|!state["suppressed"].as_array().into_iter().flatten().any(|v|v["dimension"]==*dimension&&v["value"]==*value)).map(|((dimension,value),(weight,feedback_ids,wear_ids))|json!({"dimension":dimension,"value":value,"weight":weight.clamp(-5,5),"source":{"kind":"inferred_events","feedback_ids":feedback_ids,"wear_ids":wear_ids},"algorithm_version":ALGORITHM})).collect();
        let id = uuid::Uuid::now_v7().to_string();
        let doc = json!({"schema_version":1,"id":id,"member_id":member,"algorithm_version":ALGORITHM,"inferred":inferred,"pair_projection":pair_projection,"reset_boundary":state["cutoff"],"include_prior":include_prior,"local_source_version":source_version,"local_installation_generation":self.learning_generation()?,"created_at":chrono::Utc::now().to_rfc3339()});
        let serialized = doc.to_string();
        tx.commit()?;
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":job_id}))?;
        if job["state"] != "running"
            || job["lease_token"] != lease
            || job["lease_expires"].as_i64().unwrap_or(0) <= chrono::Utc::now().timestamp_millis()
        {
            return Err(ApiError::new("lease_lost", "画像任务已取消或lease失效"));
        }
        if self.learning_state(&member)? != observed_state
            || self.learning_source_version(&member)? != source_version
        {
            return Err(ApiError::new(
                "memory_changed",
                "事件或记忆边界已变化，未发布迟到画像，请明确重新计算",
            ));
        }
        if include_prior {
            tx.execute("INSERT INTO learning_state VALUES(?1,?2) ON CONFLICT(member_id) DO UPDATE SET document=excluded.document",params![member,state.to_string()])?;
        }
        tx.execute(
            "INSERT INTO learning_profiles VALUES(?1,?2,?3)",
            params![id, member, serialized],
        )?;
        tx.execute(
            "INSERT INTO learning_receipts VALUES(?1,?2,?3,?4)",
            params![
                member,
                key,
                input.to_string(),
                json!({"profile_id":id}).to_string()
            ],
        )?;
        tx.execute(
            "INSERT INTO learning_history VALUES(?1,?2,?3)",
            params![id, member, json!({"schema_version":1,"id":id,"member_id":member,"operation":"learning.recompute","profile_id":id,"algorithm_version":ALGORITHM,"created_at":doc["created_at"]}).to_string()],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'learning.recompute',?2)",
            params![id, json!({"schema_version":1,"id":id,"member_id":member,"profile_id":id,"algorithm_version":ALGORITHM,"created_at":doc["created_at"]}).to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![id, serialized],
        )?;
        job["state"] = json!("succeeded");
        job["progress"] = json!(100);
        job["result"] = json!({"profile_id":id});
        job["error"] = Value::Null;
        job["lease_token"] = Value::Null;
        job["lease_expires"] = json!(0);
        job["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        super::jobs::save(&tx, &job)?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn learning_execute(&self, op: &str, mut input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        input["member_id"] = json!(member);
        if op == "learning.worker.run" {
            return self.learning_worker(input);
        }
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
            return self.learning_enqueue(input);
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
            profile["source_stale"] = json!(
                profile.get("id").is_some()
                    && (profile["local_installation_generation"].as_str()
                        != Some(self.learning_generation()?.as_str())
                        || profile["local_source_version"].as_i64()
                            != Some(self.learning_source_version(&member)?))
            );
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
    let memory = json!({"type":"object","required":["schema_version","id","member_id","dimension","value","weight","note","source","algorithm_version","root_id","deleted","revision","created_at"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"dimension":{"enum":["color","style","material","pattern","category","occasion"]},"value":{"type":"string"},"weight":{"type":"integer","minimum":-5,"maximum":5},"note":{"type":"string"},"source":{"type":"object","required":["kind"],"properties":{"kind":{"enum":["explicit_user"]}}},"algorithm_version":{"type":"string"},"root_id":{"type":"string"},"deleted":{"type":"boolean"},"revision":{"type":"integer","minimum":1},"created_at":{"type":"string"}}});
    // delete/reset发布收据：同一形状，operation标识具体管理动作
    let manage_receipt = json!({"type":"object","required":["schema_version","id","member_id","operation","state","created_at"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"operation":{"enum":["learning.edit","learning.delete","learning.reset"]},"state":{"type":"object","required":["cutoff","suppressed"],"properties":{"cutoff":{"type":["string","null"]},"suppressed":{"type":"array","items":{"type":"object","required":["dimension","value"],"properties":{"dimension":{"enum":["color","style","material","pattern","category","occasion"]},"value":{"type":"string"}}}}}},"created_at":{"type":"string"}}});
    let profile = json!({"type":"object","required":["schema_version","member_id","inferred","algorithm_version","explicit","source_stale"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"inferred":{"type":"array"},"algorithm_version":{"type":"string"},"explicit":{"type":"array","items":memory},"source_stale":{"type":"boolean"},"id":{"type":"string"},"created_at":{"type":"string"},"local_installation_generation":{"type":["string","null"]},"local_source_version":{"type":["integer","null"]},"reset_boundary":{"type":["string","null"]},"pair_projection":{"type":["object","null"]}}});
    let receipt = json!({"type":"object","required":["schema_version","member_id","job_id","state","result"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"job_id":{"type":"string"},"state":{"type":"string"},"result":{"type":["object","null"]}}});
    // worker.run返回完整持久化任务文档（kind固定learning_recompute）
    let job_doc = json!({"type":"object","required":["schema_version","id","member_id","kind","operation_key","state","progress","attempts","next_due","lease_token","lease_expires","payload","result","error","created_at","updated_at"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"kind":{"const":"learning_recompute"},"operation_key":{"type":"string"},"state":{"type":"string"},"progress":{"type":"integer"},"attempts":{"type":"integer"},"next_due":{"type":"integer"},"lease_token":{"type":["string","null"]},"lease_expires":{"type":"integer"},"payload":{"type":"object"},"result":{"type":["object","null"]},"error":{"type":["object","null"]},"created_at":{"type":"string"},"updated_at":{"type":"string"},"submitted_at":{"type":["string","null"]},"last_outcome":{"type":["string","null"]}}});
    ["get","set","edit","delete","reset","recompute","history","worker.run"].into_iter().map(|op|{
 let mut props=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];
 if !["get","history","worker.run"].contains(&op){props["operation_key"]=json!({"type":"string","minLength":1,"maxLength":200});required.push("operation_key");}
 if ["set","delete"].contains(&op){props["dimension"]=json!({"enum":["color","style","material","pattern","category","occasion"]});props["value"]=json!({"type":"string","minLength":1,"maxLength":100});required.extend(["dimension","value"]);}
 if ["set","edit"].contains(&op){props["weight"]=json!({"type":"integer","minimum":-5,"maximum":5});props["note"]=json!({"type":"string","maxLength":2000});required.push("weight");}
 if op=="edit"{props["root_id"]=json!({"type":"string"});props["expected_revision"]=json!({"type":"integer","minimum":1});required.extend(["root_id","expected_revision"]);}
 if op=="reset"{props["scope"]=json!({"enum":["inferred","all"]});required.push("scope");}
 if op=="recompute"{props["include_prior"]=json!({"type":"boolean","description":"明确重新纳入删除/重置前全部事实；否则保留删除和重置边界"});}
 if op=="worker.run"{props["job_id"]=json!({"type":"string"});required.push("job_id");}
 if op=="history"{props["cursor"]=json!({"type":"string"});props["limit"]=json!({"type":"integer","minimum":1,"maximum":100});}
 let output=match op {
            "get"=>profile.clone(),
            "set"|"edit"=>memory.clone(),
            "delete"|"reset"=>manage_receipt.clone(),
            "recompute"=>receipt.clone(),
            "worker.run"=>job_doc.clone(),
            "history"=>json!({"type":"object","required":["schema_version","member_id","items","next_cursor"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"items":{"type":"array"},"next_cursor":{"type":["string","null"]}}}),
            _=>profile.clone(),
        };
        let (effect,examples)=match op {
            "get"=>(
                json!({"kind":"query","network":"none","local_files":false,"async":null}),
                json!([{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["读取画像与显式偏好；无保存画像时返回空推断","source_stale表示画像落后于本机事实，落后时不参与推荐"]}]),
            ),
            "set" => (
                json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
                json!([{"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>","dimension":"color","value":"藏青","weight":3},"prerequisites":["显式偏好覆盖同特征推断权重","同特征同值已存在时报preference_exists，请edit现有修订","不改变衣物事实"]}]),
            ),
            "edit"=>(
                json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
                json!([{"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>","root_id":"<偏好root_id>","expected_revision":1,"weight":-2},"prerequisites":["乐观并发：expected_revision不符报stale_revision","修订追加不覆盖历史；已删除偏好不可编辑"]}]),
            ),
            "delete"=>(
                json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
                json!([{"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>","dimension":"color","value":"藏青"},"prerequisites":["抑制该特征并墓碑化现有显式偏好；立即发布画像使推荐遗忘","历史保留，可用recompute(include_prior=true)恢复"]}]),
            ),
            "reset"=>(
                json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
                json!([{"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>","scope":"inferred"},"prerequisites":["scope=inferred仅清推断；all同时墓碑化全部显式偏好","设置重置边界并立即发布空推断；历史保留"]}]),
            ),
            "recompute"=>(
                json!({"kind":"mutation","network":"none","local_files":false,"async":{"job_kind":"learning_recompute","process":"learning.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]}}),
                json!([{"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>"},"prerequisites":["异步入队画像重算任务，立即返回任务收据而非画像","由learning.worker.run本地重算，不联网不消耗额度","同operation_key重复调用返回同一任务，不能改变输入"]}]),
            ),
            "history"=>(
                json!({"kind":"query","network":"none","local_files":false,"async":null}),
                json!([{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["按ID升序返回学习操作历史（含set/edit/delete/reset发布收据）","游标为最后一条条目ID字符串，不属于当前成员时拒绝"]}]),
            ),
            _=>(
                json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
                json!([{"request":{"member_id":"<已授权成员ID>","job_id":"<learning_recompute任务ID>"},"prerequisites":["同步处理已入队画像重算任务；纯本地计算不消耗额度","终态succeeded/failed/cancelled/waiting_input时幂等返回任务文档","本示例不自动执行"]}]),
            ),
        };
 json!({"name":format!("learning.{op}"),"description":"成员偏好与推断记忆管理，不改变事实","effect":effect,"examples":examples,"inputSchema":{"type":"object","properties":props,"required":required,"additionalProperties":false},"outputSchema":output})
 }).collect()
}

pub(super) fn validate_portable_document(
    db: &rusqlite::Connection,
    table: &str,
    member: &str,
    doc: &Value,
) -> Result<(), ApiError> {
    fn bad() -> ApiError {
        ApiError::new("backup_invalid", "学习领域数据或来源引用损坏")
    }
    fn feature(v: &Value) -> bool {
        matches!(
            v["dimension"].as_str(),
            Some("color" | "style" | "material" | "pattern" | "category" | "occasion")
        ) && v["value"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 100)
    }
    fn state(v: &Value) -> bool {
        (v["cutoff"].is_null()
            || v["cutoff"]
                .as_str()
                .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok()))
            && v["suppressed"]
                .as_array()
                .is_some_and(|a| a.iter().all(feature))
    }
    let member_exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM members WHERE id=?1)",
        [member],
        |r| r.get(0),
    )?;
    if !member_exists {
        return Err(bad());
    }
    if table == "learning_state" {
        return if state(doc) { Ok(()) } else { Err(bad()) };
    }
    if !doc["created_at"]
        .as_str()
        .is_some_and(|v| chrono::DateTime::parse_from_rfc3339(v).is_ok())
    {
        return Err(bad());
    }
    if doc["member_id"] != member || doc["schema_version"] != 1 {
        return Err(bad());
    }
    if table == "learning_memories" {
        let revision = doc["revision"]
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or_else(bad)?;
        if !feature(doc)
            || !doc["weight"]
                .as_i64()
                .is_some_and(|v| (-5..=5).contains(&v))
            || !doc["deleted"].is_boolean()
            || doc["source"]["kind"] != "explicit_user"
            || doc["algorithm_version"] != ALGORITHM
            || !doc["note"]
                .as_str()
                .is_some_and(|s| s.chars().count() <= 2000)
        {
            return Err(bad());
        }
        let root = doc["root_id"].as_str().ok_or_else(bad)?;
        let root_doc: Option<String> = db
            .query_row(
                "SELECT document FROM learning_memories WHERE member_id=?1 AND id=?2",
                params![member, root],
                |r| r.get(0),
            )
            .optional()?;
        let root_doc = parse(root_doc.ok_or_else(bad)?)?;
        if root_doc["revision"] != 1
            || root_doc["root_id"] != root
            || root_doc["dimension"] != doc["dimension"]
            || root_doc["value"] != doc["value"]
        {
            return Err(bad());
        }
        let current_count:i64=db.query_row("SELECT COUNT(*) FROM learning_memories WHERE member_id=?1 AND json_extract(document,'$.root_id')=?2 AND json_extract(document,'$.revision')=?3",params![member,root,revision],|r|r.get(0))?;
        if current_count != 1 {
            return Err(bad());
        }
        if revision == 1 && doc["id"] != root {
            return Err(bad());
        }
        if revision > 1 {
            let count:i64=db.query_row("SELECT COUNT(*) FROM learning_memories WHERE member_id=?1 AND json_extract(document,'$.root_id')=?2 AND json_extract(document,'$.revision')=?3",params![member,root,revision-1],|r|r.get(0))?;
            if count != 1 {
                return Err(bad());
            }
        }
    }
    if table == "learning_profiles" {
        if doc["algorithm_version"] != ALGORITHM
            || (!doc["reset_boundary"].is_null()
                && !doc["reset_boundary"]
                    .as_str()
                    .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok()))
            || doc.get("include_prior").is_some_and(|v| !v.is_boolean())
        {
            return Err(bad());
        }
        let signals = doc["inferred"].as_array().ok_or_else(bad)?;
        for signal in signals {
            if signal["source"]["kind"] != "inferred_events"
                || signal["algorithm_version"] != ALGORITHM
            {
                return Err(bad());
            }
            let mut total = 0i64;
            if !feature(signal)
                || !signal["weight"]
                    .as_i64()
                    .is_some_and(|v| (-5..=5).contains(&v))
            {
                return Err(bad());
            }
            for (key, source_table) in [
                ("feedback_ids", "feedback_events"),
                ("wear_ids", "wear_events"),
            ] {
                let ids = signal["source"][key].as_array().ok_or_else(bad)?;
                let mut seen = std::collections::HashSet::new();
                for id in ids {
                    let id = id.as_str().ok_or_else(bad)?;
                    if !seen.insert(id) {
                        return Err(bad());
                    }
                    let exists: i64 = db.query_row(
                        &format!(
                            "SELECT COUNT(*) FROM {source_table} WHERE member_id=?1 AND id=?2"
                        ),
                        params![member, id],
                        |r| r.get(0),
                    )?;
                    if exists != 1 {
                        return Err(bad());
                    }
                    let raw: String = db.query_row(
                        &format!(
                            "SELECT document FROM {source_table} WHERE member_id=?1 AND id=?2"
                        ),
                        params![member, id],
                        |r| r.get(0),
                    )?;
                    let event = parse(raw)?;
                    let snapshots: Vec<Value> = if key == "wear_ids" {
                        event["items"]
                            .as_array()
                            .ok_or_else(bad)?
                            .iter()
                            .map(|v| v["snapshot"].clone())
                            .collect()
                    } else {
                        event["inventory_snapshots"]
                            .as_array()
                            .ok_or_else(bad)?
                            .clone()
                    };
                    let dimension = signal["dimension"].as_str().ok_or_else(bad)?;
                    if !snapshots.iter().any(|v| {
                        v[dimension] == signal["value"]
                            || v[dimension]
                                .as_array()
                                .is_some_and(|a| a.contains(&signal["value"]))
                    }) {
                        return Err(bad());
                    }
                    let scores = &event["scores"];
                    let base = if key == "wear_ids" {
                        1
                    } else {
                        match event["reaction"].as_str() {
                            Some("like") => 2,
                            Some("dislike") => -2,
                            _ => 0,
                        }
                    };
                    let mut weight = base + scores["overall"].as_i64().map(|v| v - 3).unwrap_or(0);
                    if dimension == "material" {
                        weight += scores["comfort"].as_i64().map(|v| v - 3).unwrap_or(0);
                    }
                    if ["color", "pattern", "style"].contains(&dimension) {
                        weight += scores["style"].as_i64().map(|v| v - 3).unwrap_or(0);
                    }
                    if weight == 0 {
                        return Err(bad());
                    }
                    total += weight;
                }
            }
            if signal["weight"].as_i64() != Some(total.clamp(-5, 5)) {
                return Err(bad());
            }
        }
        // A stored pair projection is a derived fact over its own recorded
        // source events: absence stays valid (pre-pair exporters and managed
        // reset publications), and a present projection must re-derive exactly
        // from those same events inside this archive. This is the pair
        // analogue of the inferred-signal checks above, so a tampered weight,
        // edge set or source list cannot pass backup validation.
        if let Some(projection) = doc.get("pair_projection").filter(|v| !v.is_null()) {
            let ordered_unique = |ids: &Value| -> Option<Vec<String>> {
                let mut previous: Option<&str> = None;
                let mut result = Vec::new();
                for value in ids.as_array()? {
                    let id = value.as_str()?;
                    if id.is_empty() || previous.is_some_and(|previous| previous >= id) {
                        return None;
                    }
                    previous = Some(id);
                    result.push(id.to_owned());
                }
                Some(result)
            };
            let feedback_ids =
                ordered_unique(&projection["source"]["feedback_ids"]).ok_or_else(bad)?;
            let wear_ids = ordered_unique(&projection["source"]["wear_ids"]).ok_or_else(bad)?;
            if projection["schema_version"] != 1
                || projection["algorithm_version"] != PAIR_ALGORITHM
                || !projection["entries"].is_array()
            {
                return Err(bad());
            }
            let cutoff = doc["reset_boundary"]
                .as_str()
                .map(|value| chrono::DateTime::parse_from_rfc3339(value).map_err(|_| bad()))
                .transpose()?;
            let events = |ids: &[String], table: &str| -> Result<Vec<Value>, ApiError> {
                let mut events = Vec::with_capacity(ids.len());
                for id in ids {
                    let raw: Option<String> = db
                        .query_row(
                            &format!("SELECT document FROM {table} WHERE member_id=?1 AND id=?2"),
                            params![member, id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    let event = parse(raw.ok_or_else(bad)?)?;
                    let created = chrono::DateTime::parse_from_rfc3339(
                        event["created_at"].as_str().ok_or_else(bad)?,
                    )
                    .map_err(|_| bad())?;
                    if cutoff.is_some_and(|cutoff| created <= cutoff) {
                        return Err(bad());
                    }
                    events.push(event);
                }
                Ok(events)
            };
            let feedback = events(&feedback_ids, "feedback_events")?;
            let wears = events(&wear_ids, "wear_events")?;
            if *projection != pair_projection_from_events(&feedback, &wears) {
                return Err(bad());
            }
        }
    }
    if table == "learning_history" && doc.get("state").is_some() && !state(&doc["state"]) {
        return Err(bad());
    }
    Ok(())
}

pub(super) fn validate_portable_receipts(db: &rusqlite::Connection) -> Result<(), ApiError> {
    let mut st =
        db.prepare("SELECT member_id,operation_key,request,result FROM learning_receipts")?;
    let mut rows = st.query([])?;
    while let Some(row) = rows.next()? {
        let member: String = row.get(0)?;
        let key: String = row.get(1)?;
        let request = parse(row.get(2)?)?;
        let result = parse(row.get(3)?)?;
        let input = if request.get("operation").is_some() {
            &request["input"]
        } else {
            &request
        };
        if input["member_id"] != member || input["operation_key"] != key || key.is_empty() {
            return Err(ApiError::new("backup_invalid", "学习收据身份损坏"));
        }
        let id = result["profile_id"]
            .as_str()
            .or_else(|| result["id"].as_str())
            .ok_or_else(|| ApiError::new("backup_invalid", "学习收据缺少结果身份"))?;
        let exists:i64=db.query_row("SELECT (SELECT COUNT(*) FROM learning_profiles WHERE member_id=?1 AND id=?2)+(SELECT COUNT(*) FROM learning_memories WHERE member_id=?1 AND id=?2)+(SELECT COUNT(*) FROM learning_history WHERE member_id=?1 AND id=?2)",params![member,id],|r|r.get(0))?;
        if exists < 1 || result.get("member_id").is_some_and(|v| v != &json!(member)) {
            return Err(ApiError::new("backup_invalid", "学习收据来源不属成员"));
        }
    }
    Ok(())
}
