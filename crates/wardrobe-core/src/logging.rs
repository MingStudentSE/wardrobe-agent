//! 全链路日志底座：数据目录 logs/ 下的 JSONL 追加文件，独立于 SQLite。
//!
//! 设计要点（ADR0039）：
//! - 按天分文件（`wardrobe-YYYY-MM-DD.jsonl`），单文件超限轮转为
//!   `wardrobe-<日>T<时刻>.jsonl`，启动与写入时惰性清理（保留天数 + 总量封顶）；
//! - 每次应用启动经 `begin_run()` 记为一个运行会话（run_id），事件携带
//!   当前 run_id；每次 Application API 调用分配 trace_id 并贯穿其派生任务；
//! - 脱敏按构造：事件只携带显式挑选的字段，detail 再经 denylist 兜底清除；
//! - 写入尽力而为：失败静默计数（diagnostics.summary 暴露），绝不阻塞业务。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// 日志与诊断的打开配置；产品入口使用默认值，测试注入极小值驱动轮转/清理。
#[derive(Clone, Debug)]
pub struct DiagnosticsConfig {
    pub max_log_file_bytes: u64,
    pub max_log_total_bytes: u64,
    pub log_retain_days: u64,
    pub reproduction_max_seconds: u64,
}
impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            max_log_file_bytes: 10 * 1024 * 1024,
            max_log_total_bytes: 256 * 1024 * 1024,
            log_retain_days: 14,
            reproduction_max_seconds: 600,
        }
    }
}

/// 级别序：trace(0) < debug(1) < info(2) < warn(3) < error(4)。
/// 默认 info 级（info 及以上落盘）；WARDROBE_LOG_LEVEL=debug|trace 提升详尽度。
pub(crate) fn level_number(level: &str) -> u8 {
    match level {
        "trace" => 0,
        "debug" => 1,
        "info" => 2,
        "warn" => 3,
        _ => 4,
    }
}
fn env_level_threshold() -> u8 {
    static THRESHOLD: OnceLock<u8> = OnceLock::new();
    *THRESHOLD.get_or_init(|| match std::env::var("WARDROBE_LOG_LEVEL").as_deref() {
        Ok("debug") => 1,
        Ok("trace") => 0,
        _ => 2,
    })
}
fn level_enabled(level: &str) -> bool {
    level_number(level) >= env_level_threshold()
}

static CURRENT_RUN: OnceLock<RwLock<String>> = OnceLock::new();
/// 标记一次新的应用启动（run_id）。入口进程启动时调用一次。
pub fn begin_run() -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let lock = CURRENT_RUN.get_or_init(|| RwLock::new(id.clone()));
    if let Ok(mut guard) = lock.write() {
        *guard = id.clone();
    }
    id
}
pub fn current_run() -> String {
    let lock = CURRENT_RUN.get_or_init(|| RwLock::new(uuid::Uuid::now_v7().to_string()));
    lock.read().map(|guard| guard.clone()).unwrap_or_default()
}

thread_local! {
    static TRACE_STACK: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}
/// RAII：把当前链路标识压栈，作用域结束自动恢复。
pub struct TraceScope {
    previous: Option<String>,
}
impl TraceScope {
    pub fn new(trace_id: String) -> Self {
        let previous = current_trace();
        TRACE_STACK.with(|stack| stack.borrow_mut().push(trace_id));
        Self { previous }
    }
}
impl Drop for TraceScope {
    fn drop(&mut self) {
        TRACE_STACK.with(|stack| {
            stack.borrow_mut().pop();
            if let Some(previous) = self.previous.take() {
                stack.borrow_mut().push(previous);
            }
        });
    }
}
pub fn current_trace() -> Option<String> {
    TRACE_STACK.with(|stack| stack.borrow().last().cloned())
}
/// 把当前链路切换为指定 trace（任务 claim 后链路延续用）；外层 TraceScope 结束时恢复。
pub fn continue_trace(trace_id: String) {
    TRACE_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        if let Some(top) = stack.last_mut() {
            *top = trace_id;
        } else {
            stack.push(trace_id);
        }
    });
}

const SENSITIVE_KEYS: [&str; 15] = [
    "api_key",
    "apikey",
    "token",
    "access_token",
    "refresh_token",
    "secret",
    "password",
    "passphrase",
    "authorization",
    "recovery",
    "space_key",
    "credential",
    "cookie",
    "key",
    "hash",
];
/// 兜底脱敏：递归移除疑似敏感键（按构造本就不应出现，此处保证构造即安全）。
pub fn scrub(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| {
                    let lowered = key.to_ascii_lowercase();
                    !SENSITIVE_KEYS.contains(&lowered.as_str())
                })
                .map(|(key, value)| (key, scrub(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(scrub).collect()),
        other => other,
    }
}

