use crate::{string, ApiError, Application};
use serde_json::{json, Value};
impl Application {
    pub(super) fn flatlay_generate(&self, input: Value) -> Result<Value, ApiError> {
        fields(
            &input,
            &[
                "member_id",
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
                "请明确允许发送此保存版本的全部商品展示图并使用供应商额度",
            ));
        }
        let member = self.member(&input)?;
        let outfit = self.execute(
            "outfit.version",
            json!({"member_id":member,"id":string(&input,"outfit_id")?,"version":input["version"]}),
        )?;
        let items = outfit["items"]
            .as_array()
            .ok_or_else(|| ApiError::new("storage_error", "搭配版本缺少单品"))?;
        if items.is_empty() || items.len() > 10 {
            return Err(ApiError::new(
                "image_count_limit",
                "Seedream Flash支持1–10张参考图；该保存版本超限，未上传任何图，不会截断",
            ));
        }
        let provider = self.provider_get(string(&input, "provider_id")?)?;
        crate::product::validate_provider(&provider)?;
        let mut inputs = vec![];
        for item in items {
            let id = item["snapshot"]["display_asset_id"]
                .as_str()
                .ok_or_else(|| {
                    ApiError::new(
                        "image_missing",
                        "保存版本有单品缺图，请补图保存新版本或使用离线拼贴；不跳过该单品",
                    )
                })?;
            let asset = self.asset_get(json!({"member_id":member,"id":id}))?;
            let image = crate::image_editing::read_display(&asset)?;
            crate::product::validate_reference(&image)?;
            inputs.push(json!({"clothing_id":item["clothing_id"],"asset_id":id,"display_hash":asset["display"]["id"],"role":item["role"],"layer":item["layer"]}));
        }
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(ApiError::new("invalid_input", "操作标识须为1–200字节"));
        }
        self.job_enqueue(&member,"outfit_flatlay",key,json!({"outfit_id":outfit["id"],"outfit_version":outfit["version"],"inputs":inputs,"provider":provider,"upload_images":true,"sync_scope":"local_only"}))
    }
}
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(ApiError::new("invalid_input", "搭配图操作包含不支持字段"));
    }
    Ok(())
}
pub(super) fn descriptors() -> Vec<Value> {
    let mut result = vec![
        json!({"name":"flatlay.generate","description":"明确发送保存版本的全部商品展示图，创建仅本地AI搭配图；可能使用供应商额度，不产生穿着","effect":{"kind":"mutation","network":"none","local_files":true,"async":null,"conditional_effects":[{"when":{"submission_pending":true},"effect":{"kind":"mutation","network":"explicit-provider"}}]},"examples":[{"request":{"member_id":"<已授权成员ID>","outfit_id":"<搭配ID>","version":1,"provider_id":"<已配置图片供应商ID>","operation_key":"<唯一操作标识>","upload_images":true},"prerequisites":["明确发送保存版本全部商品展示图创建AI搭配图；真实提交消耗额度","失败保留已存搭配与离线拼贴；不产生穿着；本示例不自动执行"]}],"inputSchema":{"type":"object","required":["member_id","outfit_id","version","provider_id","operation_key","upload_images"],"properties":{"member_id":{"type":"string"},"outfit_id":{"type":"string"},"version":{"type":"integer","minimum":1},"provider_id":{"type":"string"},"operation_key":{"type":"string"},"upload_images":{"const":true}},"additionalProperties":false},"outputSchema":crate::jobs::output_schema()}),
    ];
    for name in ["flatlay.process", "flatlay.save", "flatlay.preview"] {
        let (effect,examples)=match name {
            "flatlay.process"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null,"conditional_effects":[{"when":{"submission_pending":true},"effect":{"kind":"mutation","network":"explicit-provider"}}]}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<搭配图任务ID>"},"prerequisites":["推进搭配图任务；首次真实提交发送保存版本全部商品展示图并消耗额度","失败保留已存搭配与离线拼贴；本示例不自动执行"]}])),"flatlay.save"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<搭配图任务ID>"},"prerequisites":["显式保存AI搭配图，仅本地；不改变搭配模板或穿着"]}])),"flatlay.preview"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<搭配图任务ID>"},"prerequisites":["预览仅本地AI搭配图或离线拼贴；不改变衣物、搭配模板或穿着"]}])),_=>unreachable!()
        };
        result.push(json!({"name":name,"effect":effect,"examples":examples,"description":"处理/预览/明确保存仅本地AI搭配图，不改变衣物、搭配模板或穿着","inputSchema":{"type":"object","required":["member_id","id"],"properties":{"member_id":{"type":"string"},"id":{"type":"string"}},"additionalProperties":false},"outputSchema":if name=="flatlay.preview"{json!({"type":"object","required":["schema_version","id","data_url","role","sync_scope","source_outfit_id","source_version","inputs","width","height","adopted"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"data_url":{"type":"string"},"role":{"const":"outfit"},"sync_scope":{"const":"local_only"},"source_outfit_id":{"type":"string"},"source_version":{"type":"integer","minimum":1},"inputs":{"type":"array","minItems":1,"maxItems":10},"width":{"type":"integer"},"height":{"type":"integer"},"adopted":{"type":"boolean"}}})}else{crate::jobs::output_schema()}}));
    }
    result
}

