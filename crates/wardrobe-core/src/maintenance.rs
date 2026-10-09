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
fn lock_identity(path: &Path) -> Result<Value, ApiError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| ApiError::new("maintenance_lock_changed", "维护锁已消失或被替换"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ApiError::new(
            "maintenance_lock_changed",
            "维护锁须为本机普通文件",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(ApiError::new(
                "maintenance_lock_changed",
                "维护锁不能有外部硬链接",
            ));
        }
        Ok(json!([metadata.dev(), metadata.ino()]))
    }
    #[cfg(not(unix))]
    {
        Ok(json!(path.canonicalize().map_err(|_| ApiError::new(
            "maintenance_lock_changed",
            "维护锁路径无效"
        ))?))
    }
}
fn lock_path(app: &Application, doc: &Value) -> PathBuf {
    doc["_worker_lock_uri"]
        .as_str()
        .and_then(|uri| uri.strip_prefix("file://"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(app.connection.path().unwrap())
                .parent()
                .unwrap()
                .join(".maintenance-locks")
                .join(format!("{}.lock", doc["id"].as_str().unwrap()))
        })
}
fn worker_lock(app: &Application, doc: &Value) -> Result<std::fs::File, ApiError> {
    let path = lock_path(app, doc);
    let directory = path.parent().unwrap();
    std::fs::create_dir_all(directory)
        .map_err(|_| ApiError::new("storage_error", "维护执行锁目录不可写"))?;
    let metadata = std::fs::symlink_metadata(directory)
        .map_err(|_| ApiError::new("storage_error", "维护执行锁目录不可用"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ApiError::new(
            "maintenance_lock_changed",
            "维护锁目录不能指向外部位置",
        ));
    }
    if path.exists() || path.symlink_metadata().is_ok() {
        lock_identity(&path)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options
        .open(&path)
        .map_err(|_| ApiError::new("storage_error", "维护执行锁不可用"))?;
    lock.try_lock()
        .map_err(|_| ApiError::new("job_unavailable", "维护任务仍有本机执行者，不能并发接管"))?;
    let identity = lock_identity(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = lock
            .metadata()
            .map_err(|_| ApiError::new("storage_error", "维护锁不可读取"))?;
        if identity != json!([opened.dev(), opened.ino()]) {
            return Err(ApiError::new(
                "maintenance_lock_changed",
                "维护锁在打开时被替换",
            ));
        }
    }
    if !doc["_worker_identity"].is_null() && doc["_worker_identity"] != identity {
        return Err(ApiError::new(
            "maintenance_lock_changed",
            "维护锁身份改变，不能自动接管",
        ));
    }
    Ok(lock)
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
    worker_path: PathBuf,
    worker_identity: Value,
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
            worker_path: lock_path(app, doc),
            worker_identity: doc["_worker_identity"].clone(),
        }
    }
    fn checked(&self, db: &Connection) -> Result<Value, ApiError> {
        if !self.worker_identity.is_null()
            && lock_identity(&self.worker_path)? != self.worker_identity
        {
            return Err(ApiError::new(
                "maintenance_lock_changed",
                "维护锁被外部替换，执行已停止",
            ));
        }
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
        let mut doc = self.checked(db)?;
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
        let mut doc = json!({"schema_version":1,"id":id,"job_id":id,"operation":operation,"operation_key":key,"state":"queued","phase":"queued","progress":0,"attempts":0,"input":input,"result":null,"error":null,"created_at":timestamp,"updated_at":timestamp,"_generation":self.installation_generation,"_source_database":self.connection.path(),"_worker_lock_uri":format!("file://{}",Path::new(self.connection.path().unwrap()).parent().unwrap().join(".maintenance-locks").join(format!("{id}.lock")).to_string_lossy()),"_lease":null,"_lease_expires":0});
        // 链路贯穿（#44）：维护任务同样携带创建调用的 trace_id，供 worker/迁移事件还原链路
        if let Some(trace) = crate::diagnostics::current_trace() {
            doc["trace_id"] = json!(trace);
        }
        save(&tx, &doc)?;
        let pending = crate::jobs::PendingJobEvent::enqueue(&doc);
        tx.commit()?;
        self.emit_job_event(Some(pending));
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
        let state_before = doc["state"].as_str().map(str::to_owned);
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
                let _retry_lock = worker_lock(self, &doc)?;
                // A successfully acquired OS lock proves no worker still owns this
                // receipt; explicit recovery need not wait five minutes after a kill.
                let expired = doc["state"] == "running";
                if !expired && doc["state"] != "failed" && doc["state"] != "cancelled" {
                    return Err(ApiError::new(
                        "job_unavailable",
                        "只能明确重试失败、取消或已失去执行者的维护任务",
                    ));
                }
                let published = doc["operation"] == "backup.create" && doc["phase"] == "committing";
                doc["state"] = json!("queued");
                doc["phase"] = json!(if published { "committing" } else { "queued" });
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
        // 取消/重试的迁移留痕（#44）；状态未变（幂等返回路径已提前返回）不产生噪音。
        // 事件在 commit 成功后发出：commit 失败不残留未发生的事件。
        let pending = (state_before.as_deref() != doc["state"].as_str())
            .then(|| crate::jobs::PendingJobEvent::state(&doc, state_before.as_deref()));
        tx.commit()?;
        self.emit_job_event(pending);
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
        let _worker_lock = worker_lock(self, &doc)?;
        doc["_worker_identity"] = lock_identity(&lock_path(self, &doc))?;
        // The OS lock, retained for the entire worker, is authoritative locally.
        let expired = doc["state"] == "running";
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
        let published = doc["operation"] == "backup.create" && doc["phase"] == "committing";
        let state_before = doc["state"].as_str().map(str::to_owned);
        doc["state"] = json!("running");
        doc["phase"] = json!(if published { "committing" } else { "working" });
        doc["_lease"] = json!(uuid::Uuid::now_v7().to_string());
        doc["_lease_expires"] = json!(now() + 300_000);
        doc["attempts"] = json!(doc["attempts"].as_u64().unwrap_or(0) + 1);
        save(&tx, &doc)?;
        // 领取或租约接管（expired 时 state_from=running）都以文档 trace 留痕（#44）；
        // 事件在 commit 成功后发出：commit 失败不残留未发生的事件
        let pending = crate::jobs::PendingJobEvent::state(&doc, state_before.as_deref());
        tx.commit()?;
        self.emit_job_event(Some(pending));
        let control = Control::new(self, &doc);
        let operation = doc["operation"].as_str().unwrap();
        let result = if operation == "data_directory.move" {
            self.directory_run(operation, doc["input"].clone(), Some(&control))
        } else if operation == "backup.create" {
            self.recover_created_backup(&doc["input"], id, Some(&control))
                .and_then(|published| match published {
                    Some(result) => Ok(result),
                    None => self.backup_run(operation, doc["input"].clone(), Some(&control)),
                })
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
        let pending = crate::jobs::PendingJobEvent::state(&finished, Some("running"));
        save(&tx, &finished)?;
        tx.commit()?;
        self.emit_job_event(Some(pending));
        Ok(public(finished))
    }
}
pub(super) fn job_schema() -> Value {
    json!({"type":"object","required":["schema_version","id","job_id","operation","state","phase","progress","result","error"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"job_id":{"type":"string"},"operation":{"type":"string"},"operation_key":{"type":"string"},"state":{"enum":["queued","running","succeeded","failed","cancelled"]},"phase":{"type":"string"},"progress":{"type":"integer","minimum":0,"maximum":100},"attempts":{"type":"integer","minimum":0},"next_due":{"type":"integer"},"lease_expires":{"type":"integer"},"attempts_left":{"type":"integer"},"created_at":{"type":"string"},"updated_at":{"type":"string"},"input":{"type":"object"},"payload":{"type":"object"},"submitted_at":{"type":["string","null"]},"last_outcome":{"type":["string","null"]},"directory":{"type":"string"},"include_originals":{"type":"boolean"},"include_database":{"type":"boolean"},"trace_id":{"type":"string","description":"创建该任务的应用调用链路标识"},"result":{},"error":{}}})
}
pub(super) fn descriptors() -> Vec<Value> {
    ["get","list","cancel","retry","worker.run"].into_iter().map(|action|{
        let list=action=="list";
        let properties=if list{json!({"limit":{"type":"integer","minimum":1,"maximum":100},"cursor":{"type":"string"}})}else{json!({"id":{"type":"string"}})};
        let (effect,examples)=match action {
            "get"=>(json!({"kind":"query","network":"none","local_files":false,"async":null}),json!([{"request":{"id":"<维护任务ID>"},"prerequisites":["读取全空间维护任务文档"]}])),"list"=>(json!({"kind":"query","network":"none","local_files":false,"async":null}),json!([{"request":{},"prerequisites":["分页列出维护任务；游标字符串"]}])),"cancel"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"id":"<维护任务ID>"},"prerequisites":["取消排队或失败任务；已完成任务不能取消"]}])),_=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null}),json!([{"request":{"id":"<维护任务ID>"},"prerequisites":["retry回到队列；worker.run同步执行备份/恢复等维护任务并写本地文件","本示例不自动执行"]}]))
        };
        json!({"name":format!("maintenance.{action}"),"effect":effect,"examples":examples,"description":"全空间本机维护任务状态与明确执行控制","inputSchema":{"type":"object","additionalProperties":false,"required":if list{vec![]}else{vec!["id"]},"properties":properties},"outputSchema":if list{json!({"type":"object","required":["schema_version","items","next_cursor"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":job_schema()},"next_cursor":{"type":["string","null"]}}})}else{job_schema()}})
    }).collect()
}

#[cfg(test)]
mod lease_tests {
    use super::*;
    #[test]
    fn published_backup_recovers_its_durable_receipt_without_creating_another_package() {
        let root = tempfile::tempdir().unwrap();
        let app = Application::open(&root.path().join("wardrobe.sqlite")).unwrap();
        let directory = root.path().join("backup");
        let queued = app
            .execute("backup.create", json!({"directory":directory}))
            .unwrap();
        let id = queued["id"].as_str().unwrap();
        let completed = app
            .execute("maintenance.worker.run", json!({"id":id}))
            .unwrap();
        assert_eq!(completed["state"], "succeeded");
        // Deterministic receipt rollback at the post-rename/pre-SQL-commit seam.
        let mut doc = load(&app.connection, id).unwrap();
        doc["state"] = json!("running");
        doc["phase"] = json!("committing");
        doc["result"] = Value::Null;
        doc["_completed_lease"] = Value::Null;
        doc["_lease"] = json!("vanished-worker");
        doc["_lease_expires"] = json!(0);
        save(&app.connection, &doc).unwrap();
        let recovered = app
            .execute("maintenance.worker.run", json!({"id":id}))
            .unwrap();
        assert_eq!(recovered["state"], "succeeded");
        assert_eq!(recovered["result"]["directory"], json!(directory));
        assert_eq!(
            std::fs::read_dir(root.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("backup"))
                .count(),
            1
        );
    }
    #[cfg(unix)]
    #[test]
    fn replacing_a_live_lock_inode_cannot_start_a_second_worker_or_publish_first_worker() {
        let root = tempfile::tempdir().unwrap();
        let app = Application::open(&root.path().join("wardrobe.sqlite")).unwrap();
        let queued = app
            .execute(
                "backup.create",
                json!({"directory":root.path().join("backup")}),
            )
            .unwrap();
        let id = queued["id"].as_str().unwrap();
        let mut doc = load(&app.connection, id).unwrap();
        let held = worker_lock(&app, &doc).unwrap();
        let path = lock_path(&app, &doc);
        let identity = lock_identity(&path).unwrap();
        doc["state"] = json!("running");
        doc["_lease"] = json!("original-worker");
        doc["_lease_expires"] = json!(0);
        doc["_worker_identity"] = identity.clone();
        save(&app.connection, &doc).unwrap();
        let control = Control::new(&app, &doc);
        // Real external replacement while the original open file still owns its lock.
        std::fs::rename(&path, path.with_extension("held-original")).unwrap();
        std::fs::write(&path, b"replacement inode").unwrap();
        assert_ne!(lock_identity(&path).unwrap(), identity);
        assert_eq!(
            app.execute("maintenance.worker.run", json!({"id":id}))
                .unwrap_err()
                .code,
            "maintenance_lock_changed"
        );
        assert_eq!(
            control.enter_critical(&app).unwrap_err().code,
            "maintenance_lock_changed"
        );
        assert!(!root.path().join("backup").exists());
        drop(held);
    }
    #[cfg(unix)]
    #[test]
    fn maintenance_worker_rejects_an_external_symbolic_lock_target() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
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
        let foreign = external.path().join("unrelated");
        std::fs::write(&foreign, b"foreign data").unwrap();
        std::os::unix::fs::symlink(&foreign, locks.join(format!("{id}.lock"))).unwrap();
        assert!(app
            .execute("maintenance.worker.run", json!({"id":id}))
            .is_err());
        assert_eq!(std::fs::read(foreign).unwrap(), b"foreign data");
        assert!(!root.path().join("backup").exists());
    }
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
