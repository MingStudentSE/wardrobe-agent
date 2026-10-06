use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::fs;
const CHUNK: usize = 128 * 1024;
const MAX_ASSET: usize = 16 * 1024 * 1024;
impl Application {
    pub(super) fn sync_publish_assets(
        &self,
        storage: &dyn SyncStorage,
        identity: &Value,
    ) -> Result<(), ApiError> {
        let members = self.member_list()?;
        for member in members["items"]
            .as_array()
            .ok_or_else(|| error("storage_error"))?
        {
            let id = string(member, "member_id")?;
            let manifest = self.asset_sync_manifest(json!({"member_id":id}))?;
            for entry in manifest["assets"]
                .as_array()
                .ok_or_else(|| error("storage_error"))?
            {
                let asset = self.asset_get(json!({"member_id":id,"id":entry["id"]}))?;
                crate::image_editing::read_display(&asset)?;
                let name = self.sync_object_key(&format!(
                    "asset:{}:{}",
                    string(entry, "id")?,
                    string(entry, "blob_hash")?
                ))?;
                let prefix = format!("{}/blobs", string(identity, "space_id")?);
                let index_key = format!("{prefix}/{name}.commit");
                if storage.stat(&index_key).is_ok() {
                    continue;
                }
                let mut file = fs::File::open(string(&asset["display"], "local_path")?)
                    .map_err(|_| error("asset_missing"))?;
                let mut chunks = vec![];
                let mut total = 0usize;
                let mut digest = Sha256::new();
                loop {
                    let mut data = vec![0; CHUNK];
                    let mut used = 0;
                    while used < CHUNK {
                        let n = file
                            .read(&mut data[used..])
                            .map_err(|_| error("asset_missing"))?;
                        if n == 0 {
                            break;
                        }
                        used += n;
                    }
                    if used == 0 {
                        break;
                    }
                    data.truncate(used);
                    total += used;
                    if total > MAX_ASSET {
                        return Err(error("sync_too_large"));
                    }
                    digest.update(&data);
                    let key = format!(
                        "{prefix}/{}.object",
                        self.sync_object_key(&format!("chunk:{name}:{}", chunks.len()))?
                    );
                    let payload = json!({"asset_id":entry["id"],"blob_hash":entry["blob_hash"],"offset":total-used,"data_base64":STANDARD.encode(&data)});
                    let cipher = self.sync_encrypt("blobs", &payload)?;
                    let bytes = serde_json::to_vec(&cipher).map_err(|_| error("sync_invalid"))?;
                    if storage.stat(&key).is_err() {
                        storage.put(&key, &mut bytes.as_slice(), WriteCondition::CreateOnly)?;
                    }
                    let stored = bounded(storage, &key)?;
                    chunks.push(
                        json!({"key":key,"sha256":super::digest(&stored),"bytes":stored.len()}),
                    );
                }
                let hash: String = digest
                    .finalize()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                if entry["bytes"] != total || entry["blob_hash"] != hash {
                    return Err(error("asset_corrupt"));
                }
                let index = json!({"asset":entry,"member_id":id,"chunks":chunks});
                let cipher = self.sync_encrypt("blobs", &index)?;
                let bytes = serde_json::to_vec(&cipher).map_err(|_| error("sync_invalid"))?;
                storage.put(
                    &index_key,
                    &mut bytes.as_slice(),
                    WriteCondition::CreateOnly,
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn sync_discover_assets(
        &self,
        storage: &dyn SyncStorage,
        identity: &Value,
    ) -> Result<(), ApiError> {
        let prefix = format!("{}/blobs", string(identity, "space_id")?);
        let mut cursor = None;
        loop {
            let page = storage.list(&prefix, cursor.as_deref(), 64)?;
            for entry in page.items {
                if !entry.key.ends_with(".commit") {
                    continue;
                }
                let cipher: Value = serde_json::from_slice(&bounded(storage, &entry.key)?)
                    .map_err(|_| error("storage_corrupt"))?;
                let index = self.sync_decrypt("blobs", &cipher)?;
                let asset = &index["asset"];
                if asset["role"] != "product_display"
                    || asset["state"] != "adopted"
                    || asset["sync_scope"] != "workspace"
                    || asset["mime"] != "image/webp"
                    || !asset["bytes"]
                        .as_u64()
                        .is_some_and(|n| n > 0 && n <= MAX_ASSET as u64)
                    || !index["chunks"]
                        .as_array()
                        .is_some_and(|c| !c.is_empty() && c.len() <= MAX_ASSET / CHUNK)
                {
                    return Err(error("sync_invalid"));
                }
                let id = string(asset, "id")?;
                self.connection.execute("INSERT INTO sync_asset_queue VALUES(?1,?2,'pending') ON CONFLICT(id) DO UPDATE SET document=excluded.document",params![id,index.to_string()])?;
            }
            cursor = page.cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(())
    }
    pub(super) fn sync_fetch_assets(&self, storage: &dyn SyncStorage) -> Result<usize, ApiError> {
        let mut st = self
            .connection
            .prepare("SELECT id,document FROM sync_asset_queue WHERE state!='ready' ORDER BY id")?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(st);
        let mut missing = 0;
        for (id, document) in rows {
            let index: Value =
                serde_json::from_str(&document).map_err(|_| error("storage_error"))?;
            let member = string(&index, "member_id")?;
            if !self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM members WHERE id=?1)",
                [member],
                |r| r.get::<_, bool>(0),
            )? {
                missing += 1;
                continue;
            }
            let result = (|| {
                let mut bytes = Vec::new();
                for chunk in index["chunks"]
                    .as_array()
                    .ok_or_else(|| error("sync_invalid"))?
                {
                    let data = bounded(storage, string(chunk, "key")?)?;
                    if chunk["bytes"] != data.len() || chunk["sha256"] != super::digest(&data) {
                        return Err(error("asset_corrupt"));
                    }
                    let cipher: Value =
                        serde_json::from_slice(&data).map_err(|_| error("asset_corrupt"))?;
                    let payload = self.sync_decrypt("blobs", &cipher)?;
                    if payload["asset_id"] != id
                        || payload["blob_hash"] != index["asset"]["blob_hash"]
                        || payload["offset"] != bytes.len()
                    {
                        return Err(error("asset_corrupt"));
                    }
                    let decoded = STANDARD
                        .decode(string(&payload, "data_base64")?)
                        .map_err(|_| error("asset_corrupt"))?;
                    if decoded.len() > CHUNK || bytes.len() + decoded.len() > MAX_ASSET {
                        return Err(error("asset_corrupt"));
                    }
                    bytes.extend(decoded);
                }
                if index["asset"]["bytes"] != bytes.len()
                    || index["asset"]["blob_hash"] != super::digest(&bytes)
                {
                    return Err(error("asset_corrupt"));
                }
                let mut reader = image::ImageReader::with_format(
                    std::io::Cursor::new(&bytes),
                    image::ImageFormat::WebP,
                );
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(4096);
                limits.max_image_height = Some(4096);
                limits.max_alloc = Some(64 * 1024 * 1024);
                reader.limits(limits);
                let image = reader.decode().map_err(|_| error("asset_corrupt"))?;
                if index["asset"]["width"] != image.width()
                    || index["asset"]["height"] != image.height()
                {
                    return Err(error("asset_corrupt"));
                }
                let (_, display_path) = self.store_blob("display", &bytes, "webp")?;
                let thumbnail = crate::assets::thumbnail(&image);
                let thumbnail_bytes = webp::Encoder::from_image(&thumbnail)
                    .map_err(|_| error("asset_corrupt"))?
                    .encode(82.0)
                    .to_vec();
                let (_, thumbnail_path) =
                    self.store_blob("thumbnails-local-only", &thumbnail_bytes, "webp")?;
                let existing: Option<String> = self
                    .connection
                    .query_row("SELECT document FROM assets WHERE id=?1", [&id], |r| {
                        r.get(0)
                    })
                    .optional()?;
                let mut doc = existing
                    .map(|s| serde_json::from_str::<Value>(&s).map_err(|_| error("storage_error")))
                    .transpose()?
                    .unwrap_or(
                        json!({"schema_version":1,"id":id,"member_id":member,"original":null}),
                    );
                if doc["member_id"] != member {
                    return Err(error("sync_invalid"));
                }
                doc["state"] = json!("adopted");
                doc["display"] = index["asset"].clone();
                doc["display"]["id"] = index["asset"]["blob_hash"].clone();
                doc["display"]["local_path"] = json!(display_path);
                doc["thumbnail"] = json!({"local_path":thumbnail_path,"sync_scope":"local_only","width":thumbnail.width(),"height":thumbnail.height()});
                let tx = self.begin_write()?;
                tx.execute("INSERT INTO assets VALUES(?1,?2,?3) ON CONFLICT(id,member_id) DO UPDATE SET document=excluded.document",params![id,member,doc.to_string()])?;
                tx.execute(
                    "UPDATE sync_asset_queue SET state='ready' WHERE id=?1",
                    [&id],
                )?;
                tx.commit()?;
                Ok::<_, ApiError>(())
            })();
            if let Err(e) = result {
                self.connection.execute(
                    "UPDATE sync_asset_queue SET state=?1 WHERE id=?2",
                    params![e.code, id],
                )?;
                missing += 1;
            }
        }
        Ok(missing)
    }
}
// Rebuild references transactionally even before their encrypted display arrives.
pub(super) fn ensure_references(
    tx: &Transaction<'_>,
    table: &str,
    doc: &Value,
) -> Result<(), ApiError> {
    let member = string(doc, "member_id")?;
    let mut ids = std::collections::BTreeSet::new();
    fn collect(
        v: &Value,
        member: &str,
        ids: &mut std::collections::BTreeSet<String>,
    ) -> Result<(), ApiError> {
        match v {
            Value::Object(map) => {
                if let Some(m) = map.get("member_id") {
                    if m != member {
                        return Err(error("sync_invalid"));
                    }
                }
                if let Some(id) = map.get("display_asset_id").and_then(Value::as_str) {
                    ids.insert(id.into());
                }
                for id in map
                    .get("asset_ids")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    ids.insert(id.as_str().ok_or_else(|| error("sync_invalid"))?.into());
                }
                for value in map.values() {
                    collect(value, member, ids)?;
                }
            }
            Value::Array(a) => {
                for value in a {
                    collect(value, member, ids)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    collect(doc, member, &mut ids)?;
    for id in ids {
        if uuid::Uuid::parse_str(&id).is_err() {
            return Err(error("sync_invalid"));
        }
        let stub = json!({"schema_version":1,"id":id,"member_id":member,"state":"adopted","original":null,"display":{"id":null,"local_path":"","role":"product_display","state":"adopted","sync_scope":"workspace","mime":"image/webp","bytes":0,"width":0,"height":0,"pending":true},"thumbnail":{"local_path":"","sync_scope":"local_only","width":0,"height":0}});
        tx.execute(
            "INSERT OR IGNORE INTO assets VALUES(?1,?2,?3)",
            params![id, member, stub.to_string()],
        )?;
        if ["wear_events", "care_events"].contains(&table) {
            let refs = if table == "wear_events" {
                "wear_asset_refs"
            } else {
                "care_asset_refs"
            };
            tx.execute(
                &format!("INSERT OR IGNORE INTO {refs} VALUES(?1,?2,?3)"),
                params![string(doc, "id")?, id, member],
            )?;
        }
    }
    Ok(())
}
