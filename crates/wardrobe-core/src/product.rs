use crate::{string, ApiError, Application};
use serde_json::{json, Value};
impl Application {
    pub(super) fn product_generate(&self, input: Value) -> Result<Value, ApiError> {
        fields(
            &input,
            &[
                "member_id",
                "clothing_id",
                "asset_id",
                "provider_id",
                "operation_key",
                "upload_original",
            ],
        )?;
        if input["upload_original"] != true {
            return Err(ApiError::new(
                "upload_consent_required",
                "请明确允许把所选原图发送到供应商；可能使用供应商额度",
            ));
        }
        let member = self.member(&input)?;
        let clothing =
            self.clothing_get(json!({"member_id":member,"id":string(&input,"clothing_id")?}))?;
        let asset_id = string(&input, "asset_id")?;
        if clothing["status"] == "deleted"
            || !clothing["asset_ids"]
                .as_array()
                .is_some_and(|v| v.contains(&json!(asset_id)))
        {
            return Err(ApiError::new("invalid_input", "请选择此单品的现有图片"));
        }
        let asset = self.asset_get(json!({"member_id":member,"id":asset_id}))?;
        let provider = self.provider_get(string(&input, "provider_id")?)?;
        validate_provider(&provider)?;
        let source = source_image(&asset)?;
        drop(source);
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(ApiError::new("invalid_input", "操作标识须为1–200字节"));
        }
        self.job_enqueue(&member,"product_display",key,json!({"clothing_id":clothing["id"],"asset_id":asset_id,"provider":provider,"upload_original":true,"base_display_asset_id":clothing["display_asset_id"]}))
    }
}
impl Application {
    pub(super) fn product_process(&self, input: Value) -> Result<Value, ApiError> {
        self.cloud_image_process(input, "product_display")
    }
    pub(super) fn cloud_image_process(&self, input: Value, kind: &str) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id"])?;
        let member = self.member(&input)?;
        let id = string(&input, "id")?;
        let existing = self.job_get(input.clone())?;
        if existing["kind"] != kind {
            return Err(ApiError::new("invalid_input", "此任务不是商品图生成"));
        }
        if existing["submitted_at"].is_string() {
            if existing["state"] == "running"
                && existing["lease_expires"].as_i64().unwrap_or(0)
                    <= chrono::Utc::now().timestamp_millis()
            {
                let tx = self.begin_write()?;
                let mut doc = self.job_get(input)?;
                if doc["state"] != "running"
                    || doc["lease_expires"].as_i64().unwrap_or(0)
                        > chrono::Utc::now().timestamp_millis()
                {
                    return Ok(doc);
                }
                doc["state"] = json!("waiting_input");
                doc["lease_token"] = Value::Null;
                doc["error"] = json!({"code":"provider_outcome_unknown","message":"请求已提交且结果未知；供应商无查询接口，请检查供应商记录，不能重发此任务","retryable":false});
                let pending = self.save_job(&tx, &doc)?;
                tx.commit()?;
                self.emit_job_event(pending);
                return Ok(doc);
            }
            return Ok(existing);
        }
        let job = self.job_claim(&member, id, 300)?;
        let lease = job["lease_token"].as_str().unwrap();
        let result = match kind {
            "product_display" => self.product_request(&member, id, lease, &job),
            "outfit_flatlay" => self.flatlay_request(&member, id, lease, &job),
            "virtual_tryon" => self.tryon_request(&member, id, lease, &job),
            _ => Err(ApiError::new("unsupported_operation", "不支持的图像任务")),
        };
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let doc = self.job_get(json!({"member_id":member,"id":id}))?;
                if doc["state"] == "cancelled" {
                    return Ok(doc);
                }
                self.job_finish(
                    &member,
                    id,
                    lease,
                    if doc["submitted_at"].is_string() {
                        "waiting_input"
                    } else {
                        "failed"
                    },
                    Value::Null,
                    json!({"code":error.code,"message":error.message,"retryable":false}),
                )
            }
        }
    }
    fn product_request(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        job: &Value,
    ) -> Result<Value, ApiError> {
        let provider = &job["payload"]["provider"];
        let source = self.asset_get(json!({"member_id":member,"id":job["payload"]["asset_id"]}))?;
        let source_image = source_image(&source)?;
        let (image,request_id)=self.ark_generate(member,id,lease,job,&[source_image],"Create a clean studio product photograph of this exact garment on a plain neutral background, without a person. Preserve the original garment color, silhouette, cut, fabric texture, pattern, printed text, logos and construction details. Do not invent, remove or change garment features. The reference garment is the only subject.")?;
        let tx = self.begin_write()?;
        let mut doc = self.job_get(json!({"member_id":member,"id":id}))?;
        check_lease(&doc, lease)?;
        let asset=self.image_derived(&tx,source,image,"product_generation",json!({"provider_id":provider["id"],"model":provider["model"],"adapter":"ark_seedream","job_id":id,"request_id":request_id}))?;
        doc["state"] = json!("waiting_input");
        doc["progress"] = json!(100);
        doc["lease_token"] = Value::Null;
        doc["lease_expires"] = json!(0);
        doc["result"] = json!({"asset_id":asset["id"],"request_id":request_id,"verified_capabilities":["image_generation"]});
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        let pending = self.save_job(&tx, &doc)?;
        tx.commit()?;
        self.emit_job_event(pending);
        Ok(doc)
    }
    pub(super) fn ark_generate(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        job: &Value,
        reference_images: &[image::DynamicImage],
        prompt: &str,
    ) -> Result<(image::DynamicImage, String), ApiError> {
        use base64::Engine;
        let provider = &job["payload"]["provider"];
        let current = self.provider_get(provider["id"].as_str().unwrap())?;
        if current != *provider {
            return Err(ApiError::new(
                "provider_changed",
                "模型设置已更改，请明确创建新任务",
            ));
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(240))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ApiError::new("connection_error", "无法创建供应商连接"))?;
        let endpoint = provider["endpoint"].as_str().unwrap();
        let key = if provider["authentication"] == "bearer" {
            Some(
                crate::providers::credential_entry_for(provider)?
                    .get_password()
                    .map_err(|_| ApiError::new("credential_missing", "请配置并解锁系统安全凭据"))?,
            )
        } else {
            None
        };
        let mut discover = client
            .get(format!("{endpoint}/models"))
            .timeout(std::time::Duration::from_secs(20));
        if let Some(key) = &key {
            discover = discover.bearer_auth(key);
        }
        // 供应商请求事件（#44）：trace 取自任务文档；模型发现与生成各记一条
        let provider_request =
            crate::diagnostics::ProviderRequestLog::from_job(job, provider["model"].as_str().unwrap_or(""));
        provider_request.begin_send();
        let response = match discover.send() {
            Ok(response) => response,
            Err(_) => {
                provider_request.record(self,"model_discovery","error",None);
                return Err(ApiError::new("model_discovery_failed", "无法查询可用模型，尚未发送原图"));
            }
        };
        if !response.status().is_success() {
            provider_request.record(self,"model_discovery","http_error",Some(response.status().as_u16()));
            return Err(ApiError::new(
                "model_discovery_failed",
                "模型查询被拒绝，请检查账户开通及最终端点，尚未生成",
            ));
        }
        provider_request.record(self,"model_discovery","ok",None);
        let models = bounded_json(response, 1024 * 1024)?;
        if !models["data"]
            .as_array()
            .is_some_and(|v| v.iter().any(|m| m["id"] == provider["model"]))
        {
            return Err(ApiError::new(
                "model_not_enabled",
                "账户可用模型中没有所选ID，请检查已开通模型",
            ));
        }

        validate_provider(provider)?;
        if reference_images.is_empty() || reference_images.len() > 10 {
            return Err(ApiError::new(
                "image_count_limit",
                "Seedream Flash参考图须为1–10张；不会静默截断",
            ));
        }
        let mut references = vec![];
        for image in reference_images {
            validate_reference(image)?;
            let mut png = std::io::Cursor::new(Vec::new());
            image
                .write_to(&mut png, image::ImageFormat::Png)
                .map_err(|_| ApiError::new("image_processing_failed", "参考图规范化失败"))?;
            if png.get_ref().len() > 30 * 1024 * 1024 {
                return Err(ApiError::new(
                    "image_limit",
                    "规范化参考图超过供应商30MB输入限制",
                ));
            }
            references.push(json!(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png.into_inner())
            )));
        }
        let references = if references.len() == 1 {
            references.remove(0)
        } else {
            json!(references)
        };
        let body = json!({"model":provider["model"],"prompt":prompt,"image":references,"size":"2K","response_format":"b64_json","watermark":false});
        {
            let tx = self.begin_write()?;
            let mut current = self.job_get(json!({"member_id":member,"id":id}))?;
            check_lease(&current, lease)?;
            current["submitted_at"] = json!(chrono::Utc::now().to_rfc3339());
            current["progress"] = json!(25);
            let pending = self.save_job(&tx, &current)?;
            tx.commit()?;
            self.emit_job_event(pending);
        }
        let mut request = client
            .post(format!("{endpoint}/images/generations"))
            .json(&body);
        if let Some(key) = &key {
            request = request.bearer_auth(key);
        }
        provider_request.begin_send();
        let response = match request.send() {
            Ok(response) => response,
            Err(_) => {
                provider_request.record(self,"image_generations","outcome_unknown",None);
                return Err(ApiError::new(
                    "provider_outcome_unknown",
                    "生成请求结果未知；请检查供应商记录，此任务不会自动重发",
                ));
            }
        };
        if !response.status().is_success() {
            provider_request.record(self,"image_generations","http_error",Some(response.status().as_u16()));
            return Err(ApiError::new(
                "provider_generation_failed",
                "供应商未返回成功，旧图保留；请检查账户额度、模型和记录",
            ));
        }
        provider_request.record(self,"image_generations","ok",None);
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .filter(|v| {
                v.len() <= 200
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    && key
                        .as_ref()
                        .is_none_or(|secret| !v.contains(secret.as_str()))
            })
            .unwrap_or("")
            .to_owned();
        let result = bounded_json(response, 70 * 1024 * 1024)?;
        let encoded = result["data"][0]["b64_json"].as_str().ok_or_else(|| {
            ApiError::new(
                "provider_response",
                "供应商没有返回单张base64图片，旧图保留",
            )
        })?;
        if result["data"].as_array().is_none_or(|v| v.len() != 1) {
            return Err(ApiError::new(
                "provider_response",
                "供应商返回数量不符合单图请求",
            ));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| ApiError::new("provider_response", "供应商图片编码无效"))?;
        let image = crate::assets::decode(&bytes)?;
        let image = if image.width() > 1600 || image.height() > 1600 {
            image.resize(1600, 1600, image::imageops::FilterType::Lanczos3)
        } else {
            image
        };
        Ok((image, request_id))
    }
    pub(super) fn product_adopt(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id"])?;
        let member = self.member(&input)?;
        let tx = self.begin_write()?;
        let mut job = self.job_get(input)?;
        if job["kind"] != "product_display" {
            return Err(ApiError::new("invalid_input", "此任务不是商品图"));
        }
        if job["state"] == "succeeded" {
            return Ok(job);
        }
        if job["state"] != "waiting_input" || !job["result"]["asset_id"].is_string() {
            return Err(ApiError::new("invalid_state", "没有可采用的生成结果"));
        }
        let clothing_id = job["payload"]["clothing_id"].as_str().unwrap();
        let mut clothing = self.clothing_get(json!({"member_id":member,"id":clothing_id}))?;
        if clothing["status"] == "deleted"
            || clothing["display_asset_id"] != job["payload"]["base_display_asset_id"]
        {
            return Err(ApiError::new(
                "display_changed",
                "单品主图已改变，请保留并核对旧生成预览",
            ));
        }
        let source_id = &job["payload"]["asset_id"];
        let mut ids = clothing["asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let index = ids
            .iter()
            .position(|v| v == source_id)
            .ok_or_else(|| ApiError::new("display_changed", "来源图片已移除，请核对后重新生成"))?;
        let asset_id = &job["result"]["asset_id"];
        let mut asset = self.asset_get(json!({"member_id":member,"id":asset_id}))?;
        asset["state"] = json!("adopted");
        asset["display"]["state"] = json!("adopted");
        asset["display"]["sync_scope"] = json!("workspace");
        ids[index] = asset_id.clone();
        clothing["asset_ids"] = json!(ids);
        clothing["display_asset_id"] = asset_id.clone();
        clothing["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE assets SET document=?1 WHERE id=?2 AND member_id=?3",
            rusqlite::params![asset.to_string(), asset_id.as_str(), member],
        )?;
        tx.execute(
            "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
            rusqlite::params![clothing.to_string(), clothing_id, member],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'product_adopt',?2)",
            rusqlite::params![clothing_id, clothing.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            rusqlite::params![clothing_id, clothing.to_string()],
        )?;
        job["state"] = json!("succeeded");
        job["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        let pending = self.save_job(&tx, &job)?;
        tx.commit()?;
        self.emit_job_event(pending);
        Ok(job)
    }
}
pub(super) fn check_lease(doc: &Value, lease: &str) -> Result<(), ApiError> {
    if doc["state"] != "running"
        || doc["lease_token"] != lease
        || doc["lease_expires"].as_i64().unwrap_or(0) < chrono::Utc::now().timestamp_millis()
    {
        return Err(ApiError::new("lease_lost", "任务已取消或过期，旧图保留"));
    }
    Ok(())
}
fn bounded_json(response: reqwest::blocking::Response, max: u64) -> Result<Value, ApiError> {
    use std::io::Read;
    let mut bytes = Vec::new();
    response
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ApiError::new("provider_response", "读取供应商响应失败"))?;
    if bytes.len() as u64 > max {
        return Err(ApiError::new("provider_response", "供应商响应过大"));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::new("provider_response", "供应商响应格式错误"))
}

