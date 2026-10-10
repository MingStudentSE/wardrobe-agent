//! 购买适配分析（issue #24，父 #1 F17 购买场景）。
//! 候选商品草稿存放在独立的候选域：不占用衣物身份、不入库存/穿着/洞察/推荐，
//! 也从不进入 outbox（不同步、不进便携备份）。分析只读真实库存并在服务端
//! 确定性计算；候选与已有衣物的区分在数据层结构性成立。
use crate::{jobs, string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS purchase_candidates(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),document TEXT NOT NULL); CREATE INDEX IF NOT EXISTS purchase_candidate_member ON purchase_candidates(member_id,id);
         CREATE TABLE IF NOT EXISTS purchase_analyses(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),candidate_id TEXT NOT NULL REFERENCES purchase_candidates(id),document TEXT NOT NULL); CREATE INDEX IF NOT EXISTS purchase_analysis_member ON purchase_analyses(member_id,id);",
    )?;
    Ok(())
}

fn invalid(message: &str) -> ApiError {
    ApiError::new("invalid_input", message)
}
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .ok_or_else(|| invalid("输入须为JSON对象"))?
        .keys()
        .any(|key| !allowed.contains(&key.as_str()))
    {
        return Err(invalid("操作包含不支持的字段"));
    }
    Ok(())
}
fn parse_stored(raw: String, what: &str) -> Result<Value, ApiError> {
    serde_json::from_str(&raw).map_err(|_| ApiError::new("storage_error", what))
}

/// 列表读取上限：与 scenario.list 的 PLAN_LIMIT 同一上限常量，
/// 候选/分析绝不无限回包。
const LIST_LIMIT: usize = 50;

/// 候选可人工填写的可观察事实与 clothing 事实同字典：纠正走同一校验，
/// 不出现第二套事实语义。价格/尺码等未知字段保持 null，绝不编造。
const CANDIDATE_FACT_FIELDS: &[&str] = &[
    "name",
    "category",
    "sub_category",
    "brand",
    "size_label",
    "fit",
    "description",
    "notes",
    "purchase_url",
    "color_description",
    "color",
    "pattern",
    "material",
    "style",
    "occasion",
    "tags",
    "seasons",
    "favorite",
    "purchase_price_minor",
    "currency",
];

impl Application {
    pub(super) fn purchase_execute(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        match op {
            "purchase.candidate.create" => self.purchase_candidate_create(input),
            "purchase.candidate.recognize" => self.purchase_candidate_recognize(input),
            "purchase.candidate.process" => self.purchase_candidate_process(input),
            "purchase.candidate.get" => self.purchase_candidate_get(input),
            "purchase.candidate.list" => self.purchase_candidate_list(input),
            "purchase.candidate.update" => self.purchase_candidate_update(input),
            "purchase.candidate.dismiss" => self.purchase_candidate_dismiss(input),
            "purchase.analysis.run" => self.purchase_analysis_run(input),
            "purchase.analysis.get" => self.purchase_analysis_get(input),
            "purchase.analysis.list" => self.purchase_analysis_list(input),
            _ => Err(ApiError::new("unsupported_operation", "购买候选操作不可用")),
        }
    }

