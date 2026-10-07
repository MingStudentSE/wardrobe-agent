//! scenario.worker.run：AI 场景计划任务的账本驱动 worker。
//! 先本地规则生成完整候选，再经共享 provider 文本通道调用已配置模型；
//! 模型输出按服务端结构规则逐天校验，失败有限重试后按本地规则降级并
//! 如实反馈；提交结果未知进入等待核对，账本阻止盲重发。
use super::{generation_rules, parse_plan_request, ScenarioRequest, GARMENT_CATEGORIES};
use crate::{string, ApiError, Application};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};

/// 模型尝试预算：初始 + 两次重试（与 #22 的有限重试语义一致）。
const MODEL_ATTEMPTS: u64 = 3;

impl Application {
    pub(super) fn scenario_worker(&self, input: Value) -> Result<Value, ApiError> {
        if input
            .as_object()
            .is_none_or(|object| object.keys().any(|key| !["member_id", "id"].contains(&key.as_str())))
        {
            return Err(ApiError::new("invalid_input", "worker只接受成员与任务ID"));
        }
        let member = self.member(&input)?;
        let id = string(&input, "id")?;
        let original = self.job_get(json!({"member_id":member,"id":id}))?;
        if original["kind"] != "scenario_plan" {
            return Err(ApiError::new("invalid_input", "任务不是场景计划"));
        }
        if matches!(
            original["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled" | "waiting_input")
        ) {
            return Ok(original);
        }
        let claimed = self.job_claim(&member, id, 300)?;
        let lease = string(&claimed, "lease_token")?;
        match self.scenario_ai_task(&claimed) {
            Ok(_) => self.job_get(json!({"member_id":member,"id":id})),
            Err(error) if error.code == "scenario_rate_deferred" => {
                self.job_get(json!({"member_id":member,"id":id}))
            }
            Err(error) if error.code == "lease_lost" || error.code == "installation_changed" => {
                Err(error)
            }
            Err(error) => {
                let (state, code, message) = if error.code == "provider_network" {
                    (
                        "waiting_input",
                        "provider_outcome_unknown",
                        "提交结果未知，无法查询；请核对模型服务后重新发起AI计划",
                    )
                } else {
                    ("failed", error.code.as_str(), error.message.as_str())
                };
                self.job_finish(
                    &member,
                    id,
                    lease,
                    state,
                    Value::Null,
                    json!({"code":code,"message":message,"retryable":false}),
                )
            }
        }
    }

