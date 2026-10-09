//! 全链路日志核心：结构化 JSONL 事件按天追加写入数据目录 `logs/` 子目录。
//!
//! 设计不变量（ADR 语义，供 #43 查询与导出复用）：
//! - **独立于 SQLite**：日志写在 `<数据目录>/logs/wardrobe-YYYY-MM-DD[.序号].jsonl`，
//!   数据库损坏、恢复失败时仍可回查。
//! - **链路标识**：`run_id()` 每进程生成一次（含 Tauri 每命令重开 Application 的场景）；
//!   每次 Application API 调用分配 UUIDv7 `trace_id`。
//! - **原因保留**：`ApiError::reason` 只写日志、不进用户可见输出（serde 跳过）。
//! - **脱敏按构造**：写入接口只接受安全标量（操作名、标识、耗时、错误码、原因、输入字段名），
//!   永不接受原始输入/输出 JSON 值；供应商密钥、连接令牌及其 hash、恢复材料、
//!   空间密钥与图片内容因此在构造上不可能落盘。文件路径与单品名允许。
//! - **尽力而为**：任何写入失败静默降级并计数（`dropped_events`，#43 经 diagnostics.summary
//!   暴露），绝不阻塞或改变业务操作结果。
//! - **封顶**：单文件、总占用与保留天数默认 10 MiB / 256 MiB / 14 天，
//!   可经 `Application::open_with_limits` 注入供测试；启动与写入时惰性清理。

use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

/// 日志轮转与保留上限；产品入口用默认值，测试注入极小值驱动轮转与清理。
#[derive(Clone, Copy, Debug)]
pub struct LogLimits {
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub retention_days: u32,
}
impl Default for LogLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 10 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024,
            retention_days: 14,
        }
    }
}

/// 事件级别：默认 info；`WARDROBE_LOG_LEVEL=debug|trace` 提升详细度。
/// error 级事件（操作失败、panic）不受阈值约束，始终写入。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Level {
    Info,
    Debug,
    Trace,
    Error,
}
impl Level {
    fn from_env() -> Self {
        match std::env::var("WARDROBE_LOG_LEVEL").as_deref() {
            Ok("debug") => Level::Debug,
            Ok("trace") => Level::Trace,
            _ => Level::Info,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Debug => "debug",
            Level::Trace => "trace",
            Level::Error => "error",
        }
    }
}

/// 每次进程启动生成一次运行会话标识；跨天/跨文件的同一次运行以 run_id 关联。
static RUN_ID: OnceLock<String> = OnceLock::new();
static RUN_START_WRITTEN: OnceLock<()> = OnceLock::new();
/// 本进程打开过的日志汇（panic 钩子向全部汇尽力写入；按目录去重）。
static SINKS: OnceLock<Mutex<Vec<Logger>>> = OnceLock::new();

pub fn run_id() -> &'static str {
    RUN_ID.get_or_init(|| uuid::Uuid::now_v7().to_string())
}
pub(crate) fn new_trace_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// 旁路命令（不经 Application::open，如桌面端天气城市搜索）的日志入口：
/// 进程内幂等初始化默认数据目录的日志汇（GUI 的 application 命令在此前或此后
/// 打开的 Application 共享同一 logs 目录），之后尽力写入事件；失败静默降级。
/// 事件由调用方按构造脱敏后组装（run_id/ts/level 由这里补齐）。
pub fn log_bypass_event(event_type: &str, fields: Value) {
    static BYPASS: OnceLock<Option<Logger>> = OnceLock::new();
    let logger = BYPASS.get_or_init(|| {
        let data_dir = crate::default_database_path()
            .parent()
            .map(Path::to_path_buf)?;
        Some(Logger::new(&data_dir, LogLimits::default()))
    });
    if let Some(logger) = logger {
        logger.log_event(Level::Info, event_type, fields);
    }
}

// 当前调用链路的线程局部上下文：`Application::execute` 进入业务函数前置位，
// 返回后恢复。持久任务的创建（jobs::insert）在业务函数内读取它，把本次调用的
// trace_id 写进任务文档——worker 可能运行在另一进程，trace 只经任务文档传递，
// 不依赖进程状态或调用栈以外的共享内存。
thread_local! {
    static CURRENT_TRACE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
/// 在 `trace_id` 上下文内运行业务调用，返回后恢复先前置位（支持同步重入嵌套）。
pub(crate) fn with_trace<R>(trace_id: &str, run: impl FnOnce() -> R) -> R {
    let previous = CURRENT_TRACE.with(|cell| cell.replace(Some(trace_id.to_string())));
    let result = run();
    CURRENT_TRACE.with(|cell| cell.replace(previous));
    result
}
/// 当前线程正在执行的 Application 调用的 trace_id；不在调用内则为 None。
pub(crate) fn current_trace() -> Option<String> {
    CURRENT_TRACE.with(|cell| cell.borrow().clone())
}

fn sinks() -> &'static Mutex<Vec<Logger>> {
    SINKS.get_or_init(|| Mutex::new(Vec::new()))
}

/// 安装进程级 panic 钩子：panic 现场写入本进程注册过的全部日志汇（幂等）。
/// 保留既有钩子行为（默认打印到 stderr）。GUI 与 CLI 入口在 main 里调用。
pub fn install_panic_hook() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "非字符串 panic 载荷".to_string());
        let fields = json!({
            "message": message,
            "location": info.location().map(|l| l.to_string()).unwrap_or_default(),
            "thread": std::thread::current().name().unwrap_or("").to_string(),
        });
        if let Ok(sinks) = sinks().lock() {
            for logger in sinks.iter() {
                logger.log_event(Level::Error, "panic", fields.clone());
            }
        }
        previous(info);
    }));
}

#[derive(Debug, Default)]
struct LoggerState {
    active_date: Option<chrono::NaiveDate>,
    active_seq: u32,
    active_size: u64,
    total_estimate: u64,
    last_cleanup: Option<Instant>,
    dropped_events: u64,
}

const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);
const MAX_SINKS: usize = 16;

/// 每个打开的 Application 持有一个 Logger（Arc 共享状态，Clone 廉价）。
#[derive(Clone, Debug)]
pub(crate) struct Logger {
    logs_dir: PathBuf,
    limits: LogLimits,
    threshold: Level,
    state: Arc<Mutex<LoggerState>>,
}
impl Logger {
    pub(crate) fn new(data_dir: &Path, limits: LogLimits) -> Self {
        let logs_dir = data_dir.join("logs");
        let state = Arc::new(Mutex::new(Self::scan(&logs_dir)));
        let logger = Self {
            logs_dir,
            limits,
            threshold: Level::from_env(),
            state,
        };
        if fs::create_dir_all(&logger.logs_dir).is_ok() {
            if let Ok(mut sinks) = sinks().lock() {
                if !sinks.iter().any(|s| s.logs_dir == logger.logs_dir) {
                    sinks.push(logger.clone());
                    if sinks.len() > MAX_SINKS {
                        sinks.remove(0);
                    }
                }
            }
        }
        // 运行会话启动事件：每进程一次
        if RUN_START_WRITTEN.set(()).is_ok() {
            logger.log_event(Level::Info, "run_start", json!({"run_id": run_id()}));
        }
        {
            let mut state = logger.lock();
            logger.cleanup(&mut state);
        }
        logger
    }