struct LoggerState {
    file: Option<File>,
    day: String,
    size: u64,
    last_sweep: std::time::Instant,
}
pub struct Logger {
    dir: PathBuf,
    state: Mutex<LoggerState>,
    config: Mutex<DiagnosticsConfig>,
    dropped: AtomicU64,
}
impl Logger {
    pub(crate) fn new(dir: PathBuf, config: DiagnosticsConfig) -> Self {
        Self {
            dir,
            state: Mutex::new(LoggerState {
                file: None,
                day: String::new(),
                size: 0,
                last_sweep: std::time::Instant::now() - std::time::Duration::from_secs(3600),
            }),
            config: Mutex::new(config),
            dropped: AtomicU64::new(0),
        }
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub fn set_config(&self, config: DiagnosticsConfig) {
        if let Ok(mut guard) = self.config.lock() {
            *guard = config;
        }
    }
    pub fn config(&self) -> DiagnosticsConfig {
        self.config
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// 结构化事件入口：ts/run_id 由这里统一补齐，detail 兜底脱敏。
    pub fn emit(
        &self,
        level: &str,
        kind: &str,
        op: &str,
        trace_id: Option<&str>,
        result: &str,
        duration_ms: Option<u128>,
        detail: Value,
    ) {
        if !level_enabled(level) {
            return;
        }
        let mut event = json!({
            "ts": chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "level": level,
            "kind": kind,
            "run_id": current_run(),
            "op": op,
            "result": result,
        });
        if let Some(trace_id) = trace_id {
            event["trace_id"] = json!(trace_id);
        }
        if let Some(duration_ms) = duration_ms {
            event["duration_ms"] = json!(duration_ms as u64);
        }
        if let Some(object) = detail.as_object().filter(|object| !object.is_empty()) {
            event["detail"] = scrub(Value::Object(object.clone()));
        }
        self.write_line(&event.to_string());
    }

    /// 原始事件行写入（ui_event 等已构造好的事件）；仍统一补 ts/run_id 之外的字段不改动。
    pub fn write_event(&self, mut event: Value) {
        if event.get("ts").is_none() {
            event["ts"] = json!(chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
        if event.get("run_id").is_none() {
            event["run_id"] = json!(current_run());
        }
        self.write_line(&event.to_string());
    }

    fn write_line(&self, line: &str) {
        let Ok(mut state) = self.state.lock() else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let config = self.config();
        if state.last_sweep.elapsed() > std::time::Duration::from_secs(30) {
            state.last_sweep = std::time::Instant::now();
            let active = format!("wardrobe-{}.jsonl", state.day);
            let _ = self.sweep_with_active(&config, &active);
        }
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        if state.file.is_none() || state.day != day {
            if let Err(error) = self.open_day_file(&mut state, &day) {
                let _ = error;
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        let bytes = line.as_bytes();
        let rotating = state.size + bytes.len() as u64 > config.max_log_file_bytes;
        if rotating {
            let stamp = chrono::Local::now().format("%Y-%m-%dT%H%M%S%.3f").to_string();
            let archived = self.dir.join(format!("wardrobe-{stamp}.jsonl"));
            state.file = None;
            if let Some(current_path) = self.current_day_path(&state.day) {
                let _ = std::fs::rename(&current_path, &archived);
            }
            if self.open_day_file(&mut state, &day).is_err() {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        if let Some(file) = state.file.as_mut() {
            let payload = file.write_all(bytes);
            let newline = file.write_all(b"\n");
            if payload.is_ok() && newline.is_ok() {
                state.size += bytes.len() as u64 + 1;
            } else {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn current_day_path(&self, day: &str) -> Option<PathBuf> {
        Some(self.dir.join(format!("wardrobe-{day}.jsonl")))
    }

    fn open_day_file(&self, state: &mut LoggerState, day: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("wardrobe-{day}.jsonl"));
        let existed = path.exists();
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        state.size = if existed {
            file.metadata().map(|meta| meta.len()).unwrap_or(0)
        } else {
            0
        };
        state.day = day.to_string();
        state.file = Some(file);
        Ok(())
    }

    /// 保留天数 + 总量封顶的惰性清理；返回删除的文件数。
    pub fn sweep(&self, config: &DiagnosticsConfig) -> std::io::Result<usize> {
        let active = self
            .state
            .lock()
            .map(|state| format!("wardrobe-{}.jsonl", state.day))
            .unwrap_or_default();
        self.sweep_with_active(config, &active)
    }

    /// 已持有 state 锁的调用方使用；active 为当前活跃文件名。
    fn sweep_with_active(&self, config: &DiagnosticsConfig, active: &str) -> std::io::Result<usize> {
        std::fs::create_dir_all(&self.dir)?;
        let today = chrono::Local::now().date_naive();
        let mut entries: Vec<(String, PathBuf, u64)> = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("wardrobe-") || !name.contains(".jsonl") {
                continue;
            }
            let day_part = name
                .trim_start_matches("wardrobe-")
                .get(..10)
                .unwrap_or("")
                .to_string();
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            entries.push((day_part, entry.path(), size));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.file_name().cmp(&b.1.file_name())));
        let is_active = |path: &Path| -> bool {
            path.file_name().map(|name| name.to_string_lossy() == active).unwrap_or(false)
        };
        let mut removed = 0;
        let mut total: u64 = entries.iter().map(|(_, _, size)| size).sum();
        for (day, path, size) in entries.clone() {
            if is_active(&path) {
                continue;
            }
            let stale = chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d")
                .map(|date| today.signed_duration_since(date).num_days() > config.log_retain_days as i64)
                .unwrap_or(false);
            if stale {
                if std::fs::remove_file(&path).is_ok() {
                    removed += 1;
                    total = total.saturating_sub(size);
                }
            }
        }
        for (_, path, size) in entries {
            if total <= config.max_log_total_bytes {
                break;
            }
            if is_active(&path) {
                continue;
            }
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
                total = total.saturating_sub(size);
            }
        }
        Ok(removed)
    }

    pub fn usage(&self) -> (u64, usize, Vec<String>) {
        let mut total = 0;
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("wardrobe-") && name.contains(".jsonl") {
                    total += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                    files.push(name);
                }
            }
        }
        files.sort();
        (total, files.len(), files)
    }
}

static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Arc<Logger>>>> = OnceLock::new();
fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<Logger>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}
/// 取（或创建）某数据目录的日志器；同一数据目录在本进程内共享一个写入句柄。
pub fn logger_for(data_dir: &Path, config: DiagnosticsConfig) -> Arc<Logger> {
    let key = data_dir.to_path_buf();
    let mut map = registry().lock().expect("日志注册表不可用");
    if let Some(existing) = map.get(&key) {
        existing.set_config(config);
        return existing.clone();
    }
    let logger = Arc::new(Logger::new(key.join("logs"), config));
    map.insert(key, logger.clone());
    logger
}
/// 只读访问已注册日志器（不创建目录）；查询与导出用。
pub fn existing_logger(data_dir: &Path) -> Option<Arc<Logger>> {
    registry()
        .lock()
        .expect("日志注册表不可用")
        .get(&data_dir.to_path_buf())
        .cloned()
}

/// 进程级旁路写入（panic 钩子、打开失败、桌面天气搜索等无 Application 场景）。
pub fn emit_to(data_dir: &Path, level: &str, kind: &str, op: &str, result: &str, detail: Value) {
    let logger = logger_for(data_dir, DiagnosticsConfig::default());
    logger.emit(level, kind, op, None, result, None, detail);
}

/// GUI/CLI/MCP 入口安装 panic 钩子：崩溃现场写入默认数据目录日志。
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| (*text).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "未知 panic".to_string());
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_default();
        let data_dir = crate::default_database_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        emit_to(
            &data_dir,
            "error",
            "panic",
            "process.panic",
            "panic",
            json!({"message": message.chars().take(2000).collect::<String>(), "location": location, "run_id": current_run()}),
        );
        previous(info);
    }));
}

