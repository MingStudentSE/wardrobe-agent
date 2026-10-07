//! Portable facts: credentials and device trust are deliberately installation-local.
use crate::{string, ApiError, Application, DATABASE_SCHEMA_VERSION};
use rusqlite::{Connection, OptionalExtension};
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
    "recommendation_intents",
    "recommendation_saves",
    "weather_preferences",
    "feedback_events",
    "feedback_receipts",
    "feedback_asset_refs",
    "schedule_plans",
    "schedule_executions",
];
pub(super) fn migrate(tx: &rusqlite::Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS backup_preferences(id INTEGER PRIMARY KEY CHECK(id=1),document TEXT NOT NULL);CREATE TABLE IF NOT EXISTS local_installation_state(id INTEGER PRIMARY KEY CHECK(id=1),generation INTEGER NOT NULL);INSERT OR IGNORE INTO local_installation_state VALUES(1,0);CREATE TABLE IF NOT EXISTS local_submission_guards(space_id TEXT NOT NULL,member_id TEXT NOT NULL,kind TEXT NOT NULL,operation_key TEXT NOT NULL,payload_hash TEXT NOT NULL,submitted_at TEXT NOT NULL,submission_count INTEGER NOT NULL,last_outcome TEXT NOT NULL,PRIMARY KEY(space_id,member_id,kind,operation_key));")?;
    Ok(())
}
#[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
pub(super) fn process_test_gate(phase: &str) {
    let Some(directory) = std::env::var_os("WARDROBE_BACKUP_TEST_GATE") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let marker = directory.join(phase);
    if fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .is_err()
    {
        return;
    }
    let started = std::time::Instant::now();
    while !directory.join("continue").exists()
        && started.elapsed() < std::time::Duration::from_secs(15)
    {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn io(_: std::io::Error) -> ApiError {
    ApiError::new("backup_io", "备份文件操作失败，原数据保留")
}
fn copy_checked(
    reader: &mut impl Read,
    writer: &mut impl Write,
    control: Option<&crate::maintenance::Control>,
) -> Result<(), ApiError> {
    let mut buffer = [0; 65536];
    loop {
        if let Some(control) = control {
            control.tick()?;
        }
        let bytes = reader.read(&mut buffer).map_err(io)?;
        if bytes == 0 {
            break;
        }
        writer.write_all(&buffer[..bytes]).map_err(io)?;
    }
    Ok(())
}
pub(super) fn checksum(path: &Path) -> Result<(String, u64), ApiError> {
    checksum_controlled(path, None)
}
fn checksum_controlled(
    path: &Path,
    control: Option<&crate::maintenance::Control>,
) -> Result<(String, u64), ApiError> {
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
        if let Some(control) = control {
            control.tick()?;
        }
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
        if !table_exists(db, table)? {
            result.insert((*table).into(), json!(0));
            continue;
        }
        let count: u64 = db.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| {
            r.get(0)
        })?;
        result.insert((*table).into(), json!(count));
    }
    Ok(Value::Object(result))
}
fn validate_references(db: &Connection) -> Result<(), ApiError> {
    let catalog = db
        .prepare("SELECT member_id,id FROM assets")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<std::collections::HashSet<_>, _>>()?;
    fn visit(
        value: &Value,
        member: &str,
        catalog: &std::collections::HashSet<(String, String)>,
    ) -> Result<(), ApiError> {
        match value {
            Value::Object(map) => {
                for (key, item) in map {
                    if ["display_asset_id", "asset_id", "source_asset_id"].contains(&key.as_str()) {
                        if let Some(id) = item.as_str() {
                            if !catalog.contains(&(member.to_owned(), id.to_owned())) {
                                return Err(ApiError::new(
                                    "backup_invalid",
                                    "业务展示资产引用缺失或不属成员",
                                ));
                            }
                        }
                    } else if key == "asset_ids" {
                        if let Some(ids) = item.as_array() {
                            for id in ids {
                                let id = id.as_str().ok_or_else(|| {
                                    ApiError::new("backup_invalid", "资产引用格式不支持")
                                })?;
                                if !catalog.contains(&(member.to_owned(), id.to_owned())) {
                                    return Err(ApiError::new(
                                        "backup_invalid",
                                        "业务资产引用缺失或不属成员",
                                    ));
                                }
                            }
                        }
                    } else {
                        visit(item, member, catalog)?;
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, member, catalog)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for table in TABLES {
        if !table_exists(db, table)? {
            continue;
        }
        let fields = columns(db, table)?;
        if !fields.iter().any(|(name, _, _)| name == "document") {
            continue;
        }
        let has_member = fields.iter().any(|(name, _, _)| name == "member_id");
        let identity = match *table {
            "clothing"
            | "assets"
            | "outfits"
            | "wear_events"
            | "care_events"
            | "recommendation_intents"
            | "feedback_events"
            | "schedule_plans"
            | "schedule_executions" => Some(("id", "id")),
            "care_config" => Some(("clothing_id", "clothing_id")),
            "outfit_versions" => Some(("outfit_id", "id")),
            _ => None,
        };
        let owner = if has_member {
            "entity.member_id"
        } else if *table == "outfit_versions" {
            "(SELECT member_id FROM outfits WHERE id=entity.outfit_id)"
        } else {
            "NULL"
        };
        let history = matches!(*table, "wear_events" | "care_events" | "feedback_events");
        let query = format!(
            "SELECT {owner},document,{},{},{},{},{},{} FROM \"{table}\" AS entity",
            identity
                .map(|(column, _)| format!("entity.{column}"))
                .unwrap_or("NULL".into()),
            if *table == "feedback_events" {
                "root_id"
            } else if history {
                "root_event_id"
            } else {
                "NULL"
            },
            if history { "revision" } else { "NULL" },
            if *table == "outfit_versions" {
                "version"
            } else {
                "NULL"
            },
            if *table == "recommendation_intents" {
                "operation_key"
            } else {
                "NULL"
            },
            if *table == "recommendation_intents" {
                "request"
            } else {
                "NULL"
            }
        );
        let mut statement = db.prepare(&query)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let member: Option<String> = row.get(0)?;
            let raw: String = row.get(1)?;
            let document: Value = serde_json::from_str(&raw)
                .map_err(|_| ApiError::new("backup_invalid", "业务文档格式损坏"))?;
            if !document.is_object() {
                return Err(ApiError::new("backup_invalid", "业务文档须为对象"));
            }
            if let Some(member) = &member {
                if document["member_id"].as_str() != Some(member.as_str()) {
                    return Err(ApiError::new("backup_invalid", "业务成员与数据库行不一致"));
                }
            }
            if let Some((_, key)) = identity {
                let expected: String = row.get(2)?;
                if document[key].as_str() != Some(expected.as_str()) {
                    return Err(ApiError::new("backup_invalid", "业务身份与数据库行不一致"));
                }
            }
            if history {
                let root: String = row.get(3)?;
                let revision: i64 = row.get(4)?;
                if document[if *table == "feedback_events" {
                    "root_id"
                } else {
                    "root_event_id"
                }] != root
                    || document["revision"] != revision
                    || revision < 1
                {
                    return Err(ApiError::new(
                        "backup_invalid",
                        "历史根或修订与数据库行不一致",
                    ));
                }
            }
            if *table == "outfit_versions" && document["version"] != row.get::<_, i64>(5)? {
                return Err(ApiError::new("backup_invalid", "搭配版本与数据库行不一致"));
            }
            if *table == "recommendation_intents" {
                let key: String = row.get(6)?;
                let request: String = row.get(7)?;
                let request: Value = serde_json::from_str(&request)
                    .map_err(|_| ApiError::new("backup_invalid", "推荐请求格式损坏"))?;
                if document["controls"]["operation_key"] != key || request["operation_key"] != key {
                    return Err(ApiError::new(
                        "backup_invalid",
                        "推荐意图与幂等请求身份不一致",
                    ));
                }
            }
            if *table == "feedback_events" {
                crate::feedback::validate_stored_document(db, &document)
                    .map_err(|_| ApiError::new("backup_invalid", "反馈领域格式或事实引用不支持"))?;
            }
            if *table == "schedule_plans" {
                crate::schedules::validate_portable_plan(&document)
                    .map_err(|_| ApiError::new("backup_invalid", "定时计划领域格式不支持"))?;
            }
            if *table == "schedule_executions" {
                crate::schedules::validate_portable_execution(db, &document)
                    .map_err(|_| ApiError::new("backup_invalid", "定时执行历史或引用不支持"))?;
            }
            if *table == "weather_preferences" {
                crate::weather::validate_portable_document(&document)
                    .map_err(|_| ApiError::new("backup_invalid", "天气配置格式或私有字段不支持"))?;
            }
            if *table == "clothing" {
                crate::clothing::validate_stored_document(&document)
                    .map_err(|_| ApiError::new("backup_invalid", "单品领域格式不支持"))?;
            }
            let member = member
                .as_deref()
                .or_else(|| document["member_id"].as_str())
                .unwrap_or("");
            visit(&document, member, &catalog)?;
        }
    }
    validate_receipts(db)?;
    Ok(())
}
fn validate_receipts(db: &Connection) -> Result<(), ApiError> {
    let default_member: String = db.query_row(
        "SELECT default_member_id FROM workspace LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    for table in [
        "wear_receipts",
        "care_receipts",
        "recommendation_saves",
        "feedback_receipts",
    ] {
        let mut statement = db.prepare(&format!(
            "SELECT member_id,operation_key,request,result FROM {table}"
        ))?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let member: String = row.get(0)?;
            let key: String = row.get(1)?;
            let request: String = row.get(2)?;
            let result: String = row.get(3)?;
            let request: Value = serde_json::from_str(&request)
                .map_err(|_| ApiError::new("backup_invalid", "保存请求格式损坏"))?;
            let result: Value = serde_json::from_str(&result)
                .map_err(|_| ApiError::new("backup_invalid", "保存结果格式损坏"))?;
            let input = if table == "recommendation_saves"
                || (table == "feedback_receipts" && request.get("operation").is_none())
            {
                &request
            } else {
                &request["input"]
            };
            let requested_member = input["member_id"].as_str();
            let requested_member = if requested_member == Some("default") {
                Some(default_member.as_str())
            } else {
                requested_member
            };
            if requested_member != Some(member.as_str())
                || result["member_id"] != member
                || input["operation_key"] != key
            {
                return Err(ApiError::new(
                    "backup_invalid",
                    "保存收据与成员或幂等身份不一致",
                ));
            }
            let id = result["id"]
                .as_str()
                .ok_or_else(|| ApiError::new("backup_invalid", "保存结果缺少身份"))?;
            let stored = if table == "recommendation_saves" {
                let version = result["version"]
                    .as_i64()
                    .ok_or_else(|| ApiError::new("backup_invalid", "保存结果缺少版本"))?;
                db.query_row("SELECT v.document FROM outfit_versions v JOIN outfits o ON o.id=v.outfit_id WHERE v.outfit_id=?1 AND v.version=?2 AND o.member_id=?3", rusqlite::params![id,version,member], |row|row.get::<_,String>(0)).optional()?
            } else if table == "care_receipts" && result.get("wash_after_wears").is_some() {
                // Configuration receipts retain their original revision; the current configuration may be newer.
                let clothing = result["clothing_id"]
                    .as_str()
                    .ok_or_else(|| ApiError::new("backup_invalid", "护理配置缺少单品身份"))?;
                let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM care_config WHERE member_id=?1 AND clothing_id=?2 AND json_extract(document,'$.id')=?3)", rusqlite::params![member,clothing,id], |row|row.get(0))?;
                if !exists {
                    return Err(ApiError::new("backup_invalid", "护理配置收据引用缺失"));
                }
                None
            } else {
                let entity = if table == "wear_receipts" {
                    "wear_events"
                } else if table == "feedback_receipts" {
                    "feedback_events"
                } else {
                    "care_events"
                };
                let stored = db
                    .query_row(
                        &format!("SELECT document FROM {entity} WHERE id=?1 AND member_id=?2"),
                        rusqlite::params![id, member],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                if stored.is_none() {
                    return Err(ApiError::new("backup_invalid", "历史收据引用缺失"));
                }
                stored
            };
            if table == "recommendation_saves" && stored.is_none() {
                return Err(ApiError::new("backup_invalid", "保存搭配收据引用缺失"));
            }
            if let Some(stored) = stored {
                let stored: Value = serde_json::from_str(&stored)
                    .map_err(|_| ApiError::new("backup_invalid", "收据对应事实损坏"))?;
                if stored != result {
                    return Err(ApiError::new(
                        "backup_invalid",
                        "保存收据与原始不可变事实不一致",
                    ));
                }
            }
        }
    }
    let mut members = db.prepare("SELECT id,name FROM members")?;
    let mut rows = members.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        if uuid::Uuid::parse_str(&id).is_err()
            || name.trim().is_empty()
            || name.chars().count() > 120
        {
            return Err(ApiError::new("backup_invalid", "成员身份或名称格式不支持"));
        }
    }
    Ok(())
}
fn foreign_keys(
    db: &Connection,
    table: &str,
) -> Result<Vec<Vec<rusqlite::types::Value>>, ApiError> {
    let mut statement = db.prepare(&format!("PRAGMA foreign_key_list(\"{table}\")"))?;
    let fields = statement.column_count();
    let rows = statement.query_map([], |row| {
        (0..fields)
            .map(|index| row.get(index))
            .collect::<Result<Vec<rusqlite::types::Value>, _>>()
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}
fn validate_media(db: &Connection, manifest: &Value, directory: &Path) -> Result<(), ApiError> {
    let files = manifest["files"]
        .as_array()
        .ok_or_else(|| ApiError::new("backup_invalid", "缺少清单"))?;
    let mut inventory = std::collections::HashMap::new();
    for item in files {
        let name = item["path"]
            .as_str()
            .ok_or_else(|| ApiError::new("backup_invalid", "缺少文件名"))?;
        if inventory.insert(name, item).is_some() {
            return Err(ApiError::new("backup_invalid", "文件清单重复"));
        }
    }
    if !inventory.contains_key("facts.sqlite") {
        return Err(ApiError::new("backup_invalid", "缺少事实数据库"));
    }
    let mut decoded = std::collections::HashMap::new();
    let mut stmt = db.prepare("SELECT document FROM assets")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        let document: Value = serde_json::from_str(&raw)
            .map_err(|_| ApiError::new("backup_invalid", "图片记录损坏"))?;
        for variant in ["original", "display", "thumbnail"] {
            if let Some(name) = document[variant]["local_path"].as_str() {
                if !name.starts_with("media/") || !inventory.contains_key(name) {
                    return Err(ApiError::new("backup_invalid", "图片引用缺失"));
                }
                let entry = inventory[name];
                if matches!(variant, "original" | "display")
                    && (document[variant]["id"] != entry["sha256"]
                        || document[variant]["bytes"] != entry["bytes"])
                {
                    return Err(ApiError::new(
                        "backup_invalid",
                        "图片内容与不可变资产身份不一致",
                    ));
                }
                if variant != "original" {
                    let dimensions = if let Some(dimensions) = decoded.get(name) {
                        *dimensions
                    } else {
                        let image = crate::assets::read_display(&directory.join(name))
                            .map_err(|_| ApiError::new("backup_invalid", "展示图片内容损坏"))?;
                        let dimensions = (image.width(), image.height());
                        decoded.insert(name.to_owned(), dimensions);
                        dimensions
                    };
                    if document[variant]["width"] != dimensions.0
                        || document[variant]["height"] != dimensions.1
                    {
                        return Err(ApiError::new("backup_invalid", "展示图片尺寸与记录不一致"));
                    }
                }
            } else if variant != "original" {
                return Err(ApiError::new("backup_invalid", "展示引用缺失"));
            }
        }
    }
    Ok(())
}
struct RestoreFiles {
    root: std::path::PathBuf,
    committed: bool,
}
impl Drop for RestoreFiles {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
impl Application {
    pub(super) fn recover_created_backup(
        &self,
        input: &Value,
        id: &str,
        control: Option<&crate::maintenance::Control>,
    ) -> Result<Option<Value>, ApiError> {
        let directory = Path::new(string(input, "directory")?);
        if !directory.is_dir() {
            return Ok(None);
        }
        let owned = bounded_manifest(&directory.join("manifest.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|manifest| manifest["maintenance_job_id"] == id);
        if !owned {
            return Ok(None);
        }
        let validated =
            self.backup_run("backup.preview", json!({"directory":directory}), control)?;
        if validated["manifest"]["includes_originals"]
            != input["include_originals"].as_bool().unwrap_or(false)
        {
            return Err(ApiError::new(
                "backup_invalid",
                "已发布备份范围与维护意图不一致",
            ));
        }
        Ok(Some(
            json!({"schema_version":1,"directory":directory,"includes_originals":validated["manifest"]["includes_originals"]}),
        ))
    }
    pub(super) fn backup_operation(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        self.backup_run(operation, input, None)
    }
    pub(super) fn backup_run(
        &self,
        operation: &str,
        input: Value,
        control: Option<&crate::maintenance::Control>,
    ) -> Result<Value, ApiError> {
        if let Some(control) = control {
            control.tick()?;
        }
        if operation == "backup.settings.get" {
            if !input.as_object().unwrap().is_empty() {
                return Err(ApiError::new("invalid_input", "不支持的设置参数"));
            }
            return Ok(self
                .connection
                .query_row(
                    "SELECT document FROM backup_preferences WHERE id=1",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .map(|raw| serde_json::from_str(&raw))
                .transpose()
                .map_err(|_| ApiError::new("storage_error", "备份设置损坏"))?
                .unwrap_or(json!({"directory":"","include_originals":false,"revision":0})));
        }
        if operation == "backup.settings.set" {
            if input.as_object().unwrap().keys().any(|key| {
                !["directory", "include_originals", "expected_revision"].contains(&key.as_str())
            }) || !input["include_originals"].is_boolean()
            {
                return Err(ApiError::new("invalid_input", "备份设置字段不支持"));
            }
            let directory = string(&input, "directory")?;
            if !Path::new(directory).is_absolute() || directory.len() > 4096 {
                return Err(ApiError::new("invalid_input", "备份目录需要有效绝对路径"));
            }
            let expected = input["expected_revision"]
                .as_u64()
                .ok_or_else(|| ApiError::new("invalid_input", "缺少设置版本"))?;
            let tx = self.begin_write()?;
            let current = self.backup_operation("backup.settings.get", json!({}))?;
            if current["revision"] != expected {
                return Err(ApiError::new("conflict", "备份设置已改变，请重新读取"));
            }
            let document = json!({"directory":directory,"include_originals":input["include_originals"],"revision":expected+1});
            tx.execute("INSERT INTO backup_preferences VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET document=excluded.document",[document.to_string()])?;
            tx.commit()?;
            return Ok(document);
        }
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
                    let snapshot = if self.connection.is_autocommit() {
                        Some(rusqlite::Transaction::new_unchecked(
                            &self.connection,
                            rusqlite::TransactionBehavior::Deferred,
                        )?)
                    } else {
                        None
                    };
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
                        let document_index = statement
                            .column_names()
                            .iter()
                            .position(|name| *name == "document");
                        let mut rows = statement.query([])?;
                        while let Some(row) = rows.next()? {
                            if let Some(control) = control {
                                control.tick()?;
                            }
                            let mut values = (0..columns)
                                .map(|index| row.get::<_, rusqlite::types::Value>(index))
                                .collect::<Result<Vec<_>, _>>()?;
                            if *table == "weather_preferences" {
                                let document_index = document_index.unwrap();
                                let rusqlite::types::Value::Text(raw) = &values[document_index]
                                else {
                                    return Err(ApiError::new(
                                        "backup_invalid",
                                        "天气配置记录损坏",
                                    ));
                                };
                                let mut document: Value =
                                    serde_json::from_str(raw).map_err(|_| {
                                        ApiError::new("backup_invalid", "天气配置记录损坏")
                                    })?;
                                document["cache"] = Value::Null;
                                document["last_error"] = Value::Null;
                                values[document_index] =
                                    rusqlite::types::Value::Text(document.to_string());
                            }
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
                        if let Some(control) = control {
                            control.tick()?;
                        }
                        let mut doc: Value = serde_json::from_str(&raw)
                            .map_err(|_| ApiError::new("backup_invalid", "图片记录损坏"))?;
                        for variant in ["original", "display", "thumbnail"] {
                            if variant == "original" && !include_originals {
                                doc[variant]["local_path"] = Value::Null;
                                doc[variant]["backup_available"] = json!(false);
                                continue;
                            }
                            if let Some(source) = doc[variant]["local_path"].as_str() {
                                let extension = if variant == "original" {
                                    "original"
                                } else {
                                    "webp"
                                };
                                let relative =
                                    format!("media/{}.{extension}", uuid::Uuid::now_v7());
                                let mut reader = fs::File::open(source).map_err(io)?;
                                let mut writer = fs::OpenOptions::new()
                                    .create_new(true)
                                    .write(true)
                                    .open(staging.join(&relative))
                                    .map_err(io)?;
                                copy_checked(&mut reader, &mut writer, control)?;
                                writer.flush().map_err(io)?;
                                writer.sync_all().map_err(io)?;
                                let (hash, bytes) =
                                    checksum_controlled(&staging.join(&relative), control)?;
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
                    if let Some(snapshot) = snapshot {
                        snapshot.commit()?;
                    }
                    let (hash, bytes) =
                        checksum_controlled(&staging.join("facts.sqlite"), control)?;
                    inventory.push(json!({"path":"facts.sqlite","sha256":hash,"bytes":bytes}));
                    fs::write(staging.join("manifest.json"),json!({"format":"wardrobe-backup","version":1,"database_version":DATABASE_SCHEMA_VERSION,"includes_originals":include_originals,"files":inventory,"maintenance_job_id":control.filter(|context|context.owns("backup.create")).map(|context|context.publication_id())}).to_string()).map_err(io)?;
                    drop(destination);
                    let result = json!({"schema_version":1,"directory":directory,"includes_originals":include_originals});
                    if let Some(control) = control.filter(|context| context.owns("backup.create")) {
                        control.enter_critical(self)?;
                        let tx = self.begin_write()?;
                        fs::rename(&staging, directory).map_err(io)?;
                        #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
                        process_test_gate("create-published");
                        if let Err(error) = control.complete(&tx, "backup.create", &result) {
                            let _ = fs::remove_dir_all(directory);
                            return Err(error);
                        }
                        if let Err(error) = tx.commit() {
                            let _ = fs::remove_dir_all(directory);
                            return Err(error.into());
                        }
                    } else {
                        fs::rename(&staging, directory).map_err(io)?;
                    }
                    Ok(result)
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
                    if relative.contains(['\\', ':'])
                        || !(relative == "facts.sqlite" || relative.starts_with("media/"))
                        || Path::new(relative)
                            .components()
                            .any(|c| !matches!(c, std::path::Component::Normal(_)))
                    {
                        return Err(ApiError::new("backup_invalid", "非法文件路径"));
                    }
                    let (hash, bytes) = checksum_controlled(&directory.join(relative), control)?;
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
                if names.iter().any(|name| !TABLES.contains(&name.as_str()))
                    || TABLES.iter().any(|table| {
                        !names.contains(&table.to_string())
                            && !(actual_version < 21
                                && matches!(*table, "schedule_plans" | "schedule_executions"))
                    })
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
                    if !names.contains(&table.to_string()) {
                        continue;
                    }
                    if columns(&db, table)? != columns(&self.connection, table)?
                        || foreign_keys(&db, table)? != foreign_keys(&self.connection, table)?
                    {
                        return Err(ApiError::new("backup_invalid", "备份字段结构不支持"));
                    }
                }
                let mut check = db.prepare("PRAGMA foreign_key_check")?;
                if check.query([])?.next()?.is_some() {
                    return Err(ApiError::new("backup_invalid", "备份引用缺失"));
                }
                validate_media(&db, &manifest, directory)?;
                validate_references(&db)?;
                Ok(
                    json!({"valid":true,"manifest":manifest,"incoming":counts(&db)?,"current":counts(&self.connection)?,"overwrites_current_facts":true,"local_credentials_included":false,"device_trust_included":false,"clears_existing_authorization_and_device_trust":true,"discarded_local_jobs":self.connection.query_row("SELECT COUNT(*) FROM jobs",[],|r|r.get::<_,u64>(0))?}),
                )
            }
            "backup.restore" => {
                #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
                eprintln!("backup_test_phase=external_preview");
                let preview =
                    self.backup_run("backup.preview", json!({"directory":directory}), control)?;
                if input["confirm_overwrite"] != true {
                    return Err(ApiError::new(
                        "confirmation_required",
                        "恢复会覆盖当前事实，请明确确认",
                    ));
                }
                let media = self
                    .asset_root
                    .join(format!("restore-{}", uuid::Uuid::now_v7()));
                fs::create_dir_all(media.join("media")).map_err(io)?;
                let mut files_guard = RestoreFiles {
                    root: media.clone(),
                    committed: false,
                };
                #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
                eprintln!("backup_test_phase=owned_copy_and_preview");
                let staged = (|| {
                    fs::write(media.join("manifest.json"), preview["manifest"].to_string())
                        .map_err(io)?;
                    for entry in preview["manifest"]["files"].as_array().unwrap() {
                        let relative = entry["path"].as_str().unwrap();
                        let mut reader = fs::File::open(directory.join(relative)).map_err(io)?;
                        if let Some(parent) = media.join(relative).parent() {
                            fs::create_dir_all(parent).map_err(io)?;
                        }
                        let mut writer = fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(media.join(relative))
                            .map_err(io)?;
                        copy_checked(&mut reader, &mut writer, control)?;
                        writer.sync_all().map_err(io)?;
                    }
                    self.backup_run("backup.preview", json!({"directory":media}), control)?;
                    Ok::<(), ApiError>(())
                })();
                if let Err(error) = staged {
                    let _ = fs::remove_dir_all(&media);
                    return Err(error);
                }
                #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
                eprintln!("backup_test_phase=write_lock_and_prebackup");
                if let Some(control) = control {
                    control.enter_critical(self)?;
                }
                let tx = self.begin_write()?;
                let previous_credential:Option<String>=self.connection.query_row("SELECT json_extract(document,'$.credential_id') FROM encryption_state WHERE id=1",[],|r|r.get(0)).optional()?.flatten();
                let rollback = self
                    .asset_root
                    .parent()
                    .unwrap()
                    .join("backup-rollback")
                    .join(uuid::Uuid::now_v7().to_string());
                self.backup_run(
                    "backup.create",
                    json!({"directory":rollback,"include_originals":true}),
                    control,
                )?;
                #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
                eprintln!("backup_test_phase=attach");
                self.connection.execute(
                    "ATTACH DATABASE ?1 AS restore_source",
                    [media.join("facts.sqlite").to_string_lossy().as_ref()],
                )?;
                let result = (|| {
                    crate::jobs::retain_submission_guards(&tx)?;
                    tx.execute_batch("PRAGMA defer_foreign_keys=ON;DELETE FROM schedule_notifications;DELETE FROM recognition_suggestions;DELETE FROM import_drafts;DELETE FROM import_batches;DELETE FROM jobs;DELETE FROM encryption_pairings;DELETE FROM encryption_state;DELETE FROM clients;")?;
                    for table in TABLES.iter().rev() {
                        tx.execute_batch(&format!("DELETE FROM \"{table}\""))?;
                    }
                    for table in TABLES {
                        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM restore_source.sqlite_master WHERE type='table' AND name=?1)",[*table],|r|r.get(0))?;
                        if !exists && matches!(*table, "schedule_plans" | "schedule_executions") {
                            continue;
                        }
                        #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
                        eprintln!("backup_test_phase=insert:{table}");
                        tx.execute_batch(&format!(
                            "INSERT INTO \"{table}\" SELECT * FROM restore_source.\"{table}\""
                        ))?;
                    }
                    rewrite_paths(&tx, &media)?;
                    tx.execute(
                        "UPDATE local_installation_state SET generation=generation+1 WHERE id=1",
                        [],
                    )?;
                    if let Some(control) = control {
                        control.complete(&tx,"backup.restore",&json!({"schema_version":1,"restored":true,"rollback_backup":rollback,"local_trust_reset":true,"previous_unlock_key_removed":null}))?;
                    }
                    #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
                    eprintln!("backup_test_phase=commit");
                    tx.commit()?;
                    #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
                    process_test_gate("restore-committed");
                    let credential_cleanup = previous_credential.as_deref().is_none_or(|id| {
                        keyring::Entry::new("wardrobe-agent.encryption", id).is_ok_and(|entry| {
                            matches!(
                                entry.delete_credential(),
                                Ok(()) | Err(keyring::Error::NoEntry)
                            )
                        })
                    });
                    Ok(
                        json!({"restored":true,"rollback_backup":rollback,"local_trust_reset":true,"previous_unlock_key_removed":credential_cleanup}),
                    )
                })();
                let _ = self
                    .connection
                    .execute_batch("DETACH DATABASE restore_source");
                if result.is_ok() {
                    files_guard.committed = true;
                    let _ = fs::remove_file(media.join("facts.sqlite"));
                    let _ = fs::remove_file(media.join("manifest.json"));
                }
                result
            }
            _ => Err(ApiError::new("unsupported_operation", "此备份操作尚不可用")),
        }
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    ["backup.create", "backup.preview", "backup.restore", "backup.settings.get", "backup.settings.set"].into_iter().map(|name| {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        if name != "backup.settings.get" {
            properties.insert("directory".into(), json!({"type":"string"}));
            required.push("directory");
        }
        if matches!(name, "backup.create" | "backup.settings.set") {
            properties.insert("include_originals".into(), json!({"type":"boolean"}));
        }
        if name == "backup.restore" {
            properties.insert("confirm_overwrite".into(), json!({"type":"boolean"}));
            required.push("confirm_overwrite");
        }
        if matches!(name, "backup.create" | "backup.preview" | "backup.restore") {
            properties.insert("operation_key".into(), json!({"type":"string","minLength":1,"maxLength":200}));
        }
        if name == "backup.settings.set" {
            properties.insert("expected_revision".into(), json!({"type":"integer","minimum":0}));
            required.extend(["include_originals", "expected_revision"]);
        }
        json!({"name":name,"description":"本地备份、预览与显式恢复","outputSchema":if matches!(name,"backup.create"|"backup.preview"|"backup.restore"){crate::maintenance::job_schema()}else{json!({"type":"object","required":["schema_version"],"properties":{"schema_version":{"const":1}}})},"inputSchema":{"type":"object","additionalProperties":false,"required":required,"properties":properties}})
    }).collect()
}

fn table_exists(db: &Connection, table: &str) -> Result<bool, ApiError> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?)
}