    fn scan(logs_dir: &Path) -> LoggerState {
        let today = chrono::Local::now().date_naive();
        let mut state = LoggerState {
            active_date: Some(today),
            ..LoggerState::default()
        };
        if let Ok(entries) = fs::read_dir(logs_dir) {
            for entry in entries.flatten() {
                let Some((date, seq)) = parse_log_file_name(&entry.file_name().to_string_lossy())
                else {
                    continue;
                };
                // 并发容忍：另一进程（CLI 与 GUI 共享同一 logs 目录）可能正在清理，
                // stat 时文件已消失（NotFound）——视为"已被清理"，跳过而非报错
                let Ok(meta) = entry.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                state.total_estimate += meta.len();
                if date == today && seq >= state.active_seq {
                    state.active_seq = seq;
                    state.active_size = meta.len();
                }
            }
        }
        state
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LoggerState> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 一次 Application API 调用的结果事件。`input_keys` 只含字段名（脱敏按构造）。
    pub(crate) fn log_operation(
        &self,
        trace_id: &str,
        operation: &str,
        duration: Duration,
        input_keys: &[String],
        outcome: &Result<Value, crate::ApiError>,
    ) {
        let level = match outcome {
            Ok(_) => Level::Info,
            Err(_) => Level::Error,
        };
        if !self.enabled(level) {
            return;
        }
        let mut event = json!({
            "ts": timestamp(),
            "level": level.as_str(),
            "event_type": "operation",
            "run_id": run_id(),
            "trace_id": trace_id,
            "operation": operation,
            "duration_ms": duration.as_millis() as u64,
            "result": match outcome {
                Ok(_) => "ok",
                Err(error) => error.code.as_str(),
            },
        });
        if let Err(error) = outcome {
            if let Some(reason) = &error.reason {
                event["reason"] = json!(reason);
            }
        }
        if self.threshold >= Level::Debug {
            event["input_keys"] = json!(input_keys);
        }
        self.write_line(&event.to_string());
    }

    /// 打开失败事件：数据库损坏等"正是存储出问题"的场景必须留痕。
    pub(crate) fn log_open_failure(&self, error: &crate::ApiError) {
        let mut fields = json!({
            "operation": "application.open",
            "result": error.code,
        });
        if let Some(reason) = &error.reason {
            fields["reason"] = json!(reason);
        }
        self.log_event(Level::Error, "open", fields);
    }

    /// 复现记录标记（#50）：调用方捕获时间戳并随字段写入，使事件落盘 ts 与
    /// 返回给前端的窗口端点完全一致（fields 合并覆盖基础 ts，见 log_event）。
    /// 事件只含 phase 与 ts，无可注入内容。
    pub(crate) fn log_reproduction_marker(&self, phase: &str, ts: &str) {
        self.log_event(Level::Info, "reproduction", json!({"phase": phase, "ts": ts}));
    }

    /// 通用事件入口（run_start、panic；#43 的 diagnostics.ui_event 等复用）。
    /// `fields` 必须由调用方按构造脱敏后组装。
    pub(crate) fn log_event(&self, level: Level, event_type: &str, fields: Value) {
        if !self.enabled(level) {
            return;
        }
        let mut event = json!({
            "ts": timestamp(),
            "level": level.as_str(),
            "event_type": event_type,
            "run_id": run_id(),
        });
        if let (Some(target), Some(extra)) = (event.as_object_mut(), fields.as_object()) {
            for (key, value) in extra {
                target.insert(key.clone(), value.clone());
            }
        }
        self.write_line(&event.to_string());
    }

    /// 丢弃计数：写入失败静默降级的累计次数，经 diagnostics.summary 暴露。
    pub(crate) fn dropped_events(&self) -> u64 {
        self.lock().dropped_events
    }

    fn enabled(&self, level: Level) -> bool {
        level == Level::Error || level <= self.threshold
    }

    fn write_line(&self, line: &str) {
        let mut state = self.lock();
        if self.append_line(&mut state, line).is_err() {
            state.dropped_events += 1;
        }
    }

    fn append_line(&self, state: &mut LoggerState, line: &str) -> std::io::Result<()> {
        let today = chrono::Local::now().date_naive();
        if state.active_date != Some(today) {
            state.active_date = Some(today);
            state.active_seq = highest_seq(&self.logs_dir, today);
            state.active_size = fs::metadata(active_path(&self.logs_dir, today, state.active_seq))
                .map(|m| m.len())
                .unwrap_or(0);
        }
        let mut path = active_path(&self.logs_dir, today, state.active_seq);
        let mut size = if state.active_size > 0 {
            state.active_size
        } else {
            fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
        };
        // 按文件大小上限轮转：写入前预判，整行放不下即换下一个序号文件
        if size > 0 && size + line.len() as u64 > self.limits.max_file_bytes {
            state.active_seq += 1;
            path = active_path(&self.logs_dir, today, state.active_seq);
            size = 0;
        }
        fs::create_dir_all(&self.logs_dir).ok();
        let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)?;
        // 行与换行合并为一次 write：O_APPEND 下单次写入原子追加，并发写者（CLI 与 GUI
        // 两进程共享同一 logs 目录；本进程 panic 钩子向全部日志汇并发写）的字节无法落在
        // 行与换行之间。拆成两次 write 时，另一写者的整行可落在这两段之间，两个 JSON
        // 事件拼成一行损坏行，被读取端按损坏行一起跳过——并发追加双方同时丢事件（#53）。
        let mut record = String::with_capacity(line.len() + 1);
        record.push_str(line);
        record.push('\n');
        file.write_all(record.as_bytes())?;
        file.flush()?;
        let written = line.len() as u64;
        state.active_size = size + written;
        state.total_estimate = state.total_estimate.saturating_add(written);
        let due = match state.last_cleanup {
            Some(at) => at.elapsed() >= CLEANUP_INTERVAL,
            None => true,
        };
        if state.total_estimate > self.limits.max_total_bytes || due {
            self.cleanup(state);
        }
        Ok(())
    }

    /// 惰性清理：先按保留天数删过期文件，再按总占用上限从最旧开始删（活动文件除外）。
    fn cleanup(&self, state: &mut LoggerState) {
        state.last_cleanup = Some(Instant::now());
        let today = chrono::Local::now().date_naive();
        let cutoff = today - chrono::Duration::days(self.limits.retention_days as i64);
        let Ok(entries) = fs::read_dir(&self.logs_dir) else {
            return;
        };
        let mut files: Vec<(chrono::NaiveDate, u32, PathBuf, u64)> = Vec::new();
        for entry in entries.flatten() {
            let Some((date, seq)) = parse_log_file_name(&entry.file_name().to_string_lossy())
            else {
                continue;
            };
            // 并发容忍：另一进程可能正在清理同一目录，stat 时文件已消失（NotFound）
            // ——视为"已被清理"，跳过而非报错
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            files.push((date, seq, entry.path(), meta.len()));
        }
        let mut removed = 0u64;
        files.retain(|(date, _, path, size)| {
            if *date < cutoff {
                if fs::remove_file(path).is_ok() {
                    removed += size;
                    return false;
                }
            }
            true
        });
        files.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        let active = state
            .active_date
            .map(|date| active_path(&self.logs_dir, date, state.active_seq));
        let mut total = state.total_estimate.saturating_sub(removed);
        for (_, _, path, size) in files {
            if total <= self.limits.max_total_bytes {
                break;
            }
            if Some(&path) == active.as_ref() {
                continue;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        state.total_estimate = total;
    }
}

fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 事件至少要有的公共字段：查询与摘要只投影良构事件，
/// 手写残行或被截断的事件（如 `{"event_type":"stale"}`）与损坏行同样跳过。
fn well_formed(event: &Value) -> bool {
    ["ts", "level", "event_type", "run_id"]
        .iter()
        .all(|key| event.get(key).is_some_and(Value::is_string))
}

impl Logger {
    /// 按文件时间序（wardrobe-YYYY-MM-DD[.序号].jsonl → 行序，即写入序）读取全部良构事件。
    /// 尾行截断、非 JSON 行与消失中的文件（并发清理）都跳过，绝不阻断查询。
    pub(crate) fn read_events(&self) -> Vec<Value> {
        let mut files: Vec<(chrono::NaiveDate, u32, PathBuf)> = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.logs_dir) {
            for entry in entries.flatten() {
                if let Some((date, seq)) =
                    parse_log_file_name(&entry.file_name().to_string_lossy())
                {
                    files.push((date, seq, entry.path()));
                }
            }
        }
        files.sort();
        let mut events = Vec::new();
        for (_, _, path) in files {
            let content = match fs::read_to_string(&path) {
                Ok(content) => content,
                // 并发清理者恰好删掉该文件，或读取瞬时失败：尽力而为，跳过
                Err(_) => continue,
            };
            for line in content.lines().filter(|line| !line.trim().is_empty()) {
                match serde_json::from_str::<Value>(line) {
                    Ok(event) if well_formed(&event) => events.push(event),
                    _ => continue,
                }
            }
        }
        events
    }

    /// 日志占用：总字节数与文件数（并发清理容忍 NotFound）。
    pub(crate) fn occupancy(&self) -> (u64, u64) {
        let mut total = 0;
        let mut count = 0;
        if let Ok(entries) = fs::read_dir(&self.logs_dir) {
            for entry in entries.flatten() {
                if parse_log_file_name(&entry.file_name().to_string_lossy()).is_none() {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        total += meta.len();
                        count += 1;
                    }
                }
            }
        }
        (total, count)
    }
}

/// 最近错误窗口上限（诊断摘要与 CLI/GUI 共用的同一口径）。
pub(crate) const RECENT_ERRORS_LIMIT: usize = 20;

/// 把日志事件投影为诊断摘要的最近错误条目：公共字段 + 诊断有用字段，不含耗时与输入摘要。
fn recent_error_item(event: &Value) -> Value {
    let mut item = json!({
        "ts": event["ts"],
        "level": event["level"],
        "event_type": event["event_type"],
        "run_id": event["run_id"],
    });
    for key in ["trace_id", "operation", "result", "reason", "message"] {
        if event.get(key).is_some_and(|value| !value.is_null()) {
            item[key] = event[key].clone();
        }
    }
    item
}

use crate::{ApiError, Application};