// ---------- 查询与导出 ----------

#[derive(Default, Clone, Debug)]
pub struct LogFilter {
    pub level: Option<String>,
    pub min_level: Option<u8>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub trace_id: Option<String>,
    pub run_id: Option<String>,
    pub op_prefix: Option<String>,
    pub contains: Option<String>,
    pub limit: usize,
    pub offset: usize,
}
impl LogFilter {
    fn matches(&self, event: &Value, ts_ms: Option<i64>) -> bool {
        if let Some(level) = &self.level {
            if event["level"].as_str() != Some(level.as_str()) {
                return false;
            }
        }
        if let Some(min) = self.min_level {
            if event["level"]
                .as_str()
                .map(level_number)
                .is_none_or(|number| number < min)
            {
                return false;
            }
        }
        if let Some(window) = self.since_ms {
            if ts_ms.is_none_or(|ts| ts < window) {
                return false;
            }
        }
        if let Some(window) = self.until_ms {
            if ts_ms.is_none_or(|ts| ts > window) {
                return false;
            }
        }
        if let Some(trace) = &self.trace_id {
            if event["trace_id"].as_str() != Some(trace.as_str()) {
                return false;
            }
        }
        if let Some(run) = &self.run_id {
            if event["run_id"].as_str() != Some(run.as_str()) {
                return false;
            }
        }
        if let Some(prefix) = &self.op_prefix {
            if !event["op"].as_str().is_some_and(|op| op.starts_with(prefix)) {
                return false;
            }
        }
        if let Some(needle) = &self.contains {
            let haystack = event.to_string();
            if !haystack.contains(needle.as_str()) {
                return false;
            }
        }
        true
    }
}

