use crate::{jobs, string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::HashSet;
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS recognition_suggestions(id TEXT PRIMARY KEY,job_id TEXT NOT NULL UNIQUE REFERENCES jobs(id),member_id TEXT NOT NULL REFERENCES members(id),document TEXT NOT NULL); CREATE INDEX IF NOT EXISTS recognition_member ON recognition_suggestions(member_id,id);")?;
    Ok(())
}
fn invalid(message: &str) -> ApiError {
    ApiError::new("invalid_input", message)
}
impl Application {
    pub(super) fn recognition_create(&self, mut input: Value) -> Result<Value, ApiError> {
        let allowed = [
            "member_id",
            "operation_key",
            "provider_id",
            "clothing_id",
            "batch_id",
            "draft_id",
            "asset_ids",
            "upload_role",
            "confirm_upload",
        ];
        if input
            .as_object()
            .unwrap()
            .keys()
            .any(|k| !allowed.contains(&k.as_str()))
        {
            return Err(invalid("识别包含不支持的字段"));
        }
        let member = self.member(&input)?;
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
        let tx = self.begin_write()?;
        let existing:Option<String>=tx.query_row("SELECT document FROM jobs WHERE member_id=?1 AND kind='recognition' AND operation_key=?2",params![member,key],|r|r.get(0)).optional()?;
        if let Some(existing) = existing {
            let job: Value = serde_json::from_str(&existing)
                .map_err(|_| ApiError::new("storage_error", "识别任务损坏"))?;
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
        let (clothing, associated) = self.recognition_target(&member, &input)?;
        let ids = input["asset_ids"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 20)
            .ok_or_else(|| invalid("请选择1–20张图片"))?;
        let mut seen = HashSet::new();
        let mut assets = vec![];
        for id in ids {
            let id = id.as_str().ok_or_else(|| invalid("图片ID须为字符串"))?;
            if !seen.insert(id) {
                return Err(invalid("图片不能重复"));
            }
            if !associated.contains(&json!(id)) {
                return Err(ApiError::new(
                    "asset_not_associated",
                    "请选择该单品已关联的图片",
                ));
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
                "recognition_images_too_large",
                "识别所选源图片合计最多20MiB，请减少所选图片",
            ));
        }
        let payload = json!({"request":input,"provider":provider,"asset_ids":ids,"upload_role":role,"assets":assets,"baseline":clothing,"submission":"not_submitted"});
        let job = jobs::insert(&tx, &member, "recognition", key, payload)?;
        tx.commit()?;
        Ok(job)
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    let proposal = json!({"type":"object","required":["schema_version","id","job_id","member_id","state","target","baseline","raw_text","suggested_facts","uncertainties","provenance"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"state":{"enum":["pending_review","reviewed","dismissed"]},"raw_text":{"type":"string"}}});
    ["create","process","get","list","review","dismiss"].iter().map(|action|{let mut properties=json!({"member_id":{"type":"string"}});let required=match *action{"create"=>{properties["operation_key"]=json!({"type":"string","minLength":1,"maxLength":200});properties["provider_id"]=json!({"type":"string"});properties["asset_ids"]=json!({"type":"array","minItems":1,"maxItems":20,"uniqueItems":true,"items":{"type":"string"}});properties["upload_role"]=json!({"enum":["original","product_display"]});properties["confirm_upload"]=json!({"const":true});vec!["member_id","operation_key","provider_id","asset_ids","upload_role","confirm_upload"]},"list"=>vec!["member_id"],"review"=>{properties["facts"]=json!({"type":"object","minProperties":1});properties["confirm_review"]=json!({"const":true});vec!["member_id","id","facts","confirm_review"]},_=>vec!["member_id","id"]};if ["create","list"].contains(action){for k in ["clothing_id","batch_id","draft_id"]{properties[k]=json!({"type":"string"});}}else{properties["id"]=json!({"type":"string"});}let mut schema=json!({"type":"object","required":required,"additionalProperties":false,"properties":properties});if *action=="create"{schema["oneOf"]=json!([{"required":["clothing_id"],"not":{"anyOf":[{"required":["batch_id"]},{"required":["draft_id"]}]}},{"required":["batch_id","draft_id"],"not":{"required":["clothing_id"]}}]);}let output=match *action{"create"|"process"=>jobs::output_schema(),"list"=>json!({"type":"object","required":["schema_version","items"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":proposal}}}),_=>proposal.clone()};let (effect,examples)=match *action {"create"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null,"conditional_effects":[{"when":{"job_state":"after_real_submission"},"effect":{"kind":"mutation","network":"explicit-provider"}}]}),json!([{"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>","provider_id":"<已配置视觉供应商ID>","clothing_id":"<单品ID>","asset_ids":["<资产ID>"],"upload_role":"product_display","confirm_upload":true},"prerequisites":["仅上传明确确认的选择图片；视觉调用真实消耗供应商额度","上传先持久化任务收据；结果进入人工审核不自动覆盖人工值","本示例不自动执行"]}])),"process"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null,"conditional_effects":[{"when":{"submission_pending":true},"effect":{"kind":"mutation","network":"explicit-provider"}}]}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<识别任务ID>"},"prerequisites":["推进已入队识别任务；首次执行真实提交时消耗供应商额度","重启后不盲重发；截断/未知结果持久化并进入待核对","本示例不自动执行"]}])),"get"=>(json!({"kind":"query","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<建议ID>"},"prerequisites":["读取识别建议：建议值、基线、来源与不确定性说明"]}])),"list"=>(json!({"kind":"query","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["按建议ID倒序列出识别建议"]}])),"review"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<建议ID>","facts":{"description":"白衬衫"},"confirm_review":true},"prerequisites":["人工逐项审核后确认；仅可观察字段，类别须在分类字典内","人工值优先且不自动覆盖；审核写入真实衣物事实"]}])),"dismiss"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<建议ID>"},"prerequisites":["放弃建议，不改变任何衣物事实"]}])),_=>unreachable!()};json!({"name":format!("recognition.{action}"),"effect":effect,"examples":examples,"description":"识别建议及明确人工审核；只发送选择的图片，人工值不自动覆盖","inputSchema":schema,"outputSchema":output})}).collect()
}
const PROMPT: &str = "Describe only the visible garment type and main colors in this image. Say which details are uncertain. Do not guess brand, size, material, price, season or hidden features. This is an unverified visual suggestion for a person to review.";
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .unwrap()
        .keys()
        .any(|k| !allowed.contains(&k.as_str()))
    {
        return Err(invalid("识别包含不支持字段"));
    }
    Ok(())
}
// Conservative, model-independent taxonomy normalization of visible-description words.
// These are editable candidates, never facts or calibrated confidence estimates.
fn normalized_candidates(text: &str) -> Value {
    let lower = text.to_lowercase();
    let words: HashSet<&str> = lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|s| !s.is_empty())
        .collect();
    let mut facts = json!({"description":text});
    if [
        "maybe",
        "possibly",
        "might",
        "not a",
        "cannot identify",
        "can't identify",
        "no garment",
        "unsure what",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        return facts;
    }
    if words.contains("not") || words.contains("no") {
        return facts;
    }
    let mut candidates = HashSet::new();
    for (word, category) in [
        ("shirt", "top"),
        ("shirts", "top"),
        ("blouse", "top"),
        ("hoodie", "top"),
        ("sweater", "top"),
        ("trousers", "bottom"),
        ("pants", "bottom"),
        ("pant", "bottom"),
        ("jeans", "bottom"),
        ("skirt", "bottom"),
        ("shorts", "bottom"),
        ("coat", "outerwear"),
        ("jacket", "outerwear"),
        ("blazer", "outerwear"),
        ("dress", "dress"),
        ("jumpsuit", "one_piece"),
        ("shoe", "shoes"),
        ("shoes", "shoes"),
        ("sneaker", "shoes"),
        ("sneakers", "shoes"),
        ("boots", "shoes"),
        ("sandals", "shoes"),
        ("handbag", "bag"),
        ("backpack", "bag"),
        ("scarf", "accessory"),
        ("hat", "accessory"),
        ("belt", "accessory"),
    ] {
        if words.contains(word) {
            candidates.insert(category);
        }
    }
    if candidates.len() == 1 {
        facts["category"] = json!(candidates.into_iter().next().unwrap());
    }
    let colors: Vec<_> = crate::clothing::COLORS
        .iter()
        .filter(|c| words.contains(**c))
        .copied()
        .collect();
    if !colors.is_empty() {
        facts["color"] = json!(colors);
    }
    facts
}
impl Application {
    fn recognition_target(
        &self,
        member: &str,
        input: &Value,
    ) -> Result<(Value, Vec<Value>), ApiError> {
        if input.get("clothing_id").is_some() {
            if input.get("batch_id").is_some() || input.get("draft_id").is_some() {
                return Err(invalid("请选择单品或草稿中的一个目标"));
            }
            let c =
                self.clothing_get(json!({"member_id":member,"id":string(input,"clothing_id")?}))?;
            if c["status"] == "deleted" {
                return Err(ApiError::new("invalid_state", "回收站单品不可提交识别"));
            }
            let ids = c["asset_ids"].as_array().cloned().unwrap_or_default();
            Ok((c, ids))
        } else {
            let batch =
                self.imports_get(json!({"member_id":member,"id":string(input,"batch_id")?}))?;
            let id = string(input, "draft_id")?;
            let draft = batch["drafts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["id"] == id)
                .ok_or_else(|| ApiError::new("not_found", "草稿不属于当前成员批次"))?;
            if draft["state"] != "draft" {
                return Err(ApiError::new("invalid_state", "仅未入库草稿可识别"));
            }
            let mut ids = vec![];
            for item in batch["items"].as_array().unwrap() {
                if draft["item_ids"].as_array().unwrap().contains(&item["id"])
                    && item["asset_id"].is_string()
                {
                    ids.push(item["asset_id"].clone());
                }
            }
            Ok((draft.clone(), ids))
        }
    }
    pub(super) fn recognition_get(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["member_id", "id"])?;
        let m = self.member(&input)?;
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM recognition_suggestions WHERE member_id=?1 AND id=?2",
                params![m, string(&input, "id")?],
                |r| r.get(0),
            )
            .optional()?;
        serde_json::from_str(&stored.ok_or_else(|| ApiError::new("not_found", "识别建议不存在"))?)
            .map_err(|_| ApiError::new("storage_error", "识别建议损坏"))
    }
    pub(super) fn recognition_list(&self, input: Value) -> Result<Value, ApiError> {
        fields(
            &input,
            &["member_id", "clothing_id", "batch_id", "draft_id"],
        )?;
        let m = self.member(&input)?;
        let mut statement = self.connection.prepare(
            "SELECT document FROM recognition_suggestions WHERE member_id=?1 ORDER BY id DESC",
        )?;
        let mut items = vec![];
        for row in statement.query_map([m], |r| r.get::<_, String>(0))? {
            let doc: Value = serde_json::from_str(&row?)
                .map_err(|_| ApiError::new("storage_error", "识别建议损坏"))?;
            if ["clothing_id", "batch_id", "draft_id"]
                .iter()
                .all(|k| input.get(*k).is_none() || doc["target"][*k] == input[*k])
            {
                items.push(doc);
            }
        }
        Ok(json!({"schema_version":1,"items":items}))
    }
    pub(super) fn recognition_review(
        &self,
        input: Value,
        dismiss: bool,
    ) -> Result<Value, ApiError> {
        fields(
            &input,
            if dismiss {
                &["member_id", "id"]
            } else {
                &["member_id", "id", "facts", "confirm_review"]
            },
        )?;
        let tx = self.begin_write()?;
        let mut doc =
            self.recognition_get(json!({"member_id":input["member_id"],"id":input["id"]}))?;
        if (dismiss && doc["state"] == "dismissed")
            || (!dismiss
                && doc["state"] == "reviewed"
                && input["confirm_review"] == true
                && input["facts"] == doc["approved_facts"])
        {
            return Ok(doc);
        }
        if doc["state"] != "pending_review" {
            return Err(ApiError::new("invalid_state", "建议已处理"));
        }
        if dismiss {
            doc["state"] = json!("dismissed");
        } else {
            if input["confirm_review"] != true {
                return Err(ApiError::new(
                    "review_confirmation_required",
                    "请逐项审核后确认",
                ));
            }
            let facts = input["facts"]
                .as_object()
                .filter(|f| !f.is_empty())
                .ok_or_else(|| invalid("请明确选择经审核的事实"))?;
            if facts.keys().any(|k| {
                ![
                    "name",
                    "category",
                    "sub_category",
                    "color",
                    "pattern",
                    "description",
                    "style",
                    "occasion",
                    "tags",
                ]
                .contains(&k.as_str())
            }) {
                return Err(invalid("识别审核只支持可观察字段；其他事实请手工填写"));
            }
            let (mut clothing, _) =
                self.recognition_target(doc["member_id"].as_str().unwrap(), &doc["target"])?;
            if clothing != doc["baseline"] {
                return Err(ApiError::new(
                    "review_conflict",
                    "资料已有人工修改，请重分析后重新审核",
                ));
            }
            if doc["target"]["clothing_id"].is_string() {
                crate::clothing::apply_facts(&mut clothing, &input["facts"])?;
                clothing["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
                let text = clothing.to_string();
                tx.execute(
                    "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
                    params![text, clothing["id"].as_str(), doc["member_id"].as_str()],
                )?;
                tx.execute("INSERT INTO audit(entity_id,operation,document) VALUES(?1,'recognition.review',?2)",params![clothing["id"].as_str(),text])?;
                tx.execute(
                    "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
                    params![clothing["id"].as_str(), text],
                )?;
            } else {
                let mut merged = clothing
                    .get("facts")
                    .cloned()
                    .unwrap_or(json!({"name":clothing["name"],"category":clothing["category"]}));
                for (k, v) in facts {
                    merged[k] = v.clone();
                }
                merged["member_id"] = doc["member_id"].clone();
                let checked = self.clothing_document(merged.clone())?;
                merged.as_object_mut().unwrap().remove("member_id");
                clothing["facts"] = merged;
                clothing["name"] = checked["name"].clone();
                clothing["category"] = checked["category"].clone();
                tx.execute(
                    "UPDATE import_drafts SET document=?1 WHERE id=?2 AND member_id=?3",
                    params![
                        clothing.to_string(),
                        clothing["id"].as_str(),
                        doc["member_id"].as_str()
                    ],
                )?;
            }
            doc["state"] = json!("reviewed");
            doc["approved_facts"] = input["facts"].clone();
        }
        doc["reviewed_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE recognition_suggestions SET document=?1 WHERE id=?2",
            params![doc.to_string(), doc["id"].as_str()],
        )?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn recognition_process(&self, input: Value) -> Result<Value, ApiError> {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        use std::io::Read;
        fields(&input, &["member_id", "id"])?;
        let member = self.member(&input)?;
        let id = string(&input, "id")?;
        let original = self.job_get(json!({"member_id":member,"id":id}))?;
        if original["kind"] != "recognition" {
            return Err(invalid("不是识别任务"));
        }
        if original["state"] == "succeeded" {
            return Ok(original);
        }
        let mut job = self.job_claim(&member, id, 120)?;
        let lease = job["lease_token"].as_str().unwrap().to_string();
        if job["payload"]["submission"] != "not_submitted" {
            return self.job_finish(&member,id,&lease,"waiting_input",Value::Null,json!({"code":"submission_unknown","message":"上次提交结果未知，请核实供应商后另建明确任务","retryable":false}));
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
            let mut content = vec![json!({"type":"text","text":PROMPT})];
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
                    ApiError::new("image_unavailable", "原图或展示图不可读，仍可手工保存")
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
            let mut request=client.post(format!("{}/chat/completions",p["endpoint"].as_str().unwrap())).json(&json!({"model":p["model"],"messages":[{"role":"user","content":content}],"max_tokens":512,"temperature":0}));
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
        jobs::save(&tx, &job)?;
        tx.commit()?;
        let response=match request.send(){Ok(r)=>r,Err(_)=>return self.job_finish(&member,id,&lease,"waiting_input",Value::Null,json!({"code":"submission_unknown","message":"提交后的连接失败，结果未知；不会自动再次上传","retryable":false}))};
        let status = response.status();
        if status.is_server_error() {
            return self.job_finish(&member,id,&lease,"waiting_input",Value::Null,json!({"code":"submission_unknown","http_status":status.as_u16(),"message":"供应商服务错误，是否接受提交待核实","retryable":false}));
        }
        if !status.is_success() {
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
            jobs::save(&tx, &job)?;
            tx.commit()?;
            return self.job_finish(&member,id,&lease,"failed",Value::Null,json!({"code":if retry{"provider_rate_limit"}else{"provider_rejected"},"message":"供应商拒绝请求，仍可手工保存","http_status":status.as_u16(),"retryable":retry,"retry_after_ms":delay}));
        }
        let mut bytes = vec![];
        if response
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() > 1024 * 1024
        {
            return self.job_finish(&member,id,&lease,"waiting_input",Value::Null,json!({"code":"submission_unknown","message":"响应中断或过大，提交结果须核实","retryable":false}));
        }
        let response:Value=match serde_json::from_slice(&bytes){Ok(v)=>v,Err(_)=>return self.job_finish(&member,id,&lease,"waiting_input",Value::Null,json!({"code":"submission_unknown","message":"响应无效，提交结果须核实","retryable":false}))};
        let text=match response["choices"][0]["message"]["content"].as_str().filter(|s|!s.trim().is_empty()&&s.chars().count()<=10000){Some(t)=>t,None=>return self.job_finish(&member,id,&lease,"failed",Value::Null,json!({"code":"invalid_model_output","message":"模型未返回可审核建议，仍可手工保存","retryable":false}))};
        let request = &job["payload"]["request"];
        let target = if request.get("clothing_id").is_some() {
            json!({"clothing_id":request["clothing_id"]})
        } else {
            json!({"batch_id":request["batch_id"],"draft_id":request["draft_id"]})
        };
        let proposal = json!({"schema_version":1,"id":uuid::Uuid::now_v7().to_string(),"job_id":id,"member_id":member,"state":"pending_review","target":target,"baseline":job["payload"]["baseline"],"raw_text":text,"suggested_facts":normalized_candidates(text),"uncertainties":{"model_self_reported":text,"notice":"视觉建议未经核实；品牌、材质、尺寸和不可见字段不能确定。模型自报描述/分数不是准确率。"},"provenance":{"provider_id":job["payload"]["provider"]["id"],"provider_name":job["payload"]["provider"]["name"],"endpoint":job["payload"]["provider"]["endpoint"],"model":job["payload"]["provider"]["model"],"response_model":response["model"],"assets":job["payload"]["assets"],"transferred_images":job["payload"]["transferred_images"],"prompt_version":"visible-description-v1","normalization":"conservative-taxonomy-word-candidates-v1","usage":response["usage"],"created_at":chrono::Utc::now().to_rfc3339()}});
        let tx = self.begin_write()?;
        let current = self.job_get(json!({"member_id":member,"id":id}))?;
        if current["state"] != "running"
            || current["lease_token"] != lease
            || current["lease_expires"].as_i64().unwrap_or(0)
                < chrono::Utc::now().timestamp_millis()
        {
            return Err(ApiError::new("lease_lost", "任务已取消或过期，不发布建议"));
        }
        tx.execute(
            "INSERT INTO recognition_suggestions VALUES(?1,?2,?3,?4)",
            params![proposal["id"].as_str(), id, member, proposal.to_string()],
        )?;
        job["state"] = json!("succeeded");
        job["progress"] = json!(100);
        job["lease_token"] = Value::Null;
        job["lease_expires"] = json!(0);
        job["payload"]["submission"] = json!("received");
        job["result"] = json!({"suggestion_id":proposal["id"]});
        job["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        jobs::save(&tx, &job)?;
        tx.commit()?;
        Ok(job)
    }
}