impl Application {
    pub(super) fn flatlay_process(&self, input: Value) -> Result<Value, ApiError> {
        self.cloud_image_process(input, "outfit_flatlay")
    }
    pub(super) fn flatlay_request(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        job: &Value,
    ) -> Result<Value, ApiError> {
        let mut images = vec![];
        for reference in job["payload"]["inputs"]
            .as_array()
            .ok_or_else(|| ApiError::new("storage_error", "搭配图输入记录缺失"))?
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
        let(image,request_id)=self.ark_generate(member,id,lease,job,&images,"Create one flat-lay studio outfit photograph containing EVERY referenced garment as separate carefully arranged pieces on a plain neutral background. No person, no mannequin, no new garments. Preserve each garment's exact colors, silhouette, fabric, texture, patterns, printed text, logos and construction. Include all reference garments without omitting, duplicating, blending or replacing pieces. Arrange a coherent complete outfit viewed from directly above.")?;
        let tx = self.begin_write()?;
        let mut doc = self.job_get(json!({"member_id":member,"id":id}))?;
        crate::product::check_lease(&doc, lease)?;
        let bytes = webp::Encoder::from_image(&image)
            .map_err(|_| {
                ApiError::new(
                    "image_processing_failed",
                    "搭配图压缩失败，原搭配和拼贴保留",
                )
            })?
            .encode(82.0)
            .to_vec();
        let (hash, path) = self.store_blob("outfit-generated-local-only", &bytes, "webp")?;
        doc["state"] = json!("waiting_input");
        doc["progress"] = json!(100);
        doc["lease_token"] = Value::Null;
        doc["lease_expires"] = json!(0);
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        doc["result"] = json!({"id":uuid::Uuid::now_v7().to_string(),"blob_hash":hash,"local_path":path,"role":"outfit","sync_scope":"local_only","width":image.width(),"height":image.height(),"bytes":bytes.len(),"mime":"image/webp","quality":82,"request_id":request_id,"source_outfit_id":job["payload"]["outfit_id"],"source_version":job["payload"]["outfit_version"],"inputs":job["payload"]["inputs"],"provider_id":job["payload"]["provider"]["id"],"model":job["payload"]["provider"]["model"],"adopted":false});
        self.save_job(&tx, &doc)?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn flatlay_save(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id"])?;
        let tx = self.begin_write()?;
        let mut doc = self.job_get(input)?;
        if doc["kind"] != "outfit_flatlay" {
            return Err(ApiError::new("invalid_input", "不是AI搭配图任务"));
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
        self.save_job(&tx, &doc)?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn flatlay_preview(&self, input: Value) -> Result<Value, ApiError> {
        use base64::Engine;
        use sha2::Digest;
        fields(&input, &["member_id", "id"])?;
        let doc = self.job_get(input)?;
        if doc["kind"] != "outfit_flatlay" || !doc["result"]["id"].is_string() {
            return Err(ApiError::new("not_found", "没有可预览的搭配图"));
        }
        let result = &doc["result"];
        let path = result["local_path"]
            .as_str()
            .ok_or_else(|| ApiError::new("image_unavailable", "本机搭配图缺失"))?;
        let meta = std::fs::metadata(path)
            .map_err(|_| ApiError::new("image_unavailable", "本机搭配图缺失"))?;
        if !meta.is_file() || meta.len() > 50 * 1024 * 1024 {
            return Err(ApiError::new("image_limit", "本机搭配图大小异常"));
        }
        let bytes = std::fs::read(path)
            .map_err(|_| ApiError::new("image_unavailable", "本机搭配图不可读"))?;
        let actual = sha2::Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if result["blob_hash"] != actual {
            return Err(ApiError::new(
                "image_checksum_mismatch",
                "本机搭配图改变，请重新核对",
            ));
        }
        Ok(
            json!({"schema_version":1,"id":result["id"],"data_url":format!("data:image/webp;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)),"role":"outfit","sync_scope":"local_only","source_outfit_id":result["source_outfit_id"],"source_version":result["source_version"],"inputs":result["inputs"],"width":result["width"],"height":result["height"],"adopted":result["adopted"]}),
        )
    }
}
