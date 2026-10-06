use crate::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS recommendation_intents(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),operation_key TEXT NOT NULL,request TEXT NOT NULL,document TEXT NOT NULL,UNIQUE(member_id,operation_key)); CREATE TABLE IF NOT EXISTS recommendation_saves(member_id TEXT NOT NULL,operation_key TEXT NOT NULL,request TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(member_id,operation_key));")?;
    Ok(())
}
fn invalid(s: &str) -> ApiError {
    ApiError::new("invalid_input", s)
}
fn parse(s: String) -> Result<Value, ApiError> {
    serde_json::from_str(&s).map_err(|_| ApiError::new("storage_error", "推荐意图损坏"))
}
impl Application {
    pub(super) fn recommendation_execute(
        &self,
        op: &str,
        mut input: Value,
    ) -> Result<Value, ApiError> {
        let allowed = match op {
            "recommendation.generate" => vec![
                "member_id",
                "operation_key",
                "timezone",
                "limit",
                "fixed_ids",
                "excluded_ids",
                "inventory_priority_ids",
                "avoid_recent",
                "target",
                "occasion",
                "weather",
                "excluded_sets",
                "source_intent",
            ],
            "recommendation.save" => {
                vec!["member_id", "id", "proposal_index", "name", "operation_key"]
            }
            "recommendation.replace" => vec![
                "member_id",
                "id",
                "proposal_index",
                "clothing_id",
                "operation_key",
            ],
            "recommendation.change" => vec!["member_id", "id", "proposal_index", "operation_key"],
            "recommendation.list" => vec!["member_id"],
            _ => vec!["member_id", "id"],
        };
        fields(&input, &allowed)?;
        let member = self.member(&input)?;
        input["member_id"] = json!(member);
        if op == "recommendation.get" {
            let d: Option<String> = self
                .connection
                .query_row(
                    "SELECT document FROM recommendation_intents WHERE member_id=?1 AND id=?2",
                    params![member, string(&input, "id")?],
                    |r| r.get(0),
                )
                .optional()?;
            return parse(d.ok_or_else(|| ApiError::new("not_found", "推荐不存在"))?);
        }
        if ["recommendation.replace", "recommendation.change"].contains(&op) {
            let intent = self.recommendation_execute(
                "recommendation.get",
                json!({"member_id":member,"id":string(&input,"id")?}),
            )?;
            let index = input["proposal_index"]
                .as_u64()
                .ok_or_else(|| invalid("须指定推荐方案序号"))? as usize;
            let proposal = intent["proposals"]
                .as_array()
                .unwrap()
                .get(index)
                .ok_or_else(|| invalid("推荐方案不存在"))?;
            let mut controls = intent["controls"].clone();
            controls["operation_key"] = json!(string(&input, "operation_key")?);
            controls["limit"] = json!(1);
            controls["source_intent"] = json!(intent["id"]);
            let proposal_ids: Vec<String> = proposal["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["clothing_id"].as_str().unwrap().to_owned())
                .collect();
            if op == "recommendation.replace" {
                let old = string(&input, "clothing_id")?;
                if !proposal_ids.iter().any(|id| id == old) {
                    return Err(invalid("单品不属于所选方案"));
                }
                let fixed = ids(&controls, "fixed_ids")?;
                if fixed.iter().any(|id| id == old) {
                    return Err(ApiError::new(
                        "constraint_conflict",
                        "固定单品不能替换；请先修改固定条件",
                    ));
                }
                controls["fixed_ids"] = json!(proposal_ids
                    .iter()
                    .filter(|id| *id != old)
                    .collect::<Vec<_>>());
                let mut excluded = ids(&controls, "excluded_ids")?;
                excluded.push(old.to_owned());
                controls["excluded_ids"] = json!(excluded);
            } else {
                let mut sets = controls
                    .get("excluded_sets")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                sets.push(json!(proposal_ids));
                controls["excluded_sets"] = json!(sets);
            }
            return self.recommendation_execute("recommendation.generate", controls);
        }
        if op == "recommendation.list" {
            let mut st=self.connection.prepare("SELECT document FROM recommendation_intents WHERE member_id=?1 ORDER BY id DESC LIMIT 100")?;
            let mut items = vec![];
            for row in st.query_map([&member], |r| r.get::<_, String>(0))? {
                items.push(parse(row?)?);
            }
            return Ok(json!({"schema_version":1,"member_id":member,"items":items}));
        }
        if op == "recommendation.save" {
            let key = string(&input, "operation_key")?;
            if key.is_empty() || key.len() > 200 {
                return Err(invalid("操作标识须为1–200字节"));
            }
            let request = input.to_string();
            let tx = self.begin_write()?;
            if let Some((old,result))=tx.query_row("SELECT request,result FROM recommendation_saves WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?{if old!=request{return Err(ApiError::new("idempotency_conflict","同一保存标识不能改变请求"));}return parse(result);}
            let intent = self.recommendation_execute(
                "recommendation.get",
                json!({"member_id":member,"id":string(&input,"id")?}),
            )?;
            let index = input["proposal_index"]
                .as_u64()
                .ok_or_else(|| invalid("须指定推荐方案序号"))? as usize;
            let proposal = intent["proposals"]
                .as_array()
                .unwrap()
                .get(index)
                .ok_or_else(|| invalid("推荐方案不存在"))?;
            let statuses = self.care_batch(&member)?;
            for item in proposal["items"].as_array().unwrap() {
                if !statuses
                    .iter()
                    .any(|s| s["clothing_id"] == item["clothing_id"] && s["available"] == true)
                {
                    return Err(ApiError::new(
                        "constraint_conflict",
                        "推荐中的单品现已不可用，请重新推荐",
                    ));
                }
            }
            let items:Vec<Value>=proposal["items"].as_array().unwrap().iter().map(|item|json!({"clothing_id":item["clothing_id"],"role":item["role"],"layer":item["layer"]})).collect();
            let result = self.outfit_execute(
                "outfit.create",
                json!({"member_id":member,"name":string(&input,"name")?,"items":items}),
            )?;
            tx.execute(
                "INSERT INTO recommendation_saves VALUES(?1,?2,?3,?4)",
                params![member, key, request, result.to_string()],
            )?;
            tx.commit()?;
            return Ok(result);
        }
        if op != "recommendation.generate" {
            return Err(ApiError::new("unsupported_operation", "推荐操作不可用"));
        }
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        let limit = input
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .filter(|v| (1..=5).contains(v))
                    .ok_or_else(|| invalid("推荐数量须为1–5"))
            })
            .transpose()?
            .unwrap_or(3);
        let fixed = ids(&input, "fixed_ids")?;
        let excluded = ids(&input, "excluded_ids")?;
        if fixed.iter().any(|id| excluded.contains(id)) {
            return Err(ApiError::new("constraint_conflict", "固定衣物不能同时排除"));
        }
        let zone: chrono_tz::Tz = string(&input, "timezone")?
            .parse()
            .map_err(|_| invalid("须使用有效IANA时区"))?;
        let (cold, weather_context) = weather(input.get("weather"))?;
        let occasion = input
            .get("occasion")
            .map(|v| v.as_str().ok_or_else(|| invalid("场合须为文字")))
            .transpose()?
            .unwrap_or("daily");
        if !["daily", "work", "formal", "sport"].contains(&occasion) {
            return Err(invalid("请选择有效场合"));
        }
        let request = input.to_string();
        let tx = self.begin_write()?;
        if let Some((old,doc))=tx.query_row("SELECT request,document FROM recommendation_intents WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?{if old!=request{return Err(ApiError::new("idempotency_conflict","同一标识不能替换推荐约束"));}return parse(doc);}
        for id in fixed.iter().chain(excluded.iter()) {
            self.clothing_get(json!({"member_id":member,"id":id}))?;
        }
        let statuses = self.care_execute("care.availability", json!({"member_id":member}))?;
        for id in &fixed {
            let status = statuses["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["clothing_id"] == *id)
                .unwrap();
            if status["available"] != true {
                return Err(ApiError::new(
                    "constraint_conflict",
                    &format!(
                        "固定衣物不可用：{}（{}）",
                        status["name"], status["exclusion_reasons"]
                    ),
                ));
            }
        }
        let available: std::collections::HashSet<&str> = statuses["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["available"] == true)
            .map(|s| s["clothing_id"].as_str().unwrap())
            .collect();
        let excluded: std::collections::HashSet<&str> =
            excluded.iter().map(String::as_str).collect();
        let mut st = self
            .connection
            .prepare("SELECT document FROM clothing WHERE member_id=?1 ORDER BY id")?;
        let mut clothes = vec![];
        for row in st.query_map([&member], |r| r.get::<_, String>(0))? {
            let garment = parse(row?)?;
            let id = garment["id"].as_str().unwrap();
            if available.contains(id) && !excluded.contains(id) {
                clothes.push(garment);
            }
        }
        let priority = ids(&input, "inventory_priority_ids")?;
        for id in &priority {
            self.clothing_get(json!({"member_id":member,"id":id}))?;
        }
        let avoid = input
            .get("avoid_recent")
            .map(|v| v.as_bool().ok_or_else(|| invalid("近期避重复须为布尔值")))
            .transpose()?
            .unwrap_or(true);
        let target = input
            .get("target")
            .map(|v| v.as_str().ok_or_else(|| invalid("目标须为文字")))
            .transpose()?
            .unwrap_or("balanced");
        if !["balanced", "least_worn", "favorites"].contains(&target) {
            return Err(invalid("请选择平衡、少穿优先或收藏优先目标"));
        }
        let today = chrono::Utc::now().with_timezone(&zone).date_naive();
        let from = today - chrono::Duration::days(6);
        let mut recent = std::collections::BTreeSet::new();
        let mut counts = std::collections::HashMap::<String, usize>::new();
        for wear in self.effective_wears(&member)? {
            let mut ids = vec![];
            for item in wear["items"].as_array().unwrap() {
                let id = item["clothing_id"].as_str().unwrap().to_owned();
                *counts.entry(id.clone()).or_default() += 1;
                ids.push(id);
            }
            ids.sort();
            ids.dedup();
            let day = chrono::DateTime::parse_from_rfc3339(wear["occurred_at"].as_str().unwrap())
                .map_err(|_| ApiError::new("storage_error", "穿着日期损坏"))?
                .with_timezone(&zone)
                .date_naive();
            if avoid && day >= from && day <= today {
                recent.insert(ids);
            }
        }
        clothes.sort_by_key(|c| {
            let id = c["id"].as_str().unwrap();
            (
                !priority.iter().any(|p| p == id),
                if target == "least_worn" {
                    counts.get(id).copied().unwrap_or(0)
                } else {
                    0
                },
                if target == "favorites" {
                    c["favorite"] != true
                } else {
                    false
                },
                !c["occasion"]
                    .as_array()
                    .is_some_and(|values| values.iter().any(|v| v == occasion)),
                id.to_owned(),
            )
        });
        let mut forbidden = std::collections::BTreeSet::new();
        if let Some(sets) = input.get("excluded_sets") {
            for set in sets.as_array().ok_or_else(|| invalid("排除组合须为数组"))? {
                let mut ids = ids(&json!({"ids":set}), "ids")?;
                for id in &ids {
                    self.clothing_get(json!({"member_id":member,"id":id}))?;
                }
                ids.sort();
                forbidden.insert(ids);
            }
        }
        let proposals = plan(&clothes, &fixed, limit as usize, &recent, &forbidden, cold);

        let shortage_reasons: Vec<String> = if proposals.len() < limit as usize {
            vec![format!(
                "请求{}套，当前硬约束下仅有{}套不同实物组合；不会重复或虚构库存",
                limit,
                proposals.len()
            )]
        } else {
            vec![]
        };
        let now = chrono::Utc::now().to_rfc3339();
        let doc = json!({"schema_version":1,"id":uuid::Uuid::now_v7().to_string(),"member_id":member,"created_at":now,"updated_at":now,"controls":input,"proposals":proposals,"requested_count":limit,"actual_count":proposals.len(),"offline":true,"creates_wear":false,"shortage_reasons":shortage_reasons,"weather_context":weather_context,"recent_window":{"from":from.to_string(),"through":today.to_string(),"timezone":zone.name(),"source":"actual_effective_wear"}});
        tx.execute(
            "INSERT INTO recommendation_intents VALUES(?1,?2,?3,?4,?5)",
            params![doc["id"].as_str(), member, key, request, doc.to_string()],
        )?;
        tx.commit()?;
        Ok(doc)
    }
}
// Each frontier state advances one sorted role slot. We only materialize emitted
// combinations and their immediate neighbors, never the full Cartesian product.
fn plan(
    clothes: &[Value],
    fixed: &[String],
    limit: usize,
    recent: &std::collections::BTreeSet<Vec<String>>,
    forbidden: &std::collections::BTreeSet<Vec<String>>,
    cold: bool,
) -> Vec<Value> {
    use std::collections::{BTreeSet, BinaryHeap};
    let mandatory: Vec<Value> = clothes
        .iter()
        .filter(|c| fixed.contains(&c["id"].as_str().unwrap().to_owned()))
        .cloned()
        .collect();
    let has = |categories: &[&str]| {
        mandatory
            .iter()
            .any(|c| categories.contains(&c["category"].as_str().unwrap()))
    };
    let mut patterns = vec![];
    if !has(&["dress", "one_piece"]) {
        patterns.push(vec![
            ("top", vec!["top"]),
            ("bottom", vec!["bottom"]),
            ("shoes", vec!["shoes"]),
        ]);
    }
    if !has(&["top", "bottom"]) {
        for category in ["dress", "one_piece"] {
            if has(&["dress", "one_piece"]) && !has(&[category]) {
                continue;
            }
            if has(&[category]) || clothes.iter().any(|c| c["category"] == category) {
                patterns.push(vec![("body", vec![category]), ("shoes", vec!["shoes"])]);
            }
        }
    }
    if cold {
        for pattern in &mut patterns {
            pattern.push(("outerwear", vec!["outerwear"]));
        }
    }
    let missing_count = |pattern: &Vec<(&str, Vec<&str>)>| {
        pattern
            .iter()
            .filter(|(_, categories)| {
                !has(categories)
                    && !clothes
                        .iter()
                        .any(|c| categories.contains(&c["category"].as_str().unwrap()))
            })
            .count()
    };
    let best = patterns.iter().map(&missing_count).min().unwrap_or(0);
    patterns.retain(|pattern| missing_count(pattern) == best);
    let mut result = vec![];
    let mut physical = BTreeSet::new();
    let mut repeated = vec![];
    for pattern in patterns {
        let mut slots: Vec<Vec<Option<&Value>>> = vec![];
        let mut missing = vec![];
        for (role, categories) in &pattern {
            if has(categories) {
                continue;
            }
            let candidates: Vec<_> = clothes
                .iter()
                .filter(|c| categories.contains(&c["category"].as_str().unwrap()))
                .map(Some)
                .collect();
            if candidates.is_empty() {
                missing.push(*role);
                slots.push(vec![None]);
            } else {
                slots.push(candidates);
            }
        }
        let initial = vec![0usize; slots.len()];
        let mut heap = BinaryHeap::new();
        let mut visited = BTreeSet::new();
        heap.push((0i64, initial.clone()));
        visited.insert(initial);
        while let Some((_, indices)) = heap.pop() {
            let mut selected: Vec<&Value> = mandatory.iter().collect();
            for (slot, index) in slots.iter().zip(&indices) {
                if let Some(c) = slot[*index] {
                    selected.push(c);
                }
            }
            let mut ids: Vec<String> = selected
                .iter()
                .map(|c| c["id"].as_str().unwrap().to_owned())
                .collect();
            ids.sort();
            ids.dedup();
            if !ids.is_empty() && !forbidden.contains(&ids) && physical.insert(ids.clone()) {
                let items: Vec<Value> = selected
                    .iter()
                    .map(|c| {
                        let (role, layer) = role(c["category"].as_str().unwrap());
                        entry(c, role, layer)
                    })
                    .collect();
                let is_repeat = recent.contains(&ids);
                let mut proposal = json!({"items":items,"complete":missing.is_empty(),"missing_roles":missing,"repeat_relaxed":is_repeat,"reasons":[if missing.is_empty(){"离线角色规则：真实可用衣物，固定与排除约束已保留"}else{"缺少基础角色；仅展示真实库存，不能视为完整穿搭"}]});
                if is_repeat {
                    proposal["reasons"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("库存与硬约束不足，显式放宽最近7天实际穿着的软避重复"));
                    repeated.push(proposal);
                } else {
                    result.push(proposal);
                }
                if result.len() == limit {
                    return result;
                }
            }
            for slot in 0..slots.len() {
                let mut next = indices.clone();
                next[slot] += 1;
                if next[slot] < slots[slot].len() && visited.insert(next.clone()) {
                    let rank = next.iter().sum::<usize>() as i64;
                    heap.push((-rank, next));
                }
            }
        }
    }
    result.extend(repeated.into_iter().take(limit - result.len()));
    result
}
fn role(category: &str) -> (&str, i32) {
    match category {
        "top" => ("top", 0),
        "bottom" => ("bottom", 0),
        "dress" | "one_piece" => ("body", 0),
        "shoes" => ("shoes", 0),
        "outerwear" => ("outerwear", 1),
        "bag" => ("bag", 0),
        "accessory" => ("accessory", 0),
        "underwear" => ("underwear", -2),
        "sportswear" => ("sportswear", 0),
        _ => ("other", 0),
    }
}
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .ok_or_else(|| invalid("请求须为对象"))?
        .keys()
        .any(|key| !allowed.contains(&key.as_str()))
    {
        return Err(invalid("推荐包含不支持字段"));
    }
    Ok(())
}
fn weather(value: Option<&Value>) -> Result<(bool, Value), ApiError> {
    let Some(value) = value else {
        return Ok((
            false,
            json!({"mode":"off","source":"disabled","stale":false,"network_requested":false}),
        ));
    };
    fields(value, &["mode", "temperature_c", "location", "obtained_at"])?;
    let mode = string(value, "mode")?;
    if mode == "off" {
        if value.as_object().unwrap().len() != 1 {
            return Err(invalid("关闭天气时不得附带温度或位置"));
        }
        return Ok((
            false,
            json!({"mode":"off","source":"disabled","stale":false,"network_requested":false}),
        ));
    }
    if mode != "manual" {
        return Err(invalid("离线天气可关闭或手动输入"));
    }
    let temperature = value["temperature_c"]
        .as_f64()
        .filter(|t| t.is_finite() && (-80.0..=60.0).contains(t))
        .ok_or_else(|| invalid("手动温度须为-80至60摄氏度"))?;
    if value
        .get("location")
        .is_some_and(|v| v.as_str().is_none_or(|s| s.chars().count() > 120))
    {
        return Err(invalid("手动位置须为120字内文字"));
    }
    let now = chrono::Utc::now();
    let observed = match value.get("obtained_at") {
        Some(value) => chrono::DateTime::parse_from_rfc3339(
            value
                .as_str()
                .ok_or_else(|| invalid("天气时间须为RFC3339"))?,
        )
        .map_err(|_| invalid("天气时间须为RFC3339"))?
        .with_timezone(&chrono::Utc),
        None => now,
    };
    if observed > now {
        return Err(invalid("天气时间不能晚于现在"));
    }
    Ok((
        temperature <= 15.0,
        json!({"mode":"manual","source":"manual","temperature_c":temperature,"location":value.get("location"),"obtained_at":observed.to_rfc3339(),"stale":now-observed>chrono::Duration::hours(3),"network_requested":false}),
    ))
}
fn ids(input: &Value, key: &str) -> Result<Vec<String>, ApiError> {
    let Some(value) = input.get(key) else {
        return Ok(vec![]);
    };
    let array = value.as_array().ok_or_else(|| invalid("衣物ID须为数组"))?;
    let mut result = vec![];
    for value in array {
        let id = value
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("衣物ID不能为空"))?
            .to_owned();
        if result.contains(&id) {
            return Err(invalid("衣物ID不能重复"));
        }
        result.push(id);
    }
    Ok(result)
}
fn entry(c: &Value, role: &str, layer: i32) -> Value {
    json!({"clothing_id":c["id"],"name":c["name"],"category":c["category"],"role":role,"layer":layer})
}
pub(super) fn descriptors() -> Vec<Value> {
    ["generate","get","list","replace","change","save"].into_iter().map(|op|{
 let mut properties=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];
 if op!="list"{if op=="generate"{for field in ["operation_key","timezone"]{properties[field]=json!({"type":"string"});required.push(field);}properties["limit"]=json!({"type":"integer","minimum":1,"maximum":5});for field in ["fixed_ids","excluded_ids","inventory_priority_ids"]{properties[field]=json!({"type":"array","items":{"type":"string"},"uniqueItems":true});}properties["excluded_sets"]=json!({"type":"array","items":{"type":"array","items":{"type":"string"},"uniqueItems":true}});properties["source_intent"]=json!({"type":"string"});properties["avoid_recent"]=json!({"type":"boolean"});properties["target"]=json!({"enum":["balanced","least_worn","favorites"]});properties["occasion"]=json!({"enum":["daily","work","formal","sport"]});properties["weather"]=json!({"type":"object","required":["mode"],"additionalProperties":false,"properties":{"mode":{"enum":["off","manual"]},"temperature_c":{"type":"number","minimum":-80,"maximum":60},"location":{"type":"string","maxLength":120},"obtained_at":{"type":"string","format":"date-time"}}});}else{properties["id"]=json!({"type":"string"});required.push("id");if op!="get"{properties["operation_key"]=json!({"type":"string"});properties["proposal_index"]=json!({"type":"integer","minimum":0});required.extend(["operation_key","proposal_index"]);if op=="replace"{properties["clothing_id"]=json!({"type":"string"});required.push("clothing_id");}if op=="save"{properties["name"]=json!({"type":"string","minLength":1,"maxLength":120});required.push("name");}}}}
 json!({"name":format!("recommendation.{op}"),"description":format!("本地离线搭配 {op}；仅使用同成员真实可用衣物，生成与保存均不记录穿着"),"inputSchema":{"type":"object","required":required,"properties":properties,"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"id":{"type":"string"},"proposals":{"type":"array"},"items":{"type":"array"}}}})
 }).collect()
}