    fn purchase_candidate(&self, member: &str, id: &str) -> Result<Value, ApiError> {
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM purchase_candidates WHERE member_id=?1 AND id=?2",
                params![member, id],
                |r| r.get(0),
            )
            .optional()?;
        parse_stored(
            stored.ok_or_else(|| ApiError::new("not_found", "购买候选不存在"))?,
            "购买候选数据损坏",
        )
    }

    /// 手工创建候选草稿：候选域独立身份（独立 UUIDv7，domain=purchase_candidate），
    /// 不写 clothing/audit/outbox。未知事实保持 null/空数组。
    fn purchase_candidate_create(&self, input: Value) -> Result<Value, ApiError> {
        let mut allowed = vec!["member_id", "asset_ids"];
        allowed.extend(CANDIDATE_FACT_FIELDS.iter().copied());
        fields(&input, &allowed)?;
        let member = self.member(&input)?;
        let mut facts_input = serde_json::Map::new();
        for (key, value) in input.as_object().unwrap() {
            if key != "member_id" && key != "asset_ids" {
                facts_input.insert(key.clone(), value.clone());
            }
        }
        let mut facts = json!({});
        crate::clothing::apply_facts(&mut facts, &Value::Object(facts_input))?;
        if facts["name"].as_str().unwrap_or("").trim().is_empty() {
            return Err(invalid("候选商品须有1–120字符名称"));
        }
        if facts["category"].as_str().unwrap_or("").is_empty() {
            return Err(invalid("候选商品须选择类别"));
        }
        let asset_ids = input["asset_ids"].clone();
        let asset_ids = validated_asset_ids(self, &member, &asset_ids)?;
        let now = chrono::Utc::now().to_rfc3339();
        let doc = json!({
            "schema_version":1,
            "id":uuid::Uuid::now_v7().to_string(),
            "member_id":member,
            "domain":"purchase_candidate",
            "state":"draft",
            "facts":facts,
            "provenance":{"source":"manual","suggested_facts":null,"uncertainties":null,"corrected_facts":null,"recognition":null,"corrected_at":null},
            "asset_ids":asset_ids,
            "created_at":now,
            "updated_at":now,
        });
        let tx = self.begin_write()?;
        tx.execute(
            "INSERT INTO purchase_candidates VALUES(?1,?2,?3)",
            params![doc["id"].as_str(), member, doc.to_string()],
        )?;
        tx.commit()?;
        Ok(doc)
    }

    /// 候选商品图识别：复用识别管线的收据先行与图片快照机制，任务与建议
    /// 全部落在候选域。未明确确认上传前不发送任何图片。
    fn purchase_candidate_recognize(&self, input: Value) -> Result<Value, ApiError> {
        fields(
            &input,
            &[
                "member_id",
                "operation_key",
                "candidate_id",
                "provider_id",
                "asset_ids",
                "upload_role",
                "confirm_upload",
            ],
        )?;
        let member = self.member(&input)?;
        // 与 recognition.create 相同：成员别名先规范化，同键重放比较的是
        // 规范化后的请求，"default" 与 canonical UUID 视为同一意图。
        let mut input = input;
        input["member_id"] = json!(member);
        let key = string(&input, "operation_key")?;
        if key.trim().is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        if input["confirm_upload"] != true {
            return Err(ApiError::new(
                "upload_confirmation_required",
                "请明确确认供应商与发送图片",
            ));
        }
        let role = string(&input, "upload_role")?;
        if !["original", "product_display"].contains(&role) {
            return Err(invalid("请选择原图或商品展示图上传"));
        }
        let candidate = self.purchase_candidate(&member, string(&input, "candidate_id")?)?;
        if candidate["state"] != "draft" {
            return Err(invalid("仅草稿候选可提交识别"));
        }
        let tx = self.begin_write()?;
        let existing: Option<String> = tx.query_row(
            "SELECT document FROM jobs WHERE member_id=?1 AND kind='purchase_recognition' AND operation_key=?2",
            params![member, key],
            |r| r.get(0),
        ).optional()?;
        if let Some(existing) = existing {
            let job: Value = parse_stored(existing, "识别任务记录损坏")?;
            if job["payload"]["request"] != input {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "识别标识不能替换发送意图",
                ));
            }
            return Ok(job);
        }
        let provider = self.provider_get(string(&input, "provider_id")?)?;
        if !provider["capabilities"]
            .as_array()
            .is_some_and(|c| c.contains(&json!("vision")))
        {
            return Err(ApiError::new(
                "unsupported_capability",
                "所选模型未声明视觉能力",
            ));
        }
        let ids = input["asset_ids"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 20)
            .ok_or_else(|| invalid("请选择1–20张图片"))?;
        let mut seen = BTreeSet::new();
        let mut assets = vec![];
        for id in ids {
            let id = id.as_str().ok_or_else(|| invalid("图片ID须为字符串"))?;
            if !seen.insert(id) {
                return Err(invalid("图片不能重复"));
            }
            let asset = self.asset_get(json!({"member_id":member,"id":id}))?;
            let image = &asset[if role == "original" {
                "original"
            } else {
                "display"
            }];
            assets.push(json!({"asset_id":id,"role":role,"blob_hash":image["id"],"mime":image["mime"],"width":image["width"],"height":image["height"],"bytes":image["bytes"]}));
        }
        if assets
            .iter()
            .map(|a| a["bytes"].as_u64().unwrap_or(0))
            .sum::<u64>()
            > 20 * 1024 * 1024
        {
            return Err(ApiError::new(
                "purchase_images_too_large",
                "识别所选图片合计最多20MiB，请减少所选图片",
            ));
        }
        let payload = json!({
            "request":input,
            "candidate_id":candidate["id"],
            "provider":provider,
            "asset_ids":ids,
            "upload_role":role,
            "assets":assets,
            "baseline":candidate,
            "submission":"not_submitted",
        });
        let (job, pending) = self.insert_job(&tx, &member, "purchase_recognition", key, payload)?;
        tx.commit()?;
        self.emit_job_event(pending);
        Ok(job)
    }

    /// 识别 worker：与识别管线同等的提交安全（提交未知不盲重发、图片校验、
    /// 租约边界落账）。产出属性建议写入候选的 provenance，候选事实不被
    /// 自动覆盖；人工纠错经 purchase.candidate.update 显式发生。
    fn purchase_candidate_process(&self, input: Value) -> Result<Value, ApiError> {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        use std::io::Read;
        fields(&input, &["member_id", "id"])?;
        let member = self.member(&input)?;
        let id = string(&input, "id")?;
        let original = self.job_get(json!({"member_id":member,"id":id}))?;
        if original["kind"] != "purchase_recognition" {
            return Err(invalid("不是购买候选识别任务"));
        }
        if original["state"] == "succeeded" {
            return Ok(original);
        }
        let mut job = self.job_claim(&member, id, 120)?;
        let lease = job["lease_token"].as_str().unwrap().to_string();
        if job["payload"]["submission"] != "not_submitted" {
            return self.job_finish(&member, id, &lease, "waiting_input", Value::Null, json!({"code":"submission_unknown","message":"上次提交结果未知，请核实供应商后另建明确任务","retryable":false}));
        }
        let mut transferred = vec![];
        let prepare = (|| -> Result<reqwest::blocking::RequestBuilder, ApiError> {
            let p = &job["payload"]["provider"];
            if self.provider_get(p["id"].as_str().unwrap())? != *p {
                return Err(ApiError::new(
                    "provider_changed",
                    "供应商配置已变更，请重新确认",
                ));
            }
            let mut content = vec![json!({"type":"text","text":CANDIDATE_PROMPT})];
            for snap in job["payload"]["assets"].as_array().unwrap() {
                let asset = self.asset_get(json!({"member_id":member,"id":snap["asset_id"]}))?;
                let image = &asset[if snap["role"] == "original" {
                    "original"
                } else {
                    "display"
                }];
                if image["id"] != snap["blob_hash"] {
                    return Err(ApiError::new("asset_changed", "所选图片已改变，请重新确认"));
                }
                let bytes = std::fs::read(image["local_path"].as_str().unwrap()).map_err(|_| {
                    ApiError::new("image_unavailable", "原图或展示图不可读，仍可手工填写候选")
                })?;
                if Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
                    != snap["blob_hash"].as_str().unwrap()
                {
                    return Err(ApiError::new("asset_changed", "图片内容校验失败"));
                }
                let decoded = crate::assets::decode(&bytes)?;
                let normalized = decoded.resize(512, 512, image::imageops::FilterType::Lanczos3);
                let mut encoded = std::io::Cursor::new(Vec::new());
                normalized
                    .write_to(&mut encoded, image::ImageFormat::Png)
                    .map_err(|_| invalid("模型传输图片编码失败"))?;
                let encoded = encoded.into_inner();
                transferred.push(json!({"asset_id":snap["asset_id"],"source_role":snap["role"],"source_blob_hash":snap["blob_hash"],"sha256":Sha256::digest(&encoded).iter().map(|b|format!("{b:02x}")).collect::<String>(),"mime":"image/png","width":normalized.width(),"height":normalized.height(),"bytes":encoded.len(),"transform":"orientation-aware-fit-512-png"}));
                content.push(json!({"type":"image_url","image_url":{"url":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(encoded))}}));
            }
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| ApiError::new("connection_error", "无法创建HTTP客户端"))?;
            let mut request = client
                .post(format!(
                    "{}/chat/completions",
                    p["endpoint"].as_str().unwrap()
                ))
                .json(&json!({"model":p["model"],"messages":[{"role":"user","content":content}],"max_tokens":512,"temperature":0}));
            if p["authentication"] == "bearer" {
                let key = crate::providers::credential_entry_for(p)?
                    .get_password()
                    .map_err(|_| ApiError::new("credential_missing", "请配置系统安全凭据"))?;
                request = request.bearer_auth(key);
            }
            Ok(request)
        })();
        let request = match prepare {
            Ok(r) => r,
            Err(e) => {
                return self.job_finish(
                    &member,
                    id,
                    &lease,
                    "failed",
                    Value::Null,
                    json!({"code":e.code,"message":e.message,"retryable":false}),
                )
            }
        };
        job["payload"]["submission"] = json!("submitted");
        job["payload"]["transferred_images"] = json!(transferred);
        let tx = self.begin_write()?;
        let current = self.job_get(json!({"member_id":member,"id":id}))?;
        if current["state"] != "running"
            || current["lease_token"] != lease
            || current["lease_expires"].as_i64().unwrap_or(0)
                <= chrono::Utc::now().timestamp_millis()
        {
            return Err(ApiError::new(
                "lease_lost",
                "任务已取消或租约到期，不再发送图片",
            ));
        }
        let pending = self.save_job(&tx, &job)?;
        tx.commit()?;
        self.emit_job_event(pending);
        // 供应商请求事件（#44）：trace 取自任务文档——worker 可能运行在另一进程
        let provider_request = crate::diagnostics::ProviderRequestLog::from_job(&job, "");
        provider_request.begin_send();
        let response = match request.send() {
            Ok(r) => r,
            Err(_) => {
                provider_request.record(self, "image_submission", "outcome_unknown", None);
                return self.job_finish(&member, id, &lease, "waiting_input", Value::Null, json!({"code":"submission_unknown","message":"提交后的连接失败，结果未知；不会自动再次上传","retryable":false}));
            }
        };
        let status = response.status();
        if status.is_server_error() {
            provider_request.record(
                self,
                "image_submission",
                "outcome_unknown",
                Some(status.as_u16()),
            );
            return self.job_finish(&member, id, &lease, "waiting_input", Value::Null, json!({"code":"submission_unknown","http_status":status.as_u16(),"message":"供应商服务错误，是否接受提交待核实","retryable":false}));
        }
        if !status.is_success() {
            provider_request.record(
                self,
                "image_submission",
                "http_error",
                Some(status.as_u16()),
            );
            let retry = status.as_u16() == 429;
            let delay = response
                .headers()
                .get("retry-after")
                .and_then(|s| s.to_str().ok())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(2)
                .saturating_mul(1000);
            job["payload"]["submission"] = json!("not_submitted");
            let tx = self.begin_write()?;
            let current = self.job_get(json!({"member_id":member,"id":id}))?;
            if current["state"] != "running"
                || current["lease_token"] != lease
                || current["lease_expires"].as_i64().unwrap_or(0)
                    < chrono::Utc::now().timestamp_millis()
            {
                return Err(ApiError::new("lease_lost", "任务已取消或过期，不恢复队列"));
            }
            let pending = self.save_job(&tx, &job)?;
            tx.commit()?;
            self.emit_job_event(pending);
            return self.job_finish(&member, id, &lease, "failed", Value::Null, json!({"code":if retry{"provider_rate_limit"}else{"provider_rejected"},"message":"供应商拒绝请求，候选仍可手工填写","http_status":status.as_u16(),"retryable":retry,"retry_after_ms":delay}));
        }
        let mut bytes = vec![];
        if response
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() > 1024 * 1024
        {
            return self.job_finish(&member, id, &lease, "waiting_input", Value::Null, json!({"code":"submission_unknown","message":"响应中断或过大，提交结果须核实","retryable":false}));
        }
        let response: Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => {
                return self.job_finish(&member, id, &lease, "waiting_input", Value::Null, json!({"code":"submission_unknown","message":"响应无效，提交结果须核实","retryable":false}))
            }
        };
        let text = match response["choices"][0]["message"]["content"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && s.chars().count() <= 10000)
        {
            Some(t) => t,
            None => {
                return self.job_finish(&member, id, &lease, "failed", Value::Null, json!({"code":"invalid_model_output","message":"模型未返回可审核建议，候选仍可手工填写","retryable":false}))
            }
        };
        let suggested = crate::recognition::normalized_candidates(text);
        let tx = self.begin_write()?;
        let current = self.job_get(json!({"member_id":member,"id":id}))?;
        if current["state"] != "running"
            || current["lease_token"] != lease
            || current["lease_expires"].as_i64().unwrap_or(0)
                < chrono::Utc::now().timestamp_millis()
        {
            return Err(ApiError::new("lease_lost", "任务已取消或过期，不发布建议"));
        }
        let mut candidate =
            self.purchase_candidate(&member, job["payload"]["candidate_id"].as_str().unwrap())?;
        // 明确为该候选选择的识别图片并入候选图片引用（并集去重）。
        let mut asset_ids: Vec<Value> = candidate["asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for snap in job["payload"]["assets"].as_array().unwrap() {
            if !asset_ids.contains(&snap["asset_id"]) {
                asset_ids.push(snap["asset_id"].clone());
            }
        }
        candidate["asset_ids"] = json!(asset_ids);
        candidate["provenance"]["source"] = json!("vision");
        candidate["provenance"]["suggested_facts"] = suggested;
        candidate["provenance"]["uncertainties"] = json!({"model_self_reported":text,"notice":"视觉建议未经核实；品牌、材质、尺码、价格和不可见字段不能确定，未填写的候选事实保持未知。模型自报描述不是准确率。"});
        candidate["provenance"]["recognition"] = json!({
            "job_id":id,
            "provider_id":job["payload"]["provider"]["id"],
            "provider_name":job["payload"]["provider"]["name"],
            "model":job["payload"]["provider"]["model"],
            "response_model":response["model"],
            "assets":job["payload"]["assets"],
            "transferred_images":job["payload"]["transferred_images"],
            "prompt_version":CANDIDATE_PROMPT_VERSION,
            "normalization":"conservative-taxonomy-word-candidates-v1",
            "usage":response["usage"],
            "suggested_at":chrono::Utc::now().to_rfc3339(),
        });
        candidate["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE purchase_candidates SET document=?1 WHERE id=?2 AND member_id=?3",
            params![candidate.to_string(), candidate["id"].as_str(), member],
        )?;
        job["state"] = json!("succeeded");
        job["progress"] = json!(100);
        job["lease_token"] = Value::Null;
        job["lease_expires"] = json!(0);
        job["payload"]["submission"] = json!("received");
        job["result"] = json!({"candidate_id":candidate["id"]});
        job["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        // 响应完整可解析：HTTP 层成功（后续内容校验失败属任务失败，非请求层事件）
        provider_request.record(self, "image_submission", "ok", None);
        let pending = self.save_job(&tx, &job)?;
        tx.commit()?;
        self.emit_job_event(pending);
        Ok(job)
    }

    fn purchase_candidate_get(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id"])?;
        let member = self.member(&input)?;
        self.purchase_candidate(&member, string(&input, "id")?)
    }

    fn purchase_candidate_list(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "state"])?;
        let member = self.member(&input)?;
        if let Some(state) = input.get("state") {
            if !["draft", "dismissed"].contains(&state.as_str().unwrap_or("")) {
                return Err(invalid("候选状态筛选无效"));
            }
        }
        let mut statement = self.connection.prepare(
            "SELECT document FROM purchase_candidates WHERE member_id=?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let mut items = vec![];
        for row in statement.query_map(params![member, LIST_LIMIT], |r| r.get::<_, String>(0))? {
            let doc = parse_stored(row?, "购买候选数据损坏")?;
            if input
                .get("state")
                .is_none_or(|state| doc["state"] == *state)
            {
                items.push(doc);
            }
        }
        Ok(json!({"schema_version":1,"items":items}))
    }

    /// 人工纠错：显式把审核后的事实写入候选。人工值优先，建议值保留在
    /// provenance 供对照；已弃置候选不可再改。
    fn purchase_candidate_update(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id", "facts", "confirm_correction"])?;
        if input["confirm_correction"] != true {
            return Err(ApiError::new(
                "correction_confirmation_required",
                "请逐项核对建议后确认纠错",
            ));
        }
        let facts = input["facts"]
            .as_object()
            .filter(|f| !f.is_empty())
            .ok_or_else(|| invalid("请提供至少一项经核对的事实"))?;
        if facts
            .keys()
            .any(|k| !CANDIDATE_FACT_FIELDS.contains(&k.as_str()))
        {
            return Err(invalid("候选纠错只支持候选事实字段"));
        }
        let member = self.member(&input)?;
        let mut candidate = self.purchase_candidate(&member, string(&input, "id")?)?;
        if candidate["state"] != "draft" {
            return Err(invalid("已弃置候选不可纠错"));
        }
        let mut merged = candidate["facts"].clone();
        crate::clothing::apply_facts(&mut merged, &input["facts"])?;
        candidate["facts"] = merged;
        candidate["provenance"]["corrected_facts"] = input["facts"].clone();
        candidate["provenance"]["corrected_at"] = json!(chrono::Utc::now().to_rfc3339());
        candidate["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        let tx = self.begin_write()?;
        tx.execute(
            "UPDATE purchase_candidates SET document=?1 WHERE id=?2 AND member_id=?3",
            params![candidate.to_string(), candidate["id"].as_str(), member],
        )?;
        tx.commit()?;
        Ok(candidate)
    }

    fn purchase_candidate_dismiss(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id"])?;
        let member = self.member(&input)?;
        let mut candidate = self.purchase_candidate(&member, string(&input, "id")?)?;
        if candidate["state"] == "dismissed" {
            return Ok(candidate);
        }
        candidate["state"] = json!("dismissed");
        candidate["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        let tx = self.begin_write()?;
        tx.execute(
            "UPDATE purchase_candidates SET document=?1 WHERE id=?2 AND member_id=?3",
            params![candidate.to_string(), candidate["id"].as_str(), member],
        )?;
        tx.commit()?;
        Ok(candidate)
    }

    /// 分析运行：只读真实库存（生命周期 active 且洗护可用），全部数字由
    /// 服务端确定性计算并版本化保留；候选绝不进入库存或穿着。
    fn purchase_analysis_run(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "candidate_id"])?;
        let member = self.member(&input)?;
        let candidate = self.purchase_candidate(&member, string(&input, "candidate_id")?)?;
        if candidate["state"] != "draft" {
            return Err(invalid("已弃置候选不再分析"));
        }
        // 真实库存快照：active 生命周期，按 ID 排序保证确定性。
        let mut statement = self.connection.prepare(
            "SELECT document FROM clothing WHERE member_id=?1 AND coalesce(json_extract(document,'$.status'),'active')='active' ORDER BY id",
        )?;
        let inventory: Vec<Value> = statement
            .query_map([&member], |r| r.get::<_, String>(0))?
            .map(|row| parse_stored(row?, "衣物数据损坏"))
            .collect::<Result<_, _>>()?;
        let available: BTreeSet<String> = self
            .care_batch(&member)?
            .into_iter()
            .filter(|s| s["available"] == true)
            .filter_map(|s| s["clothing_id"].as_str().map(str::to_owned))
            .collect();
        let usable: Vec<&Value> = inventory
            .iter()
            .filter(|c| available.contains(c["id"].as_str().unwrap_or("")))
            .collect();
        let now = chrono::Utc::now().to_rfc3339();
        let (pairing, pairing_uncertainties) = pairing_potential(&candidate, &usable);
        let (duplicate, duplicate_uncertainties) = duplicate_risk(&candidate, &usable);
        let (gap, gap_uncertainties) = gap_analysis(&candidate, &usable);
        let mut uncertainties: Vec<String> = Vec::new();
        uncertainties.extend(pairing_uncertainties);
        uncertainties.extend(duplicate_uncertainties);
        uncertainties.extend(gap_uncertainties);
        uncertainties.push("分析只覆盖当前可用（active 且不在洗护待洗）的真实库存；候选本身不计入库存、穿着或洞察。".into());
        // 版本号在写事务内取 COUNT：并发分析运行不会取得相同版本。
        let tx = self.begin_write()?;
        let previous: u64 = tx.query_row(
            "SELECT COUNT(*) FROM purchase_analyses WHERE member_id=?1 AND candidate_id=?2",
            params![member, candidate["id"].as_str()],
            |r| r.get(0),
        )?;
        let doc = json!({
            "schema_version":1,
            "id":uuid::Uuid::now_v7().to_string(),
            "member_id":member,
            "candidate_id":candidate["id"],
            "version":previous+1,
            "created_at":now,
            "candidate":{"id":candidate["id"],"name":candidate["facts"]["name"],"category":candidate["facts"]["category"],"color":candidate["facts"]["color"],"style":candidate["facts"]["style"],"occasion":candidate["facts"]["occasion"],"seasons":candidate["facts"]["seasons"]},
            "pairing_potential":pairing,
            "duplicate_risk":duplicate,
            "gap_analysis":gap,
            "uncertainties":uncertainties,
            "creates_wear":false,
            "counts_toward_inventory":false,
            "auto_purchase":false,
        });
        tx.execute(
            "INSERT INTO purchase_analyses VALUES(?1,?2,?3,?4)",
            params![
                doc["id"].as_str(),
                member,
                candidate["id"].as_str(),
                doc.to_string()
            ],
        )?;
        tx.commit()?;
        Ok(doc)
    }

    fn purchase_analysis_get(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id"])?;
        let member = self.member(&input)?;
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM purchase_analyses WHERE member_id=?1 AND id=?2",
                params![member, string(&input, "id")?],
                |r| r.get(0),
            )
            .optional()?;
        parse_stored(
            stored.ok_or_else(|| ApiError::new("not_found", "分析结果不存在"))?,
            "分析结果数据损坏",
        )
    }

    fn purchase_analysis_list(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "candidate_id"])?;
        let member = self.member(&input)?;
        let candidate_id = input
            .get("candidate_id")
            .map(|v| v.as_str().ok_or_else(|| invalid("candidate_id 须为字符串")))
            .transpose()?;
        let mut statement = self.connection.prepare(
            "SELECT document FROM purchase_analyses WHERE member_id=?1 AND (?2 IS NULL OR candidate_id=?2) ORDER BY id DESC LIMIT ?3",
        )?;
        let mut items = vec![];
        for row in statement.query_map(params![member, candidate_id, LIST_LIMIT], |r| {
            r.get::<_, String>(0)
        })? {
            items.push(parse_stored(row?, "分析结果数据损坏")?);
        }
        Ok(json!({"schema_version":1,"items":items}))
    }
}

