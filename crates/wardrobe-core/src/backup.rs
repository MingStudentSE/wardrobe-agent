//! Portable facts: credentials and device trust are deliberately installation-local.
use crate::{string, ApiError, Application, DATABASE_SCHEMA_VERSION};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
const TABLES: &[&str] = &[
    "members",
    "workspace",
    "clothing",
    "audit",
    "outbox",
    "assets",
    "outfits",
    "outfit_items",
    "outfit_versions",
    "wear_events",
    "wear_receipts",
    "wear_asset_refs",
    "care_config",
    "care_events",
    "care_receipts",
    "care_asset_refs",
    "insights_preferences",
];
fn io(_: std::io::Error) -> ApiError {
    ApiError::new("backup_io", "备份文件操作失败，原数据保留")
}
fn checksum(path: &Path) -> Result<(String, u64), ApiError> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if !metadata.is_file() {
        return Err(ApiError::new("backup_invalid", "备份文件必须是普通文件"));
    }
    let mut file = fs::File::open(path).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata().map_err(io)?;
        if metadata.ino() != opened.ino() || metadata.dev() != opened.dev() {
            return Err(ApiError::new("backup_invalid", "备份文件已变化"));
        }
    }
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(io)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        bytes += n as u64;
    }
    Ok((
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect(),
        bytes,
    ))
}
fn rewrite_paths(db: &Connection, root: &Path) -> Result<(), ApiError> {
    let rows = db
        .prepare("SELECT id,member_id,document FROM assets")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, member, raw) in rows {
        let mut doc: Value = serde_json::from_str(&raw)
            .map_err(|_| ApiError::new("backup_invalid", "图片记录损坏"))?;
        for variant in ["original", "display", "thumbnail"] {
            if let Some(relative) = doc[variant]["local_path"].as_str() {
                if !relative.starts_with("media/")
                    || Path::new(relative)
                        .components()
                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                {
                    return Err(ApiError::new("backup_invalid", "非法图片路径"));
                }
                doc[variant]["local_path"] = json!(root.join(relative));
            }
        }
        db.execute(
            "UPDATE assets SET document=?1 WHERE id=?2 AND member_id=?3",
            rusqlite::params![doc.to_string(), id, member],
        )?;
    }
    Ok(())
}
fn bounded_manifest(path: &Path) -> Result<Vec<u8>, ApiError> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err(ApiError::new("backup_invalid", "备份清单超过范围"));
    }
    let mut result = Vec::new();
    fs::File::open(path)
        .map_err(io)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut result)
        .map_err(io)?;
    if result.len() > 16 * 1024 * 1024 {
        return Err(ApiError::new("backup_invalid", "备份清单超过范围"));
    }
    Ok(result)
}
fn columns(db: &Connection, table: &str) -> Result<Vec<(String, String, i64)>, ApiError> {
    Ok(db
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))?
        .query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(5)?)))?
        .collect::<Result<Vec<_>, _>>()?)
}
fn counts(db: &Connection) -> Result<Value, ApiError> {
    let mut result = serde_json::Map::new();
    for table in TABLES {
        let count: u64 = db.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| {
            r.get(0)
        })?;
        result.insert((*table).into(), json!(count));
    }
    Ok(Value::Object(result))
}
fn validate_media(db: &Connection, manifest: &Value) -> Result<(), ApiError> {
    let files = manifest["files"]
        .as_array()
        .ok_or_else(|| ApiError::new("backup_invalid", "缺少清单"))?;
    let mut inventory = std::collections::HashSet::new();
    for item in files {
        let name = item["path"]
            .as_str()
            .ok_or_else(|| ApiError::new("backup_invalid", "缺少文件名"))?;
        if !inventory.insert(name) {
            return Err(ApiError::new("backup_invalid", "文件清单重复"));
        }
    }
    if !inventory.contains("facts.sqlite") {
        return Err(ApiError::new("backup_invalid", "缺少事实数据库"));
    }
    let mut stmt = db.prepare("SELECT document FROM assets")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        let document: Value = serde_json::from_str(&raw)
            .map_err(|_| ApiError::new("backup_invalid", "图片记录损坏"))?;
        for variant in ["original", "display", "thumbnail"] {
            if let Some(name) = document[variant]["local_path"].as_str() {
                if !name.starts_with("media/") || !inventory.contains(name) {
                    return Err(ApiError::new("backup_invalid", "图片引用缺失"));
                }
            } else if variant != "original" {
                return Err(ApiError::new("backup_invalid", "展示引用缺失"));
            }
        }
    }
    Ok(())
}
impl Application {
    pub(super) fn backup_operation(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        let allowed: &[&str] = match operation {
            "backup.create" => &["directory", "include_originals"],
            "backup.preview" => &["directory"],
            "backup.restore" => &["directory", "confirm_overwrite"],
            _ => &[],
        };
        if input
            .as_object()
            .unwrap()
            .keys()
            .any(|key| !allowed.contains(&key.as_str()))
            || input
                .get("include_originals")
                .is_some_and(|v| !v.is_boolean())
            || input
                .get("confirm_overwrite")
                .is_some_and(|v| !v.is_boolean())
        {
            return Err(ApiError::new("invalid_input", "备份参数不支持"));
        }
        let directory = Path::new(string(&input, "directory")?);
        match operation {
            "backup.create" => {
                if directory.exists() {
                    return Err(ApiError::new("already_exists", "请选择尚不存在的备份目录"));
                }
                let staging = directory.with_extension(format!("staging-{}", uuid::Uuid::now_v7()));
                fs::create_dir_all(&staging).map_err(io)?;
                let result = (|| {
                    let snapshot = rusqlite::Transaction::new_unchecked(
                        &self.connection,
                        rusqlite::TransactionBehavior::Deferred,
                    )?;
                    let destination = Connection::open(staging.join("facts.sqlite"))?;
                    destination.execute_batch("PRAGMA journal_mode=DELETE;PRAGMA foreign_keys=OFF;PRAGMA secure_delete=ON;BEGIN IMMEDIATE;")?;
                    for table in TABLES {
                        let sql: String = self.connection.query_row(
                            "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                            [table],
                            |r| r.get(0),
                        )?;
                        destination.execute_batch(&sql)?;
                        let mut statement = self
                            .connection
                            .prepare(&format!("SELECT * FROM \"{table}\""))?;
                        let columns = statement.column_count();
                        let placeholders = (1..=columns)
                            .map(|n| format!("?{n}"))
                            .collect::<Vec<_>>()
                            .join(",");
                        let mut insert = destination
                            .prepare(&format!("INSERT INTO \"{table}\" VALUES({placeholders})"))?;
                        let mut rows = statement.query([])?;
                        while let Some(row) = rows.next()? {
                            let values = (0..columns)
                                .map(|index| row.get::<_, rusqlite::types::Value>(index))
                                .collect::<Result<Vec<_>, _>>()?;
                            insert.execute(rusqlite::params_from_iter(values))?;
                        }
                    }
                    destination.execute_batch(&format!(
                        "PRAGMA user_version={DATABASE_SCHEMA_VERSION};COMMIT;"
                    ))?;
                    let include_originals = input["include_originals"].as_bool().unwrap_or(false);
                    fs::create_dir(staging.join("media")).map_err(io)?;
                    let mut inventory = Vec::new();
                    let rows = destination
                        .prepare("SELECT id,member_id,document FROM assets")?
                        .query_map([], |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, String>(2)?,
                            ))
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    for (id, member, raw) in rows {
                        let mut doc: Value = serde_json::from_str(&raw)
                            .map_err(|_| ApiError::new("backup_invalid", "图片记录损坏"))?;
                        for variant in ["original", "display", "thumbnail"] {
                            if variant == "original" && !include_originals {
                                doc[variant]["local_path"] = Value::Null;
                                doc[variant]["backup_available"] = json!(false);
                                continue;
                            }
                            if let Some(source) = doc[variant]["local_path"].as_str() {
                                let relative = format!("media/{}", uuid::Uuid::now_v7());
                                let mut reader = fs::File::open(source).map_err(io)?;
                                let mut writer = fs::OpenOptions::new()
                                    .create_new(true)
                                    .write(true)
                                    .open(staging.join(&relative))
                                    .map_err(io)?;
                                std::io::copy(&mut reader, &mut writer).map_err(io)?;
                                writer.flush().map_err(io)?;
                                writer.sync_all().map_err(io)?;
                                let (hash, bytes) = checksum(&staging.join(&relative))?;
                                inventory
                                    .push(json!({"path":relative,"sha256":hash,"bytes":bytes}));
                                doc[variant]["local_path"] = json!(relative);
                            }
                        }
                        destination.execute(
                            "UPDATE assets SET document=?1 WHERE id=?2 AND member_id=?3",
                            rusqlite::params![doc.to_string(), id, member],
                        )?;
                    }
                    snapshot.commit()?;
                    destination.execute_batch("VACUUM;")?;
                    let (hash, bytes) = checksum(&staging.join("facts.sqlite"))?;
                    inventory.push(json!({"path":"facts.sqlite","sha256":hash,"bytes":bytes}));
                    fs::write(staging.join("manifest.json"),json!({"format":"wardrobe-backup","version":1,"database_version":DATABASE_SCHEMA_VERSION,"includes_originals":include_originals,"files":inventory}).to_string()).map_err(io)?;
                    drop(destination);
                    fs::rename(&staging, directory).map_err(io)?;
                    Ok(json!({"directory":directory,"includes_originals":include_originals}))
                })();
                if result.is_err() {
                    let _ = fs::remove_dir_all(staging);
                }
                result
            }
            "backup.preview" => {
                let manifest: Value =
                    serde_json::from_slice(&bounded_manifest(&directory.join("manifest.json"))?)
                        .map_err(|_| ApiError::new("backup_invalid", "无效备份清单"))?;
                if manifest["format"] != "wardrobe-backup"
                    || manifest["version"] != 1
                    || manifest["database_version"]
                        .as_u64()
                        .is_none_or(|v| v > DATABASE_SCHEMA_VERSION as u64)
                {
                    return Err(ApiError::new("backup_invalid", "备份格式或版本不支持"));
                }
                for entry in manifest["files"]
                    .as_array()
                    .ok_or_else(|| ApiError::new("backup_invalid", "缺少文件清单"))?
                {
                    let relative = entry["path"]
                        .as_str()
                        .ok_or_else(|| ApiError::new("backup_invalid", "无效文件路径"))?;
                    if Path::new(relative)
                        .components()
                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                    {
                        return Err(ApiError::new("backup_invalid", "非法文件路径"));
                    }
                    let (hash, bytes) = checksum(&directory.join(relative))?;
                    if entry["sha256"] != hash || entry["bytes"] != bytes {
                        return Err(ApiError::new("backup_invalid", "文件校验失败"));
                    }
                }
                let db = Connection::open_with_flags(
                    directory.join("facts.sqlite"),
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
                if integrity != "ok" {
                    return Err(ApiError::new("backup_invalid", "备份数据库损坏"));
                }
                let actual_version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
                if actual_version > DATABASE_SCHEMA_VERSION
                    || manifest["database_version"] != actual_version
                {
                    return Err(ApiError::new("backup_invalid", "备份数据库版本不匹配"));
                }
                let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?.query_map([],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
                if names.len() != TABLES.len()
                    || names.iter().any(|name| !TABLES.contains(&name.as_str()))
                {
                    return Err(ApiError::new("backup_invalid", "备份包含非法表结构"));
                }
                let dangerous: bool = db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type IN ('trigger','view'))",
                    [],
                    |r| r.get(0),
                )?;
                if dangerous {
                    return Err(ApiError::new("backup_invalid", "备份包含非法执行结构"));
                }
                for table in TABLES {
                    if columns(&db, table)? != columns(&self.connection, table)? {
                        return Err(ApiError::new("backup_invalid", "备份字段结构不支持"));
                    }
                }
                let mut check = db.prepare("PRAGMA foreign_key_check")?;
                if check.query([])?.next()?.is_some() {
                    return Err(ApiError::new("backup_invalid", "备份引用缺失"));
                }
                validate_media(&db, &manifest)?;
                Ok(
                    json!({"valid":true,"manifest":manifest,"incoming":counts(&db)?,"current":counts(&self.connection)?,"overwrites_current_facts":true,"local_credentials_included":false,"device_trust_included":false}),
                )
            }
            "backup.restore" => {
                self.backup_operation("backup.preview", json!({"directory":directory}))?;
                if input["confirm_overwrite"] != true {
                    return Err(ApiError::new(
                        "confirmation_required",
                        "恢复会覆盖当前事实，请明确确认",
                    ));
                }
                let rollback = self
                    .asset_root
                    .parent()
                    .unwrap()
                    .join("backup-rollback")
                    .join(uuid::Uuid::now_v7().to_string());
                self.backup_operation(
                    "backup.create",
                    json!({"directory":rollback,"include_originals":true}),
                )?;
                let media = self
                    .asset_root
                    .join(format!("restore-{}", uuid::Uuid::now_v7()));
                fs::create_dir_all(media.join("media")).map_err(io)?;
                for item in fs::read_dir(directory.join("media")).map_err(io)? {
                    let item = item.map_err(io)?;
                    fs::copy(item.path(), media.join("media").join(item.file_name()))
                        .map_err(io)?;
                }
                self.connection.execute(
                    "ATTACH DATABASE ?1 AS restore_source",
                    [directory.join("facts.sqlite").to_string_lossy().as_ref()],
                )?;
                let result = (|| {
                    let tx = self.begin_write()?;
                    tx.execute_batch("PRAGMA defer_foreign_keys=ON;")?;
                    for table in TABLES.iter().rev() {
                        tx.execute_batch(&format!("DELETE FROM \"{table}\""))?;
                    }
                    for table in TABLES {
                        tx.execute_batch(&format!(
                            "INSERT INTO \"{table}\" SELECT * FROM restore_source.\"{table}\""
                        ))?;
                    }
                    rewrite_paths(&tx, &media)?;
                    tx.commit()?;
                    Ok(json!({"restored":true,"rollback_backup":rollback}))
                })();
                let _ = self
                    .connection
                    .execute_batch("DETACH DATABASE restore_source");
                result
            }
            _ => Err(ApiError::new("unsupported_operation", "此备份操作尚不可用")),
        }
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    ["backup.create","backup.preview","backup.restore"].into_iter().map(|name|json!({"name":name,"description":"本地备份、预览与显式恢复","input_schema":{"type":"object","required":["directory"],"properties":{"directory":{"type":"string"},"include_originals":{"type":"boolean"},"confirm_overwrite":{"type":"boolean"}}}})).collect()
}
