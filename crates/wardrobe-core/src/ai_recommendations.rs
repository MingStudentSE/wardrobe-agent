use crate::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::io::Read;
impl Application {
    pub(super) fn recommendation_weather(
        &self,
        value: Option<&Value>,
    ) -> Result<(bool, Value), ApiError> {
        if value.is_some_and(|v| v["mode"] == "automatic") {
            let value = value.unwrap();
            if value.as_object().is_none_or(|m| m.len() != 1) {
                return Err(ApiError::new(
                    "invalid_input",
                    "自动天气只提供mode，地点使用已保存设置",
                ));
            }
            // Caller supplies member via the dedicated method below.
            return Err(ApiError::new("invalid_input", "自动天气需要成员上下文"));
        }
        crate::recommendations::weather(value)
    }
    pub(super) fn recommendation_weather_for(
        &self,
        member: &str,
        value: Option<&Value>,
    ) -> Result<(bool, Value), ApiError> {
        if !value.is_some_and(|v| v["mode"] == "automatic") {
            return self.recommendation_weather(value);
        }
        if value.unwrap().as_object().is_none_or(|m| m.len() != 1) {
            return Err(ApiError::new("invalid_input", "自动天气只提供mode"));
        }
        let mut doc = self.weather_execute("weather.get", json!({"member_id":member}))?;
        let refresh = doc["config"]["mode"] == "automatic"
            && (doc["cache"].is_null() || doc["cache"]["stale"] == true);
        if refresh {
            doc = self.weather_execute("weather.refresh", json!({"member_id":member}))?;
        }
        let mut context = doc["cache"].clone();
        if context.is_null() {
            context =
                json!({"mode":"automatic","source":"unavailable","stale":false,"obtained_at":null});
        } else {
            context["mode"] = json!("automatic");
        }
        context["last_error"] = doc["last_error"].clone();
        context["network_requested"] = json!(refresh);
        let cold = context["temperature_c"].as_f64().is_some_and(|t| t <= 15.0);
        Ok((cold, context))
    }
    pub(super) fn ai_enqueue(&self, mut input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        input["member_id"] = json!(member);
        let key = string(&input, "operation_key")?;
        // Validate caller controls before any garment facts leave the local application.
        crate::recommendations::generate_fields(&input)?;
        if key.is_empty()
            || key.len() > 200
            || string(&input, "timezone")?
                .parse::<chrono_tz::Tz>()
                .is_err()
            || input
                .get("limit")
                .is_some_and(|v| v.as_u64().is_none_or(|n| !(1..=5).contains(&n)))
        {
            return Err(ApiError::new("invalid_input", "推荐标识、数量或时区不可用"));
        }
        string(&input, "provider_id")?;
        let original = input.to_string();
        let cached=self.connection.query_row("SELECT request,document FROM recommendation_intents WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?;
        let intent_id = if let Some((request, doc)) = cached {
            if request != original {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "同一标识不能改变推荐约束",
                ));
            }
            let doc: Value = serde_json::from_str(&doc)
                .map_err(|_| ApiError::new("storage_error", "推荐意图损坏"))?;
            let exists:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE member_id=?1 AND kind='text_recommendation' AND operation_key=?2)",params![member,key],|row|row.get(0))?;
            if !exists {
                // This is the real immutable fact, not a fabricated successful task.
                let mut cached = doc;
                cached["intent_id"] = cached["id"].clone();
                return Ok(cached);
            }
            doc["id"].clone()
        } else {
            Value::Null
        };
        // No weather, catalog, credential or provider HTTP precedes this durable receipt.
        let job = self.job_enqueue(&member, "text_recommendation", key, input.clone())?;
        let mut receipt = text_receipt(&job);
        if intent_id.is_string() {
            receipt["intent_id"] = intent_id;
        }
        Ok(receipt)
    }
    pub(super) fn ai_worker(&self, input: Value) -> Result<Value, ApiError> {
        if input
            .as_object()
            .is_none_or(|m| m.keys().any(|k| !["member_id", "id"].contains(&k.as_str())))
        {
            return Err(ApiError::new("invalid_input", "worker只接受成员与任务ID"));
        }
        let member = self.member(&input)?;
        let id = string(&input, "id")?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        if job["kind"] != "text_recommendation" {
            return Err(ApiError::new("invalid_input", "不是文本推荐任务"));
        }
        if job["result"]["intent_id"].is_string()
            || ["succeeded", "failed", "cancelled", "waiting_input"]
                .contains(&job["state"].as_str().unwrap_or(""))
        {
            return Ok(job);
        }
        let now = chrono::Utc::now().timestamp_millis();
        if job["state"] == "running" && job["lease_expires"].as_i64().unwrap_or(0) > now {
            return Ok(job);
        }
        if job["submitted_at"].is_string()
            && !["rate_limited", "invalid_output"]
                .contains(&job["last_outcome"].as_str().unwrap_or(""))
        {
            let tx = self.begin_write()?;
            job = self.job_get(json!({"member_id":member,"id":id}))?;
            if job["state"] == "running"
                && job["lease_expires"].as_i64().unwrap_or(0)
                    > chrono::Utc::now().timestamp_millis()
            {
                return Ok(job);
            }
            if ["succeeded", "failed", "cancelled", "waiting_input"]
                .contains(&job["state"].as_str().unwrap_or(""))
                || job["result"]["intent_id"].is_string()
                || ["rate_limited", "invalid_output"]
                    .contains(&job["last_outcome"].as_str().unwrap_or(""))
            {
                return Ok(job);
            }
            job["state"] = json!("waiting_input");
            job["lease_token"] = Value::Null;
            job["lease_expires"] = json!(0);
            job["error"] = json!({"code":"provider_outcome_unknown","message":"结果未知，无法查询；请核对供应商或显式选择离线规则","retryable":false});
            let pending = self.save_job(&tx, &job)?;
            tx.commit()?;
            self.emit_job_event(pending);
            return Ok(job);
        }
        if job["next_due"].as_i64().unwrap_or(0) > now {
            return Ok(job);
        }
        let claimed = self.job_claim(&member, id, 300)?;
        let lease = claimed["lease_token"].as_str().unwrap();
        match self.ai_recommendation(claimed["payload"].clone(), &claimed){
            Ok(doc)=>self.job_finish(&member,id,lease,if doc["generation"]["provider_outcome"]=="unknown"{"waiting_input"}else{"succeeded"},json!({"intent_id":doc["id"]}),if doc["generation"]["provider_outcome"]=="unknown"{json!({"code":"provider_outcome_unknown","message":"请求结果未知；已提供离线建议，不盲重发","retryable":false})}else{Value::Null}),
            Err(error)=>{
                if error.code=="rate_limit_deferred" {return self.job_get(json!({"member_id":member,"id":id}));}
                let finish=self.job_finish(&member,id,lease,"failed",Value::Null,json!({"code":error.code,"message":error.message,"retryable":false}));
                if error.code=="lease_lost" {return self.job_get(json!({"member_id":member,"id":id}));}
                finish
            }
        }
    }
    fn ai_recommendation(&self, mut input: Value, claimed: &Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        input["member_id"] = json!(member);
        let key = string(&input, "operation_key")?;
        let original = input.to_string();
        if let Some((request,doc))=self.connection.query_row("SELECT request,document FROM recommendation_intents WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?{if request!=original{return Err(ApiError::new("idempotency_conflict","同一标识不能改变推荐约束"));}return serde_json::from_str(&doc).map_err(|_|ApiError::new("storage_error","推荐意图损坏"));}
        let job_id = claimed["id"].as_str().unwrap().to_owned();
        let lease = claimed["lease_token"].as_str().unwrap().to_owned();
        let provider_id = string(&input, "provider_id")?.to_owned();
        let weather = self.recommendation_weather_for(&member, input.get("weather"))?;
        let mut catalog = vec![];
        let availability = self.care_execute("care.availability", json!({"member_id":member}))?;
        for field in [
            "fixed_ids",
            "excluded_ids",
            "inventory_priority_ids",
            "retained_ids",
            "action_excluded_ids",
        ] {
            for id in crate::recommendations::ids(&input, field)? {
                self.clothing_get(json!({"member_id":member,"id":id}))?;
            }
        }
        let mut fixed = crate::recommendations::ids(&input, "fixed_ids")?;
        fixed.extend(crate::recommendations::ids(&input, "retained_ids")?);
        let mut excluded = crate::recommendations::ids(&input, "excluded_ids")?;
        excluded.extend(crate::recommendations::ids(&input, "action_excluded_ids")?);
        if fixed.iter().any(|id| {
            excluded.contains(id)
                || !availability["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s["clothing_id"] == *id && s["available"] == true)
        }) {
            return Err(ApiError::new(
                "constraint_conflict",
                "固定单品不可用或同时被排除",
            ));
        }
        if input.get("source_intent").is_some() {
            self.recommendation_execute(
                "recommendation.get",
                json!({"member_id":member,"id":string(&input,"source_intent")?}),
            )?;
        }
        let mut meta = json!({"source":"rule_fallback","provider_id":provider_id,"attempts":0,"failure_codes":[],"job_id":job_id,"_lease_token":lease});
        let mut candidate_input = input.clone();
        candidate_input["limit"] = json!(5);
        let candidate = self.recommendation_build(
            "recommendation.generate",
            candidate_input,
            None,
            None,
            Some(&weather),
            false,
        )?;
        let candidate_outfits:Vec<Value>=candidate["proposals"].as_array().unwrap().iter().map(|p|json!({"items":p["items"].as_array().unwrap().iter().map(|i|json!({"clothing_id":i["clothing_id"],"role":i["role"],"layer":i["layer"]})).collect::<Vec<_>>(),"reasons":p["reasons"]})).collect();
        if candidate_outfits.is_empty() {
            meta["failure_codes"] = json!(["no_valid_candidates"]);
            return self.recommendation_with_generation(
                "recommendation.generate",
                input,
                None,
                Some(&meta),
                Some(&weather),
            );
        }
        let selected: std::collections::BTreeSet<&str> = candidate_outfits
            .iter()
            .flat_map(|p| {
                p["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| i["clothing_id"].as_str().unwrap())
            })
            .collect();
        for id in selected {
            let c = self.clothing_get(json!({"member_id":member,"id":id}))?;
            catalog.push(json!({"clothing_id":c["id"],"name":c["name"],"category":c["category"],"occasion":c["occasion"],"favorite":c["favorite"]}));
        }
        let mut model_controls = json!({});
        for field in [
            "limit",
            "fixed_ids",
            "excluded_ids",
            "inventory_priority_ids",
            "avoid_recent",
            "target",
            "occasion",
            "retained_ids",
            "action_excluded_ids",
            "replacement_category",
            "retained_items",
        ] {
            if let Some(v) = input.get(field) {
                model_controls[field] = v.clone();
            }
        }
        if model_controls.get("limit").is_none() {
            model_controls["limit"] = json!(3);
        }
        let mut prompt = json!({"task":"Organize the validated candidate_outfits into distinct recommendations, preserving every item ID/role/layer exactly. You may add an explanation grounded only in known facts. Return JSON only: {\"proposals\":[{\"items\":[{\"clothing_id\":\"exact ID\",\"role\":\"top\",\"layer\":0}]}]}. Required body pattern top+bottom+shoes OR dress/one_piece role body+shoes. outerwear role outerwear layer1, underwear layer-2, others category role layer0. Fixed IDs must appear, excluded IDs never appear. When weather.temperature_c is 15 or below, you MUST add an outerwear item (role outerwear, layer1). Unknown sportswear does not cover body. Respect retained_items role/layer exactly. Do not invent facts. Produce requested limit, fewer only if insufficient distinct sets.","controls":model_controls,"weather":weather.1,"inventory":catalog,"candidate_outfits":candidate_outfits,"json_output":input["json_output"],"output_format":input["output_format"]});
        if prompt.to_string().len() > 65536 {
            meta["failure_codes"] = json!(["model_context_limit"]);
            return self.recommendation_with_generation(
                "recommendation.generate",
                input,
                None,
                Some(&meta),
                Some(&weather),
            );
        }
        let first_attempt = claimed["output_attempt"].as_u64().unwrap_or(1)
            + u64::from(claimed["last_outcome"] == "invalid_output");
        meta["attempts"] = claimed["output_attempt"].clone();
        for attempt in first_attempt..=2 {
            meta["attempts"] = json!(attempt);
            match self.text_recommendation_request(
                &provider_id,
                &prompt,
                &member,
                claimed,
                &lease,
                attempt,
            ) {
                Ok(draft) => {
                    let mut success = meta.clone();
                    success["source"] = json!("text_model");
                    let receipt = self.job_get(json!({"member_id":member,"id":job_id}))?;
                    success["provider_requests"] = receipt["submission_count"].clone();
                    success["rate_retries"] = json!(receipt["rate_retries"].as_u64().unwrap_or(0));
                    success["usage"] = receipt["usage"].clone();
                    match self.recommendation_with_generation(
                        "recommendation.generate",
                        input.clone(),
                        Some(&draft),
                        Some(&success),
                        Some(&weather),
                    ) {
                        Ok(doc) => return Ok(doc),
                        Err(error) if error.code == "model_invalid_output" => {
                            self.text_outcome(&member, &job_id, &lease, "invalid_output")?;
                            prompt["retry_feedback"]=json!("Previous result failed current inventory/body coverage/fixed/excluded/role/recent checks. Recheck all constraints including cold-weather outerwear.");
                            meta["failure_codes"]
                                .as_array_mut()
                                .unwrap()
                                .push(json!(error.code));
                        }
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => {
                    if [
                        "rate_limit_deferred",
                        "lease_lost",
                        "submission_already_sent",
                    ]
                    .contains(&error.code.as_str())
                    {
                        return Err(error);
                    }
                    let invalid = error.code == "model_invalid_output";
                    if invalid {
                        self.text_outcome(&member, &job_id, &lease, "invalid_output")?;
                    }
                    if ["provider_network", "provider_response"].contains(&error.code.as_str()) {
                        meta["provider_outcome"] = json!("unknown");
                    }
                    meta["failure_codes"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!(error.code));
                    if !invalid {
                        break;
                    }
                }
            }
        }
        let receipt = self.job_get(json!({"member_id":member,"id":job_id}))?;
        meta["provider_requests"] = receipt["submission_count"].clone();
        meta["rate_retries"] = json!(receipt["rate_retries"].as_u64().unwrap_or(0));
        meta["usage"] = receipt["usage"].clone();
        let result = self.recommendation_with_generation(
            "recommendation.generate",
            input,
            None,
            Some(&meta),
            Some(&weather),
        )?;
        Ok(result)
    }
    fn text_recommendation_request(
        &self,
        id: &str,
        prompt: &Value,
        member: &str,
        claimed: &Value,
        lease: &str,
        attempt: u64,
    ) -> Result<Value, ApiError> {
        let job = claimed["id"].as_str().unwrap_or("");
        let p = self.provider_get(id)?;
        if p["adapter"] != "openai_compatible"
            || !p["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("text"))
        {
            return Err(ApiError::new(
                "unsupported_capability",
                "需要配置兼容聊天接口的文本模型",
            ));
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ApiError::new("provider_network", "模型连接不可用"))?;
        let mut body = json!({"model":p["model"],"messages":[{"role":"system","content":"You are a wardrobe planner. Follow exact physical IDs and return only the requested JSON object."},{"role":"user","content":prompt.to_string()}],"max_tokens":2048,"temperature":0});
        let format = prompt["output_format"]
            .as_str()
            .unwrap_or(if prompt["json_output"] == true {
                "json_object"
            } else {
                "plain"
            });
        match format {
            "plain" => {}
            "json_object" => body["response_format"] = json!({"type":"json_object"}),
            "llama_cpp_schema" => {
                let choices: Vec<Value> = prompt["candidate_outfits"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["items"].clone())
                    .collect();
                let count = choices
                    .len()
                    .min(prompt["controls"]["limit"].as_u64().unwrap_or(3) as usize);
                let schema = json!({"type":"object","required":["proposals"],"additionalProperties":false,"properties":{"proposals":{"type":"array","minItems":count,"maxItems":count,"items":{"type":"object","required":["items","explanation"],"additionalProperties":false,"properties":{"items":{"enum":choices},"explanation":{"type":"string","maxLength":240}}}}}});
                body["response_format"] = json!({"type":"json_object","schema":schema});
            }
            _ => return Err(ApiError::new("invalid_input", "文本输出格式不可用")),
        }

        let mut request = client
            .post(format!(
                "{}/chat/completions",
                p["endpoint"].as_str().unwrap()
            ))
            .json(&body);
        if p["authentication"] == "bearer" {
            let key = crate::providers::credential_entry_for(&p)?
                .get_password()
                .map_err(|_| ApiError::new("credential_missing", "文本模型凭据不可用"))?;
            request = request.bearer_auth(key);
        }
        // 供应商请求事件（#44）：trace 取自任务文档（claimed 已携带，不再回查任务表）；
        // 限流重试的每次发送各记一条
        let provider_request =
            crate::diagnostics::ProviderRequestLog::from_job(claimed, p["model"].as_str().unwrap_or(""));
        let response = loop {
            self.text_submitted(member, job, lease, attempt)?;
            provider_request.begin_send();
            let response = match request
                .try_clone()
                .ok_or_else(|| ApiError::new("provider_error", "无法建立模型请求"))?
                .send()
            {
                Ok(response) => response,
                Err(_) => {
                    provider_request.record(self,"chat_completions","outcome_unknown",None);
                    return Err(ApiError::new("provider_network", "模型连接失败；结果未知，不盲重发"));
                }
            };
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                provider_request.record(self,"chat_completions","http_error",Some(response.status().as_u16()));
                let delay = retry_after_ms(
                    response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                    chrono::Utc::now(),
                );
                if !self.text_rate_limited(member, job, lease, delay)? {
                    return Err(ApiError::new(
                        "provider_rate_limit",
                        "供应商限流，已达到最多两次自动重试",
                    ));
                }
                if delay > 30_000 {
                    self.text_defer_rate_limit(member, job, lease)?;
                    return Err(ApiError::new(
                        "rate_limit_deferred",
                        "限流已按Retry-After排队，可到期运行worker",
                    ));
                }
                loop {
                    let current = self.job_get(json!({"member_id":member,"id":job}))?;
                    text_lease(&current, lease)?;
                    let remaining = current["next_due"].as_i64().unwrap_or(0)
                        - chrono::Utc::now().timestamp_millis();
                    if remaining <= 0 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(remaining.min(50) as u64));
                }
                continue;
            }
            if !response.status().is_success() {
                provider_request.record(self,"chat_completions","http_error",Some(response.status().as_u16()));
                return Err(ApiError::new(
                    "provider_error",
                    "模型服务返回错误，已回退规则推荐",
                ));
            }
            provider_request.record(self,"chat_completions","ok",None);
            break response;
        };
        let mut bytes = vec![];
        response
            .take(1048577)
            .read_to_end(&mut bytes)
            .map_err(|_| ApiError::new("provider_response", "模型响应读取失败"))?;
        if bytes.len() > 1048576 {
            return Err(ApiError::new("provider_response", "模型响应过大"));
        }
        let response: Value = serde_json::from_slice(&bytes)
            .map_err(|_| ApiError::new("provider_response", "模型响应不是JSON"))?;
        self.text_usage(member, job, lease, &response)?;
        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| ApiError::new("model_invalid_output", "模型缺少文本"))?;
        serde_json::from_str(content)
            .map_err(|_| ApiError::new("model_invalid_output", "模型文本不是指定JSON"))
    }
    fn text_submitted(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        attempt: u64,
    ) -> Result<(), ApiError> {
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        text_lease(&job, lease)?;
        if job["submitted_at"].is_string() {
            let invalid = job["last_outcome"] == "invalid_output"
                && attempt == job["output_attempt"].as_u64().unwrap_or(1) + 1
                && attempt <= 2;
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
        let pending = self.save_job(&tx, &job)?;
        tx.commit()?;
        self.emit_job_event(pending);
        Ok(())
    }
    fn text_usage(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        response: &Value,
    ) -> Result<(), ApiError> {
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        text_lease(&job, lease)?;
        for key in ["prompt_tokens", "completion_tokens", "total_tokens"] {
            if let Some(n) = response["usage"][key].as_u64() {
                if !job["usage"].is_object() {
                    job["usage"] = json!({});
                }
                job["usage"][key] =
                    json!(job["usage"][key].as_u64().unwrap_or(0).saturating_add(n));
            }
        }
        if response["id"].as_str().is_some_and(|v| v.len() <= 200) {
            job["provider_request_id"] = response["id"].clone();
        }
        let pending = self.save_job(&tx, &job)?;
        tx.commit()?;
        self.emit_job_event(pending);
        Ok(())
    }
    fn text_rate_limited(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        delay: i64,
    ) -> Result<bool, ApiError> {
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        text_lease(&job, lease)?;
        job["last_outcome"] = json!("rate_limited");
        job["next_due"] = json!(chrono::Utc::now().timestamp_millis().saturating_add(delay));
        job["error"] = json!({"code":"provider_rate_limit","message":"供应商限流，遵守Retry-After后重试","retryable":true});
        let retry = job["rate_retries"].as_u64().unwrap_or(0) < 2;
        let pending = self.save_job(&tx, &job)?;
        tx.commit()?;
        self.emit_job_event(pending);
        Ok(retry)
    }
    fn text_defer_rate_limit(&self, member: &str, id: &str, lease: &str) -> Result<(), ApiError> {
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        text_lease(&job, lease)?;
        job["state"] = json!("queued");
        job["lease_token"] = Value::Null;
        job["lease_expires"] = json!(0);
        let pending = self.save_job(&tx, &job)?;
        tx.commit()?;
        self.emit_job_event(pending);
        Ok(())
    }
    fn text_outcome(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        outcome: &str,
    ) -> Result<(), ApiError> {
        let tx = self.begin_write()?;
        let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
        text_lease(&job, lease)?;
        job["last_outcome"] = json!(outcome);
        let pending = self.save_job(&tx, &job)?;
        tx.commit()?;
        self.emit_job_event(pending);
        Ok(())
    }
}

fn retry_after_ms(header: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> i64 {
    if let Some(value) = header {
        if let Ok(seconds) = value.trim().parse::<u64>() {
            return seconds.saturating_mul(1000).min(i64::MAX as u64) as i64;
        }
        if let Ok(date) = chrono::DateTime::parse_from_rfc2822(value) {
            return date
                .timestamp_millis()
                .saturating_sub(now.timestamp_millis())
                .max(0);
        }
    }
    1000
}

fn text_receipt(job: &Value) -> Value {
    json!({"schema_version":1,"member_id":job["member_id"],"job_id":job["id"],"state":job["state"],"progress":job["progress"],"intent_id":job["result"]["intent_id"],"next_due":job["next_due"],"error":job["error"]})
}

pub(super) fn text_lease(job: &Value, lease: &str) -> Result<(), ApiError> {
    if job["state"] != "running"
        || job["lease_token"] != lease
        || job["lease_expires"].as_i64().unwrap_or(0) <= chrono::Utc::now().timestamp_millis()
    {
        return Err(ApiError::new(
            "lease_lost",
            "文本任务已取消或处理权已失效，不采用迟到结果",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod publication_boundary {
    use super::*;
    #[test]
    fn published_intent_survives_cancel_between_commit_and_job_finish_and_retry_is_cached() {
        let d = tempfile::tempdir().unwrap();
        let app = Application::open(&d.path().join("db")).unwrap();
        let member = app.member(&json!({"member_id":"default"})).unwrap();
        for category in ["top", "bottom", "shoes"] {
            app.execute(
                "clothing.create",
                json!({"member_id":&member,"name":category,"category":category}),
            )
            .unwrap();
        }
        let input = json!({"member_id":member,"operation_key":"publication-window","timezone":"UTC","limit":1,"strategy":"ai","provider_id":"unused-after-cache"});
        let job = app
            .job_enqueue(
                &member,
                "text_recommendation",
                "publication-window",
                input.clone(),
            )
            .unwrap();
        let id = job["id"].as_str().unwrap();
        let claimed = app.job_claim(&member, id, 300).unwrap();
        let lease = claimed["lease_token"].as_str().unwrap();
        let meta =
            json!({"source":"rule_fallback","job_id":id,"_lease_token":lease,"failure_codes":[]});
        // Exercise the narrow post-publication seam directly, without a timing-sensitive sleep.
        let published = app
            .recommendation_with_generation(
                "recommendation.generate",
                input.clone(),
                None,
                Some(&meta),
                None,
            )
            .unwrap();
        app.execute("jobs.cancel", json!({"member_id":&member,"id":id}))
            .unwrap();
        assert_eq!(
            app.job_finish(
                &member,
                id,
                lease,
                "succeeded",
                json!({"intent_id":published["id"]}),
                Value::Null
            )
            .unwrap_err()
            .code,
            "lease_lost"
        );
        // The provider ID does not exist: reaching network/provider lookup would fail this call.
        assert_eq!(
            app.execute("recommendation.generate", input).unwrap()["intent_id"],
            published["id"]
        );
        assert!(published["generation"].get("_lease_token").is_none());
    }
}