/// 供应商请求事件的调用形状（复查收拢）：trace/模型一次取好、计时随每次真实发送
/// 重置，每次发送记录一条。`record` 只转发 `log_provider_request`——事件字段与
/// 落盘格式逐字不变（provider 脱敏测试为安全网）。
pub(crate) struct ProviderRequestLog {
    trace_id: Option<String>,
    model: String,
    started: std::cell::Cell<std::time::Instant>,
}
impl ProviderRequestLog {
    /// worker 链路：trace 取任务文档（跨进程唯一通道），模型取供应商配置或任务载荷。
    pub(crate) fn from_job(job: &Value, model: &str) -> Self {
        Self {
            trace_id: job["trace_id"].as_str().map(str::to_owned),
            model: model.to_string(),
            started: std::cell::Cell::new(std::time::Instant::now()),
        }
    }
    /// 同步验证链路：trace 取当前调用上下文（线程局部，见 with_trace）。
    pub(crate) fn from_current_call(model: &str) -> Self {
        Self {
            trace_id: current_trace(),
            model: model.to_string(),
            started: std::cell::Cell::new(std::time::Instant::now()),
        }
    }
    /// 每次真实发送前重置计时起点（限流重试的每次发送各记一条自身耗时）。
    pub(crate) fn begin_send(&self) {
        self.started.set(std::time::Instant::now());
    }
    /// 记录一次发送的结果类别（ok/http_error/outcome_unknown/error）与 HTTP 状态、耗时。
    pub(crate) fn record(
        &self,
        app: &Application,
        endpoint_kind: &str,
        outcome: &str,
        http_status: Option<u16>,
    ) {
        app.log_provider_request(
            self.trace_id.as_deref(),
            endpoint_kind,
            &self.model,
            outcome,
            http_status,
            self.started.get(),
        );
    }
}

impl Application {
    /// 供应商请求事件（#44）：端点类别、模型名、结果类别（ok/http_error/outcome_unknown/error）、
    /// HTTP 状态与耗时；trace_id 来自任务文档（异步链路）或当前调用（同步验证）。
    /// 脱敏按构造：绝不记录密钥、请求头、完整请求或响应体。
    pub(crate) fn log_provider_request(
        &self,
        trace_id: Option<&str>,
        endpoint_kind: &str,
        model: &str,
        outcome: &str,
        http_status: Option<u16>,
        started: std::time::Instant,
    ) {
        let mut fields = json!({
            "operation": "provider.request",
            "endpoint_kind": endpoint_kind,
            "model": model,
            "outcome": outcome,
            "duration_ms": started.elapsed().as_millis() as u64,
        });
        if let Some(trace) = trace_id {
            fields["trace_id"] = json!(trace);
        }
        if let Some(status) = http_status {
            fields["http_status"] = json!(status);
        }
        self.logger
            .log_event(Level::Info, "provider_request", fields);
    }

    /// diagnostics.* 分发：仅本机可信入口（authorization::privileged 一律拒绝外部客户端）。
    pub(super) fn diagnostics_execute(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        match operation {
            "diagnostics.summary" => self.diagnostics_summary(input),
            "diagnostics.logs.query" => self.diagnostics_logs_query(input),
            "diagnostics.export" => self.diagnostics_export(input),
            "diagnostics.checks" => self.diagnostics_checks(input),
            "diagnostics.ui_event" => self.diagnostics_ui_event(input),
            "diagnostics.reproduction.start" => self.diagnostics_reproduction_marker(input, "start"),
            "diagnostics.reproduction.stop" => self.diagnostics_reproduction_marker(input, "stop"),
            _ => Err(ApiError::new("unsupported_operation", "此操作尚不可用")),
        }
    }

    /// diagnostics.ui_event（#50）：把前端关键交互写入同一日志流形成用户操作时间线。
    /// 封闭白名单：type 须为 view_switch / dialog_open / dialog_submit 之一，
    /// label 须为 1–40 字符的简短界面文案（前端固定文案，非用户输入内容）；
    /// 白名单外类型与超长标签拒绝且不落盘。事件只含 type 与 label 两个字段，
    /// 不接受自由文本——脱敏按构造不变，不引入旁路。
    pub(super) fn diagnostics_ui_event(&self, input: Value) -> Result<Value, ApiError> {
        const UI_EVENT_TYPES: [&str; 3] = ["view_switch", "dialog_open", "dialog_submit"];
        const UI_EVENT_LABEL_MAX_CHARS: usize = 40;
        let kind = match input.get("type") {
            Some(Value::String(text)) => text.as_str(),
            _ => return Err(ApiError::new("invalid_input", "type 须为白名单事件类型")),
        };
        if !UI_EVENT_TYPES.contains(&kind) {
            return Err(ApiError::new("invalid_input", "事件类型不在白名单内"));
        }
        let label = match input.get("label") {
            Some(Value::String(text)) => text.as_str(),
            _ => return Err(ApiError::new("invalid_input", "label 须为简短界面文案")),
        };
        let chars = label.chars().count();
        if chars == 0 || chars > UI_EVENT_LABEL_MAX_CHARS {
            return Err(ApiError::new(
                "invalid_input",
                &format!("label 须为 1–{UI_EVENT_LABEL_MAX_CHARS} 字符的界面文案"),
            ));
        }
        self.logger.log_event(Level::Info, "ui", json!({"type": kind, "label": label}));
        Ok(json!({"schema_version": 1}))
    }

    /// diagnostics.reproduction.start / stop（#50）：写入显式标记事件并返回标记时刻。
    /// 核心不持有录制状态（刷新页面即放弃，前端负责计时与自动停止）；
    /// 标记事件只含 phase 与 ts，调用方以两端时刻为界调用扩展后的 diagnostics.export。
    pub(super) fn diagnostics_reproduction_marker(
        &self,
        input: Value,
        phase: &str,
    ) -> Result<Value, ApiError> {
        let _ = input;
        let ts = timestamp();
        self.logger.log_reproduction_marker(phase, &ts);
        if phase == "start" {
            Ok(json!({"schema_version": 1, "started_at": ts}))
        } else {
            Ok(json!({"schema_version": 1, "stopped_at": ts}))
        }
    }

