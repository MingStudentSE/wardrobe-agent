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
    pub(super) fn recommendation_execute(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        if op == "recommendation.worker.run" {
            return self.ai_worker(input);
        }
        if op == "recommendation.generate" {
            generate_fields(&input)?;
        }
        if op == "recommendation.generate" && input["strategy"] == "ai" {
            return self.ai_enqueue(input);
        }
        let context = if op == "recommendation.generate" {
            Some(self.recommendation_weather_for(&self.member(&input)?, input.get("weather"))?)
        } else {
            None
        };
        self.recommendation_with_generation(op, input, None, None, context.as_ref())
    }
    pub(super) fn recommendation_with_generation(
        &self,
        op: &str,
        input: Value,
        draft: Option<&Value>,
        generation: Option<&Value>,
        weather: Option<&(bool, Value)>,
    ) -> Result<Value, ApiError> {
        self.recommendation_build(op, input, draft, generation, weather, true)
    }
    pub(super) fn recommendation_build(
        &self,
        op: &str,
        mut input: Value,
        draft: Option<&Value>,
        generation: Option<&Value>,
        resolved_weather: Option<&(bool, Value)>,
        persist: bool,
    ) -> Result<Value, ApiError> {
        let allowed = match op {
            "recommendation.generate" => vec![
                "member_id",
                "strategy",
                "provider_id",
                "json_output",
                "output_format",
                "operation_key",
                "timezone",
                "target_date",
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
                "retained_ids",
                "action_excluded_ids",
                "replacement_category",
                "retained_items",
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
                controls["retained_ids"] = json!(proposal_ids
                    .iter()
                    .filter(|id| *id != old)
                    .collect::<Vec<_>>());
                controls["retained_items"]=json!(proposal["items"].as_array().unwrap().iter().filter(|item|item["clothing_id"]!=old).map(|item|json!({"clothing_id":item["clothing_id"],"role":item["role"],"layer":item["layer"]})).collect::<Vec<_>>());
                controls["action_excluded_ids"] = json!([old]);
                controls["replacement_category"] = proposal["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["clothing_id"] == old)
                    .unwrap()["category"]
                    .clone();
            } else {
                controls.as_object_mut().unwrap().remove("retained_items");
                controls
                    .as_object_mut()
                    .unwrap()
                    .remove("replacement_category");
                controls.as_object_mut().unwrap().remove("retained_ids");
                controls
                    .as_object_mut()
                    .unwrap()
                    .remove("action_excluded_ids");
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
        if input
            .get("strategy")
            .is_some_and(|v| v != "rules" && v != "ai")
        {
            return Err(invalid("推荐策略须为rules或ai"));
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
        let mut fixed = ids(&input, "fixed_ids")?;
        for id in ids(&input, "retained_ids")? {
            if !fixed.contains(&id) {
                fixed.push(id);
            }
        }
        let mut excluded = ids(&input, "excluded_ids")?;
        for id in ids(&input, "action_excluded_ids")? {
            if !excluded.contains(&id) {
                excluded.push(id);
            }
        }
        if fixed.iter().any(|id| excluded.contains(id)) {
            return Err(ApiError::new("constraint_conflict", "固定衣物不能同时排除"));
        }
        let zone: chrono_tz::Tz = string(&input, "timezone")?
            .parse()
            .map_err(|_| invalid("须使用有效IANA时区"))?;
        let (cold, weather_context) = match resolved_weather {
            Some(v) => v.clone(),
            None => weather(input.get("weather"))?,
        };
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
        if input.get("source_intent").is_some() {
            self.recommendation_execute(
                "recommendation.get",
                json!({"member_id":member,"id":string(&input,"source_intent")?}),
            )?;
        }
        for id in fixed.iter().chain(excluded.iter()) {
            self.clothing_get(json!({"member_id":member,"id":id}))?;
        }
        let feedback_excluded = self.feedback_day_exclusions(
            &member,
            input
                .get("target_date")
                .and_then(Value::as_str)
                .map(|text| {
                    let date = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                        .map_err(|_| invalid("目标日期无效"))?;
                    use chrono::TimeZone;
                    zone.from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
                        .earliest()
                        .map(|v| v.with_timezone(&chrono::Utc))
                        .ok_or_else(|| invalid("目标当地日期不存在"))
                })
                .transpose()?,
        )?;
        for id in &feedback_excluded {
            if fixed.contains(id) {
                return Err(ApiError::new(
                    "constraint_conflict",
                    "固定单品已明确当天排除，请撤销反馈或修改固定条件",
                ));
            }
            if !excluded.contains(id) {
                excluded.push(id.clone());
            }
        }
        let wears = self.effective_wears(&member)?;
        let statuses = json!({"items":self.care_batch_from_wears(&member,&wears)?});
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
        let today = input
            .get("target_date")
            .map(|v| {
                chrono::NaiveDate::parse_from_str(v.as_str().unwrap_or(""), "%Y-%m-%d")
                    .map_err(|_| invalid("目标日期须为YYYY-MM-DD"))
            })
            .transpose()?
            .unwrap_or_else(|| chrono::Utc::now().with_timezone(&zone).date_naive());
        let from = today - chrono::Duration::days(6);
        let mut recent = std::collections::BTreeSet::new();
        let mut counts = std::collections::HashMap::<String, usize>::new();
        for wear in wears {
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
        let priority: std::collections::HashSet<String> = priority.into_iter().collect();
        clothes.sort_by_key(|c| {
            let id = c["id"].as_str().unwrap();
            (
                !priority.contains(id),
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
        let mut forbidden = self.feedback_disliked_sets(&member)?;
        let disliked_set_count = forbidden.len();
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
        let replacement = input
            .get("replacement_category")
            .map(|v| {
                v.as_str()
                    .filter(|s| crate::CATEGORIES.contains(s))
                    .ok_or_else(|| invalid("替换类别不可用"))
            })
            .transpose()?;
        let retained = input
            .get("retained_items")
            .map(|value| {
                value
                    .as_array()
                    .filter(|a| a.len() <= 100)
                    .ok_or_else(|| invalid("保留层级须为最多100项数组"))
            })
            .transpose()?;
        if let Some(items) = retained {
            let source = self.recommendation_execute(
                "recommendation.get",
                json!({"member_id":member,"id":string(&input,"source_intent")?}),
            )?;
            let held = ids(&input, "retained_ids")?;
            let mut seen = std::collections::HashSet::new();
            for item in items {
                fields(item, &["clothing_id", "role", "layer"])?;
                let id = string(item, "clothing_id")?;
                if !held.iter().any(|held| held == id)
                    || !seen.insert(id)
                    || !source["proposals"].as_array().unwrap().iter().any(|p| {
                        p["items"].as_array().unwrap().iter().any(|old| {
                            old["clothing_id"] == item["clothing_id"]
                                && old["role"] == item["role"]
                                && old["layer"] == item["layer"]
                        })
                    })
                {
                    return Err(invalid("保留单品角色层级必须来自同成员原意图"));
                }
            }
            if seen.len() != held.len() {
                return Err(invalid("保留单品层级须完整"));
            }
        }
        let rules = PlanningRules {
            fixed: &fixed,
            limit: limit as usize,
            recent: &recent,
            forbidden: &forbidden,
            cold,
            occasion,
            priority: &priority,
            replacement,
        };
        let (mut proposals, capacity_blocked) = if let Some(draft) = draft {
            (validate_model(draft, &clothes, &rules, retained)?, false)
        } else {
            plan(&clothes, &rules)
        };

        for proposal in &mut proposals {
            if let Some(retained) = retained {
                for item in proposal["items"].as_array_mut().unwrap() {
                    if let Some(old) = retained
                        .iter()
                        .find(|old| old["clothing_id"] == item["clothing_id"])
                    {
                        item["role"] = old["role"].clone();
                        item["layer"] = old["layer"].clone();
                    }
                }
            }
            let items = proposal["items"].as_array().unwrap().clone();
            let verified = items.iter().all(|item| {
                clothes
                    .iter()
                    .find(|c| c["id"] == item["clothing_id"])
                    .is_some_and(|c| {
                        c["occasion"]
                            .as_array()
                            .is_some_and(|tags| tags.iter().any(|tag| tag == occasion))
                    })
            });
            proposal["occasion_verified"] = json!(verified);

            let reasons = proposal["reasons"].as_array_mut().unwrap();
            if !verified {
                reasons.push(json!(format!(
                    "{}场合适宜性未验证：部分单品未记录对应场合标签；不作为硬排除",
                    match occasion {
                        "work" => "工作",
                        "formal" => "正式",
                        "sport" => "运动",
                        _ => "日常",
                    }
                )));
            }
            reasons.push(json!(format!(
                "目标：{}；场合：{}（只采用已记录标签，不推断未知事实）",
                match target {
                    "least_worn" => "少穿优先",
                    "favorites" => "收藏优先",
                    _ => "平衡选择",
                },
                match occasion {
                    "work" => "工作",
                    "formal" => "正式",
                    "sport" => "运动",
                    _ => "日常",
                }
            )));
            for item in items {
                let id = item["clothing_id"].as_str().unwrap();
                if priority.contains(id) {
                    reasons.push(json!(format!("库存优先：{}", item["name"])));
                }
                if target == "least_worn" {
                    reasons.push(json!(format!(
                        "{}：有效实际穿着{}次",
                        item["name"],
                        counts.get(id).copied().unwrap_or(0)
                    )));
                }
                if ["sportswear", "other", "underwear"]
                    .contains(&item["category"].as_str().unwrap())
                {
                    reasons.push(json!(format!(
                        "{}保留显式选入角色，不将未知覆盖范围视为全身衣物",
                        item["name"]
                    )));
                }
            }
            if cold {
                reasons.push(json!(if weather_context["source"] == "manual" {
                    "手动温度不高于15°C，要求外层；不承诺衣物保暖性能"
                } else {
                    "天气估计温度不高于15°C，要求外层；不承诺衣物保暖性能"
                }));
            }
        }
        let mut shortage_reasons: Vec<String> = if proposals.len() < limit as usize {
            vec![format!(
                "请求{}套，当前硬约束下仅有{}套不同实物组合；不会重复或虚构库存",
                limit,
                proposals.len()
            )]
        } else {
            vec![]
        };
        if capacity_blocked {
            shortage_reasons
                .push("固定与所需角色超出单套100件搭配限制；没有移除固定衣物".to_owned());
        }
        let mut missing = std::collections::BTreeSet::new();
        for proposal in &proposals {
            for role in proposal["missing_roles"].as_array().unwrap() {
                missing.insert(role.as_str().unwrap());
            }
        }
        for role in missing {
            shortage_reasons.push(format!("缺少可用{}角色：检查库存、排除和洗护状态", role));
        }
        if clothes.is_empty() {
            shortage_reasons.push("没有满足成员、生命周期、洗护及排除条件的真实衣物".to_owned());
        }
        let now = chrono::Utc::now().to_rfc3339();
        let mut public_generation = generation
            .cloned()
            .unwrap_or(json!({"source":"rules","attempts":0}));
        if let Some(object) = public_generation.as_object_mut() {
            object.remove("_lease_token");
        }
        let doc = json!({"schema_version":1,"id":uuid::Uuid::now_v7().to_string(),"member_id":member,"created_at":now,"updated_at":now,"controls":input,"feedback_context":{"day_excluded_ids":feedback_excluded,"disliked_set_count":disliked_set_count,"source":"explicit_effective_feedback"},"generation":public_generation,"proposals":proposals,"requested_count":limit,"actual_count":proposals.len(),"offline":public_generation["source"]!="text_model","creates_wear":false,"shortage_reasons":shortage_reasons,"weather_context":weather_context,"recent_window":{"from":from.to_string(),"through":today.to_string(),"timezone":zone.name(),"source":"actual_effective_wear"}});
        if !persist {
            return Ok(doc);
        }
        if let Some(guard) = generation {
            if let (Some(job), Some(lease)) =
                (guard["job_id"].as_str(), guard["_lease_token"].as_str())
            {
                let task = self.job_get(json!({"member_id":member,"id":job}))?;
                crate::ai_recommendations::text_lease(&task, lease)?;
            }
        }

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
struct PlanningRules<'a> {
    fixed: &'a [String],
    limit: usize,
    recent: &'a std::collections::BTreeSet<Vec<String>>,
    forbidden: &'a std::collections::BTreeSet<Vec<String>>,
    cold: bool,
    occasion: &'a str,
    priority: &'a std::collections::HashSet<String>,
    replacement: Option<&'a str>,
}
fn plan(clothes: &[Value], rules: &PlanningRules<'_>) -> (Vec<Value>, bool) {
    let PlanningRules {
        fixed,
        limit,
        recent,
        forbidden,
        cold,
        occasion,
        priority,
        replacement,
    } = *rules;
    use std::collections::{BTreeSet, BinaryHeap, HashMap};
    let mandatory: Vec<&Value> = clothes
        .iter()
        .filter(|c| fixed.iter().any(|id| c["id"] == *id))
        .collect();
    let has = |categories: &[&str]| {
        mandatory
            .iter()
            .any(|c| categories.contains(&c["category"].as_str().unwrap()))
    };
    let exists = |categories: &[&str]| {
        clothes
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
    if !has(&["top", "bottom"]) || has(&["dress", "one_piece"]) {
        for category in ["dress", "one_piece"] {
            if has(&["dress", "one_piece"]) && !has(&[category]) {
                continue;
            }
            if has(&[category]) || exists(&[category]) {
                patterns.push(vec![("body", vec![category]), ("shoes", vec!["shoes"])]);
            }
        }
    }
    if occasion == "sport" && exists(&["sportswear"]) {
        for pattern in &mut patterns {
            pattern.push(("sportswear", vec!["sportswear"]));
        }
        patterns.push(vec![
            ("sportswear", vec!["sportswear"]),
            ("shoes", vec!["shoes"]),
            ("coverage_unknown", vec![]),
        ]);
    }
    for pattern in &mut patterns {
        if cold {
            pattern.push(("outerwear", vec!["outerwear"]));
        }
        for category in [
            "outerwear",
            "bag",
            "accessory",
            "underwear",
            "sportswear",
            "other",
        ] {
            if !pattern.iter().any(|(role, _)| *role == category)
                && clothes.iter().any(|c| {
                    c["category"] == category && priority.contains(c["id"].as_str().unwrap())
                })
            {
                pattern.push((category, vec![category]));
            }
        }
    }
    if let Some(category) = replacement {
        let (role, _) = role(category);
        let categories = if role == "body" {
            vec!["dress", "one_piece"]
        } else {
            vec![category]
        };
        if !exists(&categories) {
            return (vec![], false);
        }
        if ["top", "bottom", "body", "shoes"].contains(&role) {
            patterns.retain(|p| p.iter().any(|(name, _)| *name == role));
            if role == "body" {
                patterns.retain(|p| {
                    p.iter()
                        .filter(|(name, _)| *name == "body")
                        .all(|(_, cats)| exists(cats))
                });
            }
        } else {
            for pattern in &mut patterns {
                if !pattern.iter().any(|(name, _)| *name == role) {
                    pattern.push((role, categories.clone()));
                }
            }
        }
    }
    let missing_count = |pattern: &Vec<(&str, Vec<&str>)>| {
        pattern
            .iter()
            .filter(|(_, categories)| !has(categories) && !exists(categories))
            .count()
    };
    let best = patterns.iter().map(&missing_count).min().unwrap_or(0);
    patterns.retain(|p| missing_count(p) == best);
    let ranks: HashMap<&str, usize> = clothes
        .iter()
        .enumerate()
        .map(|(i, c)| (c["id"].as_str().unwrap(), i))
        .collect();
    let mut compiled = vec![];
    let mut capacity_blocked = false;
    for pattern in patterns {
        let mut slots = vec![];
        let mut missing = vec![];
        for (role, categories) in pattern {
            if has(&categories) {
                continue;
            }
            let mut candidates: Vec<Option<&Value>> = clothes
                .iter()
                .filter(|c| categories.contains(&c["category"].as_str().unwrap()))
                .map(Some)
                .collect();
            if candidates.is_empty() {
                missing.push(role);
                candidates.push(None);
            }
            slots.push(candidates);
        }
        if mandatory.len() + slots.iter().filter(|slot| slot[0].is_some()).count() > 100 {
            capacity_blocked = true;
            continue;
        }
        compiled.push((slots, missing));
    }
    let score = |pattern: usize, indices: &Vec<usize>| {
        -(compiled[pattern]
            .0
            .iter()
            .zip(indices)
            .map(|(slot, index)| {
                slot[*index]
                    .map(|c| ranks[c["id"].as_str().unwrap()])
                    .unwrap_or(0)
            })
            .sum::<usize>() as i64)
    };
    let mut heap = BinaryHeap::new();
    let mut visited = BTreeSet::new();
    for (pattern, (slots, _)) in compiled.iter().enumerate() {
        let indices = vec![0; slots.len()];
        heap.push((score(pattern, &indices), pattern, indices.clone()));
        visited.insert((pattern, indices));
    }
    let mut result = vec![];
    let mut physical = BTreeSet::new();
    let mut repeated = vec![];
    while let Some((_, pattern, indices)) = heap.pop() {
        let (slots, missing) = &compiled[pattern];
        let mut selected = mandatory.clone();
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
            let mut layers = HashMap::new();
            let items: Vec<Value> = selected
                .iter()
                .map(|c| {
                    let (role, base) = role(c["category"].as_str().unwrap());
                    let layer = layers.entry(role).or_insert(base);
                    let value = entry(c, role, *layer);
                    *layer += 1;
                    value
                })
                .collect();
            let is_repeat = recent.contains(&ids);
            let mut proposal = json!({"items":items,"complete":missing.is_empty(),"missing_roles":missing,"repeat_relaxed":is_repeat,"reasons":[if missing.is_empty(){"离线角色规则：真实可用衣物，固定与排除约束已保留"}else{"缺少基础角色或覆盖范围未知；仅展示真实库存，不能视为完整穿搭"}]});
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
                return (result, false);
            }
        }
        for slot in 0..slots.len() {
            let mut next = indices.clone();
            next[slot] += 1;
            if next[slot] < slots[slot].len() && visited.insert((pattern, next.clone())) {
                heap.push((score(pattern, &next), pattern, next));
            }
        }
    }
    result.extend(repeated.into_iter().take(limit - result.len()));
    let blocked = capacity_blocked && result.is_empty();
    (result, blocked)
}
pub(super) fn role(category: &str) -> (&str, i32) {
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
pub(super) fn weather(value: Option<&Value>) -> Result<(bool, Value), ApiError> {
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
pub(super) fn ids(input: &Value, key: &str) -> Result<Vec<String>, ApiError> {
    let Some(value) = input.get(key) else {
        return Ok(vec![]);
    };
    let array = value
        .as_array()
        .filter(|a| a.len() <= 100)
        .ok_or_else(|| invalid("衣物ID须为最多100项的数组"))?;
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
    ["generate","get","list","replace","change","save","worker.run"].into_iter().map(|op|{
 let mut properties=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];
 if op!="list"{if op=="generate"{for field in ["operation_key","timezone"]{properties[field]=json!({"type":"string"});required.push(field);}properties["target_date"]=json!({"type":"string","format":"date"});properties["strategy"]=json!({"enum":["rules","ai"]});properties["provider_id"]=json!({"type":"string"});properties["json_output"]=json!({"type":"boolean"});properties["output_format"]=json!({"enum":["plain","json_object","llama_cpp_schema"]});properties["limit"]=json!({"type":"integer","minimum":1,"maximum":5});for field in ["fixed_ids","excluded_ids","inventory_priority_ids","retained_ids","action_excluded_ids"]{properties[field]=json!({"type":"array","items":{"type":"string"},"maxItems":100,"uniqueItems":true});}properties["excluded_sets"]=json!({"type":"array","items":{"type":"array","items":{"type":"string"},"maxItems":100,"uniqueItems":true}});properties["source_intent"]=json!({"type":"string"});properties["replacement_category"]=json!({"enum":crate::CATEGORIES});properties["retained_items"]=json!({"type":"array","maxItems":100,"items":{"type":"object","required":["clothing_id","role","layer"],"properties":{"clothing_id":{"type":"string"},"role":{"type":"string"},"layer":{"type":"integer"}},"additionalProperties":false}});properties["avoid_recent"]=json!({"type":"boolean"});properties["target"]=json!({"enum":["balanced","least_worn","favorites"]});properties["occasion"]=json!({"enum":["daily","work","formal","sport"]});properties["weather"]=json!({"type":"object","required":["mode"],"additionalProperties":false,"properties":{"mode":{"enum":["off","manual","automatic"]},"temperature_c":{"type":"number","minimum":-80,"maximum":60},"location":{"type":"string","maxLength":120},"obtained_at":{"type":"string","format":"date-time"}}});}else{properties["id"]=json!({"type":"string"});required.push("id");if op!="get"&&op!="worker.run"{properties["operation_key"]=json!({"type":"string"});properties["proposal_index"]=json!({"type":"integer","minimum":0});required.extend(["operation_key","proposal_index"]);if op=="replace"{properties["clothing_id"]=json!({"type":"string"});required.push("clothing_id");}
if op=="save"{properties["name"]=json!({"type":"string","minLength":1,"maxLength":120});required.push("name");}}}}
 json!({"name":format!("recommendation.{op}"),"description":format!("本地离线搭配 {op}；仅使用同成员真实可用衣物，生成与保存均不记录穿着"),"inputSchema":{"type":"object","required":required,"properties":properties,"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"id":{"type":"string"},"job_id":{"type":"string"},"state":{"type":"string"},"intent_id":{"type":["string","null"]},"next_due":{"type":"integer"},"proposals":{"type":"array"},"items":{"type":"array"}}}})
 }).collect()
}

// Model results use the same category/coverage planner and current availability snapshot.
fn validate_model(
    draft: &Value,
    clothes: &[Value],
    rules: &PlanningRules<'_>,
    retained: Option<&Vec<Value>>,
) -> Result<Vec<Value>, ApiError> {
    let fail = || {
        ApiError::new(
            "model_invalid_output",
            "模型组合未通过真实ID、成员、角色、固定/排除及可用性检查",
        )
    };
    fields(draft, &["proposals"]).map_err(|_| fail())?;
    let proposed = draft["proposals"]
        .as_array()
        .filter(|p| !p.is_empty() && p.len() <= rules.limit)
        .ok_or_else(fail)?;
    let offline = plan(clothes, rules).0;
    if proposed.len() != offline.len() {
        return Err(fail());
    }
    let candidate_rules = PlanningRules {
        fixed: rules.fixed,
        limit: 5,
        recent: rules.recent,
        forbidden: rules.forbidden,
        cold: rules.cold,
        occasion: rules.occasion,
        priority: rules.priority,
        replacement: rules.replacement,
    };
    let allowed_sets: std::collections::BTreeSet<Vec<String>> = plan(clothes, &candidate_rules)
        .0
        .iter()
        .map(|p| {
            let mut ids: Vec<String> = p["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["clothing_id"].as_str().unwrap().to_owned())
                .collect();
            ids.sort();
            ids
        })
        .collect();
    let non_repeated = offline
        .iter()
        .filter(|p| p["repeat_relaxed"] == false)
        .count();
    let mut repeated = 0;
    let mut out = vec![];
    let mut physical = std::collections::BTreeSet::new();
    for proposal in proposed {
        fields(proposal, &["items", "explanation"]).map_err(|_| fail())?;
        let items = proposal["items"]
            .as_array()
            .filter(|v| !v.is_empty() && v.len() <= 100)
            .ok_or_else(fail)?;
        let mut ids = vec![];
        for item in items {
            fields(item, &["clothing_id", "role", "layer"]).map_err(|_| fail())?;
            let id = item["clothing_id"].as_str().ok_or_else(fail)?;
            let garment = clothes.iter().find(|g| g["id"] == id).ok_or_else(fail)?;
            if ids.iter().any(|x| x == id)
                || item["role"] != role(garment["category"].as_str().unwrap()).0
                || item["layer"].as_i64().is_none()
            {
                return Err(fail());
            }
            ids.push(id.to_owned());
        }
        if rules.fixed.iter().any(|id| !ids.contains(id)) {
            return Err(fail());
        }
        ids.sort();
        if !allowed_sets.contains(&ids) {
            return Err(fail());
        }
        if rules.recent.contains(&ids) {
            repeated += 1;
            if repeated > offline.len() - non_repeated {
                return Err(fail());
            }
        }
        if !physical.insert(ids.clone()) || rules.forbidden.contains(&ids) {
            return Err(fail());
        }
        let selected: Vec<Value> = clothes
            .iter()
            .filter(|c| ids.iter().any(|id| c["id"] == *id))
            .cloned()
            .collect();
        let validation = PlanningRules {
            fixed: &ids,
            limit: 1,
            recent: rules.recent,
            forbidden: rules.forbidden,
            cold: rules.cold,
            occasion: rules.occasion,
            priority: rules.priority,
            replacement: rules.replacement,
        };
        let (mut generated, _) = plan(&selected, &validation);
        let mut accepted = generated.pop().ok_or_else(fail)?;
        // Do not accept a complete-looking model result with unmet basic coverage.
        if accepted["complete"] != true {
            return Err(fail());
        }
        if let Some(retained) = retained {
            for item in accepted["items"].as_array_mut().unwrap() {
                if let Some(old) = retained
                    .iter()
                    .find(|v| v["clothing_id"] == item["clothing_id"])
                {
                    item["role"] = old["role"].clone();
                    item["layer"] = old["layer"].clone();
                }
            }
        }
        if let Some(explanation) = proposal.get("explanation") {
            let text = explanation
                .as_str()
                .filter(|s| s.chars().count() <= 1000)
                .ok_or_else(fail)?;
            accepted["model_explanation"] = json!(text);
        }
        for item in items {
            let canonical = accepted["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["clothing_id"] == item["clothing_id"])
                .ok_or_else(fail)?;
            if canonical["role"] != item["role"] || canonical["layer"] != item["layer"] {
                return Err(fail());
            }
        }
        accepted["reasons"].as_array_mut().unwrap().push(json!(
            "文本模型选择已通过与离线规则相同的真实库存及角色检查"
        ));
        out.push(accepted);
    }
    Ok(out)
}

pub(super) fn generate_fields(input: &Value) -> Result<(), ApiError> {
    fields(
        input,
        &[
            "member_id",
            "operation_key",
            "timezone",
            "target_date",
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
            "retained_ids",
            "action_excluded_ids",
            "replacement_category",
            "retained_items",
            "strategy",
            "provider_id",
            "json_output",
            "output_format",
        ],
    )?;
    if let Some(date) = input.get("target_date") {
        chrono::NaiveDate::parse_from_str(date.as_str().unwrap_or(""), "%Y-%m-%d")
            .map_err(|_| invalid("目标日期须为YYYY-MM-DD"))?;
    }
    for field in [
        "fixed_ids",
        "excluded_ids",
        "inventory_priority_ids",
        "retained_ids",
        "action_excluded_ids",
    ] {
        ids(input, field)?;
    }
    for field in ["avoid_recent", "json_output"] {
        if input.get(field).is_some_and(|v| !v.is_boolean()) {
            return Err(invalid("布尔选项格式错误"));
        }
    }
    for (field, values) in [
        ("strategy", vec!["rules", "ai"]),
        ("target", vec!["balanced", "least_worn", "favorites"]),
        ("occasion", vec!["daily", "work", "formal", "sport"]),
        (
            "output_format",
            vec!["plain", "json_object", "llama_cpp_schema"],
        ),
    ] {
        if input
            .get(field)
            .is_some_and(|v| v.as_str().is_none_or(|s| !values.contains(&s)))
        {
            return Err(invalid("推荐选项不可用"));
        }
    }
    let key = string(input, "operation_key")?;
    if key.is_empty()
        || key.len() > 200
        || string(input, "timezone")?.parse::<chrono_tz::Tz>().is_err()
        || input
            .get("limit")
            .is_some_and(|v| v.as_u64().is_none_or(|n| !(1..=5).contains(&n)))
    {
        return Err(invalid("推荐标识、时区或数量不可用"));
    }
    Ok(())
}
