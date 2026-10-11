use super::{string, ApiError, Application};
use base64::Engine;
use image::{ImageDecoder, ImageReader};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    path::Path,
};

const MAX_BYTES: u64 = 50 * 1024 * 1024;
const MAX_PIXELS: u64 = 60_000_000;
fn failure() -> ApiError {
    ApiError::new(
        "image_decode_failed",
        "图片无法解码；原文件保留，未采用展示图",
    )
}
fn io_error(_: std::io::Error) -> ApiError {
    ApiError::new("storage_error", "本地图片文件操作失败")
}
pub(super) fn read_display(path: &Path) -> Result<image::DynamicImage, ApiError> {
    let file = std::fs::File::open(path).map_err(io_error)?;
    if file.metadata().map_err(io_error)?.len() > MAX_BYTES {
        return Err(failure());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(failure());
    }
    let format = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(io_error)?
        .format();
    if format != Some(image::ImageFormat::WebP) {
        return Err(failure());
    }
    decode(&bytes)
}
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS assets (id TEXT NOT NULL, member_id TEXT NOT NULL REFERENCES members(id), document TEXT NOT NULL, PRIMARY KEY(id,member_id));")?;
    Ok(())
}
impl Application {
    pub(super) fn asset_detach(&self, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let member = self.member(&input)?;
        let clothing_id = string(&input, "clothing_id")?;
        let asset_id = string(&input, "asset_id")?;
        let mut clothing = self.clothing_get(json!({"member_id":member,"id":clothing_id}))?;
        if clothing["status"] == "deleted" {
            return Err(ApiError::new("invalid_state", "回收站单品不能编辑图片"));
        }
        let mut asset = self.asset_get(json!({"member_id":member,"id":asset_id}))?;
        let ids = clothing["asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !ids.contains(&json!(asset_id)) {
            return Err(ApiError::new("not_found", "单品未关联该图片"));
        }
        let remaining: Vec<Value> = ids.into_iter().filter(|id| id != asset_id).collect();
        if clothing["display_asset_id"] == asset_id {
            clothing["display_asset_id"] = remaining.first().cloned().unwrap_or(Value::Null);
        }
        clothing["asset_ids"] = json!(remaining);
        clothing["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
            params![clothing.to_string(), clothing_id, member],
        )?;
        let references:u64=tx.query_row("SELECT count(*) FROM clothing WHERE member_id=?1 AND EXISTS (SELECT 1 FROM json_each(document,'$.asset_ids') WHERE value=?2)",params![member,asset_id],|r|r.get(0))?;
        let outfit_references:u64=tx.query_row("SELECT count(*) FROM outfit_versions JOIN outfits ON outfits.id=outfit_versions.outfit_id WHERE outfits.member_id=?1 AND EXISTS (SELECT 1 FROM json_each(outfit_versions.document,'$.items') WHERE json_extract(value,'$.snapshot.display_asset_id')=?2)",params![member,asset_id],|r|r.get(0))?;
        let wear_references: u64 = tx.query_row(
            "SELECT count(*) FROM wear_asset_refs WHERE member_id=?1 AND asset_id=?2",
            params![member, asset_id],
            |r| r.get(0),
        )?;
        let care_references: u64 = tx.query_row(
            "SELECT count(*) FROM care_asset_refs WHERE member_id=?1 AND asset_id=?2",
            params![member, asset_id],
            |r| r.get(0),
        )?;
        let feedback_references: u64 = tx.query_row(
            "SELECT count(*) FROM feedback_asset_refs WHERE member_id=?1 AND asset_id=?2",
            params![member, asset_id],
            |r| r.get(0),
        )?;
        if references == 0
            && outfit_references == 0
            && wear_references == 0
            && care_references == 0
            && feedback_references == 0
        {
            asset["state"] = json!("preview");
            asset["display"]["state"] = json!("preview");
            asset["display"]["sync_scope"] = json!("local_only");
            tx.execute(
                "UPDATE assets SET document=?1 WHERE id=?2 AND member_id=?3",
                params![asset.to_string(), asset_id, member],
            )?;
        }
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES (?1,'image_detach',?2)",
            params![clothing_id, clothing.to_string()],
        )?;
        tx.commit()?;
        Ok(clothing)
    }
    pub(super) fn asset_reorder(&self, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let member = self.member(&input)?;
        let id = string(&input, "clothing_id")?;
        let mut clothing = self.clothing_get(json!({"member_id":member,"id":id}))?;
        if clothing["status"] == "deleted" {
            return Err(ApiError::new("invalid_state", "回收站单品不能编辑图片"));
        }
        let desired = input
            .get("asset_ids")
            .and_then(Value::as_array)
            .ok_or_else(|| ApiError::new("invalid_input", "图片排序须为 ID 列表"))?;
        let existing = clothing["asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let unique: std::collections::HashSet<_> =
            desired.iter().filter_map(Value::as_str).collect();
        if desired.len() != existing.len()
            || unique.len() != desired.len()
            || desired.iter().any(|v| !existing.contains(v))
        {
            return Err(ApiError::new(
                "invalid_input",
                "排序必须包含现有图片且各出现一次",
            ));
        }
        clothing["asset_ids"] = json!(desired);
        clothing["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
            params![clothing.to_string(), id, member],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES (?1,'image_reorder',?2)",
            params![id, clothing.to_string()],
        )?;
        tx.commit()?;
        Ok(clothing)
    }
    pub(super) fn asset_sync_manifest(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let mut references=self.connection.prepare("SELECT json_extract(item.value,'$.snapshot.display_asset_id') FROM outfit_versions v JOIN outfits o ON o.id=v.outfit_id JOIN json_each(v.document,'$.items') item WHERE o.member_id=?1 UNION SELECT a.value FROM outfit_versions v JOIN outfits o ON o.id=v.outfit_id JOIN json_each(v.document,'$.items') item JOIN json_each(item.value,'$.snapshot.asset_ids') a WHERE o.member_id=?1 UNION SELECT asset_id FROM wear_asset_refs WHERE member_id=?1 UNION SELECT asset_id FROM care_asset_refs WHERE member_id=?1 UNION SELECT asset_id FROM feedback_asset_refs WHERE member_id=?1")?;
        let historical_ids = references
            .query_map([&member], |r| r.get::<_, Option<String>>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<std::collections::HashSet<_>>();
        let mut statement = self
            .connection
            .prepare("SELECT document FROM assets WHERE member_id=?1")?;
        let rows = statement.query_map([&member], |r| r.get::<_, String>(0))?;
        let mut assets = Vec::new();
        for row in rows {
            let doc: Value = serde_json::from_str(&row?)
                .map_err(|_| ApiError::new("storage_error", "图片记录损坏"))?;
            let display = &doc["display"];
            // Immutable outfit snapshots can retain secondary images even after
            // their current clothing association is removed. Only IDs previously
            // adopted into domain snapshots authorize historical display bytes.
            let historical = doc["id"]
                .as_str()
                .is_some_and(|id| historical_ids.contains(id));
            if display["pending"] != true
                && display["role"] == "product_display"
                && display["mime"] == "image/webp"
                && ((display["state"] == "adopted" && display["sync_scope"] == "workspace")
                    || historical)
            {
                assets.push(json!({"id":doc["id"],"blob_hash":display["id"],"role":"product_display","state":"adopted","sync_scope":"workspace","mime":display["mime"],"bytes":display["bytes"],"width":display["width"],"height":display["height"]}));
            }
        }
        Ok(json!({"schema_version":1,"assets":assets}))
    }
    pub(super) fn store_blob(
        &self,
        scope: &str,
        bytes: &[u8],
        extension: &str,
    ) -> Result<(String, String), ApiError> {
        let id = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let root = self.asset_root.join(scope);
        std::fs::create_dir_all(&root).map_err(io_error)?;
        let path = root.join(format!("{id}.{extension}"));
        if !path.exists() {
            let temp = root.join(format!("{}.tmp", uuid::Uuid::now_v7()));
            std::fs::write(&temp, bytes).map_err(io_error)?;
            std::fs::rename(temp, &path).map_err(io_error)?;
        }
        Ok((id, path.to_string_lossy().into_owned()))
    }
    pub(super) fn asset_import(&self, input: Value) -> Result<Value, ApiError> {
        self.asset_import_with_id(input, &uuid::Uuid::now_v7().to_string())
    }
    pub(super) fn asset_import_with_id(&self, input: Value, id: &str) -> Result<Value, ApiError> {
        if let Ok(existing) = self.asset_get(json!({"member_id":input["member_id"],"id":id})) {
            return Ok(existing);
        }
        let member = self.member(&input)?;
        let path = Path::new(string(&input, "path")?);
        let meta = std::fs::metadata(path).map_err(io_error)?;
        if !meta.is_file() || meta.len() > MAX_BYTES {
            return Err(ApiError::new(
                "image_too_large",
                "图片须为不超过 50 MiB 的文件",
            ));
        }
        let bytes = std::fs::read(path).map_err(io_error)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(ApiError::new("image_too_large", "图片超过 50 MiB"));
        }
        let (original_id, original_path) =
            self.store_blob("originals-local-only", &bytes, "original")?;
        let image = decode(&bytes)?;
        let display = if image.width() > 1600 || image.height() > 1600 {
            image.resize(1600, 1600, image::imageops::FilterType::Lanczos3)
        } else {
            image
        };
        let encoded = webp::Encoder::from_image(&display)
            .map_err(|_| failure())?
            .encode(82.0)
            .to_vec();
        let (display_id, display_path) = self.store_blob("display", &encoded, "webp")?;
        let thumb = thumbnail(&display);
        let thumbnail = webp::Encoder::from_image(&thumb)
            .map_err(|_| failure())?
            .encode(82.0)
            .to_vec();
        let (_, thumbnail_path) = self.store_blob("thumbnails-local-only", &thumbnail, "webp")?;
        let doc = json!({"schema_version":1,"id":id,"member_id":member,"state":"preview","original":{"id":original_id,"local_path":original_path,"sync_scope":"local_only","bytes":bytes.len()},"display":{"id":display_id,"local_path":display_path,"role":"product_display","state":"preview","sync_scope":"local_only","width":display.width(),"height":display.height(),"bytes":encoded.len(),"mime":"image/webp","quality":82,"soft_threshold_exceeded":encoded.len()>500*1024},"thumbnail":{"local_path":thumbnail_path,"sync_scope":"local_only","width":thumb.width(),"height":thumb.height()}});
        let tx = self.begin_write()?;
        tx.execute(
            "INSERT OR IGNORE INTO assets VALUES (?1,?2,?3)",
            params![id, member, doc.to_string()],
        )?;
        tx.commit()?;
        self.asset_get(json!({"member_id":member,"id":id}))
    }
    pub(super) fn asset_get(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let doc: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM assets WHERE id=?1 AND member_id=?2",
                params![string(&input, "id")?, member],
                |r| r.get(0),
            )
            .optional()?;
        serde_json::from_str(&doc.ok_or_else(|| ApiError::new("not_found", "图片不存在"))?)
            .map_err(|_| ApiError::new("storage_error", "图片记录损坏"))
    }
    pub(super) fn asset_preview(&self, input: Value) -> Result<Value, ApiError> {
        let variant = input
            .get("variant")
            .and_then(Value::as_str)
            .unwrap_or("display");
        if !matches!(variant, "display" | "thumbnail") {
            return Err(ApiError::new("invalid_input", "仅可预览展示图或缩略图"));
        }
        let mut doc = self.asset_get(input.clone())?;
        if doc["display"]["pending"] == true {
            return Err(ApiError::new(
                "asset_missing",
                "展示图尚未下载，可继续编辑和记录穿着，请重试同步",
            ));
        }
        if variant == "thumbnail"
            && !Path::new(doc["thumbnail"]["local_path"].as_str().unwrap_or("")).is_file()
        {
            doc = self.asset_rebuild_thumbnail(input.clone())?;
        }
        let bytes = std::fs::read(doc[variant]["local_path"].as_str().ok_or_else(failure)?)
            .map_err(io_error)?;
        Ok(
            json!({"schema_version":1,"id":doc["id"],"data_url":format!("data:image/webp;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes))}),
        )
    }
    pub(super) fn asset_rebuild_thumbnail(&self, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let mut doc = self.asset_get(input.clone())?;
        let image = read_display(Path::new(
            doc["display"]["local_path"].as_str().ok_or_else(failure)?,
        ))?;
        let image = thumbnail(&image);
        let bytes = webp::Encoder::from_image(&image)
            .map_err(|_| failure())?
            .encode(82.0)
            .to_vec();
        let (_, path) = self.store_blob("thumbnails-local-only", &bytes, "webp")?;
        doc["thumbnail"] = json!({"local_path":path,"sync_scope":"local_only","width":image.width(),"height":image.height()});
        tx.execute(
            "UPDATE assets SET document=?1 WHERE id=?2 AND member_id=?3",
            params![
                doc.to_string(),
                doc["id"].as_str(),
                doc["member_id"].as_str()
            ],
        )?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn asset_adopt(&self, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let member = self.member(&input)?;
        let mut asset =
            self.asset_get(json!({"member_id":member,"id":string(&input,"asset_id")?}))?;
        let clothing_id = string(&input, "clothing_id")?;
        let mut clothing = self.clothing_get(json!({"member_id":member,"id":clothing_id}))?;
        if clothing["status"] == "deleted" {
            return Err(ApiError::new("invalid_state", "回收站单品不能采用图片"));
        }
        let mut ids = clothing["asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !ids.contains(&asset["id"]) {
            if ids.len() >= 20 {
                return Err(ApiError::new("asset_limit", "每单品最多 20 张图片"));
            }
            ids.push(asset["id"].clone());
        }
        clothing["asset_ids"] = json!(ids);
        clothing["display_asset_id"] = asset["id"].clone();
        clothing["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        asset["state"] = json!("adopted");
        asset["display"]["state"] = json!("adopted");
        asset["display"]["sync_scope"] = json!("workspace");
        tx.execute(
            "UPDATE assets SET document=?1 WHERE id=?2 AND member_id=?3",
            params![asset.to_string(), asset["id"].as_str(), member],
        )?;
        tx.execute(
            "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
            params![clothing.to_string(), clothing_id, member],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES (?1,'image_adopt',?2)",
            params![clothing_id, clothing.to_string()],
        )?;
        tx.commit()?;
        Ok(clothing)
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    let display = json!({"type":"object","required":["id","local_path","role","state","sync_scope","width","height","bytes","mime","quality","soft_threshold_exceeded"],"properties":{"id":{"type":"string"},"local_path":{"type":"string"},"role":{"const":"product_display"},"state":{"enum":["preview","adopted"]},"sync_scope":{"enum":["local_only","workspace"]},"width":{"type":"integer"},"height":{"type":"integer"},"bytes":{"type":"integer"},"mime":{"const":"image/webp"},"quality":{"type":"integer"},"soft_threshold_exceeded":{"type":"boolean"}}});
    let asset = json!({"type":"object","required":["schema_version","id","member_id","state","original","display","thumbnail"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"state":{"enum":["preview","adopted"]},"original":{"type":"object","required":["id","local_path","sync_scope","bytes"],"properties":{"id":{"type":"string"},"local_path":{"type":"string"},"sync_scope":{"const":"local_only"},"bytes":{"type":"integer"}}},"display":display,"thumbnail":{"type":"object","required":["local_path","sync_scope","width","height"],"properties":{"local_path":{"type":"string"},"sync_scope":{"const":"local_only"},"width":{"type":"integer"},"height":{"type":"integer"}}}}});
    ["import","get","preview","rebuild_thumbnail","adopt","reorder","detach","sync_manifest"].iter().map(|action| {
        let mut properties=json!({"member_id":{"type":"string"}});
        let mut required=vec!["member_id"];
        for field in match *action {"import"=>vec!["path"],"get"|"preview"|"rebuild_thumbnail"=>vec!["id"],"adopt"|"detach"=>vec!["clothing_id","asset_id"],"reorder"=>vec!["clothing_id"],_=>vec![]} {properties[field]=json!({"type":"string"});required.push(field);}
        if *action=="preview" {properties["variant"]=json!({"type":"string","enum":["display","thumbnail"],"default":"display"});}
        if *action=="reorder" {required.push("asset_ids");properties["asset_ids"]=json!({"type":"array","maxItems":20,"uniqueItems":true,"items":{"type":"string"}});}
        let output=match *action {
            "adopt"|"reorder"|"detach"=>super::clothing::output_schema("clothing.get"),
            "preview"=>json!({"type":"object","required":["schema_version","id","data_url"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"data_url":{"type":"string"}}}),
            "sync_manifest"=>json!({"type":"object","required":["schema_version","assets"],"properties":{"schema_version":{"const":1},"assets":{"type":"array","items":{"type":"object","required":["id","blob_hash","role","state","sync_scope","mime","bytes","width","height"],"properties":{"id":{"type":"string"},"blob_hash":{"type":"string"},"role":{"const":"product_display"},"state":{"const":"adopted"},"sync_scope":{"const":"workspace"},"mime":{"const":"image/webp"},"bytes":{"type":"integer"},"width":{"type":"integer"},"height":{"type":"integer"}}}}}}),
            _=>asset.clone()
        };
        let description=match *action {"import"=>"导入本地原图并规范压缩，先预览", "get"=>"读取当前成员的本地图片元数据", "preview"=>"读取压缩展示图或缩略图", "rebuild_thumbnail"=>"从展示图重建本地缩略图", "adopt"=>"保留已有图片并采用指定图片为主图", "reorder"=>"调整现有图片顺序", "detach"=>"移除单品图片关联，保留本地文件和其他引用", _=>"只列出已采用且允许同步的展示图"};
        let mut descriptor=json!({"name":format!("assets.{action}"),"description":description,"inputSchema":{"type":"object","required":required,"properties":properties},"outputSchema":output});
        descriptor["examples"] = match *action {
            "import" => json!([{"request":{"member_id":"<已授权成员ID>","path":"</绝对/本地/图片路径>"},"prerequisites":["读取提供的本地图片文件并写入本机受管资产字节；原图保持本地"]}]),
            "get" => json!([{"request":{"member_id":"<已授权成员ID>","id":"<assets.import 返回的ID>"},"prerequisites":["读取资产文档，不含文件字节"]}]),
            "preview" => json!([{"request":{"member_id":"<已授权成员ID>","id":"<资产ID>"},"prerequisites":["读取本机受管图片字节生成预览；缩略图文件缺失时会同步重建本地缩略图"]}]),
            "rebuild_thumbnail" => json!([{"request":{"member_id":"<已授权成员ID>","id":"<资产ID>"},"prerequisites":["同步重建 local_only 缩略图，非异步任务"]}]),
            "adopt" => json!([{"request":{"member_id":"<已授权成员ID>","asset_id":"<资产ID>","clothing_id":"<单品ID>"},"prerequisites":["原图保持本地；仅采纳后的压缩商品展示图参与同步"]}]),
            "reorder" => json!([{"request":{"member_id":"<已授权成员ID>","clothing_id":"<单品ID>","asset_ids":["<资产ID>"]},"prerequisites":["调整现有图片顺序"]}]),
            "detach" => json!([{"request":{"member_id":"<已授权成员ID>","clothing_id":"<单品ID>","asset_id":"<资产ID>"},"prerequisites":["解除关联，保留本地文件和其他引用"]}]),
            _ => json!([{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["按穿搭/穿着/洗护/反馈引用重算可同步展示图清单；原图与个人图片保持本地"]}]),
        };
        json!(descriptor)
    }).collect()
}

fn dimensions(width: u32, height: u32) -> Result<(), ApiError> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ApiError::new(
            "image_dimensions_exceeded",
            "图片解码尺寸超过 60 MP",
        ));
    }
    Ok(())
}
pub(super) fn decode(bytes: &[u8]) -> Result<image::DynamicImage, ApiError> {
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
        let lib = LibHeif::new();
        let version = lib.version();
        if version[0] != 1 || version[1] != 23 {
            return Err(ApiError::new(
                "unsupported_heic_runtime",
                "HEIC 解码库版本不匹配，请重新安装应用",
            ));
        }
        // The safe wrapper does not expose the struct's version. Read only its
        // documented first byte before allowing the wrapper to copy it by value.
        // Never interpret an unknown runtime struct using the bound 104-byte type.
        let limits_version = unsafe {
            let limits = libheif_sys::heif_get_global_security_limits();
            if limits.is_null() {
                None
            } else {
                Some(*limits.cast::<u8>())
            }
        };
        if limits_version != Some(4) {
            return Err(ApiError::new(
                "unsupported_heic_runtime",
                "HEIC 解码库安全接口版本不匹配，请重新安装应用",
            ));
        }
        // SecurityLimits is copied by value by the wrapper. Its FFI layout must
        // match the bundled 1.23 runtime, including the version 4 fields.
        let mut ctx = HeifContext::new().map_err(|_| failure())?;
        let mut limits = ctx.security_limits();
        limits.set_max_image_size_pixels(MAX_PIXELS);
        limits.set_max_memory_block_size(256 * 1024 * 1024);
        ctx.set_security_limits(&limits).map_err(|_| failure())?;
        ctx.read_bytes(bytes).map_err(|_| failure())?;
        let handle = ctx.primary_image_handle().map_err(|_| failure())?;
        dimensions(handle.width(), handle.height())?;
        if handle.ispe_width() <= 0 || handle.ispe_height() <= 0 {
            return Err(failure());
        }
        dimensions(handle.ispe_width() as u32, handle.ispe_height() as u32)?;
        let decoded = lib
            .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
            .map_err(|_| failure())?;
        dimensions(decoded.width(), decoded.height())?;
        let plane = decoded.planes().interleaved.ok_or_else(failure)?;
        let row = decoded.width() as usize * 4;
        let mut pixels = Vec::with_capacity(row * decoded.height() as usize);
        for y in 0..decoded.height() as usize {
            pixels.extend_from_slice(
                plane
                    .data
                    .get(y * plane.stride..y * plane.stride + row)
                    .ok_or_else(failure)?,
            );
        }
        let image = image::RgbaImage::from_raw(decoded.width(), decoded.height(), pixels)
            .ok_or_else(failure)?;
        // libheif applies HEIF container rotation/mirroring during decode. Do not apply EXIF a second time.
        return Ok(image::DynamicImage::ImageRgba8(image));
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| failure())?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP)
    ) {
        return Err(failure());
    }
    let mut decoder = reader.into_decoder().map_err(|_| failure())?;
    let (width, height) = decoder.dimensions();
    dimensions(width, height)?;
    decoder
        .set_limits(image::Limits::default())
        .map_err(|_| failure())?;
    let orientation = decoder.orientation().map_err(|_| failure())?;
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(|_| failure())?;
    image.apply_orientation(orientation);
    Ok(image)
}

pub(super) fn thumbnail(image: &image::DynamicImage) -> image::DynamicImage {
    if image.width() > 320 || image.height() > 320 {
        image.thumbnail(320, 320)
    } else {
        image.clone()
    }
}