    /// 日志查询：按级别、时间范围、trace_id、run_id、操作前缀、关键词过滤，游标分页。
    /// 游标语义同「分页游标」（GLOSSARY）：调用方须保留查询条件，next_cursor 为 null 表示末页；
    /// 游标编码 `<事件ts毫秒>-<该毫秒内已返回的匹配事件数>`，对写入追加（更大 ts）稳定。
    pub(super) fn diagnostics_logs_query(&self, input: Value) -> Result<Value, ApiError> {
        let level: Option<String> = match input.get("level") {
            None => None,
            Some(value) => {
                let level = value.as_str().ok_or_else(|| {
                    ApiError::new("invalid_input", "级别筛选必须为字符串")
                })?;
                if !["info", "debug", "trace", "error"].contains(&level) {
                    return Err(ApiError::new("invalid_input", "级别筛选无效"));
                }
                Some(level.to_owned())
            }
        };
        let since = parse_rfc3339_field(&input, "since")?;
        let until = parse_rfc3339_field(&input, "until")?;
        if let (Some(start), Some(end)) = (since, until) {
            if start > end {
                return Err(ApiError::new("invalid_input", "时间范围起止颠倒"));
            }
        }
        let exact = |key: &str| -> Result<Option<String>, ApiError> {
            match input.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(value) => value
                    .as_str()
                    .map(|text| Some(text.to_owned()))
                    .ok_or_else(|| ApiError::new("invalid_input", &format!("{key} 必须为字符串"))),
            }
        };
        let trace_id = exact("trace_id")?;
        let run_id_filter = exact("run_id")?;
        let operation_prefix = exact("operation")?;
        let keyword = match exact("keyword")? {
            None => None,
            Some(text) => {
                if text.chars().count() > 200 {
                    return Err(ApiError::new("invalid_input", "关键词最多 200 字符"));
                }
                Some(text.to_lowercase())
            }
        };
        let limit = match input.get("limit") {
            None => 50,
            Some(value) => value
                .as_u64()
                .ok_or_else(|| ApiError::new("invalid_input", "每页数量必须为整数"))?,
        };
        if !(1..=100).contains(&limit) {
            return Err(ApiError::new("invalid_input", "每页数量必须为 1–100"));
        }
        let (cursor_millis, cursor_skip) = match input.get("cursor") {
            None | Some(Value::Null) => (None, 0u64),
            Some(value) => {
                let text = value
                    .as_str()
                    .ok_or_else(|| ApiError::new("invalid_input", "分页游标必须为字符串"))?;
                let (millis, skip) = text
                    .rsplit_once('-')
                    .and_then(|(millis, skip)| {
                        Some((millis.parse::<i64>().ok()?, skip.parse::<u64>().ok()?))
                    })
                    .ok_or_else(|| ApiError::new("invalid_input", "分页游标无效"))?;
                (Some(millis), skip)
            }
        };
        let event_millis =
            |event: &Value| -> Option<i64> {
                chrono::DateTime::parse_from_rfc3339(event["ts"].as_str()?)
                    .ok()
                    .map(|time| time.timestamp_millis())
            };
        let events = self.logger.read_events();
        // 基础筛选：按写入序（文件时间序）；查询只服务 ts 可解析的良构事件，游标推进依赖它
        let mut matched: Vec<&Value> = events
            .iter()
            .filter(|event| {
                // 游标推进依赖 ts 毫秒：ts 不可解析的事件不进入查询结果
                if event_millis(event).is_none() {
                    return false;
                }
                if let Some(start) = since {
                    let stamp = chrono::DateTime::parse_from_rfc3339(event["ts"].as_str().unwrap());
                    if !stamp.is_ok_and(|stamp| stamp >= start) {
                        return false;
                    }
                }
                if let Some(end) = until {
                    let stamp = chrono::DateTime::parse_from_rfc3339(event["ts"].as_str().unwrap());
                    if !stamp.is_ok_and(|stamp| stamp <= end) {
                        return false;
                    }
                }
                if let Some(level) = &level {
                    if event["level"].as_str() != Some(level.as_str()) {
                        return false;
                    }
                }
                if let Some(trace) = &trace_id {
                    if event["trace_id"].as_str() != Some(trace.as_str()) {
                        return false;
                    }
                }
                if let Some(run) = &run_id_filter {
                    if event["run_id"].as_str() != Some(run.as_str()) {
                        return false;
                    }
                }
                if let Some(prefix) = &operation_prefix {
                    if !event["operation"]
                        .as_str()
                        .is_some_and(|operation| operation.starts_with(prefix.as_str()))
                    {
                        return false;
                    }
                }
                if let Some(keyword) = &keyword {
                    if !event.to_string().to_lowercase().contains(keyword.as_str()) {
                        return false;
                    }
                }
                true
            })
            .collect();
        // 游标推进：跳过游标毫秒及此前的事件，再跳过同毫秒内已返回的 skip 条
        if let Some(cursor) = cursor_millis {
            let mut seen: u64 = 0;
            matched.retain(|event| {
                let millis = event_millis(event).unwrap_or(i64::MIN);
                if millis < cursor {
                    return false;
                }
                if millis == cursor {
                    seen += 1;
                    return seen > cursor_skip;
                }
                true
            });
        }
        let more = matched.len() > limit as usize;
        matched.truncate(limit as usize);
        let next_cursor = if more {
            let last_millis = matched
                .last()
                .and_then(|event| event_millis(event))
                .ok_or_else(|| ApiError::new("storage_error", "日志事件时间不可解析"))?;
            // skip 跨页累计：同毫秒内已返回总数 = 上一游标的 skip（末事件同毫秒时）+ 本页返回数
            let previous = if cursor_millis == Some(last_millis) {
                cursor_skip
            } else {
                0
            };
            let returned = matched
                .iter()
                .filter(|event| event_millis(event) == Some(last_millis))
                .count() as u64;
            Some(format!("{last_millis}-{}", previous + returned))
        } else {
            None
        };
        let items: Vec<Value> = matched.into_iter().cloned().collect();
        Ok(json!({
            "schema_version": 1,
            "items": items,
            "next_cursor": next_cursor,
        }))
    }

    /// diagnostics.export：单文件 JSONL 诊断导出。首行 meta（环境信息与摘要投影），
    /// 其后为按时间排序的日志事件。产物只来自日志文件与摘要投影，天然不含数据库、
    /// 图片与凭据——脱敏与普通日志同一构造。
    pub(super) fn diagnostics_export(&self, input: Value) -> Result<Value, ApiError> {
        let path = match input.get("path") {
            Some(Value::String(text)) if !text.is_empty() => text.clone(),
            _ => return Err(ApiError::new("invalid_input", "path 须为非空字符串")),
        };
        // 范围解析：scope（session/recent/all，默认 recent）或显式 from–to 起止时间窗
        //（含端点；与 scope 互斥，须成对提供）
        #[derive(Clone, Copy)]
        enum Window {
            Session,
            Recent,
            All,
            Explicit(i64, i64),
        }
        let has_from = input.get("from").is_some_and(|value| !value.is_null());
        let has_to = input.get("to").is_some_and(|value| !value.is_null());
        let window = if has_from || has_to {
            if input.get("scope").is_some_and(|value| !value.is_null()) {
                return Err(ApiError::new(
                    "invalid_input",
                    "scope 与显式起止时间不可同时提供",
                ));
            }
            if !has_from || !has_to {
                return Err(ApiError::new("invalid_input", "显式时间窗须同时提供 from 与 to"));
            }
            let parse_time = |key: &str| -> Result<chrono::DateTime<chrono::FixedOffset>, ApiError> {
                parse_rfc3339_field(&input, key)?.ok_or_else(|| {
                    ApiError::new("invalid_input", &format!("{key} 须为 RFC3339 时间"))
                })
            };
            let from = parse_time("from")?;
            let to = parse_time("to")?;
            if from > to {
                return Err(ApiError::new("invalid_input", "时间范围起止颠倒"));
            }
            Window::Explicit(from.timestamp_millis(), to.timestamp_millis())
        } else {
            match input.get("scope") {
                None | Some(Value::Null) => Window::Recent,
                Some(Value::String(text)) => match text.as_str() {
                    "session" => Window::Session,
                    "recent" => Window::Recent,
                    "all" => Window::All,
                    _ => {
                        return Err(ApiError::new(
                            "invalid_input",
                            "scope 须为 session、recent 或 all",
                        ))
                    }
                },
                Some(_) => {
                    return Err(ApiError::new(
                        "invalid_input",
                        "scope 须为 session、recent 或 all",
                    ))
                }
            }
        };
        let scope_name = match window {
            Window::Session => "session",
            Window::Recent => "recent",
            Window::All => "all",
            Window::Explicit(..) => "window",
        };
        // #50 加法扩展：kind（产物 meta 种类，默认 diagnostics；reproduction 用于记录复现）
        // 与 attach_checks（meta 附导出时刻的基础检查结果）。两个新参皆可缺省——
        // 不传时选取、排序、meta 其余字段与 #45 完全一致。
        let kind_name = match input.get("kind") {
            None | Some(Value::Null) => "diagnostics",
            Some(Value::String(text)) => match text.as_str() {
                "diagnostics" | "reproduction" => text.as_str(),
                _ => {
                    return Err(ApiError::new(
                        "invalid_input",
                        "kind 须为 diagnostics 或 reproduction",
                    ))
                }
            },
            Some(_) => {
                return Err(ApiError::new(
                    "invalid_input",
                    "kind 须为 diagnostics 或 reproduction",
                ))
            }
        };
        let attach_checks = match input.get("attach_checks") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            Some(_) => return Err(ApiError::new("invalid_input", "attach_checks 须为布尔值")),
        };

        // 事件选取与查询同口径：ts 不可解析的事件不进入导出（损坏行静默跳过）
        let event_millis =
            |event: &Value| -> Option<i64> {
                chrono::DateTime::parse_from_rfc3339(event["ts"].as_str()?)
                    .ok()
                    .map(|time| time.timestamp_millis())
            };
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).timestamp_millis();
        let mut selected: Vec<Value> = self
            .logger
            .read_events()
            .into_iter()
            .filter(|event| {
                let millis = match event_millis(event) {
                    Some(millis) => millis,
                    None => return false,
                };
                match window {
                    Window::Session => event["run_id"].as_str() == Some(run_id()),
                    Window::Recent => millis >= cutoff,
                    Window::All => true,
                    Window::Explicit(from, to) => millis >= from && millis <= to,
                }
            })
            .collect();
        // 稳定排序：同毫秒事件保持写入序
        selected.sort_by_key(|event| event_millis(event).unwrap_or(i64::MIN));
        let covered_from = selected.first().and_then(|e| e["ts"].as_str()).map(String::from);
        let covered_to = selected.last().and_then(|e| e["ts"].as_str()).map(String::from);
        let run_ids: std::collections::BTreeSet<String> = selected
            .iter()
            .filter_map(|event| event["run_id"].as_str().map(String::from))
            .collect();
        let mut level_counts = std::collections::BTreeMap::from([
            ("info", 0u64),
            ("debug", 0),
            ("trace", 0),
            ("error", 0),
        ]);
        for event in &selected {
            if let Some(slot) = event["level"].as_str().and_then(|level| level_counts.get_mut(level))
            {
                *slot += 1;
            }
        }
        let recent_errors: Vec<Value> = recent_errors_window(selected.iter());
        let mut meta = json!({
            "kind": kind_name,
            "schema_version": 1,
            "app_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "database_schema_version": crate::DATABASE_SCHEMA_VERSION,
            "exported_at": timestamp(),
            "scope": scope_name,
            "covered_from": covered_from,
            "covered_to": covered_to,
            "event_count": selected.len(),
            "run_ids": run_ids.into_iter().collect::<Vec<_>>(),
            "level_counts": level_counts,
            "recent_errors": recent_errors,
        });
        // attach_checks：附导出时刻的基础检查结果（记录复现停止导出时为记录时刻体检）
        if attach_checks {
            meta["checks"] = self.diagnostics_checks(json!({}))?;
        }

        let mut content = String::new();
        content.push_str(&meta.to_string());
        content.push('\n');
        for event in &selected {
            content.push_str(&event.to_string());
            content.push('\n');
        }
        std::fs::write(&path, content)
            .map_err(|_| ApiError::new("storage_error", "诊断导出失败：无法写入目标文件"))?;
        let bytes = std::fs::metadata(&path)
            .map(|meta| meta.len())
            .map_err(|_| ApiError::new("storage_error", "诊断导出失败：无法读取导出文件"))?;
        Ok(json!({
            "schema_version": 1,
            "path": path,
            "event_count": selected.len(),
            "bytes": bytes,
            "covered_from": covered_from,
            "covered_to": covered_to,
        }))
    }

    /// 健康摘要：只读投影——数据库/模式版本、本机安装代际、按状态任务计数、
    /// 供应商结果未知计数、最近错误、日志占用保留信息与丢弃计数。
    pub(super) fn diagnostics_summary(&self, _input: Value) -> Result<Value, ApiError> {
        // 按状态任务计数：全空间（诊断不经成员授权边界），未知状态不计入以保持 schema 严格
        let mut jobs_by_state = std::collections::BTreeMap::from([
            ("queued", 0u64),
            ("running", 0),
            ("waiting_input", 0),
            ("succeeded", 0),
            ("failed", 0),
            ("cancelled", 0),
        ]);
        {
            let mut statement = self.connection.prepare(
                "SELECT coalesce(json_extract(document,'$.state'),'queued') AS state, count(*) FROM jobs GROUP BY state",
            )?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let state: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                if let Some(slot) = jobs_by_state.get_mut(state.as_str()) {
                    *slot = count as u64;
                }
            }
        }
        // 供应商结果未知：waiting_input 且已提交（口径与恢复守卫的已提交谓词同源，jobs.rs）
        let provider_outcome_unknown: u64 = self.connection.query_row(
            "SELECT count(*) FROM jobs WHERE json_extract(document,'$.state')='waiting_input' AND (json_type(document,'$.submitted_at')='text' OR (kind='recognition' AND json_extract(document,'$.payload.submission') IN ('submitted','received','unknown')))",
            [],
            |row| row.get::<_, i64>(0).map(|count| count as u64),
        )?;
        let database_user_version: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let events = self.logger.read_events();
        let recent_errors: Vec<Value> = recent_errors_window(events.iter());
        let (total_bytes, file_count) = self.logger.occupancy();
        Ok(json!({
            "schema_version": 1,
            "database_schema_version": crate::DATABASE_SCHEMA_VERSION,
            "database_user_version": database_user_version,
            "installation_generation": self.installation_generation,
            "current_run_id": run_id(),
            "jobs_by_state": jobs_by_state,
            "provider_outcome_unknown": provider_outcome_unknown,
            "recent_errors": recent_errors,
            "logs": {
                "total_bytes": total_bytes,
                "file_count": file_count,
                "retention_days": self.logger.limits.retention_days,
                "max_total_bytes": self.logger.limits.max_total_bytes,
                "dropped_events": self.logger.dropped_events(),
            },
        }))
    }

    /// 基础检查（#48）：只读体检，返回固定十项（id、pass/warn/fail、通俗中文消息、
    /// 建议去向设置页标识、技术明细）。绝不修改配置或数据、不自动修复；
    /// 唯一的临时产物是试写探测文件，且无论成败都在返回前删除。
    pub(super) fn diagnostics_checks(&self, _input: Value) -> Result<Value, ApiError> {
        let data_dir = self
            .asset_root
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let db_path = PathBuf::from(self.connection.path().unwrap_or_default());
        let db_file = db_path.file_name().and_then(|name| name.to_str()).unwrap_or("");
        let mut checks: Vec<Value> = Vec::new();
        // 1. 数据目录解析与可写（解析失败在 Application 入口即被拒绝；此处验证可写与存在）
        checks.push(check_data_directory(&data_dir));
        // 2. 数据库可打开且模式版本与当前应用一致（打开失败在入口已拒绝；过高版本见 doctor 降级路径）
        let user_version: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        checks.push(match user_version.cmp(&crate::DATABASE_SCHEMA_VERSION) {
            std::cmp::Ordering::Equal => check_item(
                "database_schema",
                "pass",
                "数据库正常，模式版本与当前应用一致",
                None,
                &format!("user_version={user_version} required={}", crate::DATABASE_SCHEMA_VERSION),
            ),
            std::cmp::Ordering::Less => check_item(
                "database_schema",
                "warn",
                "数据库模式版本低于当前应用，下次打开时会自动升级",
                None,
                &format!("user_version={user_version} required={}", crate::DATABASE_SCHEMA_VERSION),
            ),
            std::cmp::Ordering::Greater => check_item(
                "database_schema",
                "fail",
                "数据库由更新版本的应用创建，请升级应用后再打开",
                None,
                &format!("user_version={user_version} required={}", crate::DATABASE_SCHEMA_VERSION),
            ),
        });
        // 3. WAL 生效（pragma 只读查询）
        let journal_mode: String = self
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        checks.push(if journal_mode == "wal" {
            check_item("database_wal", "pass", "数据库日志模式正常", None, "journal_mode=wal")
        } else {
            check_item(
                "database_wal",
                "warn",
                "数据库未启用 WAL 日志模式，可靠性与并发读取受限",
                None,
                &format!("journal_mode={journal_mode}"),
            )
        });
        // 4. 日志目录试写（写失败只降级诊断能力，不阻塞业务，故为 warn）
        checks.push(check_logs_writable(&data_dir));
        // 5. 数据卷剩余空间：fail < 100 MiB，warn < 1 GiB（初始阈值，与 ADR 语义一致）
        checks.push(check_disk_space(&data_dir));
        // 6. 本机安装代际一致（与其他进程并发恢复/更换后须重启加载）
        let generation: i64 = self.connection.query_row(
            "SELECT generation FROM local_installation_state WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        checks.push(if generation == self.installation_generation {
            check_item(
                "installation_generation",
                "pass",
                "本机安装状态正常",
                None,
                &format!("generation={generation}"),
            )
        } else {
            check_item(
                "installation_generation",
                "warn",
                "检测到数据在本机被恢复或更换，请重启应用后继续使用",
                None,
                &format!("opened_generation={} current_generation={generation}", self.installation_generation),
            )
        });
        // 7. 加密钱包状态：未配置 / 已解锁 / 已锁定（只报状态，绝不触碰密钥材料）
        checks.push(match crate::encryption::wallet_status(self) {
            crate::encryption::WalletStatus::NotConfigured => check_item(
                "encryption_wallet",
                "pass",
                "未启用加密；如需保护隐私，可到「加密与设备」开启",
                None,
                "wallet=not_configured",
            ),
            crate::encryption::WalletStatus::Unlocked => check_item(
                "encryption_wallet",
                "pass",
                "加密空间已解锁，运行正常",
                None,
                "wallet=unlocked",
            ),
            crate::encryption::WalletStatus::Locked => check_item(
                "encryption_wallet",
                "warn",
                "加密空间存在但本机无法解锁，需要恢复材料；请到「加密与设备」处理",
                Some("encryption"),
                "wallet=locked",
            ),
        });
        // 8. 模型凭据是否已配置（只报是/否计数；未配置模型属正常状态）
        checks.push(match self.credential_check_summary() {
            Ok((0, _, _, _)) => check_item(
                "model_credentials",
                "pass",
                "尚未配置模型；AI 功能需先到「模型与连接」设置",
                Some("model"),
                "providers=0",
            ),
            Ok((total, missing, local, false)) if missing == 0 => check_item(
                "model_credentials",
                "pass",
                "模型凭据已配置",
                None,
                &format!("providers={total} local_without_key={local}"),
            ),
            Ok((total, missing, local, false)) => check_item(
                "model_credentials",
                "warn",
                &format!("{missing} 个模型的 API 密钥未配置，相关功能暂不可用；请到「模型与连接」补全"),
                Some("model"),
                &format!("providers={total} credential_missing={missing} local_without_key={local}"),
            ),
            Ok((total, missing, local, true)) => check_item(
                "model_credentials",
                "warn",
                "系统安全存储不可用，无法确认模型密钥状态；请解锁系统钥匙串后重试",
                Some("model"),
                &format!("providers={total} credential_missing={missing} local_without_key={local} store_unavailable=true"),
            ),
            Err(error) => check_item(
                "model_credentials",
                "warn",
                "暂时无法读取模型配置",
                Some("model"),
                &format!("error_code={}", error.code),
            ),
        });
        // 9. 卡住任务：租约已过期仍处于 running 的持久任务计数（与 jobs 租约语义同源）
        let now = chrono::Utc::now().timestamp_millis();
        let stuck: i64 = self.connection.query_row(
            "SELECT count(*) FROM jobs WHERE json_extract(document,'$.state')='running' AND json_extract(document,'$.lease_expires') < ?1",
            [now],
            |row| row.get(0),
        )?;
        checks.push(if stuck == 0 {
            check_item("stuck_jobs", "pass", "没有卡住的任务", None, "stuck_jobs=0")
        } else {
            check_item(
                "stuck_jobs",
                "warn",
                &format!("{stuck} 个任务卡在执行中；请到「任务中心」重试或取消"),
                Some("tasks"),
                &format!("stuck_jobs={stuck}"),
            )
        });
        // 10. 待发布数据位置标记残留（.wardrobe-location.json phase=pending）
        checks.push(check_pending_marker(&data_dir, db_file));
        Ok(checks_result(checks))
    }
}

