//! Installation-local, space-wide maintenance receipts survive portable restore.
use crate::{string, ApiError, Application};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub(super) fn migrate(tx: &rusqlite::Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS maintenance_jobs(id TEXT PRIMARY KEY,operation TEXT NOT NULL,operation_key TEXT NOT NULL,document TEXT NOT NULL,UNIQUE(operation,operation_key));")?;
    Ok(())
}
pub(super) fn is_global_operation(operation: &str) -> bool {
    operation.starts_with("backup.")
        || operation.starts_with("data_directory.")
        || operation.starts_with("maintenance.")
}
fn parse(raw: String) -> Result<Value, ApiError> {
    serde_json::from_str(&raw).map_err(|_| ApiError::new("storage_error", "维护任务记录损坏"))
}
fn load(db: &Connection, id: &str) -> Result<Value, ApiError> {
    parse(
        db.query_row(
            "SELECT document FROM maintenance_jobs WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| ApiError::new("not_found", "维护任务不存在"))?,
    )
}
fn save(db: &Connection, doc: &Value) -> Result<(), ApiError> {
    db.execute("INSERT INTO maintenance_jobs VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET document=excluded.document",rusqlite::params![doc["id"].as_str(),doc["operation"].as_str(),doc["operation_key"].as_str(),doc.to_string()])?;
    Ok(())
}
fn public(mut doc: Value) -> Value {
    if let Some(map) = doc.as_object_mut() {
        map.retain(|key, _| !key.starts_with('_'));
    }
    doc
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
fn input_fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .is_none_or(|map| map.keys().any(|key| !allowed.contains(&key.as_str())))
    {
        return Err(ApiError::new(
            "invalid_input",
            "维护任务不接受成员或额外字段",
        ));
    }
    Ok(())
}
pub(super) struct Control {
    database: PathBuf,
    id: String,
    lease: String,
    operation: String,
    last: Cell<Instant>,
    critical: Cell<bool>,
}
#[cfg(test)]
thread_local! { static BEFORE_LEASE_SAVE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = std::cell::RefCell::new(None); }
#[cfg(test)]
fn before_lease_save() {
    BEFORE_LEASE_SAVE.with(|hook| {
        let callback = hook.borrow_mut().take();
        if let Some(callback) = callback {
            callback();
        }
    });
}
impl Control {
    fn new(app: &Application, doc: &Value) -> Self {
        Self {
            database: PathBuf::from(app.connection.path().unwrap()),
            id: doc["id"].as_str().unwrap().into(),
            lease: doc["_lease"].as_str().unwrap().into(),
            operation: doc["operation"].as_str().unwrap().into(),
            last: Cell::new(Instant::now() - Duration::from_secs(2)),
            critical: Cell::new(false),
        }
    }
    fn checked(&self, db: &Connection) -> Result<Value, ApiError> {
        let doc = load(db, &self.id)?;
        if doc["state"] == "cancelled" {
            return Err(ApiError::new("job_cancelled", "维护任务已取消"));
        }
        if doc["state"] != "running"
            || doc["_lease"] != self.lease
            || (!self.critical.get() && doc["_lease_expires"].as_i64().unwrap_or(0) < now())
        {
            return Err(ApiError::new("lease_lost", "维护任务执行权已失效"));
        }
        Ok(doc)
    }
    pub(super) fn tick(&self) -> Result<(), ApiError> {
        if self.critical.get() || self.last.get().elapsed() < Duration::from_millis(250) {
            return Ok(());
        }
        let path = crate::data_directory::resolve(&self.database)?;
        let mut db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        #[cfg(test)]
        before_lease_save();
        // State validation and renewal share the writer lock: cancellation cannot
        // commit between our read and write and then be overwritten.
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut doc = self.checked(&tx)?;
        if doc["_lease_expires"].as_i64().unwrap_or(0) - now() < 240_000 {
            doc["_lease_expires"] = json!(now() + 300_000);
            save(&tx, &doc)?;
        }
        tx.commit()?;
        self.last.set(Instant::now());
        Ok(())
    }
    pub(super) fn enter_critical(&self, app: &Application) -> Result<(), ApiError> {
        if self.critical.get() {
            return Ok(());
        }
        let tx = app.begin_write()?;
        let mut doc = self.checked(&tx)?;
        doc["phase"] = json!("committing");
        doc["_lease_expires"] = json!(now() + 300_000);
        save(&tx, &doc)?;
        tx.commit()?;
        self.critical.set(true);
        Ok(())
    }
    pub(super) fn complete(
        &self,
        db: &Connection,
        operation: &str,
        result: &Value,
    ) -> Result<(), ApiError> {
        if self.operation != operation {
            return Ok(());
        }
        let mut doc = self.checked(db)?;
        doc["state"] = json!("succeeded");
        doc["phase"] = json!("completed");
        doc["progress"] = json!(100);
        doc["result"] = result.clone();
        doc["_completed_lease"] = json!(self.lease);
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        save(db, &doc)
    }
    pub(super) fn owns(&self, operation: &str) -> bool {
        self.operation == operation
    }
    pub(super) fn publication_id(&self) -> &str {
        &self.id
    }
    pub(super) fn prepare_move_result(
        &self,
        db: &Connection,
        result: &Value,
    ) -> Result<(), ApiError> {
        let mut doc = load(db, &self.id)?;
        doc["_pending_result"] = result.clone();
        save(db, &doc)
    }
}
impl Application {
    pub(super) fn maintenance_enqueue(
        &self,
        operation: &str,
        mut input: Value,
    ) -> Result<Value, ApiError> {
        let fields = match operation {
            "backup.create" => vec!["directory", "include_originals", "operation_key"],
            "backup.preview" | "data_directory.move" => vec!["directory", "operation_key"],
            "backup.restore" => vec!["directory", "confirm_overwrite", "operation_key"],
            _ => return Err(ApiError::new("unsupported_operation", "不支持的维护任务")),
        };
        input_fields(&input, &fields)?;
        let directory = string(&input, "directory")?;
        if !Path::new(directory).is_absolute() || directory.len() > 4096 {
            return Err(ApiError::new("invalid_input", "维护目录需要有效绝对路径"));
        }
        if input
            .get("include_originals")
            .is_some_and(|value| !value.is_boolean())
            || input
                .get("confirm_overwrite")
                .is_some_and(|value| !value.is_boolean())
        {
            return Err(ApiError::new("invalid_input", "维护选项格式无效"));
        }
        if operation == "backup.restore" && input["confirm_overwrite"] != true {
            return Err(ApiError::new(
                "confirmation_required",
                "恢复会覆盖当前事实，请明确确认",
            ));
        }
        let key = input
            .get("operation_key")
            .map(|value| {
                value
                    .as_str()
                    .filter(|key| !key.trim().is_empty() && key.len() <= 200)
                    .ok_or_else(|| ApiError::new("invalid_input", "维护操作标识无效"))
            })
            .transpose()?
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        input.as_object_mut().unwrap().remove("operation_key");
        let tx = self.begin_write()?;
        if let Some(raw) = tx
            .query_row(
                "SELECT document FROM maintenance_jobs WHERE operation=?1 AND operation_key=?2",
                rusqlite::params![operation, key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            let doc = parse(raw)?;
            if doc["input"] != input {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "同一维护意图不能改变输入",
                ));
            }
            return Ok(public(doc));
        }
        let id = uuid::Uuid::now_v7().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let doc = json!({"schema_version":1,"id":id,"job_id":id,"operation":operation,"operation_key":key,"state":"queued","phase":"queued","progress":0,"attempts":0,"input":input,"result":null,"error":null,"created_at":timestamp,"updated_at":timestamp,"_generation":self.installation_generation,"_source_database":self.connection.path(),"_lease":null,"_lease_expires":0});
        save(&tx, &doc)?;
        tx.commit()?;
        Ok(public(doc))
    }
    pub(super) fn maintenance_raw_get(&self, id: &str) -> Result<Value, ApiError> {
        load(&self.connection, id)
    }
    fn recover_published_move(&self, db: &Connection, doc: &mut Value) -> Result<(), ApiError> {
        if doc["operation"] != "data_directory.move"
            || doc["state"] != "running"
            || doc["_pending_result"].is_null()
        {
            return Ok(());
        }
        let Some(old) = doc["_pending_result"]["old_directory"].as_str() else {
            return Ok(());
        };
        let marker = Path::new(old).join(".wardrobe-location.json");
        if !marker
            .metadata()
            .is_ok_and(|meta| meta.is_file() && meta.len() <= 16_384)
        {
            return Ok(());
        }
        let record: Value = std::fs::read(&marker)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null);
        let current = Path::new(self.connection.path().unwrap());
        let published = record["phase"] == "active"
            && record["target_database"].as_str().is_some_and(|path| {
                Path::new(path).canonicalize().ok() == current.canonicalize().ok()
            });
        if !published {
            return Ok(());
        }
        doc["state"] = json!("succeeded");
        doc["phase"] = json!("completed");
        doc["progress"] = json!(100);
        doc["result"] = doc["_pending_result"].clone();
        doc["error"] = Value::Null;
        doc["_completed_lease"] = doc["_lease"].clone();
        doc["_lease"] = Value::Null;
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        save(db, doc)
    }
    pub(super) fn maintenance_operation(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        if operation == "maintenance.list" {
            input_fields(&input, &["limit", "cursor"])?;
            let limit = input
                .get("limit")
                .map(|value| {
                    value
                        .as_u64()
                        .filter(|n| (1..=100).contains(n))
                        .ok_or_else(|| ApiError::new("invalid_input", "分页数量须为1–100"))
                })
                .transpose()?
                .unwrap_or(20);
            let cursor = input
                .get("cursor")
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| ApiError::new("invalid_input", "分页游标无效"))
                })
                .transpose()?
                .unwrap_or("~");
            let mut statement = self.connection.prepare(
                "SELECT document FROM maintenance_jobs WHERE id<?1 ORDER BY id DESC LIMIT ?2",
            )?;
            let docs = statement
                .query_map(rusqlite::params![cursor, limit + 1], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let more = docs.len() > limit as usize;
            let mut items = docs
                .into_iter()
                .take(limit as usize)
                .map(parse)
                .collect::<Result<Vec<_>, _>>()?;
            let next = if more {
                items
                    .last()
                    .map(|doc| doc["id"].clone())
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            for doc in &mut items {
                *doc = public(doc.take());
                doc["result"] = Value::Null;
            }
            return Ok(json!({"schema_version":1,"items":items,"next_cursor":next}));
        }
        input_fields(&input, &["id"])?;
        let id = string(&input, "id")?;
        if operation == "maintenance.get" {
            let tx = self.begin_write()?;
            let mut doc = load(&tx, id)?;
            self.recover_published_move(&tx, &mut doc)?;
            tx.commit()?;
            return Ok(public(doc));
        }
        if operation == "maintenance.worker.run" {
            return self.maintenance_run(id);
        }
        let tx = self.begin_write()?;
        let mut doc = load(&tx, id)?;
        self.recover_published_move(&tx, &mut doc)?;
        match operation {
            "maintenance.cancel" => {
                if doc["state"] == "cancelled" {
                    return Ok(public(doc));
                }
                if doc["phase"] == "committing" {
                    return Err(ApiError::new(
                        "maintenance_commit_started",
                        "已进入原子提交阶段，无法取消",
                    ));
                }
                if !["queued", "running"]
                    .iter()
                    .any(|state| doc["state"] == *state)
                {
                    return Err(ApiError::new(
                        "job_unavailable",
                        "维护任务已结束，不能取消已提交事实",
                    ));
                }
                doc["state"] = json!("cancelled");
                doc["phase"] = json!("cancelled");
                doc["_lease"] = Value::Null;
            }
            "maintenance.retry" => {
                let expired = doc["state"] == "running"
                    && doc["_lease_expires"].as_i64().unwrap_or(0) <= now();
                if !expired && doc["state"] != "failed" && doc["state"] != "cancelled" {
                    return Err(ApiError::new(
                        "job_unavailable",
                        "只能明确重试失败、取消或已失去执行者的维护任务",
                    ));
                }
                doc["state"] = json!("queued");
                doc["phase"] = json!("queued");
                doc["error"] = Value::Null;
                doc["_lease"] = Value::Null;
                doc["_lease_expires"] = json!(0);
                doc["_generation"] = json!(self.installation_generation);
            }
            _ => {
                return Err(ApiError::new(
                    "unsupported_operation",
                    "不支持的维护任务操作",
                ))
            }
        };
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        save(&tx, &doc)?;
        tx.commit()?;
        Ok(public(doc))
    }
    fn maintenance_run(&self, id: &str) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let mut doc = load(&tx, id)?;
        self.recover_published_move(&tx, &mut doc)?;
        if doc["state"] == "succeeded" {
            tx.commit()?;
            return Ok(public(doc));
        }
        let expired =
            doc["state"] == "running" && doc["_lease_expires"].as_i64().unwrap_or(0) <= now();
        if doc["state"] != "queued" && !expired {
            return Err(ApiError::new(
                "job_unavailable",
                "维护任务已取消、结束或正在执行",
            ));
        }
        if doc["_generation"] != self.installation_generation {
            return Err(ApiError::new(
                "data_restored",
                "维护意图早于恢复，请明确重新确认",
            ));
        }
        doc["state"] = json!("running");
        doc["phase"] = json!("working");
        doc["_lease"] = json!(uuid::Uuid::now_v7().to_string());
        doc["_lease_expires"] = json!(now() + 300_000);
        doc["attempts"] = json!(doc["attempts"].as_u64().unwrap_or(0) + 1);
        save(&tx, &doc)?;
        tx.commit()?;
        let control = Control::new(self, &doc);
        let operation = doc["operation"].as_str().unwrap();
        let result = if operation == "data_directory.move" {
            self.directory_run(operation, doc["input"].clone(), Some(&control))
        } else {
            self.backup_run(operation, doc["input"].clone(), Some(&control))
        };
        let current = Application::open(Path::new(self.connection.path().unwrap()))?;
        let tx = current.begin_write()?;
        let mut finished = load(&tx, id)?;
        if finished["state"] == "cancelled" {
            return Ok(public(finished));
        }
        if finished["_lease"] != control.lease && finished["_completed_lease"] != control.lease {
            return Err(ApiError::new("lease_lost", "维护执行者不能覆盖新任务状态"));
        }
        match result {
            Ok(result) => {
                finished["state"] = json!("succeeded");
                finished["phase"] = json!("completed");
                finished["progress"] = json!(100);
                finished["result"] = result;
                finished["error"] = Value::Null;
            }
            Err(error) => {
                finished["state"] = json!("failed");
                finished["phase"] = json!("failed");
                finished["error"] = json!({"code":error.code,"message":error.message});
            }
        }
        finished["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        finished["_lease"] = Value::Null;
        save(&tx, &finished)?;
        tx.commit()?;
        Ok(public(finished))
    }
}
pub(super) fn job_schema() -> Value {
    json!({"type":"object","required":["schema_version","id","job_id","operation","state","phase","progress","result","error"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"job_id":{"type":"string"},"operation":{"type":"string"},"state":{"enum":["queued","running","succeeded","failed","cancelled"]},"phase":{"type":"string"},"progress":{"type":"integer","minimum":0,"maximum":100},"result":{},"error":{}}})
}
pub(super) fn descriptors() -> Vec<Value> {
    ["get","list","cancel","retry","worker.run"].into_iter().map(|action|{
        let list=action=="list";
        let properties=if list{json!({"limit":{"type":"integer","minimum":1,"maximum":100},"cursor":{"type":"string"}})}else{json!({"id":{"type":"string"}})};
        json!({"name":format!("maintenance.{action}"),"description":"全空间本机维护任务状态与明确执行控制","inputSchema":{"type":"object","additionalProperties":false,"required":if list{vec![]}else{vec!["id"]},"properties":properties},"outputSchema":if list{json!({"type":"object","required":["schema_version","items","next_cursor"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":job_schema()},"next_cursor":{"type":["string","null"]}}})}else{job_schema()}})
    }).collect()
}

#[cfg(test)]
mod lease_tests {
    use super::*;
    #[test]
    fn a_live_worker_lock_prevents_a_second_worker_from_claiming_an_expired_receipt() {
        let root = tempfile::tempdir().unwrap();
        let app = Application::open(&root.path().join("wardrobe.sqlite")).unwrap();
        let queued = app
            .execute(
                "backup.create",
                json!({"directory":root.path().join("backup")}),
            )
            .unwrap();
        let id = queued["id"].as_str().unwrap();
        let locks = root.path().join(".maintenance-locks");
        std::fs::create_dir_all(&locks).unwrap();
        let lock = std::fs::File::create(locks.join(format!("{id}.lock"))).unwrap();
        lock.try_lock().unwrap();
        let mut doc = load(&app.connection, id).unwrap();
        doc["state"] = json!("running");
        doc["_lease"] = json!("still-alive");
        doc["_lease_expires"] = json!(0);
        save(&app.connection, &doc).unwrap();
        assert_eq!(
            app.execute("maintenance.worker.run", json!({"id":id}))
                .unwrap_err()
                .code,
            "job_unavailable"
        );
        assert!(!root.path().join("backup").exists());
    }
    #[test]
    fn published_directory_switch_recovers_its_job_receipt_after_worker_disappears() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source/wardrobe.sqlite");
        let destination = root.path().join("moved");
        let app = Application::open(&source).unwrap();
        let submitted = app
            .execute("data_directory.move", json!({"directory":destination}))
            .unwrap();
        let mut doc = load(&app.connection, submitted["id"].as_str().unwrap()).unwrap();
        doc["state"] = json!("running");
        doc["_lease"] = json!("interrupted-worker");
        doc["_lease_expires"] = json!(now() + 300_000);
        save(&app.connection, &doc).unwrap();
        let control = Control::new(&app, &doc);
        // Deterministic worker boundary: real copy and marker publication complete,
        // but the process does not execute maintenance_run's final receipt write.
        app.directory_run("data_directory.move", doc["input"].clone(), Some(&control))
            .unwrap();
        drop(app);
        let app = Application::open(&source).unwrap();
        let recovered = app
            .execute("maintenance.get", json!({"id":submitted["id"]}))
            .unwrap();
        assert_eq!(recovered["state"], "succeeded");
        assert_eq!(recovered["result"]["switched"], true);
        assert!(app
            .execute("maintenance.retry", json!({"id":submitted["id"]}))
            .is_err());
    }
    #[test]
    fn cancellation_committed_at_lease_refresh_boundary_cannot_be_resurrected() {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join("wardrobe.sqlite");
        let app = Application::open(&database).unwrap();
        let submitted = app
            .execute(
                "backup.create",
                json!({"directory":root.path().join("backup")}),
            )
            .unwrap();
        let id = submitted["id"].as_str().unwrap().to_owned();
        let mut doc = load(&app.connection, &id).unwrap();
        doc["state"] = json!("running");
        doc["_lease"] = json!("fixture-lease");
        doc["_lease_expires"] = json!(now() + 100_000);
        save(&app.connection, &doc).unwrap();
        let control = Control::new(&app, &doc);
        let cancel_id = id.clone();
        BEFORE_LEASE_SAVE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                let other = Application::open(&database).unwrap();
                other
                    .execute("maintenance.cancel", json!({"id":cancel_id}))
                    .unwrap();
            }))
        });
        let result = control.tick();
        assert!(
            result.is_err(),
            "committed cancellation must stop lease renewal"
        );
        assert_eq!(
            app.execute("maintenance.get", json!({"id":id})).unwrap()["state"],
            "cancelled"
        );
    }
}