    /// 单次驱动内完成候选生成 →（≤3 次）模型请求 → 校验采纳/降级 → 持久化。
    /// 账本记录每次真实提交；同任务重放不重复扣用额度。
    fn scenario_ai_task(&self, claimed: &Value) -> Result<(), ApiError> {
        let member = self.member(&claimed["payload"])?;
        let id = claimed["id"].as_str().unwrap().to_owned();
        let lease = claimed["lease_token"].as_str().unwrap().to_owned();
        let payload = claimed["payload"].clone();
        let key = string(&payload, "operation_key")?.to_owned();
        let request = parse_plan_request(self, &payload)?;
        // 1. 本地规则候选：既是降级结果，也是模型的候选与校验基线。
        let rule_plan = self.scenario_compute(&request, generation_rules(0, &[]), None)?;
        // 2. 模型轮次；通过校验则采纳，否则带失败码降级为本地规则。
        let plan = match self.scenario_model_days(&request, &rule_plan, claimed)? {
            Some(days) => {
                let attempts = claimed["output_attempt"].as_u64().unwrap_or(1).max(1);
                self.scenario_compute(
                    &request,
                    json!({
                        "source":"text_model",
                        "attempts":attempts,
                        "failure_codes":[],
                        "provider_requests":claimed["submission_count"].as_u64().unwrap_or(1),
                    }),
                    Some(&days),
                )?
            }
            None => {
                let failures = self.scenario_failures(&member, &id)?;
                let attempts = claimed["output_attempt"].as_u64().unwrap_or(0);
                self.scenario_compute(
                    &request,
                    json!({
                        "source":"rule_fallback",
                        "attempts":attempts,
                        "failure_codes":failures,
                        "model_feedback":"模型输出未通过服务端校验，已按本地规则生成；失败码如实记录，可重新发起AI计划或直接使用该计划",
                    }),
                    None,
                )?
            }
        };
        // 3. 幂等持久化：同 (member, operation_key) 已有相同请求的计划则复用。
        let tx = self.begin_write()?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT request,document FROM scenario_plans WHERE member_id=?1 AND operation_key=?2",
                rusqlite::params![member, key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let payload_json = payload.to_string();
        let plan_id = match existing {
            Some((old, document)) if old == payload_json => {
                let stored: Value = serde_json::from_str(&document)
                    .map_err(|_| ApiError::new("storage_error", "计划记录损坏"))?;
                stored["id"].clone()
            }
            Some(_) => {
                self.job_finish(
                    &member,
                    &id,
                    &lease,
                    "failed",
                    Value::Null,
                    json!({"code":"idempotency_conflict","message":"同一标识已保存不同的计划请求","retryable":false}),
                )?;
                return Ok(());
            }
            None => {
                tx.execute(
                    "INSERT INTO scenario_plans VALUES(?1,?2,?3,?4,?5)",
                    rusqlite::params![
                        plan["id"].as_str(),
                        member,
                        key,
                        &payload_json,
                        plan.to_string()
                    ],
                )?;
                plan["id"].clone()
            }
        };
        tx.commit()?;
        self.job_finish(
            &member,
            &id,
            &lease,
            "succeeded",
            json!({"plan_id":plan_id}),
            Value::Null,
        )?;
        Ok(())
    }

    /// 模型轮次序列：构造候选提示 →（≤MODEL_ATTEMPTS）真实请求 → 服务端校验。
    /// Some(days) 表示模型输出通过校验；None 表示有限重试后仍未通过（降级）。
    fn scenario_model_days(
        &self,
        request: &ScenarioRequest,
        rule_plan: &Value,
        claimed: &Value,
    ) -> Result<Option<Vec<Vec<String>>>, ApiError> {
        let member = &request.member;
        let id = claimed["id"].as_str().unwrap().to_owned();
        let lease = claimed["lease_token"].as_str().unwrap().to_owned();
        let provider = self.provider_get(string(&claimed["payload"], "provider_id")?)?;
        // 候选上下文：规则逐天方案 + 涉及的真实单品目录（只含安全字段）。
        let mut catalog: Vec<Value> = vec![];
        for day in rule_plan["days"].as_array().unwrap() {
            for item in day["items"].as_array().unwrap() {
                let garment_id = item["clothing_id"].as_str().unwrap();
                if !catalog
                    .iter()
                    .any(|entry| entry["clothing_id"] == json!(garment_id))
                {
                    if let Ok(garment) = self.clothing_get(json!({"member_id":member,"id":garment_id})) {
                        catalog.push(json!({
                            "clothing_id":garment["id"],
                            "name":garment["name"],
                            "category":garment["category"],
                            "occasion":garment["occasion"],
                            "favorite":garment["favorite"],
                        }));
                    }
                }
            }
        }
        let candidates: Vec<Value> = rule_plan["days"]
            .as_array()
            .unwrap()
            .iter()
            .map(|day| {
                json!({
                    "index":day["index"],
                    "date":day["date"],
                    "clothing_ids":day["items"].as_array().unwrap().iter()
                        .map(|item|item["clothing_id"].clone()).collect::<Vec<_>>(),
                    "complete":day["complete"],
                })
            })
            .collect();
        let mut controls = json!({
            "kind":request.kind,
            "start_date":request.start_date.to_string(),
            "days":request.days,
            "fixed_ids":request.fixed,
            "excluded_ids":request.excluded,
        });
        if let Some(weather) = &request.weather {
            controls["weather"] = weather.clone();
        }
        let prompt = json!({
            "task":"Assign real clothing IDs to each day of this scenario plan. Return JSON only: {\"days\":[{\"index\":0,\"clothing_ids\":[\"exact ID\"]}]}. Every day must cover a complete outfit: body(dress/one_piece) or top+bottom, plus shoes; when weather.temperature_c is 15 or below add outerwear. Garment categories (top, bottom, dress, one_piece, sportswear, underwear) must never repeat on two adjacent days; shoes/outerwear/bag/accessory may repeat. Every fixed_ids ID must be placed on day 0. Excluded IDs never appear. Use ONLY clothing_ids from the inventory; do not invent facts.",
            "controls":controls,
            "weather":rule_plan["weather_context"],
            "inventory":catalog,
            "candidate_days":candidates,
        });
        if prompt.to_string().len() > 65536 {
            return Ok(None);
        }
        let mut failures: Vec<String> = vec![];
        let first_attempt = claimed["output_attempt"].as_u64().unwrap_or(0).max(1);
        for attempt in first_attempt..=MODEL_ATTEMPTS {
            match self.scenario_model_turn(&provider, &prompt, member, &id, &lease, attempt) {
                Ok(draft) => match self.scenario_validate_days(request, rule_plan, &draft) {
                    Ok(days) => return Ok(Some(days)),
                    Err(_) => failures.push("model_invalid_output".to_owned()),
                },
                Err(error) => match error.code.as_str() {
                    "model_invalid_output" => {
                        failures.push("model_invalid_output".to_owned());
                    }
                    "provider_rejected" => {
                        // 明确的模型服务拒绝：如实降级并记录失败码。
                        failures.push("provider_rejected".to_owned());
                        self.scenario_save_failures(member, &id, &lease, attempt, &failures)?;
                        return Ok(None);
                    }
                    _ => return Err(error),
                },
            }
            // 记账 invalid_output + 本次尝试号：下一次尝试的提交账本据此放行。
            self.scenario_save_failures(member, &id, &lease, attempt, &failures)?;
        }
        Ok(None)
    }

    /// 一次真实的模型 POST：账本先行（submitted），网络失败结果未知；
    /// 429 按 Retry-After 有限自动重试或排队；文本解析为 JSON 返回。
    fn scenario_model_turn(
        &self,
        provider: &Value,
        prompt: &Value,
        member: &str,
        id: &str,
        lease: &str,
        attempt: u64,
    ) -> Result<Value, ApiError> {
        let body = json!({
            "model":provider["model"],
            "messages":[
                {"role":"system","content":"You are a wardrobe scenario planner. Follow exact physical IDs and return only the requested JSON object."},
                {"role":"user","content":prompt.to_string()},
            ],
            "temperature":0,
            "max_tokens":2048,
        });
        let request = self.provider_text_request(provider, &body)?;
        let response = loop {
            self.scenario_submitted(member, id, lease, attempt)?;
            let response = request
                .try_clone()
                .ok_or_else(|| ApiError::new("provider_error", "无法建立模型请求"))?
                .send()
                .map_err(|_| ApiError::new("provider_network", "模型连接失败，结果未知，不盲重发"))?;
            if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
                break response;
            }
            let delay = crate::ai_recommendations::retry_after_ms(
                response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok()),
                chrono::Utc::now(),
            );
            if !self.scenario_rate_limited(member, id, lease, delay)? {
                return Err(ApiError::new(
                    "provider_rate_limit",
                    "供应商限流，已达到最多两次自动重试",
                ));
            }
            if delay > 30_000 {
                self.scenario_defer_rate_limit(member, id, lease)?;
                return Err(ApiError::new(
                    "scenario_rate_deferred",
                    "限流已按Retry-After排队，可到期运行worker",
                ));
            }
            loop {
                let current = self.job_get(json!({"member_id":member,"id":id}))?;
                crate::ai_recommendations::text_lease(&current, lease)?;
                let remaining = current["next_due"].as_i64().unwrap_or(0)
                    - chrono::Utc::now().timestamp_millis();
                if remaining <= 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(remaining.min(50) as u64));
            }
        };
        if !response.status().is_success() {
            return Err(ApiError::new("provider_rejected", "模型服务拒绝本次请求"));
        }
        let outer = crate::providers::read_chat_response(response)?;
        let content = outer["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| ApiError::new("model_invalid_output", "模型缺少文本"))?;
        serde_json::from_str(content)
            .map_err(|_| ApiError::new("model_invalid_output", "模型文本不是指定JSON"))
    }

    /// 服务端结构校验：真实ID/归属/可用、排除永不出现、固定落位第0天、
    /// 贴身衣物不连续两天重复、逐天基础覆盖（低温含外层）、天序完整。
    /// 校验池与规则计算同源（scenario_available_pool），绝不放宽。
    fn scenario_validate_days(
        &self,
        request: &ScenarioRequest,
        rule_plan: &Value,
        draft: &Value,
    ) -> Result<Vec<Vec<String>>, ApiError> {
        let fail = || ApiError::new("model_invalid_output", "模型逐天指派未通过服务端校验");
        let object = draft
            .as_object()
            .filter(|object| {
                object.contains_key("days")
                    && object
                        .keys()
                        .all(|key| ["days", "explanation"].contains(&key.as_str()))
            })
            .ok_or_else(fail)?;
        if let Some(explanation) = object.get("explanation") {
            explanation
                .as_str()
                .filter(|text| text.chars().count() <= 1000)
                .ok_or_else(fail)?;
        }
        let days = draft["days"].as_array().ok_or_else(fail)?;
        if days.is_empty() || days.len() != request.days {
            return Err(fail());
        }
        // 允许集合：与规则计算同源的可用池（active + 洗护可用 − 排除）。
        let allowed: std::collections::HashMap<String, String> = self
            .scenario_available_pool(&request.member, &request.excluded)?
            .into_iter()
            .map(|garment| {
                (
                    garment["id"].as_str().unwrap().to_owned(),
                    garment["category"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect();
        let mut placed: Vec<String> = vec![];
        let mut previous_garments: Vec<String> = vec![];
        for (position, day) in days.iter().enumerate() {
            let day_object = day
                .as_object()
                .filter(|object| {
                    object.contains_key("index")
                        && object.contains_key("clothing_ids")
                        && object
                            .keys()
                            .all(|key| ["index", "clothing_ids"].contains(&key.as_str()))
                })
                .ok_or_else(fail)?;
            let index = day_object["index"].as_u64().ok_or_else(fail)?;
            if index != position as u64 {
                return Err(fail());
            }
            let ids: Vec<String> = day_object["clothing_ids"]
                .as_array()
                .filter(|ids| !ids.is_empty() && ids.len() <= 100)
                .ok_or_else(fail)?
                .iter()
                .map(|id| {
                    id.as_str()
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned)
                        .ok_or_else(fail)
                })
                .collect::<Result<Vec<String>, ApiError>>()?;
            let mut unique = std::collections::HashSet::new();
            let mut day_garments: Vec<String> = vec![];
            for garment_id in &ids {
                let category = allowed.get(garment_id).ok_or_else(fail)?;
                if GARMENT_CATEGORIES.contains(&category.as_str())
                    && previous_garments.iter().any(|previous| previous == garment_id)
                {
                    return Err(fail());
                }
                if GARMENT_CATEGORIES.contains(&category.as_str()) {
                    day_garments.push(garment_id.clone());
                }
                if !unique.insert(garment_id.clone()) {
                    return Err(fail());
                }
            }
            if position == 0 {
                for fixed in &request.fixed {
                    if !ids.iter().any(|id| id == fixed) {
                        return Err(fail());
                    }
                }
            }
            placed.extend(ids.iter().cloned());
            previous_garments = day_garments;
        }
        for fixed in &request.fixed {
            if !placed.iter().any(|id| id == fixed) {
                return Err(fail());
            }
        }
        // 逐天基础覆盖：body 或 top+bottom + shoes；低温（≤15°C）要求外层。
        let cold = rule_plan["weather_context"]["temperature_c"]
            .as_f64()
            .is_some_and(|temperature| temperature <= 15.0);
        for day in days.iter() {
            let ids: Vec<&str> = day["clothing_ids"]
                .as_array()
                .ok_or_else(fail)?
                .iter()
                .filter_map(|id| id.as_str())
                .collect();
            let has = |categories: &[&str]| {
                ids.iter().any(|garment_id| {
                    allowed
                        .get(*garment_id)
                        .is_some_and(|category| categories.contains(&category.as_str()))
                })
            };
            let covered = has(&["dress", "one_piece"]) || (has(&["top"]) && has(&["bottom"]));
            if !covered || !has(&["shoes"]) || (cold && !has(&["outerwear"])) {
                return Err(fail());
            }
        }
        Ok(days
            .iter()
            .map(|day| {
                day["clothing_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap().to_owned())
                    .collect()
            })
            .collect())
    }

    // ------------------------------------------------------------------
    // 提交账本：与 text_recommendation 同语义的持久化提交记录。
    // ------------------------------------------------------------------
    fn scenario_submitted(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        attempt: u64,
    ) -> Result<(), ApiError> {
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        crate::ai_recommendations::text_lease(&job, lease)?;
        crate::schedules::validate_related_job(&tx, &job)?;
        if job["submitted_at"].is_string() {
            let invalid = job["last_outcome"] == "invalid_output"
                && attempt == job["output_attempt"].as_u64().unwrap_or(1) + 1
                && attempt <= MODEL_ATTEMPTS;
            let limited = job["last_outcome"] == "rate_limited"
                && attempt == job["output_attempt"].as_u64().unwrap_or(1)
                && job["rate_retries"].as_u64().unwrap_or(0) < 2
                && job["next_due"].as_i64().unwrap_or(0) <= chrono::Utc::now().timestamp_millis();
            if !invalid && !limited {
                return Err(ApiError::new(
                    "submission_already_sent",
                    "结果未知或重试预算已耗尽，不能盲重发",
                ));
            }
            if limited {
                job["rate_retries"] = json!(job["rate_retries"].as_u64().unwrap_or(0) + 1);
            }
        }
        job["submitted_at"] = json!(chrono::Utc::now().to_rfc3339());
        job["submission_count"] = json!(job["submission_count"].as_u64().unwrap_or(0) + 1);
        job["output_attempt"] = json!(attempt);
        job["next_due"] = json!(0);
        job["last_outcome"] = json!("unknown");
        job["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        crate::jobs::save(&tx, &job)?;
        tx.commit()?;
        Ok(())
    }

    fn scenario_save_failures(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        attempt: u64,
        failures: &[String],
    ) -> Result<(), ApiError> {
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        crate::ai_recommendations::text_lease(&job, lease)?;
        job["last_outcome"] = json!("invalid_output");
        job["output_attempt"] = json!(attempt);
        job["scenario_failures"] = json!(failures);
        job["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        crate::jobs::save(&tx, &job)?;
        tx.commit()?;
        Ok(())
    }

    fn scenario_failures(&self, member: &str, id: &str) -> Result<Vec<String>, ApiError> {
        let job = self.job_get(json!({"member_id":member,"id":id}))?;
        Ok(job["scenario_failures"]
            .as_array()
            .map(|codes| {
                codes
                    .iter()
                    .filter_map(|code| code.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }

    fn scenario_rate_limited(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        delay: i64,
    ) -> Result<bool, ApiError> {
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        crate::ai_recommendations::text_lease(&job, lease)?;
        job["last_outcome"] = json!("rate_limited");
        job["error"] =
            json!({"code":"provider_rate_limit","message":"供应商限流，遵守Retry-After后重试","retryable":true});
        let retry = job["rate_retries"].as_u64().unwrap_or(0) < 2;
        if retry {
            job["next_due"] =
                json!(chrono::Utc::now().timestamp_millis().saturating_add(delay));
        }
        crate::jobs::save(&tx, &job)?;
        tx.commit()?;
        Ok(retry)
    }

    fn scenario_defer_rate_limit(&self, member: &str, id: &str, lease: &str) -> Result<(), ApiError> {
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        crate::ai_recommendations::text_lease(&job, lease)?;
        job["state"] = json!("queued");
        job["lease_token"] = Value::Null;
        job["lease_expires"] = json!(0);
        crate::jobs::save(&tx, &job)?;
        tx.commit()?;
        Ok(())
    }
}