/// 单个检查项：id、pass/warn/fail、通俗中文消息、建议去向（设置页标识或 null）、技术明细。
fn check_item(id: &str, status: &str, message: &str, setting: Option<&str>, detail: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "message": message,
        "setting": setting.map(Value::from).unwrap_or(Value::Null),
        "detail": detail,
    })
}

/// 试写探测：新建唯一命名文件写入一字节后立即删除；探测绝不留下文件，也绝不改动既有内容。
fn probe_writable(dir: &Path) -> std::io::Result<()> {
    let probe = dir.join(format!(".wardrobe-check-{}.tmp", uuid::Uuid::now_v7()));
    let result = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
        .and_then(|mut file| file.write_all(b"ok"));
    let _ = fs::remove_file(&probe);
    result
}

/// 数据目录检查（Application 路径与 doctor 降级路径共用同一消息与结构）。
fn check_data_directory(data_dir: &Path) -> Value {
    match probe_writable(data_dir) {
        Ok(()) => check_item(
            "data_directory",
            "pass",
            "数据目录正常",
            None,
            &format!("dir={} writable=true", data_dir.display()),
        ),
        Err(error) => check_item(
            "data_directory",
            "fail",
            "数据目录无法写入，新数据可能无法保存，请到「备份与恢复」更换数据位置",
            Some("backup"),
            &format!("dir={} writable=false error={error}", data_dir.display()),
        ),
    }
}

