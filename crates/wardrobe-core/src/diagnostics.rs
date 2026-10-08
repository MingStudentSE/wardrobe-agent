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
    io::Write,
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

    /// 丢弃计数：#43 经 diagnostics.summary 暴露；此前仅模块内测试使用。
    #[cfg_attr(not(test), allow(dead_code))]
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
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
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