fn source_image(asset: &Value) -> Result<image::DynamicImage, ApiError> {
    use sha2::Digest;
    let path = asset["original"]["local_path"].as_str().ok_or_else(|| {
        ApiError::new(
            "original_unavailable",
            "请在有原图的设备生成，或重新导入原图",
        )
    })?;
    let metadata = std::fs::metadata(path)
        .map_err(|_| ApiError::new("original_unavailable", "本机原图不可读，旧图保留"))?;
    if metadata.len() > 30 * 1024 * 1024 {
        return Err(ApiError::new("image_limit", "Seedream 原图须不超过30MB"));
    }
    let bytes =
        std::fs::read(path).map_err(|_| ApiError::new("original_unavailable", "本机原图不可读"))?;
    let hash = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if asset["original"]["id"] != hash {
        return Err(ApiError::new(
            "original_checksum_mismatch",
            "本机原图改变，请重新导入",
        ));
    }
    let image = crate::assets::decode(&bytes)?;
    validate_reference(&image)?;
    Ok(image)
}
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(ApiError::new("invalid_input", "商品图操作包含不支持字段"));
    }
    Ok(())
}
pub(super) fn descriptors() -> Vec<Value> {
    let mut result = vec![
        json!({"name":"product.generate","description":"明确上传所选单品原图，创建商品展示图任务；可能使用供应商额度，旧图不变","effect":{"kind":"mutation","network":"none","local_files":true,"async":null,"conditional_effects":[{"when":{"submission_pending":true},"effect":{"kind":"mutation","network":"explicit-provider"}}]},"examples":[{"request":{"member_id":"<已授权成员ID>","clothing_id":"<单品ID>","asset_id":"<资产ID>","provider_id":"<已配置图片供应商ID>","operation_key":"<唯一操作标识>","upload_original":true},"prerequisites":["明确上传所选单品原图创建商品图任务；真实提交消耗供应商额度","旧图保持不变直到显式采用；本示例不自动执行"]}],"inputSchema":{"type":"object","required":["member_id","clothing_id","asset_id","provider_id","operation_key","upload_original"],"properties":{"member_id":{"type":"string"},"clothing_id":{"type":"string"},"asset_id":{"type":"string"},"provider_id":{"type":"string"},"operation_key":{"type":"string"},"upload_original":{"const":true}},"additionalProperties":false},"outputSchema":crate::jobs::output_schema()}),
    ];
    for name in ["product.process", "product.adopt"] {
        let (effect,examples)=match name {
            "product.process"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null,"conditional_effects":[{"when":{"submission_pending":true},"effect":{"kind":"mutation","network":"explicit-provider"}}]}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<商品图任务ID>"},"prerequisites":["推进已入队商品图任务；首次真实提交消耗供应商额度","未知结果先查询不盲重发；本示例不自动执行"]}])),"product.adopt"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<商品图任务ID>"},"prerequisites":["显式采用后压缩展示图成为当前图；拒绝/取消/失败保留旧图","仅最终采用的商品图具备同步资格"]}])),_=>unreachable!()
        };
        result.push(json!({"name":name,"effect":effect,"examples":examples,"description":"处理或明确采用商品图，保持旧图直到采用","inputSchema":{"type":"object","required":["member_id","id"],"properties":{"member_id":{"type":"string"},"id":{"type":"string"}},"additionalProperties":false},"outputSchema":crate::jobs::output_schema()}));
    }
    result
}

pub(super) fn validate_provider(provider: &Value) -> Result<(), ApiError> {
    if provider["adapter"] != "ark_seedream"
        || !provider["capabilities"]
            .as_array()
            .is_some_and(|v| v.contains(&json!("image_generation")))
    {
        return Err(ApiError::new(
            "unsupported_capability",
            "请选择专用 Seedream 图片模型",
        ));
    }
    if !provider["model"]
        .as_str()
        .unwrap_or("")
        .starts_with("doubao-seedream-5-0-flash-")
    {
        return Err(ApiError::new(
            "unsupported_model",
            "当前专用适配器支持Seedream5.0Flash，请确认开通的模型ID",
        ));
    }
    Ok(())
}
pub(super) fn validate_reference(image: &image::DynamicImage) -> Result<(), ApiError> {
    let (w, h) = (image.width(), image.height());
    if w < 15 || h < 15 || u64::from(w) * u64::from(h) > 36_000_000 || w > h * 16 || h > w * 16 {
        return Err(ApiError::new(
            "image_limit",
            "输入每边须大于14px、最多3600万像素，比例1:16至16:1",
        ));
    }
    Ok(())
}