/// 日志目录试写检查（共用）。
fn check_logs_writable(data_dir: &Path) -> Value {
    let logs_dir = data_dir.join("logs");
    match probe_writable(&logs_dir) {
        Ok(()) => check_item(
            "logs_writable",
            "pass",
            "日志记录正常",
            None,
            &format!("logs={} writable=true", logs_dir.display()),
        ),
        Err(error) => check_item(
            "logs_writable",
            "warn",
            "日志无法写入，问题排查能力受限；请到「备份与恢复」检查数据位置所在磁盘",
            Some("backup"),
            &format!("logs={} writable=false error={error}", logs_dir.display()),
        ),
    }
}

/// 数据卷剩余空间检查：fail < 100 MiB，warn < 1 GiB（初始阈值，与 ADR 语义一致）。
fn check_disk_space(data_dir: &Path) -> Value {
    match available_bytes(data_dir) {
        Some(free) if free < 100 * 1024 * 1024 => check_item(
            "disk_space",
            "fail",
            "磁盘空间严重不足（不足 100 MB），可能无法保存新数据，请清理或更换数据位置",
            Some("backup"),
            &format!("free_bytes={free}"),
        ),
        Some(free) if free < 1024 * 1024 * 1024 => check_item(
            "disk_space",
            "warn",
            "磁盘空间不足 1 GB，请尽快清理或到「备份与恢复」更换数据位置",
            Some("backup"),
            &format!("free_bytes={free}"),
        ),
        Some(free) => check_item(
            "disk_space",
            "pass",
            "磁盘空间充足",
            None,
            &format!("free_bytes={free}"),
        ),
        None => check_item(
            "disk_space",
            "warn",
            "暂时无法检测磁盘剩余空间",
            None,
            "free_space=unknown",
        ),
    }
}

/// 依赖数据库的检查项在降级路径下的占位：如实说明无法检查。
fn check_unavailable_without_database(id: &str) -> Value {
    check_item(
        id,
        "warn",
        "数据库未能打开，无法检查此项；请先按 database_schema 项的提示处理",
        None,
        "database_unavailable",
    )
}

/// 组装检查结果封套（Application 路径与降级路径共用同一 schema）。
fn checks_result(checks: Vec<Value>) -> Value {
    let counts = json!({
        "pass": checks.iter().filter(|c| c["status"] == "pass").count(),
        "warn": checks.iter().filter(|c| c["status"] == "warn").count(),
        "fail": checks.iter().filter(|c| c["status"] == "fail").count(),
    });
    json!({"schema_version": 1, "checks": checks, "counts": counts})
}

/// CLI `doctor` 的同源体检报告（#48）：正常路径直接执行 diagnostics.summary 与
/// diagnostics.checks（与 GUI 诊断页完全同一操作）；数据库无法打开（过高模式版本、
/// 位置切换锁占用、文件损坏等）时降级为文件系统级检查与 database_schema=fail——
/// 「打不开」正是最需要体检的场景。产物只读：除打开自身的既有迁移语义外无任何写入。
pub fn doctor_report(db_path: &Path) -> Value {
    match Application::open(db_path) {
        Ok(app) => {
            let summary = app.execute("diagnostics.summary", json!({})).ok();
            let checks = app.execute("diagnostics.checks", json!({})).ok();
            json!({
                "schema_version": 1,
                "error": Value::Null,
                "summary": summary,
                "checks": checks,
            })
        }
        Err(error) => json!({
            "schema_version": 1,
            "error": serde_json::to_value(&error).unwrap_or(Value::Null),
            "summary": Value::Null,
            "checks": checks_without_database(db_path, &error),
        }),
    }
}

/// doctor 降级路径：数据库打不开时仍给出十项体检——文件系统级四项照常真实检查，
/// database_schema 报 fail 并携带打开失败原因，其余依赖数据库的项如实报 warn。
fn checks_without_database(db_path: &Path, error: &crate::ApiError) -> Value {
    let data_dir = db_path.parent().unwrap_or(Path::new("."));
    let db_file = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let detail = match error.reason() {
        Some(reason) => format!(
            "open_failed code={} message={} reason={reason}",
            error.code, error.message
        ),
        None => format!("open_failed code={} message={}", error.code, error.message),
    };
    checks_result(vec![
        check_data_directory(data_dir),
        check_item(
            "database_schema",
            "fail",
            "数据库无法打开；请按技术明细中的原因处理（例如升级应用）",
            None,
            &detail,
        ),
        check_unavailable_without_database("database_wal"),
        check_logs_writable(data_dir),
        check_disk_space(data_dir),
        check_unavailable_without_database("installation_generation"),
        check_unavailable_without_database("encryption_wallet"),
        check_unavailable_without_database("model_credentials"),
        check_unavailable_without_database("stuck_jobs"),
        check_pending_marker(data_dir, db_file),
    ])
}

/// 数据卷剩余空间（fdp 需要读取目录本身，不做任何写入）；不可检测时返回 None。
#[cfg(unix)]
fn available_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let target = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut volume: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY：target 是合法的 NUL 结尾 C 字符串，volume 是有效的 statvfs 存储
    let result = unsafe { libc::statvfs(target.as_ptr(), &mut volume) };
    if result == 0 {
        Some(volume.f_bavail as u64 * volume.f_bsize as u64)
    } else {
        None
    }
}
#[cfg(not(unix))]
fn available_bytes(_path: &Path) -> Option<u64> {
    None
}

/// 待发布数据位置标记残留检查：与当前数据库同目录的 .wardrobe-location.json。
/// phase=active 是已完成切换的常态；phase=pending 且 source_file 与当前库不符为残留（warn），
/// 相符则说明切换未完成（fail，正常应已被打开时的恢复逻辑消费或因迁移锁拒绝打开）。
fn check_pending_marker(data_dir: &Path, db_file: &str) -> Value {
    let marker = data_dir.join(".wardrobe-location.json");
    if !marker.exists() {
        return check_item("pending_location_marker", "pass", "数据位置正常", None, "marker=none");
    }
    let parsed: Result<Value, String> = std::fs::File::open(&marker)
        .map_err(|error| format!("marker_read_error={error}"))
        .and_then(|file| {
            let mut bytes = Vec::new();
            file.take(16385)
                .read_to_end(&mut bytes)
                .map_err(|error| format!("marker_read_error={error}"))?;
            serde_json::from_slice(&bytes).map_err(|error| format!("marker_parse_error={error}"))
        });
    match parsed {
        Ok(record) if record["phase"] == "active" => check_item(
            "pending_location_marker",
            "pass",
            "数据位置正常",
            None,
            "marker=active",
        ),
        Ok(record) if record["phase"] == "pending" => {
            let matches_current = record["source_file"].as_str() == Some(db_file);
            let detail = format!(
                "marker=pending source_file={} matches_current={matches_current}",
                record["source_file"].as_str().unwrap_or(""),
            );
            if matches_current {
                check_item(
                    "pending_location_marker",
                    "fail",
                    "数据位置切换未完成，旧目录仍在等待发布；请到「备份与恢复」重新发起或确认数据位置",
                    Some("backup"),
                    &detail,
                )
            } else {
                check_item(
                    "pending_location_marker",
                    "warn",
                    "检测到未完成的数据位置切换标记残留；请到「备份与恢复」确认数据位置",
                    Some("backup"),
                    &detail,
                )
            }
        }
        Ok(record) => check_item(
            "pending_location_marker",
            "warn",
            "数据位置标记无法识别；请到「备份与恢复」确认数据位置",
            Some("backup"),
            &format!("marker=unknown phase={}", record["phase"]),
        ),
        Err(detail) => check_item(
            "pending_location_marker",
            "warn",
            "数据位置标记损坏或不可读；请到「备份与恢复」确认数据位置",
            Some("backup"),
            &detail,
        ),
    }
}


fn log_file_name(date: chrono::NaiveDate, seq: u32) -> String {
    if seq == 0 {
        format!("wardrobe-{date}.jsonl")
    } else {
        format!("wardrobe-{date}.{seq}.jsonl")
    }
}

