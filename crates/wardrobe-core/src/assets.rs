use super::{string, ApiError, Application};
use base64::Engine;
use image::{ImageDecoder, ImageReader};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};

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
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS assets (id TEXT NOT NULL, member_id TEXT NOT NULL REFERENCES members(id), document TEXT NOT NULL, PRIMARY KEY(id,member_id));")?;
    Ok(())
}
impl Application {
    pub(super) fn asset_reorder(&self, input: Value) -> Result<Value, ApiError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
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
        tx.execute(
            "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
            params![clothing.to_string(), id, member],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES (?1,'image_reorder',?2)",
            params![id, clothing.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES (?1,?2)",
            params![id, clothing.to_string()],
        )?;
        tx.commit()?;
        Ok(clothing)
    }
    pub(super) fn asset_sync_manifest(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let mut statement = self
            .connection
            .prepare("SELECT document FROM assets WHERE member_id=?1")?;
        let rows = statement.query_map([member], |r| r.get::<_, String>(0))?;
        let mut assets = Vec::new();
        for row in rows {
            let doc: Value = serde_json::from_str(&row?)
                .map_err(|_| ApiError::new("storage_error", "图片记录损坏"))?;
            let display = &doc["display"];
            if display["role"] == "product_display"
                && display["state"] == "adopted"
                && display["sync_scope"] == "workspace"
            {
                assets.push(json!({"id":doc["id"],"blob_hash":display["id"],"role":"product_display","state":"adopted","sync_scope":"workspace","mime":display["mime"],"bytes":display["bytes"],"width":display["width"],"height":display["height"]}));
            }
        }
        Ok(json!({"schema_version":1,"assets":assets}))
    }
    fn store_blob(
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
        let id = uuid::Uuid::now_v7().to_string();
        let doc = json!({"schema_version":1,"id":id,"member_id":member,"state":"preview","original":{"id":original_id,"local_path":original_path,"sync_scope":"local_only","bytes":bytes.len()},"display":{"id":display_id,"local_path":display_path,"role":"product_display","state":"preview","sync_scope":"local_only","width":display.width(),"height":display.height(),"bytes":encoded.len(),"mime":"image/webp","quality":82,"soft_threshold_exceeded":encoded.len()>500*1024},"thumbnail":{"local_path":thumbnail_path,"sync_scope":"local_only","width":thumb.width(),"height":thumb.height()}});
        self.connection.execute(
            "INSERT OR IGNORE INTO assets VALUES (?1,?2,?3)",
            params![id, member, doc.to_string()],
        )?;
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
        let mut doc = self.asset_get(input.clone())?;
        let image = image::open(doc["display"]["local_path"].as_str().ok_or_else(failure)?)
            .map_err(|_| failure())?;
        let image = thumbnail(&image);
        let bytes = webp::Encoder::from_image(&image)
            .map_err(|_| failure())?
            .encode(82.0)
            .to_vec();
        let (_, path) = self.store_blob("thumbnails-local-only", &bytes, "webp")?;
        doc["thumbnail"] = json!({"local_path":path,"sync_scope":"local_only","width":image.width(),"height":image.height()});
        self.connection.execute(
            "UPDATE assets SET document=?1 WHERE id=?2 AND member_id=?3",
            params![
                doc.to_string(),
                doc["id"].as_str(),
                doc["member_id"].as_str()
            ],
        )?;
        Ok(doc)
    }
    pub(super) fn asset_adopt(&self, input: Value) -> Result<Value, ApiError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
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
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES (?1,?2)",
            params![clothing_id, clothing.to_string()],
        )?;
        tx.commit()?;
        Ok(clothing)
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    ["import","get","preview","rebuild_thumbnail","adopt","reorder","sync_manifest"].iter().map(|action|json!({"name":format!("assets.{action}"),"description":"本地图片导入、预览与采用","inputSchema":{"type":"object","required":["member_id"],"properties":{"member_id":{"type":"string"},"path":{"type":"string"},"id":{"type":"string"},"asset_id":{"type":"string"},"clothing_id":{"type":"string"},"variant":{"enum":["display","thumbnail"]}}},"outputSchema":{"type":"object","required":["schema_version"]}})).collect()
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
fn decode(bytes: &[u8]) -> Result<image::DynamicImage, ApiError> {
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
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
        let decoded = LibHeif::new()
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

fn thumbnail(image: &image::DynamicImage) -> image::DynamicImage {
    if image.width() > 320 || image.height() > 320 {
        image.thumbnail(320, 320)
    } else {
        image.clone()
    }
}