fn validated_asset_ids(
    app: &Application,
    member: &str,
    ids: &Value,
) -> Result<Vec<Value>, ApiError> {
    let empty = Vec::new();
    let list = ids.as_array().unwrap_or(&empty);
    if list.len() > 20 {
        return Err(invalid("图片最多20张"));
    }
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    for id in list {
        let id = id.as_str().ok_or_else(|| invalid("图片ID须为字符串"))?;
        if !seen.insert(id) {
            return Err(invalid("图片不能重复"));
        }
        app.asset_get(json!({"member_id":member,"id":id}))?;
        result.push(json!(id));
    }
    Ok(result)
}

/// 保守候选描述提示词：只描述可见衣物类型与主色；品牌、尺码、材质、价格、
/// 季节与不可见特性一律不猜测。与识别管线同方向，针对购买候选明确禁止
/// 推断价格与尺码。
const CANDIDATE_PROMPT: &str = "Describe only the visible garment type and main colors in this product image. Say which details are uncertain. Do not guess brand, size, material, price, season or hidden features. This is an unverified visual suggestion for a purchase candidate a person will review.";
const CANDIDATE_PROMPT_VERSION: &str = "candidate-visible-description-v1";

/// 搭配潜力：复用推荐域的角色/覆盖规则机械（top+bottom+shoes、body+shoes、
/// 外层等），候选固定占据其角色槽位，其余槽位只来自当前可用的真实库存。
/// 组合总数为各覆盖槽位数量的精确乘积；展示上限 5 套并附带精确总数。
/// 覆盖角色缺库存时如实列出 missing_roles，候选与库存件在条目中严格区分
/// （candidate:true，库存件带真实 clothing_id）。
fn pairing_potential(candidate: &Value, usable: &[&Value]) -> (Value, Vec<String>) {
    let mut uncertainties = vec![];
    let category = candidate["facts"]["category"].as_str().unwrap_or("");
    let mapped = if category.is_empty() {
        None
    } else if crate::CATEGORIES.contains(&category) {
        Some(crate::recommendations::role(category).0.to_string())
    } else {
        // 自定义类别无法映射到搭配角色；如实说明而不是猜测。
        uncertainties.push(format!(
            "候选类别 {category} 是自定义类别，无法映射到搭配角色；搭配潜力无法计算。"
        ));
        None
    };
    let Some(role) = mapped else {
        if category.is_empty() {
            uncertainties.push("候选类别未知；无法判断可填补的搭配角色，搭配潜力无法计算。".into());
        }
        return (
            json!({
                "role":Value::Null,
                "proposals":[],
                "combinations_total":0,
                "listed":0,
                "candidate_fixed":true,
                "rules":"候选类别未知或不可映射时无法生成搭配组合；不猜测角色。",
            }),
            uncertainties,
        );
    };
    let coverage: Vec<(&str, &str)> = match role.as_str() {
        // 候选即覆盖角色：其余基础覆盖角色由库存供给。
        "body" => vec![("shoes", "shoes")],
        "top" => vec![("bottom", "bottom"), ("shoes", "shoes")],
        "bottom" => vec![("top", "top"), ("shoes", "shoes")],
        "shoes" => vec![("top", "top"), ("bottom", "bottom")],
        // 候选是附加角色：基础覆盖全部由库存供给（与推荐域的显式选入角色一致，
        // 内衣/运动服/其他不视为全身覆盖）。
        _ => vec![("top", "top"), ("bottom", "bottom"), ("shoes", "shoes")],
    };
    let slots: Vec<(String, Vec<&Value>)> = coverage
        .iter()
        .map(|(slot_role, slot_category)| {
            (
                slot_role.to_string(),
                usable
                    .iter()
                    .copied()
                    .filter(|c| c["category"].as_str() == Some(*slot_category))
                    .collect(),
            )
        })
        .collect();
    let missing: Vec<String> = slots
        .iter()
        .filter(|(_, items)| items.is_empty())
        .map(|(slot_role, _)| slot_role.clone())
        .collect();
    // 精确组合总数：各覆盖槽位数量的乘积（候选固定占据其角色，不计入乘积）。
    let combinations_total: u64 = slots.iter().map(|(_, items)| items.len() as u64).product();
    let candidate_entry = json!({
        "clothing_id":Value::Null,
        "candidate_id":candidate["id"],
        "name":candidate["facts"]["name"],
        "category":category,
        "role":role,
        "layer":crate::recommendations::role(category).1,
        "candidate":true,
    });
    let mut proposals = vec![];
    let mut counters = vec![0usize; slots.len()];
    for _ in 0..5 {
        let mut items = vec![candidate_entry.clone()];
        for (index, (_, candidates)) in slots.iter().enumerate() {
            if candidates.is_empty() {
                continue;
            }
            let item = candidates[counters[index]];
            let (entry_role, base) =
                crate::recommendations::role(item["category"].as_str().unwrap());
            items.push(json!({
                "clothing_id":item["id"],
                "candidate_id":Value::Null,
                "name":item["name"],
                "category":item["category"],
                "role":entry_role,
                "layer":base,
                "candidate":false,
            }));
        }
        proposals.push(json!({
            "complete":missing.is_empty(),
            "missing_roles":missing.clone(),
            "items":items,
        }));
        // 字典序推进槽位下标；全部组合枚举完即停。
        let mut carry = slots.len();
        loop {
            if carry == 0 {
                finish_pairing(&missing, &mut uncertainties);
                return (
                    pairing_document(role, proposals, combinations_total),
                    uncertainties,
                );
            }
            carry -= 1;
            counters[carry] += 1;
            if counters[carry] < slots[carry].1.len() {
                break;
            }
            counters[carry] = 0;
            if carry == 0 {
                finish_pairing(&missing, &mut uncertainties);
                return (
                    pairing_document(role, proposals, combinations_total),
                    uncertainties,
                );
            }
        }
    }
    finish_pairing(&missing, &mut uncertainties);
    (
        pairing_document(role, proposals, combinations_total),
        uncertainties,
    )
}

