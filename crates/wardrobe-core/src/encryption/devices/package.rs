//! Application-owned, transport-independent encrypted snapshot packages.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::Path,
};

const MAX_FILE_BYTES: u64 = 24 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_OBJECTS: usize = 10000;
fn error() -> ApiError {
    ApiError::new("package_invalid", "密文包缺失、损坏或与当前设备状态不符")
}
fn io(_: std::io::Error) -> ApiError {
    error()
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn write_object(path: &Path, bytes: &[u8]) -> Result<(), ApiError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)
}
fn bounded(path: &Path) -> Result<Vec<u8>, ApiError> {
    let meta = fs::symlink_metadata(path).map_err(io)?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err(error());
    }
    let file = fs::File::open(path).map_err(io)?;
    let opened = file.metadata().map_err(io)?;
    if !opened.is_file() || opened.len() > MAX_FILE_BYTES {
        return Err(error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.dev() != opened.dev() || meta.ino() != opened.ino() {
            return Err(error());
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() as u64 > MAX_FILE_BYTES || !fs::symlink_metadata(path).map_err(io)?.is_file() {
        return Err(error());
    }
    Ok(bytes)
}
fn ready(s: &Value) -> Result<(), ApiError> {
    if s["pending"] == true || s["revoked"] == true {
        return Err(ApiError::new("device_revoked", "设备尚未确认或已撤销"));
    }
    Ok(())
}
fn projection(v: &Value, keys: &[&str]) -> Value {
    let mut result = serde_json::Map::new();
    for key in keys {
        if let Some(value) = v.get(*key) {
            result.insert((*key).into(), value.clone());
        }
    }
    Value::Object(result)
}
fn garment(v: &Value, assets: &HashSet<String>) -> Value {
    let mut result = projection(
        v,
        &[
            "schema_version",
            "id",
            "member_id",
            "name",
            "category",
            "sub_category",
            "taxonomy_version",
            "brand",
            "size_label",
            "fit",
            "description",
            "notes",
            "purchase_date",
            "purchase_store",
            "purchase_url",
            "color_description",
            "color",
            "pattern",
            "material",
            "style",
            "occasion",
            "tags",
            "seasons",
            "purchase_price_minor",
            "price_exponent",
            "currency",
            "status",
            "status_before_delete",
            "deleted_at",
            "favorite",
            "created_at",
            "updated_at",
        ],
    );
    if let Some(ids) = v["asset_ids"].as_array() {
        result["asset_ids"] = json!(ids
            .iter()
            .filter(|id| id.as_str().is_some_and(|id| assets.contains(id)))
            .collect::<Vec<_>>());
    }
    if let Some(id) = v["display_asset_id"].as_str() {
        if assets.contains(id) {
            result["display_asset_id"] = json!(id);
        }
    }
    result
}
fn items(v: &Value, assets: &HashSet<String>) -> Value {
    json!(v
        .as_array()
        .map(|items| items
            .iter()
            .map(|item| {
                let mut safe = projection(item, &["clothing_id", "role", "layer"]);
                safe["snapshot"] = garment(&item["snapshot"], assets);
                safe
            })
            .collect::<Vec<_>>())
        .unwrap_or_default())
}
fn learning_doc(table: &str, v: &Value) -> Value {
    let keys: &[&str] = match table {
        "learning_profiles" => &[
            "schema_version",
            "id",
            "member_id",
            "algorithm_version",
            "inferred",
            "reset_boundary",
            "include_prior",
            "created_at",
        ],
        "learning_state" => &["member_id", "cutoff", "suppressed"],
        _ => &[
            "schema_version",
            "id",
            "member_id",
            "operation",
            "profile_id",
            "dimension",
            "value",
            "weight",
            "note",
            "source",
            "algorithm_version",
            "root_id",
            "deleted",
            "revision",
            "created_at",
            "state",
        ],
    };
    let mut safe = projection(v, keys);
    if let Some(signals) = safe["inferred"].as_array_mut() {
        for signal in signals {
            let source = projection(&signal["source"], &["kind", "feedback_ids", "wear_ids"]);
            *signal = projection(
                signal,
                &["dimension", "value", "weight", "algorithm_version"],
            );
            signal["source"] = source;
        }
    }
    if safe.get("source").is_some() {
        safe["source"] = projection(&safe["source"], &["kind", "feedback_ids", "wear_ids"]);
    }
    safe
}
fn recommendation_controls(v: &Value) -> Value {
    projection(
        v,
        &[
            "member_id",
            "operation_key",
            "timezone",
            "limit",
            "strategy",
            "temperature_c",
            "occasion",
            "fixed_ids",
            "excluded_ids",
            "inventory_priority_ids",
            "retained_ids",
            "retained_items",
            "action_excluded_ids",
            "excluded_sets",
            "source_intent",
            "replacement_category",
        ],
    )
}
fn recommendation_proposal(v: &Value) -> Value {
    let mut safe = projection(
        v,
        &["complete", "missing_roles", "repeat_relaxed", "reasons"],
    );
    safe["items"] = json!(v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| projection(v, &["clothing_id", "name", "category", "role", "layer"]))
        .collect::<Vec<_>>());
    safe
}
fn recommendation_doc(v: &Value) -> Value {
    let mut safe = projection(
        v,
        &[
            "schema_version",
            "id",
            "member_id",
            "created_at",
            "updated_at",
            "requested_count",
            "actual_count",
            "offline",
            "creates_wear",
            "shortage_reasons",
            "recent_window",
            "learning_context",
            "feedback_context",
        ],
    );
    safe["controls"] = recommendation_controls(&v["controls"]);
    safe["proposals"] = json!(v["proposals"]
        .as_array()
        .into_iter()
        .flatten()
        .map(recommendation_proposal)
        .collect::<Vec<_>>());
    safe
}
fn feedback_doc(v: &Value, assets: &HashSet<String>) -> Value {
    let mut safe = projection(
        v,
        &[
            "schema_version",
            "id",
            "member_id",
            "root_id",
            "revision",
            "revoked",
            "intent_id",
            "proposal_index",
            "inventory_observed_at",
            "controls",
            "reaction",
            "wear_event_id",
            "day_scope",
            "clothing_ids",
            "scores",
            "created_at",
            "reason",
        ],
    );
    safe["inventory_snapshots"] = json!(v["inventory_snapshots"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| garment(v, assets))
        .collect::<Vec<_>>());
    safe["proposal"] = recommendation_proposal(&v["proposal"]);
    safe["controls"] = recommendation_controls(&v["controls"]);
    safe
}
fn portable_receipts(
    app: &Application,
    table: &str,
    assets: &HashSet<String>,
) -> Result<Vec<Value>, ApiError> {
    let mut st=app.connection.prepare(&format!("SELECT member_id,operation_key,request,result FROM {table} ORDER BY member_id,operation_key"))?;
    let mut rows = st.query([])?;
    let mut result = vec![];
    let mut total = 0usize;
    while let Some(row) = rows.next()? {
        let member: String = row.get(0)?;
        let key: String = row.get(1)?;
        let request: Value =
            serde_json::from_str(&row.get::<_, String>(2)?).map_err(|_| error())?;
        let result_doc: Value =
            serde_json::from_str(&row.get::<_, String>(3)?).map_err(|_| error())?;
        let input = if request.get("operation").is_some() {
            &request["input"]
        } else {
            &request
        };
        let input = projection(
            input,
            &[
                "member_id",
                "operation_key",
                "dimension",
                "value",
                "weight",
                "note",
                "root_id",
                "expected_revision",
                "scope",
                "include_prior",
                "intent_id",
                "proposal_index",
                "reaction",
                "scores",
                "timezone",
                "clothing_ids",
                "wear",
                "reason",
            ],
        );
        let request = if request.get("operation").is_some() {
            json!({"operation":request["operation"],"input":input})
        } else {
            input
        };
        let result_doc = if table == "feedback_receipts" {
            feedback_doc(&result_doc, assets)
        } else {
            learning_doc("learning_history", &result_doc)
        };
        let row =
            json!({"member_id":member,"operation_key":key,"request":request,"result":result_doc});
        total += row.to_string().len();
        if total > MAX_BYTES {
            return Err(ApiError::new(
                "package_too_large",
                "学习或反馈收据超出单包容量，未导出任何文件",
            ));
        }
        result.push(row);
    }
    Ok(result)
}
fn facts(app: &Application, table: &str, assets: &HashSet<String>) -> Result<Vec<Value>, ApiError> {
    // Identifiers come only from this private, fixed call site list.
    let mut statement = app.connection.prepare(&format!(
        "SELECT document,{} FROM {table} ORDER BY rowid",
        if table == "learning_state" {
            "member_id"
        } else {
            "NULL"
        }
    ))?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    let mut result = Vec::new();
    let mut total = 0usize;
    for row in rows {
        let (row, owner) = row?;
        total = total.checked_add(row.len()).ok_or_else(error)?;
        if total > MAX_BYTES {
            return Err(ApiError::new(
                "package_too_large",
                "事实快照超出单包容量，未导出任何文件",
            ));
        }
        let v: Value = serde_json::from_str(&row).map_err(|_| error())?;
        let safe = if table.starts_with("learning_") {
            let mut value = v.clone();
            if table == "learning_state" {
                value["member_id"] = json!(owner);
            }
            learning_doc(table, &value)
        } else if table == "recommendation_intents" {
            recommendation_doc(&v)
        } else if table == "feedback_events" {
            feedback_doc(&v, assets)
        } else if table == "clothing" {
            garment(&v, assets)
        } else if table == "insights_preferences" {
            projection(
                &v,
                &[
                    "schema_version",
                    "id",
                    "member_id",
                    "stale_days",
                    "revision",
                    "created_at",
                    "updated_at",
                ],
            )
        } else if table == "care_config" {
            projection(
                &v,
                &[
                    "schema_version",
                    "id",
                    "member_id",
                    "clothing_id",
                    "wash_after_wears",
                    "revision",
                    "created_at",
                    "updated_at",
                ],
            )
        } else {
            let mut safe = projection(
                &v,
                &[
                    "schema_version",
                    "clothing_id",
                    "action",
                    "id",
                    "member_id",
                    "name",
                    "version",
                    "status",
                    "favorite",
                    "created_at",
                    "updated_at",
                    "root_event_id",
                    "revision",
                    "kind",
                    "supersedes_event_id",
                    "notes",
                    "reason",
                    "local_date",
                    "timezone",
                    "local_time",
                    "time_precision",
                    "occurred_at",
                ],
            );
            if table == "care_events" {
                safe["snapshot"] = garment(&v["snapshot"], assets);
            } else {
                safe["items"] = items(&v["items"], assets);
            }
            if !v["source_outfit"].is_null() {
                let mut source = projection(
                    &v["source_outfit"],
                    &[
                        "schema_version",
                        "id",
                        "member_id",
                        "name",
                        "version",
                        "status",
                        "favorite",
                        "created_at",
                        "updated_at",
                    ],
                );
                source["items"] = items(&v["source_outfit"]["items"], assets);
                safe["source_outfit"] = source;
            }
            safe
        };
        result.push(safe);
    }
    Ok(result)
}
fn history_refs(
    app: &Application,
    table: &str,
    assets: &HashSet<String>,
) -> Result<Vec<Value>, ApiError> {
    let mut stmt = app.connection.prepare(&format!(
        "SELECT event_id,asset_id,member_id FROM {table} ORDER BY event_id,asset_id"
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut result = vec![];
    let mut total = 0usize;
    for row in rows {
        let (event, asset, member) = row?;
        if !assets.contains(&asset) {
            return Err(error());
        }
        let reference = json!({"event_id":event,"asset_id":asset,"member_id":member});
        total += reference.to_string().len();
        if total > MAX_BYTES {
            return Err(ApiError::new(
                "package_too_large",
                "历史引用超出单包容量，未导出任何文件",
            ));
        }
        result.push(reference);
    }
    Ok(result)
}
fn encrypt_object(
    s: &Value,
    w: &Secret,
    package: &str,
    kind: &str,
    value: Value,
) -> Result<(String, Vec<u8>, Value), ApiError> {
    let name = uuid::Uuid::from_bytes(random::<16>()?).to_string();
    let plain =
        Zeroizing::new(json!({"package_id":package,"object_id":name,"value":value}).to_string());
    if plain.len() > MAX_BYTES {
        return Err(ApiError::new(
            "package_too_large",
            "单个密文对象超出大小限制",
        ));
    }
    let epoch = s["epoch"].as_u64().ok_or_else(error)?;
    let master = master(w, epoch)?;
    let key = derive(master.as_ref(), string(s, "space_id")?, epoch, kind)?;
    let envelope = seal(
        key.as_ref(),
        string(s, "space_id")?,
        epoch,
        kind,
        plain.as_bytes(),
    )?;
    let bytes = serde_json::to_vec(&envelope).map_err(|_| error())?;
    let descriptor = json!({"name":name,"kind":kind,"sha256":hash(&bytes),"bytes":bytes.len()});
    Ok((name, bytes, descriptor))
}
fn manifest(s: &Value, supplied: &Value) -> Result<(), ApiError> {
    fields(supplied, &["payload_base64", "signature"])?;
    let raw = bytes(supplied, "payload_base64", MAX_BYTES)?;
    let key = VerifyingKey::from_bytes(&exact::<32>(s, "owner_public")?).map_err(|_| error())?;
    key.verify_strict(
        &raw,
        &Signature::from_bytes(&exact::<64>(supplied, "signature")?),
    )
    .map_err(|_| error())?;
    let payload: Value = serde_json::from_slice(&raw).map_err(|_| error())?;
    if payload["space_id"] != s["space_id"]
        || payload["epoch"] != s["epoch"]
        || payload["revision"] != s["revision"]
        || supplied != &s["manifest"]
    {
        return Err(ApiError::new(
            "device_state_required",
            "密文包需要匹配的已验证设备状态，请先更新设备清单",
        ));
    }
    Ok(())
}
pub(super) fn execute(app: &Application, op: &str, input: Value) -> Result<Value, ApiError> {
    if op == "encryption.package.export" {
        fields(&input, &["directory"])?;
        let target = fs::canonicalize(string(&input, "directory")?).map_err(io)?;
        if !target.is_dir() {
            return Err(error());
        }
        let local = fs::canonicalize(app.asset_root.parent().ok_or_else(error)?).map_err(io)?;
        if target.starts_with(local) {
            return Err(ApiError::new(
                "invalid_input",
                "请选择应用数据目录之外的独立存储文件夹",
            ));
        }
        let tx = app.begin_write()?;
        let (mut s, w) = existing(app)?;
        ready(&s)?;
        if s["manifest"].is_null() {
            assert_owner(&s)?;
            s["manifest"] = signed_manifest(&s, &w)?;
            update(&tx, &s, &w)?;
        }
        manifest(&s, &s["manifest"])?;
        let members = app.execute("member.list", json!({}))?["items"].clone();
        let mut approved = Vec::new();
        let mut objects = Vec::new();
        let mut total_bytes = 0u64;
        let package = uuid::Uuid::from_bytes(random::<16>()?).to_string();
        for member in members.as_array().ok_or_else(error)? {
            let id = string(member, "member_id")?;
            let list = app.asset_sync_manifest(json!({"member_id":id}))?;
            for entry in list["assets"].as_array().ok_or_else(error)? {
                let asset = app.asset_get(json!({"member_id":id,"id":entry["id"]}))?;
                let decoded = crate::image_editing::read_display(&asset)?;
                let data = bounded(Path::new(string(&asset["display"], "local_path")?))?;
                if hash(&data) != entry["blob_hash"]
                    || data.len() as u64 != entry["bytes"].as_u64().ok_or_else(error)?
                {
                    return Err(error());
                }
                if image::guess_format(&data).map_err(|_| error())? != image::ImageFormat::WebP
                    || decoded.width() == 0
                    || decoded.height() == 0
                    || decoded.width() > 4096
                    || decoded.height() > 4096
                    || entry["width"] != decoded.width()
                    || entry["height"] != decoded.height()
                {
                    return Err(error());
                }
                let mut info = entry.clone();
                info["member_id"] = json!(id);
                let mut blob = info.clone();
                blob["data_base64"] = json!(STANDARD.encode(data));
                let object = encrypt_object(&s, &w, &package, "blobs", blob)?;
                total_bytes = total_bytes
                    .checked_add(object.1.len() as u64)
                    .ok_or_else(error)?;
                if objects.len() >= MAX_OBJECTS - 1 || total_bytes > MAX_PACKAGE_BYTES {
                    return Err(ApiError::new(
                        "package_too_large",
                        "文件包超过容量限制，未导出任何文件",
                    ));
                }
                objects.push(object);
                approved.push(info);
            }
        }
        let ids = approved
            .iter()
            .filter_map(|a| a["id"].as_str().map(str::to_owned))
            .collect();
        let snapshot = json!({"schema_version":1,"space_id":s["space_id"],"members":members,"clothing":facts(app,"clothing",&ids)?,"outfits":facts(app,"outfits",&ids)?,"outfit_versions":facts(app,"outfit_versions",&ids)?,"wear_events":facts(app,"wear_events",&ids)?,"care_config":facts(app,"care_config",&ids)?,"care_events":facts(app,"care_events",&ids)?,"care_asset_refs":history_refs(app,"care_asset_refs",&ids)?,"insights_preferences":facts(app,"insights_preferences",&ids)?,"recommendation_intents":facts(app,"recommendation_intents",&ids)?,"feedback_events":facts(app,"feedback_events",&ids)?,"feedback_receipts":portable_receipts(app,"feedback_receipts",&ids)?,"feedback_asset_refs":history_refs(app,"feedback_asset_refs",&ids)?,"learning_memories":facts(app,"learning_memories",&ids)?,"learning_profiles":facts(app,"learning_profiles",&ids)?,"learning_state":facts(app,"learning_state",&ids)?,"learning_history":facts(app,"learning_history",&ids)?,"learning_receipts":portable_receipts(app,"learning_receipts",&ids)?});
        let snapshot_object = encrypt_object(&s, &w, &package, "snapshots", snapshot)?;
        total_bytes = total_bytes
            .checked_add(snapshot_object.1.len() as u64)
            .ok_or_else(error)?;
        if total_bytes > MAX_PACKAGE_BYTES {
            return Err(ApiError::new(
                "package_too_large",
                "文件包超过容量限制，未导出任何文件",
            ));
        }
        let snapshot_name = snapshot_object.0.clone();
        objects.push(snapshot_object);
        let index = json!({"package_id":package,"sender_device_id":s["device_id"],"manifest_sha256":hash(&serde_json::to_vec(&s["manifest"]).map_err(|_|error())?),"snapshot":snapshot_name,"objects":objects.iter().map(|o|o.2.clone()).collect::<Vec<_>>()});
        let epoch = s["epoch"].as_u64().ok_or_else(error)?;
        let master = master(&w, epoch)?;
        let key = derive(master.as_ref(), string(&s, "space_id")?, epoch, "snapshots")?;
        let index_plain = Zeroizing::new(index.to_string());
        if index_plain.len() > MAX_BYTES {
            return Err(error());
        }
        let header = json!({"protocol_version":1,"space_id":s["space_id"],"epoch":epoch,"manifest":s["manifest"],"index":seal(key.as_ref(),string(&s,"space_id")?,epoch,"snapshots",index_plain.as_bytes())?});
        let header_bytes = serde_json::to_vec(&header).map_err(|_| error())?;
        if header_bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(ApiError::new(
                "package_too_large",
                "设备清单或文件索引超出单包容量，未导出任何文件",
            ));
        }
        let staging = target.join(format!(".wardrobe-{package}.tmp"));
        let published = target.join(format!("wardrobe-{package}"));
        fs::create_dir(&staging).map_err(io)?;
        let emitted = (|| {
            for (name, data, _) in &objects {
                write_object(&staging.join(name), data)?;
            }
            write_object(&staging.join("package.json"), &header_bytes)?;
            // Windows File::open(directory) is not a directory handle; all
            // object files are flushed above and publish uses atomic rename.
            #[cfg(unix)]
            fs::File::open(&staging)
                .map_err(io)?
                .sync_all()
                .map_err(io)?;
            tx.commit()?;
            fs::rename(&staging, &published).map_err(io)?;
            Ok(
                json!({"schema_version":1,"path":published,"asset_count":approved.len(),"epoch":epoch}),
            )
        })();
        if emitted.is_err() {
            let _ = fs::remove_dir_all(staging);
        }
        emitted
    } else {
        fields(&input, &["path"])?;
        let root = Path::new(string(&input, "path")?);
        if !fs::symlink_metadata(root).map_err(io)?.is_dir() {
            return Err(error());
        }
        let (s, w) = existing(app)?;
        ready(&s)?;
        let header: Value =
            serde_json::from_slice(&bounded(&root.join("package.json"))?).map_err(|_| error())?;
        fields(
            &header,
            &["protocol_version", "space_id", "epoch", "manifest", "index"],
        )?;
        if header["protocol_version"] != 1
            || header["space_id"] != s["space_id"]
            || header["epoch"] != s["epoch"]
        {
            return Err(error());
        }
        manifest(&s, &header["manifest"])?;
        let epoch = s["epoch"].as_u64().ok_or_else(error)?;
        let master = master(&w, epoch)?;
        let key = derive(master.as_ref(), string(&s, "space_id")?, epoch, "snapshots")?;
        let index: Value = serde_json::from_slice(&open(
            key.as_ref(),
            string(&s, "space_id")?,
            epoch,
            "snapshots",
            &header["index"],
        )?)
        .map_err(|_| error())?;
        fields(
            &index,
            &[
                "package_id",
                "sender_device_id",
                "manifest_sha256",
                "snapshot",
                "objects",
            ],
        )?;
        if index["manifest_sha256"]
            != hash(&serde_json::to_vec(&s["manifest"]).map_err(|_| error())?)
            || !s["devices"]
                .as_array()
                .ok_or_else(error)?
                .iter()
                .any(|d| d["device_id"] == index["sender_device_id"])
        {
            return Err(error());
        }
        let mut snapshot = None;
        let mut assets = Vec::new();
        let mut names = HashSet::new();
        let mut total_bytes = 0u64;
        let descriptors = index["objects"].as_array().ok_or_else(error)?;
        if descriptors.len() > MAX_OBJECTS {
            return Err(error());
        }
        for descriptor in descriptors {
            fields(descriptor, &["name", "kind", "sha256", "bytes"])?;
            let name = string(descriptor, "name")?;
            if uuid::Uuid::parse_str(name).is_err() || !names.insert(name) {
                return Err(error());
            }
            let kind = string(descriptor, "kind")?;
            if !["snapshots", "blobs"].contains(&kind) {
                return Err(error());
            }
            let bytes = bounded(&root.join(name))?;
            total_bytes = total_bytes
                .checked_add(bytes.len() as u64)
                .ok_or_else(error)?;
            if total_bytes > MAX_PACKAGE_BYTES {
                return Err(ApiError::new(
                    "package_too_large",
                    "密文包超出 256 MiB 读取限制",
                ));
            }
            if descriptor["sha256"] != hash(&bytes) || descriptor["bytes"] != bytes.len() {
                return Err(error());
            }
            let envelope = serde_json::from_slice(&bytes).map_err(|_| error())?;
            let object_key = derive(master.as_ref(), string(&s, "space_id")?, epoch, kind)?;
            let object: Value = serde_json::from_slice(&open(
                object_key.as_ref(),
                string(&s, "space_id")?,
                epoch,
                kind,
                &envelope,
            )?)
            .map_err(|_| error())?;
            fields(&object, &["package_id", "object_id", "value"])?;
            if object["package_id"] != index["package_id"] || object["object_id"] != name {
                return Err(error());
            }
            if kind == "snapshots" {
                if index["snapshot"] != name || snapshot.is_some() {
                    return Err(error());
                }
                snapshot = Some(object["value"].clone());
            } else {
                let blob = &object["value"];
                let data = bytes_decode(blob)?;
                if hash(&data) != blob["blob_hash"]
                    || blob["role"] != "product_display"
                    || blob["state"] != "adopted"
                    || blob["sync_scope"] != "workspace"
                    || blob["mime"] != "image/webp"
                    || blob["bytes"] != data.len()
                {
                    return Err(error());
                }
                let mut reader = image::ImageReader::with_format(
                    std::io::Cursor::new(&data),
                    image::ImageFormat::WebP,
                );
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(4096);
                limits.max_image_height = Some(4096);
                limits.max_alloc = Some(64 * 1024 * 1024);
                reader.limits(limits);
                let image = reader.decode().map_err(|_| error())?;
                if image.width() == 0
                    || image.height() == 0
                    || image.width() > 4096
                    || image.height() > 4096
                    || blob["width"] != image.width()
                    || blob["height"] != image.height()
                {
                    return Err(error());
                }
                assets.push(blob.clone());
            }
        }
        Ok(
            json!({"schema_version":1,"snapshot":snapshot.ok_or_else(error)?,"assets":assets,"epoch":epoch}),
        )
    }
}
fn bytes_decode(blob: &Value) -> Result<Vec<u8>, ApiError> {
    bytes(blob, "data_base64", MAX_BYTES)
}
