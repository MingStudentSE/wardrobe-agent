use crate::{string, ApiError, Application};
use chrono::{NaiveDate, Utc};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS insights_preferences(member_id TEXT PRIMARY KEY REFERENCES members(id), stale_days INTEGER NOT NULL CHECK(stale_days BETWEEN 1 AND 36500));")?;
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
    fn insights_days(&self, member: &str) -> Result<u64, ApiError> {
        Ok(self
            .connection
            .query_row(
                "SELECT stale_days FROM insights_preferences WHERE member_id=?1",
                [member],
                |r| r.get::<_, u64>(0),
            )
            .optional()?
            .unwrap_or(90))
    }
    pub(super) fn insights_configure(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "stale_days"])?;
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
        let tx = self.begin_write()?;
        tx.execute("INSERT INTO insights_preferences VALUES(?1,?2) ON CONFLICT(member_id) DO UPDATE SET stale_days=excluded.stale_days",params![member,days])?;
        tx.commit()?;
        Ok(json!({"schema_version":1,"member_id":member,"stale_days":days}))
    }
    pub(super) fn insights_summary(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "as_of_date", "timezone"])?;
        let _snapshot = self.connection.unchecked_transaction()?;
        let member = self.member(&input)?;
        let stale_days = self.insights_days(&member)?;
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
            json!({"schema_version":1,"member_id":member,"as_of_date":date,"timezone":zone.to_string(),"stale_days":stale_days,"inventory":{"scope":"active_and_archived","total":inventory.len(),"by_status":statuses,"category_denominator":inventory.len(),"color_denominator":inventory.len(),"categories":distribution(categories,0,inventory.len()),"colors":distribution(colors,unknown,inventory.len())},"effective_event_count":event_count,"frequency":frequency,"stale":stale,"never_worn":never,"combinations":combinations.into_iter().map(|(ids,(count,days))|json!({"names":ids.iter().map(|id|counts[id].snapshot["name"].clone()).collect::<Vec<_>>(),"clothing_ids":ids,"wearCount":count,"wearDays":days.len()})).collect::<Vec<_>>(),"rules":{"frequency_scope":"effective_wear_history_including_deleted","wear_days":"distinct_saved_local_date","combination":"unordered_exact_clothing_id_set","last_worn":"occurred_at_converted_to_query_timezone","stale_boundary":"days_since_worn_greater_than_or_equal_threshold","inventory":"current_active_and_archived_even_for_past_as_of"}}),
        )
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    vec![
        json!({"name":"insights.summary","description":"按有效实际穿着重算衣橱数量、分布、次数与当地天数；不从搭配推断穿过","inputSchema":{"type":"object","required":["member_id","as_of_date","timezone"],"properties":{"member_id":{"type":"string"},"as_of_date":{"type":"string"},"timezone":{"type":"string"}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id","inventory","frequency","stale","stale_days","never_worn","combinations","rules"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"inventory":{"type":"object"},"frequency":{"type":"array"},"stale":{"type":"array"},"stale_days":{"type":"integer"},"never_worn":{"type":"array"},"combinations":{"type":"array"},"rules":{"type":"object"}}}}),
        json!({"name":"insights.configure","description":"按成员保存久未穿阈值；null重置90天，不保存派生统计","inputSchema":{"type":"object","required":["member_id","stale_days"],"properties":{"member_id":{"type":"string"},"stale_days":{"type":["integer","null"],"minimum":1,"maximum":36500}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id","stale_days"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"stale_days":{"type":"integer","minimum":1,"maximum":36500}}}}),
    ]
}