fn active_path(logs_dir: &Path, date: chrono::NaiveDate, seq: u32) -> PathBuf {
    logs_dir.join(log_file_name(date, seq))
}

fn parse_log_file_name(name: &str) -> Option<(chrono::NaiveDate, u32)> {
    let rest = name.strip_prefix("wardrobe-")?.strip_suffix(".jsonl")?;
    let (date_text, seq) = match rest.split_once('.') {
        Some((date, seq)) => (date, seq.parse::<u32>().ok()?),
        None => (rest, 0),
    };
    Some((
        chrono::NaiveDate::parse_from_str(date_text, "%Y-%m-%d").ok()?,
        seq,
    ))
}

fn highest_seq(logs_dir: &Path, date: chrono::NaiveDate) -> u32 {
    let mut highest = 0;
    if let Ok(entries) = fs::read_dir(logs_dir) {
        for entry in entries.flatten() {
            if let Some((file_date, seq)) =
                parse_log_file_name(&entry.file_name().to_string_lossy())
            {
                if file_date == date && seq > highest {
                    highest = seq;
                }
            }
        }
    }
    highest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_failures_silently_increment_the_drop_counter() {
        // 父路径是文件，目录创建与写入必然失败；日志不得 panic 或阻塞
        let logger = Logger::new(Path::new("/dev/null/wardrobe"), LogLimits::default());
        // 若本测试是进程内首个 Logger，run_start 已经写入并计数；从当前基线起断言增量
        let before = logger.dropped_events();
        logger.log_event(Level::Info, "run_start", json!({"run_id": run_id()}));
        logger.log_operation(
            new_trace_id().as_str(),
            "space.default",
            Duration::from_millis(1),
            &[],
            &Ok(json!({})),
        );
        assert!(logger.dropped_events() > before, "写入失败必须计数");
    }

    #[test]
    fn log_file_names_parse_back_to_date_and_sequence() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 8).unwrap();
        assert_eq!(parse_log_file_name("wardrobe-2026-10-08.jsonl"), Some((date, 0)));
        assert_eq!(
            parse_log_file_name("wardrobe-2026-10-08.3.jsonl"),
            Some((date, 3))
        );
        assert_eq!(parse_log_file_name("wardrobe.sqlite"), None);
        assert_eq!(parse_log_file_name("other-2026-10-08.jsonl"), None);
    }

    #[test]
    fn log_limits_default_to_documented_caps() {
        let limits = LogLimits::default();
        assert_eq!(limits.max_file_bytes, 10 * 1024 * 1024);
        assert_eq!(limits.max_total_bytes, 256 * 1024 * 1024);
        assert_eq!(limits.retention_days, 14);
    }
}

/// 最近错误窗口投影：时间升序事件流中 level=error 的末尾 RECENT_ERRORS_LIMIT 条
/// （diagnostics.summary 与 diagnostics.export 同一口径）。
fn recent_errors_window<'a>(events: impl IntoIterator<Item = &'a Value>) -> Vec<Value> {
    let errors: Vec<&Value> = events
        .into_iter()
        .filter(|event| event["level"] == "error")
        .collect();
    let window_start = errors.len().saturating_sub(RECENT_ERRORS_LIMIT);
    errors[window_start..]
        .iter()
        .map(|event| recent_error_item(event))
        .collect()
}

/// RFC3339 时间输入字段的共用解析（logs.query 的 since/until 与 export 的 from/to）：
/// 缺省或 null 视为未提供；非字符串或不可解析报同一 invalid_input。
fn parse_rfc3339_field(
    input: &Value,
    key: &str,
) -> Result<Option<chrono::DateTime<chrono::FixedOffset>>, ApiError> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.as_str() {
            Some(text) => chrono::DateTime::parse_from_rfc3339(text)
                .map(Some)
                .map_err(|_| ApiError::new("invalid_input", &format!("{key} 须为 RFC3339 时间"))),
            None => Err(ApiError::new("invalid_input", &format!("{key} 须为 RFC3339 时间"))),
        },
    }
}

/// 最近错误条目的 outputSchema（摘要与查询共用同一投影形状）。
pub(crate) fn recent_error_schema() -> Value {
    json!({
        "type":"object",
        "required":["ts","level","event_type","run_id"],
        "properties":{
            "ts":{"type":"string","description":"RFC3339 毫秒 UTC"},
            "level":{"type":"string"},
            "event_type":{"type":"string"},
            "run_id":{"type":"string"},
            "trace_id":{"type":"string"},
            "operation":{"type":"string"},
            "result":{"type":"string","description":"错误码"},
            "reason":{"type":"string","description":"底层真实原因（仅本机可信入口可见）"},
            "message":{"type":"string","description":"panic 现场消息"}
        }
    })
}

fn summary_schema() -> Value {
    json!({
        "type":"object",
        "required":["schema_version","database_schema_version","database_user_version","installation_generation","current_run_id","jobs_by_state","provider_outcome_unknown","recent_errors","logs"],
        "properties":{
            "schema_version":{"type":"integer","const":1},
            "database_schema_version":{"type":"integer","description":"当前应用要求的数据库模式版本"},
            "database_user_version":{"type":"integer","description":"数据库实际模式版本"},
            "installation_generation":{"type":"integer","description":"本机安装代际"},
            "current_run_id":{"type":"string","description":"当前运行会话标识"},
            "jobs_by_state":{"type":"object","required":["queued","running","waiting_input","succeeded","failed","cancelled"],"properties":{"queued":{"type":"integer"},"running":{"type":"integer"},"waiting_input":{"type":"integer"},"succeeded":{"type":"integer"},"failed":{"type":"integer"},"cancelled":{"type":"integer"}},"additionalProperties":false},
            "provider_outcome_unknown":{"type":"integer","description":"已提交且结果未知的持久任务计数"},
            "recent_errors":{"type":"array","description":"最近错误（时间升序，窗口封顶）","items":recent_error_schema()},
            "logs":{"type":"object","required":["total_bytes","file_count","retention_days","max_total_bytes","dropped_events"],"properties":{"total_bytes":{"type":"integer","description":"logs/ 目录当前总占用字节"},"file_count":{"type":"integer"},"retention_days":{"type":"integer","description":"保留天数上限"},"max_total_bytes":{"type":"integer","description":"总占用上限字节"},"dropped_events":{"type":"integer","description":"写入失败静默丢弃的事件计数"}},"additionalProperties":false}
        }
    })
}

/// 基础检查的检查项与计数 outputSchema（GUI 诊断页与 CLI doctor 同一契约）。
fn checks_schema() -> Value {
    json!({
        "type":"object",
        "required":["schema_version","checks","counts"],
        "properties":{
            "schema_version":{"type":"integer","const":1},
            "checks":{"type":"array","description":"固定顺序的检查项（只读体检，不自动修复）","items":{
                "type":"object",
                "required":["id","status","message","setting","detail"],
                "properties":{
                    "id":{"type":"string","description":"检查项标识：data_directory/database_schema/database_wal/logs_writable/disk_space/installation_generation/encryption_wallet/model_credentials/stuck_jobs/pending_location_marker"},
                    "status":{"enum":["pass","warn","fail"]},
                    "message":{"type":"string","description":"通俗中文消息"},
                    "setting":{"type":["string","null"],"description":"warn/fail 项建议去向的设置页标识（model/tasks/backup/encryption/diagnostics）；null 表示无需去向"},
                    "detail":{"type":"string","description":"技术明细（按构造不含密钥材料）"}
                },
                "additionalProperties":false
            }},
            "counts":{"type":"object","required":["pass","warn","fail"],"properties":{"pass":{"type":"integer","minimum":0},"warn":{"type":"integer","minimum":0},"fail":{"type":"integer","minimum":0}},"additionalProperties":false}
        }
    })
}

