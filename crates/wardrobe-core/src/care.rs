use crate::{string, ApiError, Application};
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::BTreeSet;
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS care_config(member_id TEXT NOT NULL REFERENCES members(id),clothing_id TEXT NOT NULL,document TEXT NOT NULL,PRIMARY KEY(member_id,clothing_id));CREATE TABLE IF NOT EXISTS care_events(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),root_event_id TEXT NOT NULL,revision INTEGER NOT NULL,document TEXT NOT NULL);CREATE INDEX IF NOT EXISTS care_member_root ON care_events(member_id,root_event_id,revision);CREATE TABLE IF NOT EXISTS care_receipts(member_id TEXT NOT NULL,operation_key TEXT NOT NULL,request TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(member_id,operation_key));CREATE TABLE IF NOT EXISTS care_asset_refs(event_id TEXT NOT NULL REFERENCES care_events(id),asset_id TEXT NOT NULL,member_id TEXT NOT NULL,PRIMARY KEY(event_id,asset_id),FOREIGN KEY(asset_id,member_id) REFERENCES assets(id,member_id));")?;
    Ok(())
}
fn invalid(s: &str) -> ApiError {
    ApiError::new("invalid_input", s)
}
fn parse(s: String) -> Result<Value, ApiError> {
    serde_json::from_str(&s).map_err(|_| ApiError::new("storage_error", "洗护记录损坏"))
}
impl Application {
    pub(super) fn care_execute(&self, op: &str, mut input: Value) -> Result<Value, ApiError> {
        // A deferred read transaction keeps all projection queries on one WAL snapshot.
        // Recursive reads reuse an existing snapshot or a caller's IMMEDIATE write transaction.
        if [
            "care.status",
            "care.dirty_list",
            "care.reminders",
            "care.availability",
            "care.get",
            "care.list",
            "care.history",
        ]
        .contains(&op)
            && self.connection.is_autocommit()
        {
            let tx = self.connection.unchecked_transaction()?;
            let result = self.care_execute(op, input)?;
            tx.commit()?;
            return Ok(result);
        }
        let allowed = match op {
            "care.status" => vec!["member_id", "clothing_id"],
            "care.dirty_list" | "care.reminders" | "care.availability" => vec!["member_id"],
            "care.get" => vec!["member_id", "id"],
            "care.list" => vec!["member_id", "clothing_id", "include_revoked"],
            "care.history" => vec!["member_id", "root_event_id"],
            "care.configure" => vec![
                "member_id",
                "clothing_id",
                "wash_after_wears",
                "operation_key",
                "expected_revision",
            ],
            "care.record" | "care.mark_dirty" => vec![
                "member_id",
                "clothing_id",
                "action",
                "dirty",
                "local_date",
                "local_time",
                "timezone",
                "notes",
                "operation_key",
            ],
            "care.correct" => vec![
                "member_id",
                "root_event_id",
                "expected_revision",
                "action",
                "dirty",
                "local_date",
                "local_time",
                "timezone",
                "notes",
                "operation_key",
            ],
            "care.revoke" => vec![
                "member_id",
                "root_event_id",
                "expected_revision",
                "reason",
                "operation_key",
            ],
            _ => return Err(ApiError::new("unsupported_operation", "洗护操作不可用")),
        };
        if input
            .as_object()
            .unwrap()
            .keys()
            .any(|k| !allowed.contains(&k.as_str()))
        {
            return Err(invalid("洗护操作包含不支持的字段"));
        }
        if (op == "care.record" && input.get("dirty").is_some())
            || (op == "care.mark_dirty" && input.get("action").is_some())
            || (op == "care.correct"
                && input.get("action").is_some()
                && input.get("dirty").is_some())
        {
            return Err(invalid("手动待洗与洗护类型须分别明确"));
        }
        if op == "care.configure"
            && input
                .get("expected_revision")
                .and_then(Value::as_u64)
                .is_none()
        {
            return Err(invalid("须提供当前配置revision（首次为0）"));
        }
        let member = self.member(&input)?;
        input["member_id"] = json!(member);
        if op == "care.status" {
            return self.care_status(&member, string(&input, "clothing_id")?);
        }
        if ["care.dirty_list", "care.reminders", "care.availability"].contains(&op) {
            let items: Vec<Value> = self
                .care_batch(&member)?
                .into_iter()
                .filter(|s| op == "care.availability" || s["dirty"] == true)
                .collect();
            return Ok(
                json!({"schema_version":1,"member_id":member,"items":items,"notification_scheduled":false}),
            );
        }
        if op == "care.get" {
            let d: Option<String> = self
                .connection
                .query_row(
                    "SELECT document FROM care_events WHERE member_id=?1 AND id=?2",
                    params![member, string(&input, "id")?],
                    |r| r.get(0),
                )
                .optional()?;
            return parse(d.ok_or_else(|| ApiError::new("not_found", "洗护事件不存在"))?);
        }
        if op == "care.list" || op == "care.history" {
            if op == "care.list" {
                crate::sync::require_resolved_events(&self.connection, "care_events", &member)?;
            }
            let mut st=self.connection.prepare(if op=="care.history"{"SELECT document FROM care_events WHERE member_id=?1 ORDER BY root_event_id,revision"}else{"SELECT document FROM care_events e WHERE member_id=?1 AND id=(SELECT id FROM care_events WHERE root_event_id=e.root_event_id ORDER BY revision DESC,id DESC LIMIT 1) ORDER BY json_extract(document,'$.occurred_at') DESC,root_event_id DESC"})?;
            let revoked = input
                .get("include_revoked")
                .map(|v| v.as_bool().ok_or_else(|| invalid("撤销筛选须为布尔值")))
                .transpose()?
                .unwrap_or(false);
            let filter = input
                .get("clothing_id")
                .map(|v| v.as_str().ok_or_else(|| invalid("单品ID须为文字")))
                .transpose()?;
            let root = if op == "care.history" {
                Some(string(&input, "root_event_id")?)
            } else {
                None
            };
            let mut items = vec![];
            for r in st.query_map([&member], |r| r.get::<_, String>(0))? {
                let e = parse(r?)?;
                if root.is_none_or(|r| e["root_event_id"] == r)
                    && filter.is_none_or(|c| e["clothing_id"] == c)
                    && (op == "care.history" || revoked || e["kind"] != "revocation")
                {
                    items.push(e);
                }
            }
            if root.is_some() && items.is_empty() {
                return Err(ApiError::new("not_found", "洗护记录不存在"));
            }
            return Ok(json!({"schema_version":1,"member_id":member,"items":items}));
        }
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        let request = json!({"operation":op,"input":input}).to_string();
        let tx = self.begin_write()?;
        if let Some((old, result)) = tx
            .query_row(
                "SELECT request,result FROM care_receipts WHERE member_id=?1 AND operation_key=?2",
                params![member, key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if old != request {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "操作标识已用于不同洗护意图",
                ));
            }
            return parse(result);
        }
        let mut result;
        if op == "care.configure" {
            let id = string(&input, "clothing_id")?;
            self.clothing_get(json!({"member_id":member,"id":id}))?;
            let threshold = input
                .get("wash_after_wears")
                .ok_or_else(|| invalid("须提供正整数或null阈值"))?;
            if !threshold.is_null()
                && threshold
                    .as_u64()
                    .filter(|v| *v > 0 && *v <= i64::MAX as u64)
                    .is_none()
            {
                return Err(invalid("洗护阈值须为正整数或null"));
            }
            let old: Option<String> = tx
                .query_row(
                    "SELECT document FROM care_config WHERE member_id=?1 AND clothing_id=?2",
                    params![member, id],
                    |r| r.get(0),
                )
                .optional()?;
            let old = old.map(parse).transpose()?;
            let rev = old
                .as_ref()
                .and_then(|v| v["revision"].as_u64())
                .unwrap_or(0);
            if let Some(expected) = input.get("expected_revision") {
                if expected.as_u64() != Some(rev) {
                    return Err(ApiError::new("version_conflict", "阈值已改变，请重新读取"));
                }
            }
            result = json!({"schema_version":1,"member_id":member,"clothing_id":id,"wash_after_wears":threshold,"revision":rev+1,"created_at":old.as_ref().map(|v|v["created_at"].clone()).unwrap_or(json!(Utc::now().to_rfc3339())),"updated_at":Utc::now().to_rfc3339(),"id":old.as_ref().map(|v|v["id"].clone()).unwrap_or(json!(uuid::Uuid::now_v7().to_string()))});
            tx.execute("INSERT INTO care_config VALUES(?1,?2,?3) ON CONFLICT(member_id,clothing_id) DO UPDATE SET document=excluded.document",params![member,id,result.to_string()])?;
        } else {
            let previous = if ["care.correct", "care.revoke"].contains(&op) {
                let d:Option<String>=tx.query_row("SELECT document FROM care_events WHERE member_id=?1 AND root_event_id=?2 ORDER BY revision DESC,id DESC LIMIT 1",params![member,string(&input,"root_event_id")?],|r|r.get(0)).optional()?;
                let p = parse(d.ok_or_else(|| ApiError::new("not_found", "洗护记录不存在"))?)?;
                if input["expected_revision"].as_u64() != p["revision"].as_u64() {
                    return Err(ApiError::new("version_conflict", "洗护记录已改变"));
                }
                if p["kind"] == "revocation" {
                    return Err(ApiError::new("invalid_state", "已撤销记录不可修改"));
                }
                Some(p)
            } else {
                None
            };
            let id = uuid::Uuid::now_v7().to_string();
            if op == "care.revoke" {
                let reason = string(&input, "reason")?;
                if reason.trim().is_empty() || reason.chars().count() > 2000 {
                    return Err(invalid("撤销原因须为1–2000字符"));
                }
                result = previous.clone().unwrap();
                result["kind"] = json!("revocation");
                result["reason"] = json!(reason);
            } else {
                let timing = crate::wears::timing(&input)?;
                let action = if op == "care.mark_dirty"
                    || previous.as_ref().is_some_and(|p| {
                        ["mark_dirty", "clear_dirty"].contains(&p["action"].as_str().unwrap_or(""))
                    }) {
                    match input["dirty"].as_bool() {
                        Some(true) => "mark_dirty",
                        Some(false) => "clear_dirty",
                        None => return Err(invalid("手动待洗须为布尔值")),
                    }
                } else {
                    string(&input, "action")?
                };
                if ![
                    "wash",
                    "dry_clean",
                    "maintenance",
                    "mark_dirty",
                    "clear_dirty",
                ]
                .contains(&action)
                    || (op == "care.record" && ["mark_dirty", "clear_dirty"].contains(&action))
                {
                    return Err(invalid("洗护类型须为wash/dry_clean/maintenance"));
                }
                let snapshot = if let Some(p) = &previous {
                    p["snapshot"].clone()
                } else {
                    let c = self.clothing_get(
                        json!({"member_id":member,"id":string(&input,"clothing_id")?}),
                    )?;
                    if c["status"] == "deleted" {
                        return Err(ApiError::new("invalid_state", "回收站衣物不能新增洗护"));
                    }
                    c
                };
                let notes = input
                    .get("notes")
                    .map(|v| {
                        v.as_str()
                            .filter(|s| s.chars().count() <= 2000)
                            .ok_or_else(|| invalid("备注最多2000字符"))
                    })
                    .transpose()?
                    .unwrap_or_else(|| {
                        previous
                            .as_ref()
                            .and_then(|p| p["notes"].as_str())
                            .unwrap_or("")
                    });
                result = json!({"schema_version":1,"member_id":member,"clothing_id":snapshot["id"],"snapshot":snapshot,"action":action,"notes":notes,"kind":if previous.is_some(){"correction"}else{"record"}});
                for (k, v) in timing.as_object().unwrap() {
                    result[k] = v.clone();
                }
            }
            result["id"] = json!(id);
            result["root_event_id"] = previous
                .as_ref()
                .map(|p| p["root_event_id"].clone())
                .unwrap_or(json!(id));
            result["revision"] = json!(
                previous
                    .as_ref()
                    .and_then(|p| p["revision"].as_u64())
                    .unwrap_or(0)
                    + 1
            );
            result["supersedes_event_id"] = previous
                .as_ref()
                .map(|p| p["id"].clone())
                .unwrap_or(Value::Null);
            result["created_at"] = json!(Utc::now().to_rfc3339());
            result["updated_at"] = result["created_at"].clone();
            tx.execute(
                "INSERT INTO care_events VALUES(?1,?2,?3,?4,?5)",
                params![
                    id,
                    member,
                    result["root_event_id"].as_str(),
                    result["revision"].as_u64(),
                    result.to_string()
                ],
            )?;
            let mut assets = BTreeSet::new();
            if let Some(id) = result["snapshot"]["display_asset_id"].as_str() {
                assets.insert(id);
            }
            for id in result["snapshot"]["asset_ids"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(id) = id.as_str() {
                    assets.insert(id);
                }
            }
            for asset in assets {
                tx.execute(
                    "INSERT INTO care_asset_refs VALUES(?1,?2,?3)",
                    params![id, asset, member],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO care_receipts VALUES(?1,?2,?3,?4)",
            params![member, key, request, result.to_string()],
        )?;
        let entity = result["id"]
            .as_str()
            .or(result["clothing_id"].as_str())
            .unwrap();
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,?2,?3)",
            params![entity, op, result.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![entity, result.to_string()],
        )?;
        tx.commit()?;
        Ok(result)
    }
    fn care_status(&self, member: &str, id: &str) -> Result<Value, ApiError> {
        let clothing = self.clothing_get(json!({"member_id":member,"id":id}))?;
        let config: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM care_config WHERE member_id=?1 AND clothing_id=?2",
                params![member, id],
                |r| r.get(0),
            )
            .optional()?;
        let config = config
            .map(parse)
            .transpose()?
            .unwrap_or(json!({"wash_after_wears":null,"revision":0}));
        let wears = self.effective_wears(member)?;
        let events =
            self.care_execute("care.list", json!({"member_id":member,"clothing_id":id}))?;
        let mut timeline = vec![];
        for w in wears {
            if w["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["clothing_id"] == id)
            {
                timeline.push(event_order(w, 0));
            }
        }
        for e in events["items"].as_array().unwrap() {
            timeline.push(event_order(e.clone(), 1));
        }
        Ok(project_status(member, &clothing, &config, timeline))
    }
    // One member-wide read per source, then partition by physical clothing ID.
    // Caller owns the existing DEFERRED or IMMEDIATE snapshot.
    pub(super) fn care_batch(&self, member: &str) -> Result<Vec<Value>, ApiError> {
        let wears = self.effective_wears(member)?;
        self.care_batch_from_wears(member, &wears)
    }
    pub(super) fn care_batch_from_wears(
        &self,
        member: &str,
        wears: &[Value],
    ) -> Result<Vec<Value>, ApiError> {
        let mut configs = std::collections::HashMap::new();
        let mut st = self
            .connection
            .prepare("SELECT clothing_id,document FROM care_config WHERE member_id=?1")?;
        for row in st.query_map([member], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (id, doc) = row?;
            configs.insert(id, parse(doc)?);
        }
        let mut timelines: std::collections::HashMap<String, Vec<(String, u8, String, Value)>> =
            std::collections::HashMap::new();
        for wear in wears {
            let event = (
                wear["occurred_at"].as_str().unwrap().to_owned(),
                0,
                wear["root_event_id"].as_str().unwrap().to_owned(),
                json!({"time_precision":wear["time_precision"]}),
            );
            for item in wear["items"].as_array().unwrap() {
                timelines
                    .entry(item["clothing_id"].as_str().unwrap().to_owned())
                    .or_default()
                    .push(event.clone());
            }
        }
        let events = self.care_execute("care.list", json!({"member_id":member}))?;
        for event in events["items"].as_array().unwrap() {
            timelines
                .entry(event["clothing_id"].as_str().unwrap().to_owned())
                .or_default()
                .push(event_order(event.clone(), 1));
        }
        let mut st = self
            .connection
            .prepare("SELECT document FROM clothing WHERE member_id=?1 ORDER BY id")?;
        let mut result = vec![];
        for row in st.query_map([member], |r| r.get::<_, String>(0))? {
            let clothing = parse(row?)?;
            let id = clothing["id"].as_str().unwrap();
            let config = configs
                .remove(id)
                .unwrap_or(json!({"wash_after_wears":null,"revision":0}));
            result.push(project_status(
                member,
                &clothing,
                &config,
                timelines.remove(id).unwrap_or_default(),
            ));
        }
        Ok(result)
    }
}
fn event_order(event: Value, kind: u8) -> (String, u8, String, Value) {
    (
        event["occurred_at"].as_str().unwrap().to_owned(),
        kind,
        event["root_event_id"].as_str().unwrap().to_owned(),
        event,
    )
}
fn project_status(
    member: &str,
    clothing: &Value,
    config: &Value,
    mut timeline: Vec<(String, u8, String, Value)>,
) -> Value {
    let id = clothing["id"].as_str().unwrap();
    timeline.sort_by(|a, b| (&a.0, a.1, &a.2).cmp(&(&b.0, b.1, &b.2)));
    let mut count = 0;
    let mut dirty = false;
    let mut last = Value::Null;
    let mut uncertain = false;
    for (_, kind, _, e) in timeline {
        uncertain |= e["time_precision"] == "date";
        if kind == 0 {
            count += 1;
        } else {
            match e["action"].as_str().unwrap() {
                "wash" | "dry_clean" => {
                    count = 0;
                    dirty = false;
                    last = e;
                }
                "mark_dirty" => dirty = true,
                "clear_dirty" => dirty = false,
                _ => {}
            }
        }
    }
    let threshold = config["wash_after_wears"].as_u64();
    let due = threshold.is_some_and(|n| count >= n);
    let mut reasons = vec![];
    if clothing["status"] != "active" {
        reasons.push(clothing["status"].as_str().unwrap());
    }
    if dirty {
        reasons.push("manual_dirty");
    }
    if due {
        reasons.push("threshold_reached");
    }
    json!({"schema_version":1,"member_id":member,"clothing_id":id,"name":clothing["name"],"lifecycle_status":clothing["status"],"wash_after_wears":config["wash_after_wears"],"configuration_revision":config["revision"],"wears_since_clean":count,"manual_dirty":dirty,"dirty":dirty||due,"available":reasons.is_empty(),"exclusion_reasons":reasons,"last_clean":last,"chronology_uncertain":uncertain,"ordering":"occurred_at; wear_before_care_at_same_instant; root_event_id","notification_scheduled":false})
}
pub(super) fn descriptors() -> Vec<Value> {
    ["configure","mark_dirty","record","correct","revoke","status","get","list","history","dirty_list","reminders","availability"].into_iter().map(|op|{let mut props=json!({"member_id":{"type":"string"}});
let mut req=vec!["member_id"];
if ["configure","mark_dirty","record","status","list"].contains(&op){props["clothing_id"]=json!({"type":"string"});
if op!="list"{req.push("clothing_id");
}}
if ["configure","mark_dirty","record","correct","revoke"].contains(&op){props["operation_key"]=json!({"type":"string"});
req.push("operation_key");
}
if op=="configure"{props["wash_after_wears"]=json!({"type":["integer","null"],"minimum":1});
props["expected_revision"]=json!({"type":"integer","minimum":0});
req.push("wash_after_wears");
req.push("expected_revision");
}
if ["mark_dirty","record","correct"].contains(&op){for k in ["local_date","timezone"]{props[k]=json!({"type":"string"});
req.push(k);
}for k in ["local_time","notes"]{props[k]=json!({"type":"string"});
}
if op!="mark_dirty" {props["action"]=json!({"enum":["wash","dry_clean","maintenance"]});
}
if op!="record" {props["dirty"]=json!({"type":"boolean"});
}
if op=="record"{req.push("action");
}
if op=="mark_dirty"{req.push("dirty");
}}
if ["correct","revoke","history"].contains(&op){props["root_event_id"]=json!({"type":"string"});
req.push("root_event_id");
}
if ["correct","revoke"].contains(&op){props["expected_revision"]=json!({"type":"integer","minimum":1});
req.push("expected_revision");
}
if op=="revoke"{props["reason"]=json!({"type":"string"});
req.push("reason");
}
if op=="get"{props["id"]=json!({"type":"string"});
req.push("id");
}
if op=="list"{props["include_revoked"]=json!({"type":"boolean"});
}json!({"name":format!("care.{op}"),"description":format!("洗护 {op}"),"inputSchema":{"type":"object","additionalProperties":false,"required":req,"properties":props},"outputSchema":{"type":"object","required":["schema_version","member_id"],"properties":{"schema_version":{"const":1}}}})}).collect()
}
