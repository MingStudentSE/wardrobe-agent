use crate::{string, ApiError, Application};
use chrono::{NaiveDate, Utc};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS insights_preferences(member_id TEXT PRIMARY KEY REFERENCES members(id), document TEXT NOT NULL);")?;
    Ok(())
}
#[derive(Default)]
struct Frequency {
    snapshot: Value,
    count: u64,
    days: BTreeSet<String>,
    last: Option<NaiveDate>,
}
fn invalid() -> ApiError {
    ApiError::new(
        "invalid_input",
        "请提供有效统计日期、IANA时区和1–36500天阈值",
    )
}
fn fields(v: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if !v
        .as_object()
        .is_some_and(|o| o.keys().all(|k| allowed.contains(&k.as_str())))
    {
        return Err(invalid());
    }
    Ok(())
}
fn distribution(counts: BTreeMap<String, u64>, unknown: u64, denominator: usize) -> Vec<Value> {
    let bucket = |value: Value, count: u64| json!({"value":value,"count":count,"percent":if denominator==0{0.0}else{count as f64*100.0/denominator as f64}});
    let mut result: Vec<_> = counts
        .into_iter()
        .map(|(value, n)| bucket(json!(value), n))
        .collect();
    if unknown > 0 {
        result.push(bucket(Value::Null, unknown));
    }
    result
}
impl Application {
    fn insights_preferences(&self, member: &str) -> Result<Value, ApiError> {
        let stored = self
            .connection
            .query_row(
                "SELECT document FROM insights_preferences WHERE member_id=?1",
                [member],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        stored.map(|s| serde_json::from_str(&s).map_err(|_| ApiError::new("storage_error", "洞察偏好损坏"))).unwrap_or_else(|| Ok(json!({"schema_version":1,"id":null,"member_id":member,"stale_days":90,"revision":0,"created_at":null,"updated_at":null})))
    }
    pub(super) fn insights_configure(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "stale_days", "expected_revision"])?;
        let member = self.member(&input)?;
        let value = input.get("stale_days").ok_or_else(invalid)?;
        let days = if value.is_null() {
            90
        } else {
            value
                .as_u64()
                .filter(|n| *n >= 1 && *n <= 36500)
                .ok_or_else(invalid)?
        };
        let expected = input["expected_revision"]
            .as_u64()
            .filter(|n| *n < u64::MAX)
            .ok_or_else(invalid)?;
        let tx = self.begin_write()?;
        let previous = self.insights_preferences(&member)?;
        if previous["revision"] != expected {
            return Err(ApiError::new(
                "version_conflict",
                "洞察偏好已修改，请重新读取后保存",
            ));
        }
        let now = Utc::now().to_rfc3339();
        let id = previous["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let doc = json!({"schema_version":1,"id":id,"member_id":member,"stale_days":days,"revision":expected+1,"created_at":previous["created_at"].as_str().unwrap_or(&now),"updated_at":now});
        tx.execute("INSERT INTO insights_preferences VALUES(?1,?2) ON CONFLICT(member_id) DO UPDATE SET document=excluded.document",params![member,doc.to_string()])?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'insights.configure',?2)",
            params![id, doc.to_string()],
        )?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn insights_summary(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "as_of_date", "timezone"])?;
        let _snapshot = self.connection.unchecked_transaction()?;
        let member = self.member(&input)?;
        let preferences = self.insights_preferences(&member)?;
        let stale_days = preferences["stale_days"].as_u64().ok_or_else(invalid)?;
        let zone: chrono_tz::Tz = string(&input, "timezone")?.parse().map_err(|_| invalid())?;
        let date = string(&input, "as_of_date")?;
        let as_of = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| invalid())?;
        if as_of.to_string() != date || as_of > Utc::now().with_timezone(&zone).date_naive() {
            return Err(invalid());
        }
        let mut statement = self
            .connection
            .prepare("SELECT document FROM clothing WHERE member_id=?1 ORDER BY id")?;
        let clothes: Vec<Value> = statement
            .query_map([&member], |r| r.get::<_, String>(0))?
            .map(|r| {
                serde_json::from_str(&r?)
                    .map_err(|_| ApiError::new("storage_error", "衣物数据损坏"))
            })
            .collect::<Result<_, _>>()?;
        let mut statuses: BTreeMap<String, u64> = BTreeMap::from([
            ("active".into(), 0),
            ("archived".into(), 0),
            ("deleted".into(), 0),
        ]);
        let mut categories = BTreeMap::new();
        let mut colors = BTreeMap::new();
        let mut unknown = 0;
        let inventory: Vec<_> = clothes
            .iter()
            .filter(|c| c["status"] != "deleted")
            .collect();
        for c in &clothes {
            *statuses
                .entry(c["status"].as_str().unwrap_or("unknown").into())
                .or_default() += 1;
        }
        for c in &inventory {
            *categories
                .entry(c["category"].as_str().unwrap_or("other").into())
                .or_default() += 1;
            let values: BTreeSet<_> = c["color"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if values.is_empty() {
                unknown += 1;
            }
            for color in values {
                *colors.entry(color.into()).or_default() += 1;
            }
        }
        let inventory_ids: BTreeSet<_> =
            inventory.iter().filter_map(|c| c["id"].as_str()).collect();
        let mut counts: BTreeMap<String, Frequency> = BTreeMap::new();
        let mut combinations: BTreeMap<Vec<String>, (u64, BTreeSet<String>)> = BTreeMap::new();
        let mut event_count = 0;
        for event in self.effective_wears(&member)? {
            let instant = chrono::DateTime::parse_from_rfc3339(string(&event, "occurred_at")?)
                .map_err(|_| ApiError::new("storage_error", "穿着时间损坏"))?;
            if instant.with_timezone(&zone).date_naive() > as_of {
                continue;
            }
            event_count += 1;
            let mut combination = vec![];
            for item in event["items"].as_array().ok_or_else(invalid)? {
                let id = string(item, "clothing_id")?;
                combination.push(id.to_string());
                let entry = counts.entry(id.into()).or_default();
                entry.snapshot = item["snapshot"].clone();
                entry.count += 1;
                entry.days.insert(string(&event, "local_date")?.into());
                entry.last = Some(instant.with_timezone(&zone).date_naive());
            }
            combination.sort();
            let entry = combinations.entry(combination).or_default();
            entry.0 += 1;
            entry.1.insert(string(&event, "local_date")?.into());
        }
        for c in &inventory {
            let entry = counts.entry(string(c, "id")?.into()).or_default();
            if entry.count == 0 {
                entry.snapshot = (*c).clone();
            }
        }
        let frequency:Vec<Value>=counts.iter().map(|(id,f)|json!({"clothing_id":id,"name":f.snapshot["name"],"wearCount":f.count,"wearDays":f.days.len(),"in_inventory":inventory_ids.contains(id.as_str()),"last_worn_date":f.last.map(|d|d.to_string()),"days_since_worn":f.last.map(|d|(as_of-d).num_days())})).collect();
        let stale: Vec<_> = frequency
            .iter()
            .filter(|f| {
                f["in_inventory"] == true
                    && f["days_since_worn"]
                        .as_u64()
                        .is_some_and(|d| d >= stale_days)
            })
            .cloned()
            .collect();
        let never: Vec<Value> = inventory
            .iter()
            .filter(|c| {
                counts
                    .get(c["id"].as_str().unwrap_or(""))
                    .is_none_or(|f| f.count == 0)
            })
            .map(|c| json!({"clothing_id":c["id"],"name":c["name"]}))
            .collect();
        Ok(
            json!({"schema_version":1,"member_id":member,"as_of_date":date,"timezone":zone.to_string(),"preferences":preferences,"stale_days":stale_days,"inventory":{"scope":"active_and_archived","total":inventory.len(),"by_status":statuses,"category_denominator":inventory.len(),"color_denominator":inventory.len(),"categories":distribution(categories,0,inventory.len()),"colors":distribution(colors,unknown,inventory.len())},"effective_event_count":event_count,"frequency":frequency,"stale":stale,"never_worn":never,"combinations":combinations.into_iter().map(|(ids,(count,days))|json!({"names":ids.iter().map(|id|counts[id].snapshot["name"].clone()).collect::<Vec<_>>(),"clothing_ids":ids,"wearCount":count,"wearDays":days.len()})).collect::<Vec<_>>(),"rules":{"frequency_scope":"effective_wear_history_including_deleted","wear_days":"distinct_saved_local_date","combination":"unordered_exact_clothing_id_set","last_worn":"occurred_at_converted_to_query_timezone","stale_boundary":"days_since_worn_greater_than_or_equal_threshold","inventory":"current_active_and_archived_even_for_past_as_of"}}),
        )
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    let examples = json!({
        "insights.summary": [{"request":{"member_id":"<已授权成员ID>","as_of_date":"2020-04-01","timezone":"Asia/Shanghai"},"prerequisites":["按有效实际穿着重算统计；读取持久化配置与穿着记录"]}],
        "insights.configure": [{"request":{"member_id":"<已授权成员ID>","stale_days":91,"expected_revision":0},"prerequisites":["expected_revision 乐观锁；null 重置默认90天，不保存派生统计"]}],
    });
    let mut descriptors = vec![
        json!({"name":"insights.summary","description":"按有效实际穿着重算衣橱数量、分布、次数与当地天数；不从搭配推断穿过","inputSchema":{"type":"object","required":["member_id","as_of_date","timezone"],"properties":{"member_id":{"type":"string"},"as_of_date":{"type":"string"},"timezone":{"type":"string"}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id","inventory","frequency","stale","stale_days","never_worn","combinations","rules","preferences"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"inventory":{"type":"object"},"frequency":{"type":"array"},"stale":{"type":"array"},"stale_days":{"type":"integer"},"never_worn":{"type":"array"},"combinations":{"type":"array"},"rules":{"type":"object"},"preferences":{"type":"object"}}}}),
        json!({"name":"insights.configure","description":"按成员保存久未穿阈值；null重置90天，不保存派生统计","inputSchema":{"type":"object","required":["member_id","stale_days","expected_revision"],"properties":{"member_id":{"type":"string"},"expected_revision":{"type":"integer","minimum":0},"stale_days":{"type":["integer","null"],"minimum":1,"maximum":36500}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","id","member_id","stale_days","revision","created_at","updated_at"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"revision":{"type":"integer","minimum":1},"created_at":{"type":"string"},"updated_at":{"type":"string"},"member_id":{"type":"string"},"stale_days":{"type":"integer","minimum":1,"maximum":36500}}}}),
    ];
    for descriptor in descriptors.iter_mut() {
        let name = descriptor["name"].as_str().unwrap().to_string();
        descriptor["examples"] = examples[&name].clone();
    }
    descriptors
}
