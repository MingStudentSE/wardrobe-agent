use crate::{string, ApiError, Application};
use chrono::{TimeZone, Utc};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS feedback_events(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),root_id TEXT NOT NULL,revision INTEGER NOT NULL,document TEXT NOT NULL,UNIQUE(root_id,revision)); CREATE INDEX IF NOT EXISTS feedback_member_root ON feedback_events(member_id,root_id,revision); CREATE TABLE IF NOT EXISTS feedback_receipts(member_id TEXT NOT NULL,operation_key TEXT NOT NULL,request TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(member_id,operation_key)); CREATE TABLE IF NOT EXISTS feedback_asset_refs(event_id TEXT NOT NULL REFERENCES feedback_events(id),asset_id TEXT NOT NULL,member_id TEXT NOT NULL,PRIMARY KEY(event_id,asset_id),FOREIGN KEY(asset_id,member_id) REFERENCES assets(id,member_id));")?;
    Ok(())
}
fn parse(s: String) -> Result<Value, ApiError> {
    serde_json::from_str(&s).map_err(|_| ApiError::new("storage_error", "反馈记录损坏"))
}
fn invalid(s: &str) -> ApiError {
    ApiError::new("invalid_input", s)
}
fn reaction(value: &Value) -> Result<&str, ApiError> {
    let value = value.as_str().ok_or_else(|| invalid("反馈须为文字"))?;
    if ![
        "like",
        "dislike",
        "skip",
        "day_exclude",
        "actual_wore",
        "changed",
    ]
    .contains(&value)
    {
        return Err(invalid("反馈类型不支持"));
    }
    Ok(value)
}
fn scores(value: &Value) -> Result<Value, ApiError> {
    let obj = value.as_object().ok_or_else(|| invalid("评分须为对象"))?;
    let mut scores = json!({"overall":null,"comfort":null,"style":null});
    for (key, value) in obj {
        if !["overall", "comfort", "style"].contains(&key.as_str())
            || (!value.is_null() && !value.as_u64().is_some_and(|n| (1..=5).contains(&n)))
        {
            return Err(invalid("评分须为空或1–5整数"));
        }
        scores[key] = value.clone();
    }
    Ok(scores)
}
fn selected(proposal: &Value, values: &Value) -> Result<Vec<String>, ApiError> {
    let values = values
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 100)
        .ok_or_else(|| invalid("须选择1–100件单品"))?;
    let items = proposal["items"]
        .as_array()
        .ok_or_else(|| invalid("方案单品格式损坏"))?;
    let mut ids = vec![];
    for value in values {
        let id = value
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("单品ID须为文字"))?;
        if ids.contains(&id.to_owned()) || !items.iter().any(|item| item["clothing_id"] == id) {
            return Err(invalid("排除ID须是方案内不重复单品"));
        }
        ids.push(id.to_owned());
    }
    Ok(ids)
}
pub(super) fn validate_stored_document(
    db: &rusqlite::Connection,
    doc: &Value,
) -> Result<(), ApiError> {
    let kind = reaction(&doc["reaction"])?;
    if doc["schema_version"] != 1
        || !doc["revoked"].is_boolean()
        || scores(&doc["scores"])? != doc["scores"]
    {
        return Err(invalid("反馈格式不支持"));
    }
    let member = string(doc, "member_id")?;
    let revision = doc["revision"]
        .as_u64()
        .filter(|r| *r > 0)
        .ok_or_else(|| invalid("反馈修订损坏"))?;
    if revision == 1 {
        if !doc["supersedes_event_id"].is_null() {
            return Err(invalid("初始反馈不能引用前驱"));
        }
    } else {
        let predecessor = string(doc, "supersedes_event_id")?;
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM feedback_events WHERE id=?1 AND member_id=?2 AND root_id=?3 AND revision<?4)",
            params![predecessor, member, string(doc, "root_id")?, revision],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(invalid("反馈前驱缺失或根与成员不符"));
        }
    }
    let invalid_refs: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM feedback_asset_refs r LEFT JOIN assets a ON a.id=r.asset_id AND a.member_id=r.member_id WHERE r.event_id=?1 AND (r.member_id<>?2 OR a.id IS NULL))",
        params![string(doc, "id")?, member],
        |row| row.get(0),
    )?;
    if invalid_refs {
        return Err(invalid("反馈展示资产不属记录成员"));
    }
    for field in ["created_at", "inventory_observed_at"] {
        chrono::DateTime::parse_from_rfc3339(string(doc, field)?)
            .map_err(|_| invalid("反馈观察时间损坏"))?;
    }
    let raw: Option<String> = db
        .query_row(
            "SELECT document FROM recommendation_intents WHERE id=?1 AND member_id=?2",
            params![string(doc, "intent_id")?, member],
            |r| r.get(0),
        )
        .optional()?;
    let intent = parse(raw.ok_or_else(|| invalid("反馈来源意图不存在"))?)?;
    let index = doc["proposal_index"]
        .as_u64()
        .ok_or_else(|| invalid("反馈方案序号损坏"))? as usize;
    if intent["proposals"].as_array().and_then(|v| v.get(index)) != Some(&doc["proposal"])
        || doc["controls"] != intent["controls"]
    {
        return Err(invalid("反馈方案与不可变意图不符"));
    }
    let items = doc["proposal"]["items"]
        .as_array()
        .ok_or_else(|| invalid("反馈方案损坏"))?;
    let snapshots = doc["inventory_snapshots"]
        .as_array()
        .ok_or_else(|| invalid("反馈观察快照损坏"))?;
    if snapshots.len() != items.len() {
        return Err(invalid("反馈观察快照数量不符"));
    }
    for (snapshot, item) in snapshots.iter().zip(items) {
        crate::clothing::validate_stored_document(snapshot)?;
        if snapshot["id"] != item["clothing_id"] || snapshot["member_id"] != member {
            return Err(invalid("反馈观察快照身份不符"));
        }
    }
    if kind == "day_exclude" {
        selected(&doc["proposal"], &doc["clothing_ids"])?;
        let scope = &doc["day_scope"];
        let zone: chrono_tz::Tz = string(scope, "timezone")?
            .parse()
            .map_err(|_| invalid("排除时区损坏"))?;
        let text = string(scope, "local_date")?;
        let date = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .map_err(|_| invalid("排除日期损坏"))?;
        if date.to_string() != text {
            return Err(invalid("排除日期格式损坏"));
        }
        let midnight = date
            .succ_opt()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .ok_or_else(|| invalid("排除日期越界"))?;
        let expiry = zone
            .from_local_datetime(&midnight)
            .earliest()
            .ok_or_else(|| invalid("排除到期时间损坏"))?;
        let stored = chrono::DateTime::parse_from_rfc3339(string(scope, "expires_at")?)
            .map_err(|_| invalid("排除到期时间损坏"))?;
        if stored != expiry {
            return Err(invalid("排除到期时间与本地日期不符"));
        }
    } else if !doc["day_scope"].is_null() || doc["clothing_ids"] != json!([]) {
        return Err(invalid("普通反馈不包含当天排除"));
    }
    if ["actual_wore", "changed"].contains(&kind) {
        let raw: Option<String> = db
            .query_row(
                "SELECT document FROM wear_events WHERE id=?1 AND member_id=?2",
                params![string(doc, "wear_event_id")?, member],
                |r| r.get(0),
            )
            .optional()?;
        let wear = parse(raw.ok_or_else(|| invalid("实际穿着事实缺失或不属成员"))?)?;
        let mut actual: Vec<&str> = wear["items"]
            .as_array()
            .ok_or_else(|| invalid("实际穿着单品损坏"))?
            .iter()
            .map(|i| string(i, "clothing_id"))
            .collect::<Result<_, _>>()?;
        let mut proposed: Vec<&str> = items
            .iter()
            .map(|i| string(i, "clothing_id"))
            .collect::<Result<_, _>>()?;
        actual.sort();
        proposed.sort();
        if (kind == "actual_wore" && actual != proposed)
            || (kind == "changed" && actual == proposed)
        {
            return Err(invalid("实际穿着组合与反馈类型不符"));
        }
    } else if !doc["wear_event_id"].is_null() {
        return Err(invalid("普通反馈不引用实际穿着"));
    }
    Ok(())
}
impl Application {
    pub(super) fn feedback_disliked_sets(
        &self,
        member: &str,
    ) -> Result<std::collections::BTreeSet<Vec<String>>, ApiError> {
        let mut st=self.connection.prepare("SELECT e.document FROM feedback_events e WHERE member_id=?1 AND revision=(SELECT MAX(revision) FROM feedback_events WHERE root_id=e.root_id) AND json_extract(document,'$.revoked')=0 AND json_extract(document,'$.reaction')='dislike'")?;
        let mut sets = std::collections::BTreeSet::new();
        for row in st.query_map([member], |r| r.get::<_, String>(0))? {
            let doc = parse(row?)?;
            let mut ids: Vec<String> = doc["proposal"]["items"]
                .as_array()
                .ok_or_else(|| ApiError::new("storage_error", "反馈方案损坏"))?
                .iter()
                .map(|i| {
                    i["clothing_id"]
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| ApiError::new("storage_error", "反馈单品损坏"))
                })
                .collect::<Result<_, _>>()?;
            ids.sort();
            sets.insert(ids);
        }
        Ok(sets)
    }
    pub(super) fn feedback_day_exclusions(&self, member: &str) -> Result<Vec<String>, ApiError> {
        let mut statement=self.connection.prepare("SELECT e.document FROM feedback_events e WHERE member_id=?1 AND revision=(SELECT MAX(revision) FROM feedback_events WHERE root_id=e.root_id) AND json_extract(document,'$.revoked')=0 AND json_extract(document,'$.reaction')='day_exclude'")?;
        let mut ids = vec![];
        for row in statement.query_map([member], |r| r.get::<_, String>(0))? {
            let doc = parse(row?)?;
            let end = doc["day_scope"]["expires_at"]
                .as_str()
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .ok_or_else(|| ApiError::new("storage_error", "当天排除期限损坏"))?;
            if end > Utc::now() {
                for id in doc["clothing_ids"]
                    .as_array()
                    .ok_or_else(|| ApiError::new("storage_error", "排除ID损坏"))?
                {
                    let id = id
                        .as_str()
                        .ok_or_else(|| ApiError::new("storage_error", "排除ID损坏"))?
                        .to_owned();
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
        Ok(ids)
    }
    fn feedback_revision(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        let request = json!({"operation":op,"input":input}).to_string();
        let tx = self.begin_write()?;
        if let Some((old,result))=tx.query_row("SELECT request,result FROM feedback_receipts WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?{if old!=request{return Err(ApiError::new("idempotency_conflict","同操作标识不能改变反馈"));}return parse(result);}
        let old:Option<String>=tx.query_row("SELECT document FROM feedback_events WHERE member_id=?1 AND root_id=?2 ORDER BY revision DESC LIMIT 1",params![member,string(&input,"root_id")?],|r|r.get(0)).optional()?;
        let mut doc = parse(old.ok_or_else(|| ApiError::new("not_found", "反馈不存在"))?)?;
        if input["expected_revision"]
            .as_u64()
            .filter(|n| *n > 0)
            .is_none()
        {
            return Err(invalid("须提供正整数expected_revision"));
        }
        if input["expected_revision"] != doc["revision"] {
            return Err(ApiError::new("version_conflict", "反馈已变更，请重新读取"));
        }
        if doc["revoked"] == true {
            return Err(ApiError::new("invalid_state", "反馈已撤销，请新增明确反馈"));
        }
        if op == "feedback.revoke" {
            let reason = string(&input, "reason")?;
            if reason.trim().is_empty() || reason.chars().count() > 2000 {
                return Err(invalid("撤销原因须为1–2000字符"));
            }
            doc["revoked"] = json!(true);
            doc["reason"] = json!(reason);
        } else {
            if input.get("reaction").is_none() && input.get("scores").is_none() {
                return Err(invalid("须修改反馈或评分"));
            }
            if let Some(reaction) = input.get("reaction") {
                let reaction = reaction.as_str().ok_or_else(|| invalid("反馈须为文字"))?;
                if !["like", "dislike", "skip"].contains(&reaction)
                    || !["like", "dislike", "skip"]
                        .contains(&doc["reaction"].as_str().unwrap_or(""))
                {
                    return Err(invalid(
                        "当天排除和实际穿着不能改为普通意见；请撤销反馈，穿着事实另行更正",
                    ));
                }
                doc["reaction"] = json!(reaction);
            }
            if let Some(scores) = input.get("scores") {
                let validated = self::scores(scores)?;
                for key in scores.as_object().unwrap().keys() {
                    doc["scores"][key] = validated[key].clone();
                }
            }
        }
        doc["supersedes_event_id"] = doc["id"].clone();
        doc["id"] = json!(uuid::Uuid::now_v7().to_string());
        doc["revision"] = json!(doc["revision"].as_u64().unwrap() + 1);
        doc["created_at"] = json!(Utc::now().to_rfc3339());
        tx.execute(
            "INSERT INTO feedback_events VALUES(?1,?2,?3,?4,?5)",
            params![
                doc["id"].as_str(),
                member,
                doc["root_id"].as_str(),
                doc["revision"].as_u64(),
                doc.to_string()
            ],
        )?;
        tx.execute("INSERT INTO feedback_asset_refs SELECT ?1,asset_id,member_id FROM feedback_asset_refs WHERE event_id=?2",params![doc["id"].as_str(),doc["supersedes_event_id"].as_str()])?;
        tx.execute(
            "INSERT INTO feedback_receipts VALUES(?1,?2,?3,?4)",
            params![member, key, request, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,?2,?3)",
            params![doc["id"].as_str(), op, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![doc["id"].as_str(), doc.to_string()],
        )?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn feedback_execute(&self, op: &str, mut input: Value) -> Result<Value, ApiError> {
        let allowed = match op {
            "feedback.try_another" => {
                vec!["member_id", "operation_key", "intent_id", "proposal_index"]
            }
            "feedback.record" => vec![
                "member_id",
                "operation_key",
                "intent_id",
                "proposal_index",
                "reaction",
                "scores",
                "wear",
                "timezone",
                "clothing_ids",
            ],
            "feedback.correct" => vec![
                "member_id",
                "operation_key",
                "root_id",
                "expected_revision",
                "reaction",
                "scores",
            ],
            "feedback.revoke" => vec![
                "member_id",
                "operation_key",
                "root_id",
                "expected_revision",
                "reason",
            ],
            "feedback.history" => vec!["member_id", "root_id", "limit", "cursor"],
            "feedback.list" => vec!["member_id", "limit", "cursor", "include_revoked"],
            _ => vec!["member_id", "id"],
        };
        if input
            .as_object()
            .unwrap()
            .keys()
            .any(|k| !allowed.contains(&k.as_str()))
        {
            return Err(invalid("反馈包含不支持字段"));
        }
        let member = self.member(&input)?;
        input["member_id"] = json!(member);
        if op == "feedback.try_another" {
            let key = string(&input, "operation_key")?;
            if key.is_empty() || key.len() > 200 {
                return Err(invalid("操作标识须为1–200字节"));
            }
            let token = Sha256::digest(key.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let feedback=self.feedback_execute("feedback.record",json!({"member_id":member,"operation_key":format!("try-feedback:{token}"),"intent_id":string(&input,"intent_id")?,"proposal_index":input["proposal_index"],"reaction":"skip"}))?;
            let recommendation=self.recommendation_execute("recommendation.change",json!({"member_id":member,"operation_key":format!("try-recommendation:{token}"),"id":input["intent_id"],"proposal_index":input["proposal_index"]})).map_err(|e|ApiError::new(&e.code,&format!("跳过反馈已保存；换套未完成：{}",e.message)))?;
            return Ok(
                json!({"schema_version":1,"member_id":member,"feedback":feedback,"recommendation":recommendation}),
            );
        }
        if op == "feedback.history" {
            let root = string(&input, "root_id")?;
            let limit = input
                .get("limit")
                .map(|v| {
                    v.as_u64()
                        .filter(|v| (1..=100).contains(v))
                        .ok_or_else(|| invalid("数量须为1–100"))
                })
                .transpose()?
                .unwrap_or(50);
            let cursor = input
                .get("cursor")
                .map(|v| v.as_u64().ok_or_else(|| invalid("历史游标须为修订整数")))
                .transpose()?
                .unwrap_or(0);
            let mut statement=self.connection.prepare("SELECT document FROM feedback_events WHERE member_id=?1 AND root_id=?2 AND revision>?3 ORDER BY revision LIMIT ?4")?;
            let mut items = vec![];
            for row in statement.query_map(params![member, root, cursor, limit + 1], |r| {
                r.get::<_, String>(0)
            })? {
                items.push(parse(row?)?);
            }
            let exists: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM feedback_events WHERE member_id=?1 AND root_id=?2)",
                params![member, root],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(ApiError::new("not_found", "反馈历史不存在"));
            }
            let more = items.len() > limit as usize;
            items.truncate(limit as usize);
            let next = if more {
                items.last().unwrap()["revision"].clone()
            } else {
                Value::Null
            };
            return Ok(
                json!({"schema_version":1,"member_id":member,"items":items,"next_cursor":next}),
            );
        }
        if ["feedback.correct", "feedback.revoke"].contains(&op) {
            return self.feedback_revision(op, input);
        }
        if op == "feedback.get" {
            let s: Option<String> = self
                .connection
                .query_row(
                    "SELECT document FROM feedback_events WHERE member_id=?1 AND id=?2",
                    params![member, string(&input, "id")?],
                    |r| r.get(0),
                )
                .optional()?;
            return parse(s.ok_or_else(|| ApiError::new("not_found", "反馈不存在"))?);
        }
        if op == "feedback.list" {
            let limit = input
                .get("limit")
                .map(|v| {
                    v.as_u64()
                        .filter(|n| (1..=100).contains(n))
                        .ok_or_else(|| invalid("分页数量须为1–100"))
                })
                .transpose()?
                .unwrap_or(50);
            let cursor = input
                .get("cursor")
                .map(|v| v.as_str().ok_or_else(|| invalid("游标须为字符串")))
                .transpose()?
                .unwrap_or("");
            let revoked = input
                .get("include_revoked")
                .map(|v| {
                    v.as_bool()
                        .ok_or_else(|| invalid("include_revoked须为布尔值"))
                })
                .transpose()?
                .unwrap_or(false);
            let mut st=self.connection.prepare("SELECT e.document FROM feedback_events e WHERE member_id=?1 AND (?2='' OR id<?2) AND revision=(SELECT MAX(revision) FROM feedback_events WHERE root_id=e.root_id) AND (?3 OR json_extract(document,'$.revoked')=0) ORDER BY id DESC LIMIT ?4")?;
            let mut items = vec![];
            for row in st.query_map(params![member, cursor, revoked, limit + 1], |r| {
                r.get::<_, String>(0)
            })? {
                items.push(parse(row?)?);
            }
            let more = items.len() > limit as usize;
            items.truncate(limit as usize);
            let next = if more {
                items.last().map(|v| v["id"].clone()).unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            return Ok(
                json!({"schema_version":1,"member_id":member,"items":items,"next_cursor":next}),
            );
        }
        if op != "feedback.record" {
            return Err(ApiError::new("unsupported_operation", "反馈操作不可用"));
        }
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        let reaction = reaction(&input["reaction"])?;
        let scores = scores(input.get("scores").unwrap_or(&json!({})))?;
        let request = input.to_string();
        let tx = self.begin_write()?;
        if let Some((old,result))=tx.query_row("SELECT request,result FROM feedback_receipts WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?{if old!=request{return Err(ApiError::new("idempotency_conflict","同操作标识不能改变反馈"));}return parse(result);}
        let intent = self.recommendation_execute(
            "recommendation.get",
            json!({"member_id":member,"id":string(&input,"intent_id")?}),
        )?;
        let index = input["proposal_index"]
            .as_u64()
            .ok_or_else(|| invalid("须指定方案序号"))? as usize;
        let proposal = intent["proposals"]
            .as_array()
            .unwrap()
            .get(index)
            .ok_or_else(|| invalid("方案不存在"))?;
        let mut day_scope = Value::Null;
        let mut clothing_ids = vec![];
        if reaction == "day_exclude" {
            let zone: chrono_tz::Tz = string(&input, "timezone")?
                .parse()
                .map_err(|_| invalid("须使用有效IANA时区"))?;
            let date = Utc::now().with_timezone(&zone).date_naive();
            let end = zone
                .from_local_datetime(&date.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap())
                .earliest()
                .ok_or_else(|| invalid("次日零点不存在，请选择明确时区"))?;
            clothing_ids = selected(proposal, &input["clothing_ids"])?;
            day_scope = json!({"local_date":date.to_string(),"timezone":zone.name(),"expires_at":end.with_timezone(&Utc).to_rfc3339()});
        } else if input.get("timezone").is_some() || input.get("clothing_ids").is_some() {
            return Err(invalid("仅当天排除接受日期范围与单品选择"));
        }
        let mut wear_event_id = Value::Null;
        if ["actual_wore", "changed"].contains(&reaction) {
            let wear = input
                .get("wear")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    ApiError::new("confirmation_required", "须明确确认实际穿着日期与衣物")
                })?;
            if wear.get("confirmed") != Some(&json!(true)) {
                return Err(ApiError::new(
                    "confirmation_required",
                    "反馈不会隐式记录穿着，请明确确认",
                ));
            }
            if wear.keys().any(|k| {
                ![
                    "confirmed",
                    "local_date",
                    "timezone",
                    "local_time",
                    "clothing_ids",
                    "notes",
                ]
                .contains(&k.as_str())
            }) {
                return Err(invalid("穿着确认包含不支持字段"));
            }
            let proposal_ids: Vec<Value> = proposal["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["clothing_id"].clone())
                .collect();
            let actual = if reaction == "actual_wore" {
                if wear.get("clothing_ids").is_some() {
                    return Err(invalid("实际穿了使用所选方案，改穿请选择实际衣物"));
                }
                json!(proposal_ids)
            } else {
                wear.get("clothing_ids")
                    .cloned()
                    .ok_or_else(|| invalid("改穿须明确实际衣物"))?
            };
            if reaction == "changed" {
                let mut original: Vec<String> = proposal_ids
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect();
                original.sort();
                let mut selected: Vec<String> = actual
                    .as_array()
                    .ok_or_else(|| invalid("改穿衣物须为数组"))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| invalid("衣物ID须为文字"))
                    })
                    .collect::<Result<_, _>>()?;
                selected.sort();
                if original == selected {
                    return Err(invalid("实际组合没有改变，请选择实际穿了"));
                }
            }
            let mut wear_input = json!({"member_id":member,"operation_key":format!("feedback:{}",Sha256::digest(key.as_bytes()).iter().map(|b|format!("{b:02x}")).collect::<String>()),"clothing_ids":actual});
            for f in ["local_date", "timezone", "local_time", "notes"] {
                if let Some(v) = wear.get(f) {
                    wear_input[f] = v.clone();
                }
            }
            let event = self.wear_write_in_tx(&tx, "wear.record", wear_input)?;
            wear_event_id = event["id"].clone();
        } else if input.get("wear").is_some() {
            return Err(invalid("普通反馈不接受穿着确认"));
        }
        let mut inventory_snapshots = vec![];
        for item in proposal["items"].as_array().unwrap() {
            inventory_snapshots
                .push(self.clothing_get(json!({"member_id":member,"id":item["clothing_id"]}))?);
        }
        let id = uuid::Uuid::now_v7().to_string();
        let doc = json!({"schema_version":1,"member_id":member,"id":id,"root_id":id,"revision":1,"revoked":false,"intent_id":intent["id"],"proposal_index":index,"proposal":proposal,"inventory_snapshots":inventory_snapshots,"inventory_observed_at":Utc::now().to_rfc3339(),"controls":intent["controls"],"reaction":reaction,"wear_event_id":wear_event_id,"day_scope":day_scope,"clothing_ids":clothing_ids,"scores":scores,"created_at":chrono::Utc::now().to_rfc3339()});
        tx.execute("INSERT INTO feedback_events(id,member_id,root_id,revision,document) VALUES(?1,?2,?1,1,?3)",params![id,member,doc.to_string()])?;
        for snapshot in &inventory_snapshots {
            if let Some(asset) = snapshot["display_asset_id"].as_str() {
                tx.execute(
                    "INSERT OR IGNORE INTO feedback_asset_refs VALUES(?1,?2,?3)",
                    params![id, asset, member],
                )?;
            }
        }
        tx.execute("INSERT INTO feedback_receipts(member_id,operation_key,request,result) VALUES(?1,?2,?3,?4)",params![member,key,request,doc.to_string()])?;
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
    ["record","get","list","correct","revoke","history","try_another"].into_iter().map(|op|{
 let mut props=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];
 match op {"try_another"=>{required.extend(["operation_key","intent_id","proposal_index"]);props["operation_key"]=json!({"type":"string"});props["intent_id"]=json!({"type":"string"});props["proposal_index"]=json!({"type":"integer","minimum":0});},"record"=>{for f in ["operation_key","intent_id"]{props[f]=json!({"type":"string"});required.push(f);}required.extend(["proposal_index","reaction"]);props["proposal_index"]=json!({"type":"integer","minimum":0});props["reaction"]=json!({"enum":["like","dislike","skip","day_exclude","actual_wore","changed"]});props["wear"]=json!({"type":"object","additionalProperties":false,"required":["confirmed","local_date","timezone"],"properties":{"confirmed":{"const":true},"local_date":{"type":"string"},"timezone":{"type":"string"},"local_time":{"type":"string"},"notes":{"type":"string"},"clothing_ids":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":100,"uniqueItems":true}}});props["timezone"]=json!({"type":"string"});props["clothing_ids"]=json!({"type":"array","minItems":1,"maxItems":100,"uniqueItems":true,"items":{"type":"string"}});props["scores"]=json!({"type":"object","additionalProperties":false,"properties":{"overall":{"type":["integer","null"],"minimum":1,"maximum":5},"comfort":{"type":["integer","null"],"minimum":1,"maximum":5},"style":{"type":["integer","null"],"minimum":1,"maximum":5}}});},"correct"|"revoke"=>{required.extend(["root_id","operation_key","expected_revision"]);props["root_id"]=json!({"type":"string"});props["operation_key"]=json!({"type":"string"});props["expected_revision"]=json!({"type":"integer","minimum":1});if op=="revoke"{required.push("reason");props["reason"]=json!({"type":"string"});}else{props["reaction"]=json!({"enum":["like","dislike","skip"]});props["scores"]=json!({"type":"object","additionalProperties":false,"properties":{"overall":{"type":["integer","null"],"minimum":1,"maximum":5},"comfort":{"type":["integer","null"],"minimum":1,"maximum":5},"style":{"type":["integer","null"],"minimum":1,"maximum":5}}});}},
 "history"=>{required.push("root_id");props["root_id"]=json!({"type":"string"});props["limit"]=json!({"type":"integer","minimum":1,"maximum":100});props["cursor"]=json!({"type":"integer","minimum":0});},
 "get"=>{required.push("id");props["id"]=json!({"type":"string"});},_=>{props["limit"]=json!({"type":"integer","minimum":1,"maximum":100});props["cursor"]=json!({"type":"string"});props["include_revoked"]=json!({"type":"boolean"});}}
 json!({"name":format!("feedback.{op}"),"description":"显式推荐反馈，不隐式记录穿着","inputSchema":{"type":"object","properties":props,"required":required,"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id"]}})
}).collect()
}