pub(crate) fn parse_ts_ms(event: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(event["ts"].as_str()?)
        .ok()
        .map(|time| time.timestamp_millis())
}

/// 读取并过滤事件（新→旧）；损坏行跳过。
pub fn read_events(logger: &Logger, filter: &LogFilter) -> Vec<Value> {
    let (total, _, files) = logger.usage();
    let _ = total;
    let mut events: Vec<(i64, Value)> = Vec::new();
    for name in files {
        let path = logger.dir().join(&name);
        let Ok(file) = File::open(&path) else {
            continue;
        };
        for line in BufReader::new(file).lines() {
            let Ok(line) = line else { continue };
            let Ok(event) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let ts_ms = parse_ts_ms(&event);
            if filter.matches(&event, ts_ms) {
                events.push((ts_ms.unwrap_or(i64::MIN), event));
            }
        }
    }
    events.sort_by(|a, b| b.0.cmp(&a.0));
    events
        .into_iter()
        .skip(filter.offset)
        .take(filter.limit.max(1))
        .map(|(_, event)| event)
        .collect()
}

pub(crate) fn default_export_name() -> String {
    chrono::Local::now().format("衣橱日志-%Y%m%d-%H%M.jsonl").to_string()
}

/// 导出单文件 JSONL 诊断：首行 meta，其后按时间升序的事件行。
pub fn write_export(
    logger: &Logger,
    events: Vec<Value>,
    meta_extra: Value,
    destination: &Path,
) -> Result<Value, crate::ApiError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|_| {
            crate::ApiError::new("export_failed", "无法创建导出目录，请检查位置是否可写")
        })?;
    }
    let mut ascending = events;
    ascending.sort_by_key(|event| parse_ts_ms(event).unwrap_or(i64::MIN));
    let mut counts: HashMap<String, u64> = HashMap::new();
    let mut runs: Vec<String> = Vec::new();
    let mut recent_errors: Vec<Value> = Vec::new();
    for event in &ascending {
        let level = event["level"].as_str().unwrap_or("info").to_string();
        *counts.entry(level).or_default() += 1;
        if let Some(run) = event["run_id"].as_str() {
            if !runs.iter().any(|known| known == run) {
                runs.push(run.to_string());
            }
        }
        if event["level"].as_str() == Some("error") {
            recent_errors.push(json!({
                "ts": event["ts"],
                "op": event["op"],
                "code": event["result"],
                "trace_id": event.get("trace_id").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    recent_errors.reverse();
    recent_errors.truncate(5);
    let from = ascending
        .first()
        .and_then(|event| event["ts"].as_str())
        .unwrap_or("");
    let to = ascending
        .last()
        .and_then(|event| event["ts"].as_str())
        .unwrap_or("");
    let mut meta = json!({
        "kind": meta_extra.get("kind").and_then(Value::as_str).unwrap_or("diagnostics"),
        "app_version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "database_schema_version": crate::DATABASE_SCHEMA_VERSION,
        "exported_at": chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "from": from,
        "to": to,
        "run_ids": runs,
        "counts_by_level": counts,
        "event_count": ascending.len(),
        "recent_errors": recent_errors,
        "log_dropped": logger.dropped(),
    });
    if let Some(extra) = meta_extra.as_object() {
        for (key, value) in extra {
            if key != "kind" {
                meta[key] = value.clone();
            }
        }
    }
    let mut body = String::new();
    body.push_str(&meta.to_string());
    body.push('\n');
    for event in &ascending {
        body.push_str(&event.to_string());
        body.push('\n');
    }
    let bytes = body.len() as u64;
    std::fs::write(destination, body).map_err(|_| {
        crate::ApiError::new("export_failed", "诊断文件写入失败，请换一个位置重试")
    })?;
    Ok(json!({
        "path": destination.to_string_lossy(),
        "events": ascending.len(),
        "bytes": bytes,
        "from": from,
        "to": to,
        "kind": meta["kind"],
    }))
}
