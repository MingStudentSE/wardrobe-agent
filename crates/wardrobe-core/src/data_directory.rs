use crate::{string, ApiError, Application, DATABASE_SCHEMA_VERSION};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
fn io(_: std::io::Error) -> ApiError {
    ApiError::new("directory_io", "数据目录复制或校验失败，旧目录保留")
}
pub(super) fn resolve(path: &Path) -> Result<PathBuf, ApiError> {
    let mut current = path.to_owned();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..8 {
        if !seen.insert(current.clone()) {
            return Err(ApiError::new("directory_invalid", "数据目录位置循环"));
        }
        let marker = current
            .parent()
            .unwrap_or(Path::new("."))
            .join(".wardrobe-location.json");
        if !marker.exists() {
            return Ok(current);
        }
        let mut bytes = Vec::new();
        fs::File::open(&marker)
            .map_err(io)?
            .take(16385)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes.len() > 16384 {
            return Err(ApiError::new("directory_invalid", "位置记录损坏"));
        }
        let mut record: Value = serde_json::from_slice(&bytes)
            .map_err(|_| ApiError::new("directory_invalid", "位置记录损坏"))?;
        if record["source_file"].as_str() != current.file_name().and_then(|s| s.to_str()) {
            return Ok(current);
        }
        if record["phase"] == "pending" {
            #[cfg(test)]
            AFTER_PENDING_READ.with(|hook| {
                if let Some(hook) = hook.borrow_mut().take() {
                    hook();
                }
            });
            let lock = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(current.parent().unwrap().join(".wardrobe-move.lock"))
                .map_err(io)?;
            match lock.try_lock() {
                Ok(()) => {
                    // The publisher can activate the location between our first read
                    // and lock acquisition. Only an orphaned pending marker is removable.
                    let mut latest = Vec::new();
                    match fs::File::open(&marker) {
                        Ok(file) => {
                            file.take(16385).read_to_end(&mut latest).map_err(io)?;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            return Ok(current)
                        }
                        Err(error) => return Err(io(error)),
                    }
                    if latest.len() > 16384 {
                        return Err(ApiError::new("directory_invalid", "位置记录损坏"));
                    }
                    record = serde_json::from_slice(&latest)
                        .map_err(|_| ApiError::new("directory_invalid", "位置记录损坏"))?;
                    if record["source_file"].as_str()
                        != current.file_name().and_then(|s| s.to_str())
                    {
                        return Ok(current);
                    }
                    if record["phase"] == "pending" {
                        fs::remove_file(&marker).map_err(io)?;
                        return Ok(current);
                    }
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    return Err(ApiError::new(
                        "data_directory_moving",
                        "数据目录正在切换，请稍后重试",
                    ))
                }
                Err(std::fs::TryLockError::Error(e)) => return Err(io(e)),
            }
        }
        let target = PathBuf::from(string(&record, "target_database")?);
        if !target.is_absolute() || !target.is_file() {
            return Err(ApiError::new(
                "directory_invalid",
                "新数据目录不可读，请保留旧源后处理位置设置",
            ));
        }
        current = target;
    }
    Err(ApiError::new("directory_invalid", "位置跳转过多"))
}
fn publish(marker: &Path, record: &Value) -> Result<(), ApiError> {
    let temporary = marker.with_extension(format!("{}.tmp", uuid::Uuid::now_v7()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(io)?;
        file.write_all(record.to_string().as_bytes()).map_err(io)?;
        file.sync_all().map_err(io)?;
        drop(file);
        fs::rename(&temporary, marker).map_err(io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn copy_tree(source: &Path, destination: &Path) -> Result<u64, ApiError> {
    fs::create_dir_all(destination).map_err(io)?;
    let mut total = 0;
    for item in fs::read_dir(source).map_err(io)? {
        let item = item.map_err(io)?;
        let kind = item.file_type().map_err(io)?;
        let target = destination.join(item.file_name());
        if kind.is_dir() {
            total += copy_tree(&item.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(item.path(), &target).map_err(io)?;
            let before = crate::backup::checksum(&item.path())?;
            let after = crate::backup::checksum(&target)?;
            if before != after {
                return Err(ApiError::new("directory_invalid", "复制字节校验失败"));
            }
            fs::File::open(&target)
                .map_err(io)?
                .sync_all()
                .map_err(io)?;
            total += after.1;
        } else {
            return Err(ApiError::new(
                "directory_invalid",
                "数据目录包含不支持的链接",
            ));
        }
    }
    Ok(total)
}
fn relocate_value(value: &mut Value, old: &Path, new: &Path) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                if [
                    "local_path",
                    "path",
                    "model_path",
                    "source_path",
                    "result_path",
                ]
                .contains(&key.as_str())
                {
                    if let Some(text) = item.as_str() {
                        if let Ok(relative) = Path::new(text).strip_prefix(old) {
                            *item = json!(new.join(relative));
                        }
                    }
                } else {
                    relocate_value(item, old, new);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                relocate_value(item, old, new);
            }
        }
        _ => {}
    }
}
impl Application {
    pub(super) fn redirected(&self) -> Result<Option<PathBuf>, ApiError> {
        let Some(path) = self.connection.path() else {
            return Ok(None);
        };
        let resolved = resolve(Path::new(path))?;
        Ok((resolved != Path::new(path)).then_some(resolved))
    }
    pub(super) fn directory_operation(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        self.directory_run(operation, input, None)
    }
    pub(super) fn directory_run(
        &self,
        operation: &str,
        input: Value,
        control: Option<&crate::maintenance::Control>,
    ) -> Result<Value, ApiError> {
        if let Some(control) = control {
            control.tick()?;
        }
        if operation == "data_directory.status" {
            if !input.as_object().unwrap().is_empty() {
                return Err(ApiError::new("invalid_input", "数据目录状态无输入字段"));
            }
            return Ok(
                json!({"database":self.connection.path(),"directory":self.asset_root.parent()}),
            );
        }
        if input.as_object().unwrap().keys().any(|k| k != "directory") {
            return Err(ApiError::new("invalid_input", "不支持的数据目录字段"));
        }
        let requested = PathBuf::from(string(&input, "directory")?);
        if requested.exists() {
            return Err(ApiError::new("already_exists", "新数据目录必须尚不存在"));
        }
        let parent = requested
            .parent()
            .ok_or_else(|| ApiError::new("invalid_input", "缺少目标父目录"))?;
        fs::create_dir_all(parent).map_err(io)?;
        let destination = fs::canonicalize(parent)
            .map_err(io)?
            .join(requested.file_name().unwrap());
        let source = fs::canonicalize(self.asset_root.parent().unwrap()).map_err(io)?;
        if destination.starts_with(&source) || source.starts_with(&destination) {
            return Err(ApiError::new("invalid_input", "新旧数据目录不能相互包含"));
        }
        let staging = destination.with_extension(format!("staging-{}", uuid::Uuid::now_v7()));
        fs::create_dir(&staging).map_err(io)?;
        let move_lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(source.join(".wardrobe-move.lock"))
            .map_err(io)?;
        move_lock
            .try_lock()
            .map_err(|_| ApiError::new("data_directory_moving", "已有数据目录迁移在运行"))?;
        let marker = source.join(".wardrobe-location.json");
        let mut pending = false;
        let result = (|| {
            if let Some(control) = control {
                control.enter_critical(self)?;
            }
            let tx = self.begin_write()?;
            let database = staging.join("wardrobe.sqlite");
            let target = Connection::open(&database)?;
            target.execute_batch("PRAGMA foreign_keys=OFF;BEGIN IMMEDIATE;")?;
            let tables=self.connection.prepare("SELECT name,sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<Result<Vec<_>,_>>()?;
            for (name, sql) in tables {
                target.execute_batch(&sql)?;
                let mut query = self
                    .connection
                    .prepare(&format!("SELECT * FROM \"{name}\""))?;
                let count = query.column_count();
                let placeholders = (1..=count)
                    .map(|n| format!("?{n}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let mut insert =
                    target.prepare(&format!("INSERT INTO \"{name}\" VALUES({placeholders})"))?;
                let mut rows = query.query([])?;
                while let Some(row) = rows.next()? {
                    let mut values = (0..count)
                        .map(|i| row.get::<_, rusqlite::types::Value>(i))
                        .collect::<Result<Vec<_>, _>>()?;
                    for value in &mut values {
                        if let rusqlite::types::Value::Text(raw) = value {
                            if let Ok(mut document) = serde_json::from_str::<Value>(raw) {
                                relocate_value(
                                    &mut document,
                                    self.asset_root.parent().unwrap(),
                                    &destination,
                                );
                                relocate_value(&mut document, &source, &destination);
                                *raw = document.to_string();
                            }
                        }
                    }
                    insert.execute(rusqlite::params_from_iter(values))?;
                }
            }
            let indexes = self
                .connection
                .prepare("SELECT sql FROM sqlite_master WHERE type='index' AND sql IS NOT NULL")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for sql in indexes {
                target.execute_batch(&sql)?;
            }
            target.execute_batch(&format!(
                "PRAGMA user_version={DATABASE_SCHEMA_VERSION};COMMIT;"
            ))?;
            let bytes = if self.asset_root.exists() {
                copy_tree(&self.asset_root, &staging.join("assets"))?
            } else {
                0
            };
            let integrity: String = target.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            if integrity != "ok" {
                return Err(ApiError::new("directory_invalid", "新数据库校验失败"));
            }
            {
                let mut check = target.prepare("PRAGMA foreign_key_check")?;
                if check.query([])?.next()?.is_some() {
                    return Err(ApiError::new("directory_invalid", "新数据库引用校验失败"));
                }
            }
            drop(target);
            fs::File::open(&database)
                .map_err(io)?
                .sync_all()
                .map_err(io)?;
            fs::rename(&staging, &destination).map_err(io)?;
            let newdb = destination.join("wardrobe.sqlite");
            let verified = Application::open(&newdb)?;
            if verified.default_space()? != self.default_space()? {
                return Err(ApiError::new("directory_invalid", "新数据库公开读取不一致"));
            }
            if let Some(control) = control {
                control.prepare_move_result(&verified.connection,&json!({"schema_version":1,"switched":true,"directory":destination,"old_directory":source,"copied_media_bytes":bytes}))?;
            }
            drop(verified);
            let mut record = json!({"version":1,"phase":"pending","source_file":Path::new(self.connection.path().unwrap()).file_name().and_then(|name|name.to_str()),"target_database":newdb});
            publish(&marker, &record)?;
            pending = true;
            tx.commit()?;
            record["phase"] = json!("active");
            publish(&marker, &record)?;
            pending = false;
            #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
            crate::backup::process_test_gate("directory-published");
            Ok(
                json!({"switched":true,"directory":destination,"old_directory":source,"copied_media_bytes":bytes}),
            )
        })();
        if result.is_err() {
            if pending {
                let _ = fs::remove_file(&marker);
            }
            let _ = fs::remove_dir_all(staging);
        }
        result
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    vec![
        json!({"name":"data_directory.status","examples":[{"request":{},"prerequisites":["读取当前本机数据目录位置；只读不改"]}],"outputSchema":{"type":"object","required":["schema_version"],"properties":{"schema_version":{"const":1}}},"inputSchema":{"type":"object","additionalProperties":false,"properties":{}}}),
        json!({"name":"data_directory.move","examples":[{"request":{"directory":"</绝对/本地/新数据目录>","operation_key":"<唯一操作标识>"},"prerequisites":["校验目标完整副本后切换并保留旧源；由maintenance.worker.run本地执行","本示例不自动执行"]}],"outputSchema":crate::maintenance::job_schema(),"inputSchema":{"type":"object","additionalProperties":false,"required":["directory"],"properties":{"directory":{"type":"string"},"operation_key":{"type":"string","minLength":1,"maxLength":200}}}}),
    ]
}

#[cfg(test)]
thread_local! {
    static AFTER_PENDING_READ: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_reader_preserves_concurrently_published_active_location() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        let source_db = source.join("wardrobe.sqlite");
        let target_db = target.join("wardrobe.sqlite");
        fs::write(&source_db, []).unwrap();
        fs::write(&target_db, []).unwrap();
        let marker = source.join(".wardrobe-location.json");
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(source.join(".wardrobe-move.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let record =
            json!({"source_file":"wardrobe.sqlite","phase":"pending","target_database":target_db});
        publish(&marker, &record).unwrap();
        let publish_marker = marker.clone();
        AFTER_PENDING_READ.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                let mut active = record;
                active["phase"] = json!("active");
                publish(&publish_marker, &active).unwrap();
                drop(lock);
            }))
        });
        assert_eq!(resolve(&source_db).unwrap(), target_db);
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(marker).unwrap()).unwrap()["phase"],
            "active"
        );
    }
}
