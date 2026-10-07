use crate::{string, ApiError, Application};
use serde_json::{json, Value};
use sha2::Digest;
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(ApiError::new("invalid_input", "试穿操作包含不支持字段"));
    }
    Ok(())
}
fn read_private(result: &Value) -> Result<(Vec<u8>, image::DynamicImage), ApiError> {
    let path = string(result, "local_path")?;
    let meta = std::fs::metadata(path)
        .map_err(|_| ApiError::new("image_unavailable", "本机私有图片缺失"))?;
    if !meta.is_file() || meta.len() > 50 * 1024 * 1024 {
        return Err(ApiError::new("image_limit", "图片大小超限"));
    }
    let bytes = std::fs::read(path)
        .map_err(|_| ApiError::new("image_unavailable", "本机私有图片不可读"))?;
    let hash = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if result["blob_hash"] != hash {
        return Err(ApiError::new("image_checksum_mismatch", "本机私有图片改变"));
    }
    let image = crate::assets::decode(&bytes)?;
    crate::product::validate_reference(&image)?;
    Ok((bytes, image))
}
impl Application {
    pub(super) fn tryon_person_import(&self, input: Value) -> Result<Value, ApiError> {
        fields(
            &input,
            &[
                "member_id",
                "path",
                "person_kind",
                "rights_confirmed",
                "operation_key",
            ],
        )?;
        let member = self.member(&input)?;
        let kind = string(&input, "person_kind")?;
        if !["self", "model"].contains(&kind) || input["rights_confirmed"] != true {
            return Err(ApiError::new(
                "person_consent_required",
                "请选择本人或模特，并确认有权使用此图片",
            ));
        }
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(ApiError::new("invalid_input", "操作标识须为1–200字节"));
        }
        // A job is a local private record, never an adoptable/syncable clothing asset.
        let path = string(&input, "path")?;
        let meta = std::fs::metadata(path)
            .map_err(|_| ApiError::new("image_unavailable", "请选择本机人物图片"))?;
        if !meta.is_file() || meta.len() > 30 * 1024 * 1024 {
            return Err(ApiError::new("image_limit", "人物图片须小于30MB"));
        }
        let image = crate::assets::decode(
            &std::fs::read(path).map_err(|_| ApiError::new("image_unavailable", "图片不可读"))?,
        )?;
        crate::product::validate_reference(&image)?;
        let mut png = std::io::Cursor::new(vec![]);
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|_| ApiError::new("image_processing_failed", "人物图片处理失败"))?;
        let bytes = png.into_inner();
        if bytes.len() > 30 * 1024 * 1024 {
            return Err(ApiError::new("image_limit", "标准化人物图片超过30MB"));
        }
        let (hash, path) = self.store_blob("tryon-person-local-only", &bytes, "png")?;
        let tx = self.begin_write()?;
        let mut job = crate::jobs::insert(
            &tx,
            &member,
            "tryon_person",
            key,
            json!({"person_kind":kind,"rights_confirmed":true,"sync_scope":"local_only","blob_hash":hash}),
        )?;
        if job["state"] == "succeeded" {
            return Ok(job);
        }
        job["state"] = json!("succeeded");
        job["progress"] = json!(100);
        job["result"] = json!({"id":job["id"],"local_path":path,"blob_hash":hash,"mime":"image/png","width":image.width(),"height":image.height(),"sync_scope":"local_only","person_kind":kind});
        crate::jobs::save(&tx, &job)?;
        tx.commit()?;
        Ok(job)
    }
    pub(super) fn tryon_generate(&self, input: Value) -> Result<Value, ApiError> {
        fields(
            &input,
            &[
                "member_id",
                "person_id",
                "outfit_id",
                "version",
                "provider_id",
                "operation_key",
                "upload_images",
            ],
        )?;
        if input["upload_images"] != true {
            return Err(ApiError::new(
                "upload_consent_required",
                "请允许发送人物与全部衣物图片，并使用供应商额度",
            ));
        }
        let member = self.member(&input)?;
        let person = self.job_get(json!({"member_id":member,"id":string(&input,"person_id")?}))?;
        if person["kind"] != "tryon_person" || person["state"] != "succeeded" {
            return Err(ApiError::new("invalid_input", "请选择已确认的本机人物图片"));
        }
        read_private(&person["result"])?;
        let outfit = self.execute(
            "outfit.version",
            json!({"member_id":member,"id":string(&input,"outfit_id")?,"version":input["version"]}),
        )?;
        let items = outfit["items"]
            .as_array()
            .ok_or_else(|| ApiError::new("storage_error", "搭配版本缺少单品"))?;
        if items.is_empty() || items.len() > 9 {
            return Err(ApiError::new(
                "image_count_limit",
                "Seedream Flash最多10张参考图：人物1张与衣物1–9张；未上传，不截断",
            ));
        }
        let provider = self.provider_get(string(&input, "provider_id")?)?;
        crate::product::validate_provider(&provider)?;
        if !provider["capabilities"]
            .as_array()
            .is_some_and(|c| c.contains(&json!("virtual_tryon")))
        {
            return Err(ApiError::new(
                "unsupported_capability",
                "请选择明确配置虚拟试穿能力的专用图片模型",
            ));
        }
        let mut inputs = vec![];
        for item in items {
            let id = item["snapshot"]["display_asset_id"]
                .as_str()
                .ok_or_else(|| ApiError::new("image_missing", "保存版本有衣物缺图，不跳过单品"))?;
            let asset = self.asset_get(json!({"member_id":member,"id":id}))?;
            crate::product::validate_reference(&crate::image_editing::read_display(&asset)?)?;
            inputs.push(json!({"clothing_id":item["clothing_id"],"asset_id":id,"display_hash":asset["display"]["id"],"role":item["role"],"layer":item["layer"]}));
        }
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(ApiError::new("invalid_input", "操作标识须为1–200字节"));
        }
        self.job_enqueue(&member,"virtual_tryon",key,json!({"person":person["result"],"outfit_id":outfit["id"],"outfit_version":outfit["version"],"inputs":inputs,"provider":provider,"upload_images":true,"sync_scope":"local_only"}))
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    let mut result = vec![
        json!({"name":"tryon.person.import","description":"确认使用权后导入本人或模特图片；仅本机，不进入衣物或同步","inputSchema":{"type":"object","required":["member_id","path","person_kind","rights_confirmed","operation_key"],"properties":{"member_id":{"type":"string"},"path":{"type":"string"},"person_kind":{"enum":["self","model"]},"rights_confirmed":{"const":true},"operation_key":{"type":"string"}},"additionalProperties":false},"outputSchema":crate::jobs::output_schema()}),
        json!({"name":"tryon.generate","description":"明确发送人物及保存搭配全部衣物生成试穿；仅本机，不预测尺码/合身","inputSchema":{"type":"object","required":["member_id","person_id","outfit_id","version","provider_id","operation_key","upload_images"],"properties":{"member_id":{"type":"string"},"person_id":{"type":"string"},"outfit_id":{"type":"string"},"version":{"type":"integer","minimum":1},"provider_id":{"type":"string"},"operation_key":{"type":"string"},"upload_images":{"const":true}},"additionalProperties":false},"outputSchema":crate::jobs::output_schema()}),
        json!({"name":"tryon.compare","description":"比较1–4个本机试穿方案，各自明确前图、结果与输入；不预测合身","effect":{"kind":"query","network":"none","local_files":false,"async":null},"examples":[{"request":{"member_id":"<已授权成员ID>","ids":["<项目>"]},"prerequisites":["比较1-4个本机试穿方案，各自明确前图、结果与输入；不预测合身"]}],"inputSchema":{"type":"object","required":["member_id","ids"],"properties":{"member_id":{"type":"string"},"ids":{"type":"array","minItems":1,"maxItems":4,"uniqueItems":true,"items":{"type":"string"}}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","sync_scope","fit_guarantee","items"],"properties":{"schema_version":{"const":1},"sync_scope":{"const":"local_only"},"fit_guarantee":{"const":false},"items":{"type":"array","minItems":1,"maxItems":4}}}}),
    ];
    for name in [
        "tryon.process",
        "tryon.save",
        "tryon.preview",
        "tryon.person.preview",
    ] {
        result.push(json!({"name":name,"description":"处理、保存或预览本机私有试穿，不改变搭配/穿着/同步","inputSchema":{"type":"object","required":["member_id","id"],"properties":{"member_id":{"type":"string"},"id":{"type":"string"}},"additionalProperties":false},"outputSchema":if name.ends_with("preview"){json!({"type":"object","required":["schema_version","id","data_url","sync_scope","width","height"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"data_url":{"type":"string"},"sync_scope":{"const":"local_only"},"width":{"type":"integer"},"height":{"type":"integer"}}})}else{crate::jobs::output_schema()}}));
    }
    result
}

impl Application {
    pub(super) fn tryon_process(&self, input: Value) -> Result<Value, ApiError> {
        self.cloud_image_process(input, "virtual_tryon")
    }
    pub(super) fn tryon_request(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        job: &Value,
    ) -> Result<Value, ApiError> {
        let (_, person) = read_private(&job["payload"]["person"])?;
        let mut images = vec![person];
        for reference in job["payload"]["inputs"]
            .as_array()
            .ok_or_else(|| ApiError::new("storage_error", "试穿图输入记录缺失"))?
        {
            let asset = self.asset_get(json!({"member_id":member,"id":reference["asset_id"]}))?;
            if asset["display"]["id"] != reference["display_hash"] {
                return Err(ApiError::new(
                    "source_changed",
                    "保存版本的图片改变，请核对后创建新意图",
                ));
            }
            images.push(crate::image_editing::read_display(&asset)?);
        }
        let prompt=format!("Create one virtual try-on image. The person in reference image 1 must wear EVERY garment in reference images 2 onward. Preserve the person identity, body shape, pose and background. Preserve each garment exact color, silhouette, fabric, texture, print, logos and construction. Respect these garment roles and layering: {}. Do not omit, duplicate, blend, invent or replace garments. This is an illustrative outfit visualization, not a prediction of size or physical fit.",job["payload"]["inputs"]);
        let (image, request_id) = self.ark_generate(member, id, lease, job, &images, &prompt)?;
        let tx = self.begin_write()?;
        let mut doc = self.job_get(json!({"member_id":member,"id":id}))?;
        crate::product::check_lease(&doc, lease)?;
        let bytes = webp::Encoder::from_image(&image)
            .map_err(|_| {
                ApiError::new(
                    "image_processing_failed",
                    "试穿图压缩失败，原搭配和拼贴保留",
                )
            })?
            .encode(82.0)
            .to_vec();
        let (hash, path) = self.store_blob("tryon-generated-local-only", &bytes, "webp")?;
        doc["state"] = json!("waiting_input");
        doc["progress"] = json!(100);
        doc["lease_token"] = Value::Null;
        doc["lease_expires"] = json!(0);
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        doc["result"] = json!({"id":uuid::Uuid::now_v7().to_string(),"blob_hash":hash,"local_path":path,"role":"tryon","sync_scope":"local_only","width":image.width(),"height":image.height(),"bytes":bytes.len(),"mime":"image/webp","quality":82,"request_id":request_id,"source_outfit_id":job["payload"]["outfit_id"],"source_version":job["payload"]["outfit_version"],"person":job["payload"]["person"],"inputs":job["payload"]["inputs"],"provider_id":job["payload"]["provider"]["id"],"model":job["payload"]["provider"]["model"],"adopted":false});
        crate::jobs::save(&tx, &doc)?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn tryon_save(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id"])?;
        let tx = self.begin_write()?;
        let mut doc = self.job_get(input)?;
        if doc["kind"] != "virtual_tryon" {
            return Err(ApiError::new("invalid_input", "不是AI试穿图任务"));
        }
        if doc["state"] == "succeeded" {
            return Ok(doc);
        }
        if doc["state"] != "waiting_input" || !doc["result"]["id"].is_string() {
            return Err(ApiError::new("invalid_state", "无可保存的生成预览"));
        }
        doc["state"] = json!("succeeded");
        doc["result"]["adopted"] = json!(true);
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        crate::jobs::save(&tx, &doc)?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn tryon_preview(&self, input: Value) -> Result<Value, ApiError> {
        use base64::Engine;
        use sha2::Digest;
        fields(&input, &["member_id", "id"])?;
        let doc = self.job_get(input)?;
        if doc["kind"] != "virtual_tryon" || !doc["result"]["id"].is_string() {
            return Err(ApiError::new("not_found", "没有可预览的试穿图"));
        }
        let result = &doc["result"];
        let path = result["local_path"]
            .as_str()
            .ok_or_else(|| ApiError::new("image_unavailable", "本机试穿图缺失"))?;
        let meta = std::fs::metadata(path)
            .map_err(|_| ApiError::new("image_unavailable", "本机试穿图缺失"))?;
        if !meta.is_file() || meta.len() > 50 * 1024 * 1024 {
            return Err(ApiError::new("image_limit", "本机试穿图大小异常"));
        }
        let bytes = std::fs::read(path)
            .map_err(|_| ApiError::new("image_unavailable", "本机试穿图不可读"))?;
        let actual = sha2::Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if result["blob_hash"] != actual {
            return Err(ApiError::new(
                "image_checksum_mismatch",
                "本机试穿图改变，请重新核对",
            ));
        }
        Ok(
            json!({"schema_version":1,"id":result["id"],"data_url":format!("data:image/webp;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)),"role":"tryon","sync_scope":"local_only","source_outfit_id":result["source_outfit_id"],"source_version":result["source_version"],"person":result["person"],"provider_id":result["provider_id"],"model":result["model"],"inputs":result["inputs"],"width":result["width"],"height":result["height"],"adopted":result["adopted"]}),
        )
    }
}

impl Application {
    pub(super) fn tryon_person_preview(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id"])?;
        let job = self.job_get(input)?;
        if job["kind"] != "tryon_person" || job["state"] != "succeeded" {
            return Err(ApiError::new("not_found", "人物图片不存在"));
        }
        private_preview(&job["result"])
    }
    pub(super) fn tryon_compare(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "ids"])?;
        let member = self.member(&input)?;
        let ids = input["ids"]
            .as_array()
            .ok_or_else(|| ApiError::new("invalid_input", "请选择1–4个试穿方案"))?;
        if ids.is_empty() || ids.len() > 4 {
            return Err(ApiError::new("comparison_limit", "一次比较1–4个方案"));
        }
        let mut items = vec![];
        let mut unique = std::collections::HashSet::new();
        for id in ids {
            let id = id
                .as_str()
                .ok_or_else(|| ApiError::new("invalid_input", "方案须为ID"))?;
            if !unique.insert(id) {
                return Err(ApiError::new("invalid_input", "请选择不同方案"));
            }
            let job = self.job_get(json!({"member_id":member,"id":id}))?;
            let after = self.tryon_preview(json!({"member_id":member,"id":id}))?;
            let before = private_preview(&job["payload"]["person"])?;
            items.push(json!({"job_id":id,"before":before,"after":after}));
        }
        Ok(
            json!({"schema_version":1,"sync_scope":"local_only","fit_guarantee":false,"items":items}),
        )
    }
}
fn private_preview(result: &Value) -> Result<Value, ApiError> {
    use base64::Engine;
    let (bytes, _) = read_private(result)?;
    Ok(
        json!({"schema_version":1,"id":result["id"],"person_kind":result["person_kind"],"width":result["width"],"height":result["height"],"sync_scope":"local_only","data_url":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes))}),
    )
}