/// 槽位枚举结束后的收尾：把缺失角色写入不确定性说明。
fn finish_pairing(missing: &[String], uncertainties: &mut Vec<String>) {
    if !missing.is_empty() {
        uncertainties.push(format!(
            "当前可用库存缺少{}角色；搭配组合不完整，不能视为完整穿搭。",
            missing.join("、")
        ));
    }
}

/// 搭配潜力收尾文档。
fn pairing_document(role: String, proposals: Vec<Value>, combinations_total: u64) -> Value {
    json!({
        "role":role,
        "proposals":proposals,
        "combinations_total":combinations_total,
        "listed":proposals.len(),
        "candidate_fixed":true,
        "rules":"候选固定占据其角色槽位；覆盖角色只使用当前可用（active 且不在洗护待洗）的真实库存；组合总数=各覆盖槽位库存数的精确乘积（0 表示无法组成完整覆盖组合）；展示最多5套，不完整组合带 missing_roles；条目 candidate:true 为候选、其余带真实 clothing_id。覆盖角色与推荐域角色规则同源。",
    })
}

/// 重复购买：与同类别可用库存逐项比较颜色/风格/场合/季节的重合度。
/// 权重（千分制）：颜色400、风格250、场合200、季节150；每维取 Jaccard
/// 重合（交集/并集，整数运算）。双方都无数据的维度跳过并记录为未比较。
/// high_overlap：≥700 且至少两个维度可比。
fn duplicate_risk(candidate: &Value, usable: &[&Value]) -> (Value, Vec<String>) {
    let mut uncertainties = vec![];
    let category = candidate["facts"]["category"].as_str().unwrap_or("");
    if category.is_empty() {
        uncertainties.push("候选类别未知；无法计算重复购买风险。".into());
        return (
            json!({
                "available":false,
                "compared_total":0,
                "high_overlap_total":0,
                "items":[],
                "listed":0,
                "rules":"类别未知时不猜测重复风险。",
            }),
            uncertainties,
        );
    }
    let same_category: Vec<&Value> = usable
        .iter()
        .copied()
        .filter(|c| c["category"].as_str() == Some(category))
        .collect();
    let dims: [(&str, u64); 4] = [
        ("color", 400),
        ("style", 250),
        ("occasion", 200),
        ("seasons", 150),
    ];
    let mut items = vec![];
    let mut high = 0u64;
    for item in &same_category {
        let mut score = 0u64;
        let mut comparable = 0u32;
        let mut unknown_dims = vec![];
        let mut matched = json!({});
        for (field, weight) in dims {
            let candidate_values: BTreeSet<String> = candidate["facts"][field]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            let item_values: BTreeSet<String> = item[field]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            if candidate_values.is_empty() && item_values.is_empty() {
                unknown_dims.push(json!(field));
                continue;
            }
            if candidate_values.is_empty() || item_values.is_empty() {
                // 一方未记录：该维度不比较也不猜测；如实计数。
                unknown_dims.push(json!(field));
                continue;
            }
            let inter = candidate_values.intersection(&item_values).count() as u64;
            let union = candidate_values.union(&item_values).count() as u64;
            score += weight * inter / union;
            comparable += 1;
            matched[field] = json!(candidate_values
                .intersection(&item_values)
                .collect::<Vec<_>>());
        }
        let high_overlap = score >= 700 && comparable >= 2;
        if high_overlap {
            high += 1;
        }
        items.push(json!({
            "clothing_id":item["id"],
            "name":item["name"],
            "score":score,
            "high_overlap":high_overlap,
            "matched":matched,
            "unknown_dimensions":unknown_dims,
        }));
    }
    items.sort_by(|a, b| {
        b["score"]
            .as_u64()
            .cmp(&a["score"].as_u64())
            .then_with(|| a["clothing_id"].as_str().cmp(&b["clothing_id"].as_str()))
    });
    let listed: Vec<Value> = items.iter().take(24).cloned().collect();
    if same_category.is_empty() {
        uncertainties.push(format!(
            "当前可用库存没有类别为 {category} 的单品；与已有衣物重复的可能性无法从库存判断。"
        ));
    }
    if candidate["facts"]["color"]
        .as_array()
        .is_none_or(Vec::is_empty)
    {
        uncertainties.push("候选未记录颜色；重复比较跳过颜色维度，结果可能低估重合。".into());
    }
    (
        json!({
            "available":true,
            "category":category,
            "compared_total":same_category.len(),
            "high_overlap_total":high,
            "items":listed,
            "listed":listed.len(),
            "rules":"同类别可用库存逐项比较；权重（千分制）颜色400/风格250/场合200/季节150，每维Jaccard=交集/并集（整数运算）；high_overlap=总分≥700且≥2个维度可比；任一方未记录的维度跳过并列入 unknown_dimensions；展示最多24项并附带精确总数。",
        }),
        uncertainties,
    )
}

