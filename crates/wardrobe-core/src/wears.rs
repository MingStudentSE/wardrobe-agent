use crate::{string, ApiError, Application};
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS wear_events(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),root_event_id TEXT NOT NULL,revision INTEGER NOT NULL,document TEXT NOT NULL); CREATE INDEX IF NOT EXISTS wear_member_root ON wear_events(member_id,root_event_id,revision); CREATE TABLE IF NOT EXISTS wear_receipts(member_id TEXT NOT NULL REFERENCES members(id),operation_key TEXT NOT NULL,request TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(member_id,operation_key)); CREATE TABLE IF NOT EXISTS wear_asset_refs(event_id TEXT NOT NULL REFERENCES wear_events(id),asset_id TEXT NOT NULL,member_id TEXT NOT NULL,PRIMARY KEY(event_id,asset_id),FOREIGN KEY(asset_id,member_id) REFERENCES assets(id,member_id));")?;
    Ok(())
}
fn invalid(message: &str) -> ApiError {
    ApiError::new("invalid_input", message)
}
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .unwrap()
        .keys()
        .any(|k| !allowed.contains(&k.as_str()))
    {
        return Err(invalid("穿着操作包含不支持的字段"));
    }
    Ok(())
}
pub(super) fn timing(input: &Value) -> Result<Value, ApiError> {
    let date = string(input, "local_date")?;
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| invalid("日期须为有效YYYY-MM-DD"))?;
    if parsed.to_string() != date {
        return Err(invalid("日期须为YYYY-MM-DD"));
    }
    let zone: chrono_tz::Tz = string(input, "timezone")?
        .parse()
        .map_err(|_| invalid("时区须为有效IANA标识"))?;
    if parsed > Utc::now().with_timezone(&zone).date_naive() {
        return Err(ApiError::new("future_date", "未来日期不能记录实际穿着"));
    }
    let local_time = input
        .get("local_time")
        .map(|v| v.as_str().ok_or_else(|| invalid("时间须为HH:MM")))
        .transpose()?;
    let time = match local_time {
        Some(s) => {
            let t = NaiveTime::parse_from_str(s, "%H:%M").map_err(|_| invalid("时间须为HH:MM"))?;
            if t.format("%H:%M").to_string() != s {
                return Err(invalid("时间须为HH:MM"));
            }
            t
        }
        None => NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
    };
    let instant = zone
        .from_local_datetime(&parsed.and_time(time))
        .single()
        .ok_or_else(|| invalid("当地时间不存在或有夏令时歧义，请选明确时刻"))?;
    Ok(
        json!({"local_date":date,"timezone":zone.to_string(),"local_time":local_time,"time_precision":if local_time.is_some(){"minute"}else{"date"},"occurred_at":instant.with_timezone(&Utc).to_rfc3339()}),
    )
}
impl Application {
    pub(super) fn wear_execute(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        match operation {
            "wear.get" => fields(&input, &["member_id", "id"])?,
            "wear.list" => fields(
                &input,
                &[
                    "member_id",
                    "clothing_id",
                    "include_revoked",
                    "limit",
                    "cursor",
                ],
            )?,
            "wear.summary" => fields(&input, &["member_id"])?,
            "wear.correct" => fields(
                &input,
                &[
                    "member_id",
                    "root_event_id",
                    "expected_revision",
                    "operation_key",
                    "local_date",
                    "timezone",
                    "local_time",
                    "clothing_ids",
                    "source_outfit_id",
                    "source_version",
                    "notes",
                ],
            )?,
            "wear.revoke" => fields(
                &input,
                &[
                    "member_id",
                    "root_event_id",
                    "expected_revision",
                    "operation_key",
                    "reason",
                ],
            )?,
            "wear.history" => fields(&input, &["member_id", "root_event_id"])?,
            "wear.record" => fields(
                &input,
                &[
                    "member_id",
                    "operation_key",
                    "local_date",
                    "timezone",
                    "local_time",
                    "clothing_ids",
                    "source_outfit_id",
                    "source_version",
                    "notes",
                ],
            )?,
            _ => return Err(ApiError::new("unsupported_operation", "穿着操作不可用")),
        }
        let member = self.member(&input)?;
        if operation == "wear.summary" {
            return self.wear_summary(&member);
        }
        if operation == "wear.get" {
            let row: Option<String> = self
                .connection
                .query_row(
                    "SELECT document FROM wear_events WHERE member_id=?1 AND id=?2",
                    params![member, string(&input, "id")?],
                    |r| r.get(0),
                )
                .optional()?;
            return serde_json::from_str(
                &row.ok_or_else(|| ApiError::new("not_found", "穿着事件不存在"))?,
            )
            .map_err(|_| ApiError::new("storage_error", "穿着事件损坏"));
        }
        if operation == "wear.list" {
            return self.wear_list(&member, &input);
        }
        if operation == "wear.history" {
            return self.wear_history(&member, string(&input, "root_event_id")?);
        }
        let tx = self.begin_write()?;
        let result = self.wear_write_in_tx(&tx, operation, input)?;
        tx.commit()?;
        Ok(result)
    }
    pub(super) fn wear_write_in_tx(
        &self,
        tx: &Transaction<'_>,
        operation: &str,
        mut input: Value,
    ) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        input["member_id"] = json!(member);
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        let request = json!({"operation":operation,"input":input}).to_string();
        if let Some((old, result)) = tx
            .query_row(
                "SELECT request,result FROM wear_receipts WHERE member_id=?1 AND operation_key=?2",
                params![member, key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if old != request {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "操作标识已用于不同穿着意图",
                ));
            }
            return serde_json::from_str(&result)
                .map_err(|_| ApiError::new("storage_error", "穿着收据损坏"));
        }
        let previous = if operation != "wear.record" {
            let root = string(&input, "root_event_id")?;
            let serialized:Option<String>=tx.query_row("SELECT document FROM wear_events WHERE member_id=?1 AND root_event_id=?2 ORDER BY revision DESC LIMIT 1",params![member,root],|r|r.get(0)).optional()?;
            let previous: Value = serde_json::from_str(
                &serialized.ok_or_else(|| ApiError::new("not_found", "穿着记录不存在"))?,
            )
            .map_err(|_| ApiError::new("storage_error", "穿着记录损坏"))?;
            let expected = input["expected_revision"]
                .as_u64()
                .filter(|v| *v > 0)
                .ok_or_else(|| invalid("须提供expected_revision"))?;
            if previous["revision"] != expected {
                return Err(ApiError::new(
                    "version_conflict",
                    "穿着已被更正，请重新读取",
                ));
            }
            if previous["kind"] == "revocation" {
                return Err(ApiError::new("invalid_state", "已撤销的穿着不能再次修改"));
            }
            Some(previous)
        } else {
            None
        };
        if operation == "wear.revoke" {
            let previous = previous.as_ref().unwrap();
            let reason = string(&input, "reason")?;
            if reason.trim().is_empty() || reason.chars().count() > 2000 {
                return Err(invalid("撤销原因须为1–2000字符"));
            }
            let mut event = previous.clone();
            event["id"] = json!(uuid::Uuid::now_v7().to_string());
            event["revision"] = json!(previous["revision"].as_u64().unwrap() + 1);
            event["kind"] = json!("revocation");
            event["supersedes_event_id"] = previous["id"].clone();
            event["reason"] = json!(reason);
            event["created_at"] = json!(Utc::now().to_rfc3339());
            self.persist_wear(tx, &member, key, &request, operation, &event)?;
            return Ok(event);
        }
        let time = timing(&input)?;
        let notes = input
            .get("notes")
            .map(|v| {
                v.as_str()
                    .filter(|s| s.chars().count() <= 2000)
                    .ok_or_else(|| invalid("备注须为最多2000字符的文字"))
            })
            .transpose()?
            .unwrap_or("");
        let source = match input.get("source_outfit_id") {
            Some(id) => {
                let id = id.as_str().ok_or_else(|| invalid("来源搭配须为字符串"))?;
                let version = input["source_version"]
                    .as_u64()
                    .filter(|v| *v > 0)
                    .ok_or_else(|| invalid("来源搭配需要明确版本"))?;
                Some(self.outfit_execute(
                    "outfit.version",
                    json!({"member_id":member,"id":id,"version":version}),
                )?)
            }
            None => {
                if input.get("source_version").is_some() {
                    return Err(invalid("版本需要来源搭配"));
                }
                previous
                    .as_ref()
                    .and_then(|p| p.get("source_outfit").filter(|s| !s.is_null()).cloned())
            }
        };
        let keep_items = operation == "wear.correct"
            && !input.as_object().unwrap().contains_key("clothing_ids")
            && !input.as_object().unwrap().contains_key("source_outfit_id");
        let ids = match input.get("clothing_ids") {
            Some(ids) => ids
                .as_array()
                .filter(|v| !v.is_empty() && v.len() <= 100)
                .ok_or_else(|| invalid("须选择1–100件实际衣物"))?
                .clone(),
            None => {
                if keep_items {
                    vec![]
                } else {
                    source
                        .as_ref()
                        .ok_or_else(|| invalid("须选择衣物或来源搭配"))?["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|i| i["clothing_id"].clone())
                        .collect()
                }
            }
        };
        let mut seen = HashSet::new();
        let mut items = vec![];
        for id in ids {
            let id = id.as_str().ok_or_else(|| invalid("单品ID须为字符串"))?;
            if !seen.insert(id.to_string()) {
                return Err(invalid("实际穿着不能重复同一单品"));
            }
            let snapshot = self.clothing_get(json!({"member_id":member,"id":id}))?;
            if snapshot["status"] == "deleted" {
                return Err(ApiError::new(
                    "invalid_state",
                    "回收站单品不能记录新的实际穿着",
                ));
            }
            items.push(json!({"clothing_id":id,"snapshot":snapshot}));
        }
        if keep_items {
            items = previous.as_ref().unwrap()["items"]
                .as_array()
                .unwrap()
                .clone();
        }
        let id = uuid::Uuid::now_v7().to_string();
        let mut event = json!({"schema_version":1,"id":id,"member_id":member,"root_event_id":id,"revision":1,"kind":"record","supersedes_event_id":null,"items":items,"source_outfit":source,"notes":notes,"created_at":Utc::now().to_rfc3339()});
        if let Some(previous) = &previous {
            event["root_event_id"] = previous["root_event_id"].clone();
            event["revision"] = json!(previous["revision"].as_u64().unwrap() + 1);
            event["kind"] = json!("correction");
            event["supersedes_event_id"] = previous["id"].clone();
            if input.get("notes").is_none() {
                event["notes"] = previous["notes"].clone();
            }
        }
        for (k, v) in time.as_object().unwrap() {
            event[k] = v.clone();
        }
        self.persist_wear(tx, &member, key, &request, operation, &event)?;
        Ok(event)
    }
    fn persist_wear(
        &self,
        tx: &Transaction<'_>,
        member: &str,
        key: &str,
        request: &str,
        operation: &str,
        event: &Value,
    ) -> Result<(), ApiError> {
        let document = event.to_string();
        let id = event["id"].as_str().unwrap();
        tx.execute(
            "INSERT INTO wear_events VALUES(?1,?2,?3,?4,?5)",
            params![
                id,
                member,
                event["root_event_id"].as_str(),
                event["revision"].as_u64(),
                document
            ],
        )?;
        for item in event["items"].as_array().unwrap().iter().chain(
            event["source_outfit"]["items"]
                .as_array()
                .into_iter()
                .flatten(),
        ) {
            let snapshot = &item["snapshot"];
            let mut assets = BTreeSet::new();
            if let Some(asset) = snapshot["display_asset_id"].as_str() {
                assets.insert(asset);
            }
            for asset in snapshot["asset_ids"].as_array().into_iter().flatten() {
                if let Some(asset) = asset.as_str() {
                    assets.insert(asset);
                }
            }
            for asset in assets {
                tx.execute(
                    "INSERT OR IGNORE INTO wear_asset_refs VALUES(?1,?2,?3)",
                    params![id, asset, member],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO wear_receipts VALUES(?1,?2,?3,?4)",
            params![member, key, request, document],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,?2,?3)",
            params![id, operation, document],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![id, document],
        )?;
        Ok(())
    }
    fn wear_history(&self, member: &str, root: &str) -> Result<Value, ApiError> {
        let mut statement=self.connection.prepare("SELECT document FROM wear_events WHERE member_id=?1 AND root_event_id=?2 ORDER BY revision")?;
        let mut items = vec![];
        for row in statement.query_map(params![member, root], |r| r.get::<_, String>(0))? {
            items.push(
                serde_json::from_str::<Value>(&row?)
                    .map_err(|_| ApiError::new("storage_error", "穿着历史损坏"))?,
            );
        }
        if items.is_empty() {
            return Err(ApiError::new("not_found", "穿着记录不存在"));
        }
        Ok(json!({"schema_version":1,"member_id":member,"items":items}))
    }
    fn wear_list(&self, member: &str, input: &Value) -> Result<Value, ApiError> {
        let include_revoked = input
            .get("include_revoked")
            .map(|v| v.as_bool().ok_or_else(|| invalid("撤销筛选须为布尔值")))
            .transpose()?
            .unwrap_or(false);
        let clothing = input
            .get("clothing_id")
            .map(|v| v.as_str().ok_or_else(|| invalid("单品筛选须为字符串")))
            .transpose()?;
        let limit = input
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .filter(|n| (1..=100).contains(n))
                    .ok_or_else(|| invalid("每页数量须为1–100"))
            })
            .transpose()?
            .unwrap_or(24) as usize;
        let cursor = input
            .get("cursor")
            .map(|v| v.as_str().ok_or_else(|| invalid("游标须为字符串")))
            .transpose()?;
        let mut statement=self.connection.prepare("SELECT document FROM wear_events e WHERE member_id=?1 AND revision=(SELECT MAX(revision) FROM wear_events WHERE root_event_id=e.root_event_id) ORDER BY json_extract(document,'$.occurred_at') DESC,root_event_id DESC")?;
        let mut events = vec![];
        for row in statement.query_map([member], |r| r.get::<_, String>(0))? {
            let event: Value = serde_json::from_str(&row?)
                .map_err(|_| ApiError::new("storage_error", "穿着历史损坏"))?;
            if (include_revoked || event["kind"] != "revocation")
                && clothing.is_none_or(|id| {
                    event["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|i| i["clothing_id"] == id)
                })
            {
                events.push(event);
            }
        }
        let start = match cursor {
            Some(cursor) => events
                .iter()
                .position(|e| e["root_event_id"] == cursor)
                .map(|p| p + 1)
                .ok_or_else(|| invalid("游标不属于当前穿着筛选"))?,
            None => 0,
        };
        let more = events.len() > start + limit;
        let items: Vec<_> = events.into_iter().skip(start).take(limit).collect();
        let next = if more {
            items.last().map(|e| e["root_event_id"].clone())
        } else {
            None
        };
        Ok(json!({"schema_version":1,"member_id":member,"items":items,"next_cursor":next}))
    }
    // One stable projection shared by downstream care and insights; never materialized counters.
    pub(super) fn effective_wears(&self, member: &str) -> Result<Vec<Value>, ApiError> {
        let mut statement=self.connection.prepare("SELECT document FROM wear_events e WHERE member_id=?1 AND revision=(SELECT MAX(revision) FROM wear_events WHERE root_event_id=e.root_event_id) ORDER BY json_extract(document,'$.occurred_at'),root_event_id")?;
        let mut result = vec![];
        for row in statement.query_map([member], |r| r.get::<_, String>(0))? {
            let event: Value = serde_json::from_str(&row?)
                .map_err(|_| ApiError::new("storage_error", "穿着历史损坏"))?;
            if event["kind"] != "revocation" {
                result.push(event);
            }
        }
        Ok(result)
    }
    fn wear_summary(&self, member: &str) -> Result<Value, ApiError> {
        let mut counts: BTreeMap<String, (u64, BTreeSet<String>)> = BTreeMap::new();
        for event in self.effective_wears(member)? {
            for item in event["items"].as_array().unwrap() {
                let entry = counts
                    .entry(item["clothing_id"].as_str().unwrap().into())
                    .or_default();
                entry.0 += 1;
                entry.1.insert(event["local_date"].as_str().unwrap().into());
            }
        }
        let items:Vec<Value>=counts.into_iter().map(|(id,(count,days))|json!({"clothing_id":id,"wearCount":count,"wearDays":days.len()})).collect();
        Ok(json!({"schema_version":1,"member_id":member,"items":items}))
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    ["record","correct","revoke","get","list","history","summary"].into_iter().map(|op|{
        let mut properties=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];
        if ["record","correct"].contains(&op){for key in ["operation_key","local_date","timezone"]{properties[key]=json!({"type":"string"});required.push(key);}for key in ["local_time","source_outfit_id","notes"]{properties[key]=json!({"type":"string"});}properties["clothing_ids"]=json!({"type":"array","minItems":1,"maxItems":100,"uniqueItems":true,"items":{"type":"string"}});properties["source_version"]=json!({"type":"integer","minimum":1});}
        if ["correct","revoke","history"].contains(&op){properties["root_event_id"]=json!({"type":"string"});required.push("root_event_id");}
        if ["correct","revoke"].contains(&op){properties["expected_revision"]=json!({"type":"integer","minimum":1});required.push("expected_revision");}
        if op=="revoke"{for key in ["operation_key","reason"]{properties[key]=json!({"type":"string"});required.push(key);}}
        if op=="get"{properties["id"]=json!({"type":"string"});required.push("id");}
        if op=="list"{properties["clothing_id"]=json!({"type":"string"});properties["include_revoked"]=json!({"type":"boolean"});properties["limit"]=json!({"type":"integer","minimum":1,"maximum":100});properties["cursor"]=json!({"type":"string"});}
        json!({"name":format!("wear.{op}"),"description":format!("真实穿着 {op}"),"inputSchema":{"type":"object","additionalProperties":false,"required":required,"properties":properties},"outputSchema":if ["record","correct","revoke","get"].contains(&op){json!({"type":"object","required":["schema_version","id","member_id","root_event_id","revision","kind","items","source_outfit","local_date","timezone","local_time","time_precision","occurred_at","created_at","notes","supersedes_event_id"],"properties":{"schema_version":{"const":1},"revision":{"type":"integer","minimum":1},"kind":{"enum":["record","correction","revocation"]},"items":{"type":"array","minItems":1},"source_outfit":{"type":["object","null"]}}})}else{json!({"type":"object","required":["schema_version","member_id","items"],"properties":{"schema_version":{"const":1},"items":{"type":"array"}}})}})
    }).map(|mut descriptor|{
        let op=descriptor["name"].as_str().unwrap().strip_prefix("wear.").unwrap().to_string();
        descriptor["effect"]=match op.as_str() {
            "record"|"correct"|"revoke"=>json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
            _=>json!({"kind":"query","network":"none","local_files":false,"async":null}),
        };
        let mid=json!("<已授权成员ID>");
        descriptor["examples"]=match op.as_str() {
            "record"=>json!([{"request":{"member_id":mid,"operation_key":"<幂等操作键>","local_date":"2020-01-01","timezone":"UTC","clothing_ids":["<单品ID>"],"source_outfit_id":"<可选搭配ID>"},"prerequisites":["同 operation_key 幂等；可选关联搭配版本"]}]),
            "correct"=>json!([{"request":{"member_id":mid,"operation_key":"<新幂等键>","root_event_id":"<wear.record 返回的根事件ID>","expected_revision":1,"local_date":"2020-01-02","timezone":"UTC"},"prerequisites":["按 expected_revision 乐观更正，修订号+1"]}]),
            "revoke"=>json!([{"request":{"member_id":mid,"operation_key":"<新幂等键>","root_event_id":"<根事件ID>","expected_revision":2,"reason":"<作废原因>"},"prerequisites":["作废记录并保留历史"]}]),
            "get"=>json!([{"request":{"member_id":mid,"id":"<事件ID>"},"prerequisites":["读取单条穿着事件"]}]),
            "list"=>json!([{"request":{"member_id":mid,"limit":20},"prerequisites":["include_revoked 默认不含作废记录"]}]),
            "history"=>json!([{"request":{"member_id":mid,"root_event_id":"<根事件ID>"},"prerequisites":["返回同一根事件的完整修订链"]}]),
            _=>json!([{"request":{"member_id":mid},"prerequisites":["读取持久化穿着统计"]}]),
        };
        descriptor
    }).collect()
}
