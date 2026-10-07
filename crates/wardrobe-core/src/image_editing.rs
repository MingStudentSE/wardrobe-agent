use crate::{assets, string, ApiError, Application};
use rusqlite::params;
use serde_json::{json, Value};
use sha2::Digest;
impl Application {
    pub(super) fn image_links(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let member = self.member(&input)?;
        let id = string(&input, "clothing_id")?;
        let mut clothing = self.clothing_get(json!({"member_id":member,"id":id}))?;
        if clothing["status"] == "deleted" {
            return Err(ApiError::new("invalid_state", "回收站单品不能编辑图片"));
        }
        let mut ids = clothing["asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut adopt = vec![];
        match operation {
            "images.merge" => {
                let source = self.clothing_get(
                    json!({"member_id":member,"id":string(&input,"source_clothing_id")?}),
                )?;
                if source["status"] == "deleted" {
                    return Err(ApiError::new("invalid_state", "回收站单品不能归并图片"));
                }
                let selected = input["asset_ids"]
                    .as_array()
                    .ok_or_else(|| ApiError::new("invalid_input", "请选择来源单品图片"))?;
                if selected.is_empty() || selected.len() > 20 {
                    return Err(ApiError::new("invalid_input", "归并须选择1–20张图片"));
                }
                let mut unique = std::collections::HashSet::new();
                for asset in selected {
                    let asset_id = asset
                        .as_str()
                        .ok_or_else(|| ApiError::new("invalid_input", "图片须为ID"))?;
                    if !unique.insert(asset_id)
                        || !source["asset_ids"]
                            .as_array()
                            .is_some_and(|v| v.contains(asset))
                    {
                        return Err(ApiError::new(
                            "invalid_input",
                            "只能归并来源单品的图片，各选择一次",
                        ));
                    }
                    self.asset_get(json!({"member_id":member,"id":asset_id}))?;
                    if !ids.contains(asset) {
                        ids.push(asset.clone());
                        adopt.push(asset.clone());
                    }
                }
                if clothing["display_asset_id"].is_null() {
                    clothing["display_asset_id"] = ids.first().cloned().unwrap_or(Value::Null);
                }
            }
            "images.replace" => {
                let old = json!(string(&input, "asset_id")?);
                let new = json!(string(&input, "replacement_id")?);
                let position = ids
                    .iter()
                    .position(|v| v == &old)
                    .ok_or_else(|| ApiError::new("invalid_input", "被替换图片不属于此单品"))?;
                if old != new && ids.contains(&new) {
                    return Err(ApiError::new("invalid_input", "替换图片已经在单品中"));
                }
                self.asset_get(json!({"member_id":member,"id":new}))?;
                ids[position] = new.clone();
                adopt.push(new.clone());
                if clothing["display_asset_id"] == old {
                    clothing["display_asset_id"] = new;
                }
            }
            _ => {
                let asset = json!(string(&input, "asset_id")?);
                if !ids.contains(&asset) {
                    return Err(ApiError::new("invalid_input", "主图须属于此单品"));
                }
                clothing["display_asset_id"] = asset;
            }
        }
        if ids.len() > 20 {
            return Err(ApiError::new("asset_limit", "每单品最多20张图片"));
        }
        for id in adopt {
            let mut asset = self.asset_get(json!({"member_id":member,"id":id}))?;
            asset["state"] = json!("adopted");
            asset["display"]["state"] = json!("adopted");
            asset["display"]["sync_scope"] = json!("workspace");
            tx.execute(
                "UPDATE assets SET document=?1 WHERE id=?2 AND member_id=?3",
                params![asset.to_string(), id.as_str(), member],
            )?;
        }
        clothing["asset_ids"] = json!(ids);
        clothing["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
            params![clothing.to_string(), id, member],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,?2,?3)",
            params![id, operation, clothing.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![id, clothing.to_string()],
        )?;
        tx.commit()?;
        Ok(clothing)
    }
    pub(super) fn image_transform(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let member = self.member(&input)?;
        let id = string(&input, "asset_id")?;
        let asset = self.asset_get(json!({"member_id":member,"id":id}))?;
        let (image, action, parameters) = match operation {
            "images.rotate" => {
                let degrees = input["degrees"]
                    .as_u64()
                    .ok_or_else(|| ApiError::new("invalid_input", "旋转角度须为90、180或270"))?;
                let image = read_display(&asset)?;
                let rotated = match degrees {
                    90 => image.rotate90(),
                    180 => image.rotate180(),
                    270 => image.rotate270(),
                    _ => return Err(ApiError::new("invalid_input", "旋转角度须为90、180或270")),
                };
                (rotated, "rotate", json!({"degrees":degrees}))
            }
            _ => {
                let original = std::fs::read(
                    asset["original"]["local_path"].as_str().unwrap_or(""),
                )
                .map_err(|_| {
                    ApiError::new(
                        "original_unavailable",
                        "本机没有原图；请在导入设备恢复或重新导入原图",
                    )
                })?;
                let actual = sha2::Sha256::digest(&original)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                if asset["original"]["id"] != actual {
                    return Err(ApiError::new(
                        "original_checksum_mismatch",
                        "本机原图已改变，请重新导入；旧展示图保留",
                    ));
                }
                let image = assets::decode(&original)?;
                let resized = if image.width() > 1600 || image.height() > 1600 {
                    image.resize(1600, 1600, image::imageops::FilterType::Lanczos3)
                } else {
                    image
                };
                (resized, "restore", json!({}))
            }
        };
        let derived = self.image_derived(&tx, asset, image, action, parameters)?;
        tx.commit()?;
        Ok(derived)
    }
    pub(super) fn image_derived(
        &self,
        tx: &rusqlite::Transaction<'_>,
        mut asset: Value,
        image: image::DynamicImage,
        action: &str,
        parameters: Value,
    ) -> Result<Value, ApiError> {
        let member = asset["member_id"]
            .as_str()
            .ok_or_else(|| ApiError::new("storage_error", "图片成员缺失"))?
            .to_owned();
        let source_display = asset["display"]["id"].clone();
        let encoded = webp::Encoder::from_image(&image)
            .map_err(|_| ApiError::new("image_processing_failed", "处理失败，原图与旧展示图保留"))?
            .encode(82.0)
            .to_vec();
        let (display_id, path) = self.store_blob("display", &encoded, "webp")?;
        let thumb = assets::thumbnail(&image);
        let bytes = webp::Encoder::from_image(&thumb)
            .map_err(|_| ApiError::new("image_processing_failed", "缩略图处理失败"))?
            .encode(82.0)
            .to_vec();
        let (_, thumb_path) = self.store_blob("thumbnails-local-only", &bytes, "webp")?;
        let previous = asset["display"].clone();
        asset["id"] = json!(uuid::Uuid::now_v7().to_string());
        asset["state"] = json!("preview");
        asset["display"]["state"] = json!("preview");
        asset["display"]["sync_scope"] = json!("local_only");
        asset["display"]["id"] = json!(display_id);
        asset["display"]["local_path"] = json!(path);
        asset["display"]["width"] = json!(image.width());
        asset["display"]["height"] = json!(image.height());
        asset["display"]["bytes"] = json!(encoded.len());
        asset["display"]["soft_threshold_exceeded"] = json!(encoded.len() > 500 * 1024);
        asset["thumbnail"] = json!({"local_path":thumb_path,"sync_scope":"local_only","width":thumb.width(),"height":thumb.height()});
        if !asset["lineage"].is_array() {
            asset["lineage"] = json!([]);
        }
        let original_id = asset["original"]["id"].clone();
        asset["lineage"].as_array_mut().unwrap().push(json!({"id":uuid::Uuid::now_v7().to_string(),"action":action,"source_display_id":source_display,"source_original_id":original_id,"result_display_id":display_id,"parameters":parameters,"algorithm":"wardrobe-image-1","created_at":chrono::Utc::now().to_rfc3339(),"previous_display":previous}));
        tx.execute(
            "INSERT INTO assets(id,member_id,document) VALUES (?1,?2,?3)",
            params![asset["id"].as_str(), member, asset.to_string()],
        )?;
        Ok(asset)
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    let mut asset = crate::assets::descriptors()
        .into_iter()
        .find(|v| v["name"] == "assets.get")
        .unwrap()["outputSchema"]
        .clone();
    asset["properties"]["lineage"] = json!({"type":"array","items":{"type":"object","required":["id","action","source_display_id","source_original_id","result_display_id","parameters","algorithm","created_at"],"properties":{"id":{"type":"string"},"action":{"enum":["rotate","restore","cutout","product_generation"]},"source_display_id":{"type":"string"},"source_original_id":{"type":"string"},"result_display_id":{"type":"string"},"parameters":{"type":"object"},"algorithm":{"type":"string"},"created_at":{"type":"string"},"previous_display":{"type":"object"}}}});
    let mut result = vec![];
    for action in ["rotate", "restore", "merge", "replace", "main"] {
        let mut properties = json!({"member_id":{"type":"string"}});
        let mut required = vec!["member_id"];
        if ["rotate", "restore", "replace", "main"].contains(&action) {
            properties["asset_id"] = json!({"type":"string"});
            required.push("asset_id");
        }
        if action == "rotate" {
            properties["degrees"] = json!({"type":"integer","enum":[90,180,270]});
            required.push("degrees");
        }
        if ["merge", "replace", "main"].contains(&action) {
            properties["clothing_id"] = json!({"type":"string"});
            required.push("clothing_id");
        }
        if action == "merge" {
            properties["source_clothing_id"] = json!({"type":"string"});
            properties["asset_ids"] = json!({"type":"array","minItems":1,"maxItems":20,"uniqueItems":true,"items":{"type":"string"}});
            required.extend(["source_clothing_id", "asset_ids"]);
        }
        if action == "replace" {
            properties["replacement_id"] = json!({"type":"string"});
            required.push("replacement_id");
        }
        result.push(json!({"name":format!("images.{action}"),"description":match action{"rotate"=>"生成旋转预览，保留原图与旧展示图","restore"=>"从本机原图生成恢复预览","merge"=>"复制来源单品选中图片到目标，来源单品保留","replace"=>"在原位置替换图片关联，旧图与历史保留",_=>"将已有图片设为主图"},"inputSchema":{"type":"object","required":required,"properties":properties,"additionalProperties":false},"outputSchema":if ["rotate","restore"].contains(&action){asset.clone()}else{crate::clothing::output_schema("clothing.get")}}));
    }
    result.extend(crate::cutout::descriptors());
    result
}

pub(super) fn validate_input(operation: &str, input: &Value) -> Result<(), ApiError> {
    let allowed: &[&str] = match operation {
        "images.rotate" => &["member_id", "asset_id", "degrees"],
        "images.restore" => &["member_id", "asset_id"],
        "images.merge" => &[
            "member_id",
            "clothing_id",
            "source_clothing_id",
            "asset_ids",
        ],
        "images.replace" => &["member_id", "clothing_id", "asset_id", "replacement_id"],
        "images.main" => &["member_id", "clothing_id", "asset_id"],
        "images.cutout" => &["member_id", "asset_id", "operation_key"],
        "images.model.status" => &["member_id"],
        "images.model.download" => &["member_id", "operation_key"],
        "images.worker.run" => &["member_id", "job_id"],
        _ => return Ok(()),
    };
    if input
        .as_object()
        .is_some_and(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(ApiError::new("invalid_input", "图片操作包含不支持字段"));
    }
    Ok(())
}

pub(super) fn read_display(asset: &Value) -> Result<image::DynamicImage, ApiError> {
    let path = asset["display"]["local_path"]
        .as_str()
        .ok_or_else(|| ApiError::new("image_unavailable", "展示图尚未下载"))?;
    let metadata = std::fs::metadata(path)
        .map_err(|_| ApiError::new("image_unavailable", "展示图尚未下载或已损坏"))?;
    if !metadata.is_file() || metadata.len() > 50 * 1024 * 1024 {
        return Err(ApiError::new(
            "image_unavailable",
            "展示图大小异常，请重新导入",
        ));
    }
    let bytes =
        std::fs::read(path).map_err(|_| ApiError::new("image_unavailable", "展示图读取失败"))?;
    if bytes.len() > 50 * 1024 * 1024 {
        return Err(ApiError::new("image_unavailable", "展示图大小异常"));
    }
    let hash = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if asset["display"]["id"] != hash {
        return Err(ApiError::new(
            "image_unavailable",
            "展示图校验失败，请重新导入；旧关联保留",
        ));
    }
    assets::decode(&bytes)
}