/// 缺口分析：按内置类别与场景统计 active 真实库存的覆盖；数量为 0 即覆盖
/// 空洞。候选填补声明只在候选类别/场景与空洞精确相等时作出，绝不推断。
fn gap_analysis(candidate: &Value, usable: &[&Value]) -> (Value, Vec<String>) {
    let mut uncertainties = vec![];
    let mut categories: Vec<Value> = crate::CATEGORIES
        .iter()
        .map(|category| {
            let count = usable
                .iter()
                .filter(|c| c["category"].as_str() == Some(category))
                .count();
            json!({"category":category,"count":count,"hole":count==0})
        })
        .collect();
    let mut custom_counts: std::collections::BTreeMap<String, u64> = Default::default();
    for item in usable {
        if let Some(id) = item["category"]
            .as_str()
            .filter(|id| !crate::CATEGORIES.contains(id))
        {
            *custom_counts.entry(id.to_string()).or_default() += 1;
        }
    }
    for (category, count) in custom_counts {
        categories.push(json!({
            "category":category,
            "count":count,
            "hole":false,
            "custom":true,
        }));
    }
    let candidate_category = candidate["facts"]["category"].as_str().unwrap_or("");
    if candidate_category.is_empty() {
        uncertainties.push("候选类别未知；无法判断候选填补哪个类别空洞。".into());
    }
    let occasions = ["daily", "work", "formal", "sport"];
    let mut occasion_rows: Vec<Value> = vec![];
    for occasion in occasions {
        let count = usable
            .iter()
            .filter(|c| {
                c["occasion"]
                    .as_array()
                    .is_some_and(|tags| tags.iter().any(|tag| tag == occasion))
            })
            .count();
        occasion_rows.push(json!({"occasion":occasion,"count":count,"hole":count==0}));
    }
    let untagged = usable
        .iter()
        .filter(|c| c["occasion"].as_array().is_none_or(Vec::is_empty))
        .count();
    if untagged > 0 {
        uncertainties.push(format!(
            "库存中有 {untagged} 件单品未记录任何场合标签；场景覆盖按已记录标签统计，不推断未知。"
        ));
    }
    let candidate_occasions: Vec<String> = candidate["facts"]["occasion"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let fills_categories: Vec<String> = if candidate_category.is_empty() {
        vec![]
    } else {
        categories
            .iter()
            .filter(|row| {
                row["hole"] == true && row["category"].as_str() == Some(candidate_category)
            })
            .filter_map(|row| row["category"].as_str().map(str::to_owned))
            .collect()
    };
    let fills_occasions: Vec<String> = if candidate_occasions.is_empty() {
        vec![]
    } else {
        occasion_rows
            .iter()
            .filter(|row| {
                row["hole"] == true
                    && candidate_occasions
                        .contains(&row["occasion"].as_str().unwrap_or("").to_string())
            })
            .filter_map(|row| row["occasion"].as_str().map(str::to_owned))
            .collect()
    };
    if !candidate_category.is_empty()
        && fills_categories.is_empty()
        && crate::CATEGORIES.contains(&candidate_category)
    {
        let existing = categories
            .iter()
            .find(|row| row["category"].as_str() == Some(candidate_category))
            .map(|row| row["count"].as_u64().unwrap_or(0))
            .unwrap_or(0);
        uncertainties.push(format!(
            "候选类别 {candidate_category} 已有 {existing} 件可用库存；本候选不填补类别空洞（重复购买风险请看重复分析）。"
        ));
    }
    (
        json!({
            "inventory_scope":"active_and_care_available",
            "categories":categories,
            "occasions":occasion_rows,
            "candidate_fills":{"categories":fills_categories,"occasions":fills_occasions},
            "rules":"按内置类别与四场景统计当前可用（active 且不在洗护待洗）库存；hole=数量为0；候选填补声明只在候选类别/已记录场景与空洞精确相等时作出；自定义类别单列，不参与内置空洞判定。",
        }),
        uncertainties,
    )
}

pub(super) fn descriptors() -> Vec<Value> {
    let candidate_output = candidate_schema();
    let job_output = jobs::output_schema();
    let descriptors = vec![
        op(
            "purchase.candidate.create",
            "创建购买候选草稿：候选域独立身份，不计入库存/穿着/洞察，不同步",
            json!({"type":"object","required":["member_id","name","category"],"additionalProperties":false,"properties":candidate_input_properties(vec!["member_id","name","category"], true)}),
            candidate_output.clone(),
            json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
            json!([{"request":{"member_id":"<已授权成员ID>","name":"<候选商品名称>","category":"<类别ID>"},"prerequisites":["候选存放在候选域：不占衣物身份、不入库存或统计","未知事实保持 null；不自动购买"]}]),
        ),
        op(
            "purchase.candidate.recognize",
            "为购买候选提交商品图识别：先持久化任务收据，确认上传后才发送所选图片",
            json!({"type":"object","required":["member_id","operation_key","candidate_id","provider_id","asset_ids","upload_role","confirm_upload"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"operation_key":{"type":"string","minLength":1,"maxLength":200},"candidate_id":{"type":"string"},"provider_id":{"type":"string"},"asset_ids":{"type":"array","minItems":1,"maxItems":20,"uniqueItems":true,"items":{"type":"string"}},"upload_role":{"enum":["original","product_display"]},"confirm_upload":{"const":true}}}),
            job_output.clone(),
            json!({"kind":"mutation","network":"none","local_files":true,"async":null,"conditional_effects":[{"when":{"job_state":"after_real_submission"},"effect":{"kind":"mutation","network":"explicit-provider"}}]}),
            json!([{"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>","candidate_id":"<候选ID>","provider_id":"<已配置视觉供应商ID>","asset_ids":["<资产ID>"],"upload_role":"product_display","confirm_upload":true},"prerequisites":["仅上传明确确认的图片；视觉调用真实消耗供应商额度","建议写入候选provenance，不自动覆盖人工事实","本示例不自动执行"]}]),
        ),
        op(
            "purchase.candidate.process",
            "推进已入队候选识别任务；提交未知不盲重发，失败如实反馈",
            json!({"type":"object","required":["member_id","id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}}),
            job_output.clone(),
            json!({"kind":"mutation","network":"none","local_files":true,"async":null,"conditional_effects":[{"when":{"submission_pending":true},"effect":{"kind":"mutation","network":"explicit-provider"}}]}),
            json!([{"request":{"member_id":"<已授权成员ID>","id":"<识别任务ID>"},"prerequisites":["首次执行真实提交时消耗供应商额度","提交结果未知时持久化 waiting_input，不自动重发","失败时候选保持原样并如实说明不确定性"]}]),
        ),
        op(
            "purchase.candidate.get",
            "读取购买候选：候选事实、识别建议、不确定性与来源",
            json!({"type":"object","required":["member_id","id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}}),
            candidate_output.clone(),
            json!({"kind":"query","network":"none","local_files":false,"async":null}),
            json!([{"request":{"member_id":"<已授权成员ID>","id":"<候选ID>"},"prerequisites":["候选属于候选域；建议值未经核实需人工纠错"]}]),
        ),
        op(
            "purchase.candidate.list",
            "按成员列出购买候选（可按状态筛选，按ID倒序）",
            json!({"type":"object","required":["member_id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"state":{"enum":["draft","dismissed"]}}}),
            json!({"type":"object","required":["schema_version","items"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":candidate_output.clone()}}}),
            json!({"kind":"query","network":"none","local_files":false,"async":null}),
            json!([{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["仅返回当前成员的候选；不包含库存单品"]}]),
        ),
        op(
            "purchase.candidate.update",
            "人工纠错候选事实：人工值优先，识别建议保留在 provenance 对照",
            json!({"type":"object","required":["member_id","id","facts","confirm_correction"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"},"facts":{"type":"object","minProperties":1},"confirm_correction":{"const":true}}}),
            candidate_output.clone(),
            json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
            json!([{"request":{"member_id":"<已授权成员ID>","id":"<候选ID>","facts":{"category":"top"},"confirm_correction":true},"prerequisites":["人工纠错显式发生；类别须在分类字典内","未知尺码/价格保持未知，不编造"]}]),
        ),
        op(
            "purchase.candidate.dismiss",
            "弃置候选草稿：保留记录，不再分析",
            json!({"type":"object","required":["member_id","id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}}),
            candidate_output,
            json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
            json!([{"request":{"member_id":"<已授权成员ID>","id":"<候选ID>"},"prerequisites":["弃置不删除记录；已弃置候选不可纠错或分析"]}]),
        ),
        op(
            "purchase.analysis.run",
            "运行购买适配分析：搭配潜力/重复购买/缺口，全部来自当前可用真实库存的确定性计算；结果版本化保留",
            json!({"type":"object","required":["member_id","candidate_id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"candidate_id":{"type":"string"}}}),
            analysis_schema(),
            json!({"kind":"mutation","network":"none","local_files":false,"async":null}),
            json!([{"request":{"member_id":"<已授权成员ID>","candidate_id":"<候选ID>"},"prerequisites":["数字来自确定性查询；候选不计入库存或穿着","每次运行生成新版本并保留历史；缺失事实如实列为不确定性","不自动购买"]}]),
        ),
        op(
            "purchase.analysis.get",
            "读取一次购买适配分析结果",
            json!({"type":"object","required":["member_id","id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}}),
            analysis_schema(),
            json!({"kind":"query","network":"none","local_files":false,"async":null}),
            json!([{"request":{"member_id":"<已授权成员ID>","id":"<分析ID>"},"prerequisites":["读取版本化分析；引用的库存ID可交叉核对"]}]),
        ),
        op(
            "purchase.analysis.list",
            "列出购买适配分析历史（可按候选筛选，按ID倒序）",
            json!({"type":"object","required":["member_id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"candidate_id":{"type":"string"}}}),
            json!({"type":"object","required":["schema_version","items"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":analysis_schema()}}}),
            json!({"kind":"query","network":"none","local_files":false,"async":null}),
            json!([{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["返回版本化历史；候选与真实库存严格区分"]}]),
        ),
    ];
    descriptors
}

fn candidate_input_properties(required: Vec<&str>, _with_all: bool) -> Value {
    let mut properties = json!({
        "member_id":{"type":"string"},
        "name":{"type":"string","minLength":1,"maxLength":120},
        "category":{"type":"string"},
        "asset_ids":{"type":"array","maxItems":20,"uniqueItems":true,"items":{"type":"string"}},
    });
    for key in [
        "sub_category",
        "brand",
        "size_label",
        "fit",
        "description",
        "notes",
        "purchase_url",
        "color_description",
    ] {
        properties[key] = json!({"type":["string","null"],"maxLength":if key=="description"||key=="notes"{10000}else{500}});
    }
    for key in ["color", "pattern", "material", "style", "occasion", "tags"] {
        properties[key] = json!({"type":"array","maxItems":100,"items":{"type":"string","minLength":1,"maxLength":120}});
    }
    properties["seasons"] = json!({"type":"array","maxItems":4,"items":{"type":"string","enum":["spring","summer","autumn","winter"]}});
    properties["favorite"] = json!({"type":"boolean"});
    properties["purchase_price_minor"] =
        json!({"type":["integer","null"],"minimum":0,"maximum":9007199254740991u64});
    properties["currency"] = json!({"type":["string","null"]});
    let _ = required;
    properties
}

fn candidate_schema() -> Value {
    let facts = candidate_input_properties(vec![], true);
    json!({
        "type":"object",
        "required":["schema_version","id","member_id","domain","state","facts","provenance","asset_ids","created_at","updated_at"],
        "properties":{
            "schema_version":{"const":1},
            "id":{"type":"string"},
            "member_id":{"type":"string"},
            "domain":{"const":"purchase_candidate"},
            "state":{"enum":["draft","dismissed"]},
            "facts":facts,
            "provenance":{"type":"object"},
            "asset_ids":{"type":"array","items":{"type":"string"}},
            "created_at":{"type":"string"},
            "updated_at":{"type":"string"},
        }
    })
}

fn analysis_schema() -> Value {
    let pairing_item = json!({
        "type":"object",
        "required":["complete","missing_roles","items"],
        "properties":{
            "complete":{"type":"boolean"},
            "missing_roles":{"type":"array","items":{"type":"string"}},
            "items":{"type":"array","items":{"type":"object","required":["clothing_id","candidate_id","name","category","role","layer","candidate"],"properties":{"clothing_id":{"type":["string","null"]},"candidate_id":{"type":["string","null"]},"name":{"type":["string","null"]},"category":{"type":["string","null"]},"role":{"type":["string","null"]},"layer":{"type":["integer","null"]},"candidate":{"type":"boolean"}}}},
        }
    });
    let duplicate_item = json!({
        "type":"object",
        "required":["clothing_id","name","score","high_overlap","matched","unknown_dimensions"],
        "properties":{
            "clothing_id":{"type":["string","null"]},
            "name":{"type":["string","null"]},
            "score":{"type":"integer"},
            "high_overlap":{"type":"boolean"},
            "matched":{"type":"object"},
            "unknown_dimensions":{"type":"array","items":{"type":"string"}},
        }
    });
    json!({
        "type":"object",
        "required":["schema_version","id","member_id","candidate_id","version","created_at","candidate","pairing_potential","duplicate_risk","gap_analysis","uncertainties","creates_wear","counts_toward_inventory","auto_purchase"],
        "properties":{
            "schema_version":{"const":1},
            "id":{"type":"string"},
            "member_id":{"type":"string"},
            "candidate_id":{"type":"string"},
            "version":{"type":"integer","minimum":1},
            "created_at":{"type":"string"},
            "candidate":{"type":"object"},
            "pairing_potential":{"type":"object","required":["role","proposals","combinations_total","listed","candidate_fixed","rules"],"properties":{"role":{"type":["string","null"]},"proposals":{"type":"array","items":pairing_item},"combinations_total":{"type":"integer","minimum":0},"listed":{"type":"integer","minimum":0},"candidate_fixed":{"type":"boolean"},"rules":{"type":"string"}}},
            "duplicate_risk":{"type":"object","required":["available","compared_total","high_overlap_total","items","listed","rules"],"properties":{"available":{"type":"boolean"},"category":{"type":["string","null"]},"compared_total":{"type":"integer","minimum":0},"high_overlap_total":{"type":"integer","minimum":0},"items":{"type":"array","items":duplicate_item},"listed":{"type":"integer","minimum":0},"rules":{"type":"string"}}},
            "gap_analysis":{"type":"object","required":["inventory_scope","categories","occasions","candidate_fills","rules"],"properties":{"inventory_scope":{"type":"string"},"categories":{"type":"array","items":{"type":"object","required":["category","count","hole"],"properties":{"category":{"type":["string","null"]},"count":{"type":"integer","minimum":0},"hole":{"type":"boolean"},"custom":{"type":"boolean"}}}},"occasions":{"type":"array","items":{"type":"object","required":["occasion","count","hole"],"properties":{"occasion":{"type":"string"},"count":{"type":"integer","minimum":0},"hole":{"type":"boolean"}}}},"candidate_fills":{"type":"object","required":["categories","occasions"],"properties":{"categories":{"type":"array","items":{"type":"string"}},"occasions":{"type":"array","items":{"type":"string"}}}},"rules":{"type":"string"}}},
            "uncertainties":{"type":"array","items":{"type":"string"}},
            "creates_wear":{"const":false},
            "counts_toward_inventory":{"const":false},
            "auto_purchase":{"const":false},
        }
    })
}

fn op(
    name: &str,
    description: &str,
    input: Value,
    output: Value,
    effect: Value,
    examples: Value,
) -> Value {
    json!({
        "name":name,
        "description":description,
        "inputSchema":input,
        "outputSchema":output,
        "effect":effect,
        "examples":examples,
    })
}