pub(super) fn descriptors() -> Vec<Value> {
    vec![
        json!({
            "name":"diagnostics.summary",
            "description":"仅可信本机入口读取健康摘要：数据库模式版本、本机安装代际、按状态任务计数、供应商结果未知计数、最近错误、日志占用保留信息与丢弃计数",
            "effect":{"kind":"query","network":"none","local_files":true,"async":null},
            "examples":[{"request":{},"prerequisites":["仅可信本机入口可用；外部客户端授权永远无法获得 diagnostics 操作","只读投影：与持久任务实际状态和日志文件一致，不修改任何数据"]}],
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "outputSchema":summary_schema()
        }),
        json!({
            "name":"diagnostics.checks",
            "description":"仅可信本机入口运行基础检查（只读体检，与 CLI doctor 同源）：数据目录可写、数据库模式版本、WAL 生效、日志目录试写、数据卷剩余空间、本机安装代际、加密钱包状态（未配置/已锁定/已解锁）、模型凭据是否已配置（是/否）、卡住任务（租约过期仍 running）计数、待发布数据位置标记残留；不修改配置或数据，不自动修复",
            "effect":{"kind":"query","network":"none","local_files":true,"async":null},
            "examples":[{
                "request":{},
                "prerequisites":["仅可信本机入口可用；外部客户端授权永远无法获得 diagnostics 操作","只读检查：不修改任何配置或数据；唯一临时产物是试写探测文件且返回前删除","warn/fail 项的 setting 为建议去向设置页标识；技术明细按构造不含密钥材料"]
            }],
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "outputSchema":checks_schema()
        }),
        json!({
            "name":"diagnostics.logs.query",
            "description":"仅可信本机入口查询日志事件：按级别、时间范围、trace_id、run_id、操作前缀与关键词过滤，游标分页续页",
            "effect":{"kind":"query","network":"none","local_files":true,"async":null},
            "examples":[{
                "request":{"level":"error","trace_id":"<错误输出携带的链路标识>","limit":50},
                "prerequisites":["仅可信本机入口可用；外部客户端授权永远无法获得 diagnostics 操作","调用方须保留查询条件并以 next_cursor 续页；next_cursor 为 null 表示末页","事件按构造脱敏：不含密钥、令牌、恢复材料与空间密钥"]
            }],
            "inputSchema":{
                "type":"object",
                "additionalProperties":false,
                "properties":{
                    "level":{"enum":["info","debug","trace","error"],"description":"精确级别匹配"},
                    "since":{"type":"string","description":"RFC3339 起始时间（含）"},
                    "until":{"type":"string","description":"RFC3339 结束时间（含）"},
                    "trace_id":{"type":"string"},
                    "run_id":{"type":"string"},
                    "operation":{"type":"string","description":"操作名前缀"},
                    "keyword":{"type":"string","maxLength":200,"description":"事件文本子串（大小写不敏感）"},
                    "limit":{"type":"integer","minimum":1,"maximum":100},
                    "cursor":{"type":"string","description":"上一页 next_cursor；调用方须保留查询条件"}
                }
            },
            "outputSchema":{
                "type":"object",
                "required":["schema_version","items","next_cursor"],
                "properties":{
                    "schema_version":{"type":"integer","const":1},
                    "items":{"type":"array","description":"按写入时间序的日志事件","items":{"type":"object","required":["ts","level","event_type","run_id"],"properties":{"ts":{"type":"string"},"level":{"type":"string"},"event_type":{"type":"string"},"run_id":{"type":"string"},"trace_id":{"type":"string"},"operation":{"type":"string"},"duration_ms":{"type":"integer"},"result":{"type":"string"},"reason":{"type":"string"},"input_keys":{"type":"array","items":{"type":"string"}},"message":{"type":"string"},"location":{"type":"string"},"thread":{"type":"string"}}}},
                    "next_cursor":{"type":["string","null"],"description":"续页游标；null 表示末页"}
                }
            }
        }),
        json!({
            "name":"diagnostics.export",
            "description":"仅可信本机入口导出诊断文件到指定路径：单文件 JSONL，首行 meta（应用版本、操作系统、数据库模式版本、导出时间、覆盖时间范围、run_id 列表、按级别计数、最近错误摘要；kind=reproduction 时附基础检查结果），其后为按时间排序的日志事件；范围 session（本次启动）/recent（默认，最近 24 小时跨重启）/all（保留期内全部）或显式 from–to 起止时间窗（含端点，与 scope 互斥）",
            "effect":{"kind":"query","network":"none","local_files":true,"async":null},
            "examples":[{
                "request":{"path":"</绝对/本地/诊断文件>.jsonl","scope":"recent"},
                "prerequisites":["仅可信本机入口可用；外部客户端授权永远无法获得 diagnostics 操作","产物只来自日志文件与摘要投影：不含数据库、图片与凭据（脱敏与普通日志同一构造）","scope 缺省为 recent；显式 from/to 须成对提供且含端点","kind 缺省 diagnostics；kind=reproduction 与 attach_checks=true 用于记录复现的窗口导出","写入目标文件（已存在则覆盖）；保存对话框与 dev 回退路径由桌面端入口负责"]
            }],
            "inputSchema":{
                "type":"object",
                "additionalProperties":false,
                "required":["path"],
                "properties":{
                    "path":{"type":"string","minLength":1,"description":"导出目标文件路径（单文件 JSONL）"},
                    "scope":{"enum":["session","recent","all"],"description":"范围：本次启动 / 最近 24 小时跨重启（默认）/ 保留期内全部"},
                    "from":{"type":"string","description":"RFC3339 显式窗口起点（含）；须与 to 成对提供，与 scope 互斥"},
                    "to":{"type":"string","description":"RFC3339 显式窗口终点（含）；须与 from 成对提供，与 scope 互斥"},
                    "kind":{"enum":["diagnostics","reproduction"],"description":"产物 meta 种类；缺省 diagnostics；记录复现停止导出时为 reproduction"},
                    "attach_checks":{"type":"boolean","description":"true 时 meta 附带导出时刻的 diagnostics.checks 结果（记录复现用）；缺省 false"}
                }
            },
            "outputSchema":export_schema()
        }),
        json!({
            "name":"diagnostics.ui_event",
            "description":"仅可信本机入口把前端关键交互写入同一日志流形成用户操作时间线（记录复现期间由桌面端调用）：封闭白名单事件类型（view_switch 视图切换 / dialog_open 对话框打开 / dialog_submit 对话框提交）加简短界面文案标签；白名单外类型与超长标签拒绝且不落盘，不接受自由文本",
            "effect":{"kind":"query","network":"none","local_files":true,"async":null},
            "examples":[{
                "request":{"type":"view_switch","label":"穿搭库"},
                "prerequisites":["仅可信本机入口可用；外部客户端授权永远无法获得 diagnostics 操作","type 须为封闭白名单之一；label 须为 1–40 字符的前端固定界面文案（如「穿搭库」），不是用户输入内容","事件只含 type 与 label：脱敏按构造不变，不引入旁路"]
            }],
            "inputSchema":{
                "type":"object",
                "additionalProperties":false,
                "required":["type","label"],
                "properties":{
                    "type":{"enum":["view_switch","dialog_open","dialog_submit"],"description":"白名单事件类型"},
                    "label":{"type":"string","minLength":1,"maxLength":40,"description":"简短界面文案（前端固定文案，如「穿搭库」「记录穿着」）"}
                }
            },
            "outputSchema":{"type":"object","required":["schema_version"],"properties":{"schema_version":{"type":"integer","const":1}},"additionalProperties":false}
        }),
        json!({
            "name":"diagnostics.reproduction.start",
            "description":"仅可信本机入口写入复现记录的开始标记事件并返回标记时刻（记录复现流程由桌面端调用）；核心不持有录制状态，最长时长与自动停止由前端计时负责",
            "effect":{"kind":"query","network":"none","local_files":true,"async":null},
            "examples":[{
                "request":{},
                "prerequisites":["仅可信本机入口可用；外部客户端授权永远无法获得 diagnostics 操作","写入一条 event_type=reproduction、phase=start 的标记事件，返回的 started_at 与落盘 ts 一致，可作导出窗口起点（含端点）","不接受任何输入字段"]
            }],
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "outputSchema":{"type":"object","required":["schema_version","started_at"],"properties":{"schema_version":{"type":"integer","const":1},"started_at":{"type":"string","description":"开始标记事件落盘时刻（RFC3339，窗口起点含端点）"}},"additionalProperties":false}
        }),
        json!({
            "name":"diagnostics.reproduction.stop",
            "description":"仅可信本机入口写入复现记录的停止标记事件并返回标记时刻；随后桌面端以窗口起止为界调用 diagnostics.export（kind=reproduction、attach_checks=true）得到单文件复现记录",
            "effect":{"kind":"query","network":"none","local_files":true,"async":null},
            "examples":[{
                "request":{},
                "prerequisites":["仅可信本机入口可用；外部客户端授权永远无法获得 diagnostics 操作","写入一条 event_type=reproduction、phase=stop 的标记事件，返回的 stopped_at 与落盘 ts 一致，可作导出窗口终点（含端点）","不接受任何输入字段"]
            }],
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "outputSchema":{"type":"object","required":["schema_version","stopped_at"],"properties":{"schema_version":{"type":"integer","const":1},"stopped_at":{"type":"string","description":"停止标记事件落盘时刻（RFC3339，窗口终点含端点）"}},"additionalProperties":false}
        }),
    ]
}

/// diagnostics.export 的 outputSchema：路径与导出统计（事件数、字节、覆盖范围）；
/// meta 细节在产物文件首行，schema_version 与产物 meta 同为 1。
fn export_schema() -> Value {
    json!({
        "type":"object",
        "required":["schema_version","path","event_count","bytes","covered_from","covered_to"],
        "properties":{
            "schema_version":{"type":"integer","const":1},
            "path":{"type":"string","description":"导出目标文件路径"},
            "event_count":{"type":"integer","minimum":0,"description":"包含的日志事件数"},
            "bytes":{"type":"integer","minimum":0,"description":"导出文件字节数"},
            "covered_from":{"type":["string","null"],"description":"覆盖的最早事件时间（RFC3339）；无事件时为 null"},
            "covered_to":{"type":["string","null"],"description":"覆盖的最晚事件时间（RFC3339）；无事件时为 null"}
        }
    })
}
