//! 诊断操作面：diagnostics.*（仅本机可信入口）与 CLI doctor 共享的检查实现。
//! 基础检查只读诊断、不自动修复；GUI 与 CLI 经同一套 run_checks/build_summary。

use crate::logging::{self, LogFilter, Logger};
use crate::{string, ApiError, Application, DATABASE_SCHEMA_VERSION};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const UI_EVENT_TYPES: [&str; 4] = ["view", "dialog_open", "action", "error"];

pub(super) fn dispatch(app: &Application, operation: &str, input: Value) -> Result<Value, ApiError> {
    match operation {
        "diagnostics.logs.query" => logs_query_operation(app, input),
        "diagnostics.summary" => summary_operation(app, input),
        "diagnostics.checks" => checks_operation(app, input),
        "diagnostics.export" => export_operation(app, input),
        "diagnostics.ui_event" => ui_event_operation(app, input),
        "diagnostics.reproduction.start" => reproduction_start(app, input),
        "diagnostics.reproduction.stop" => reproduction_stop(app, input),
        _ => Err(ApiError::new("unsupported_operation", "此操作尚不可用")),
    }
}

fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .unwrap()
        .keys()
        .any(|key| !allowed.contains(&key.as_str()))
    {
        return Err(ApiError::new("invalid_input", "操作包含不支持的字段"));
    }
    Ok(())
}

fn optional_string(input: &Value, key: &str) -> Result<Option<String>, ApiError> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(ApiError::new(
            "invalid_input",
            &format!("字段 {key} 须为字符串"),
        )),
    }
}

fn parse_time_ms(input: &Value, key: &str) -> Result<Option<i64>, ApiError> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => chrono::DateTime::parse_from_rfc3339(text)
            .map(|time| Some(time.timestamp_millis()))
            .map_err(|_| ApiError::new("invalid_input", &format!("时间 {key} 须为 RFC3339 格式"))),
        Some(_) => Err(ApiError::new("invalid_input", &format!("字段 {key} 须为字符串"))),
    }
}

fn logger(app: &Application) -> std::sync::Arc<Logger> {
    app.logger()
}

// ---------- diagnostics.logs.query ----------

fn parse_filter(input: &Value) -> Result<LogFilter, ApiError> {
    fields(
        input,
        &[
            "level",
            "since",
            "until",
            "trace_id",
            "run_id",
            "op_prefix",
            "contains",
            "cursor",
            "limit",
        ],
    )?;
    let limit = match input.get("limit") {
        None | Some(Value::Null) => 100usize,
        Some(Value::Number(number)) => {
            let value = number.as_u64().ok_or_else(|| {
                ApiError::new("invalid_input", "limit 须为 1–500 之间的整数")
            })?;
            if !(1..=500).contains(&value) {
                return Err(ApiError::new("invalid_input", "limit 须为 1–500 之间的整数"));
            }
            value as usize
        }
        Some(_) => return Err(ApiError::new("invalid_input", "limit 须为整数")),
    };
    let offset = match input.get("cursor") {
        None | Some(Value::Null) => 0,
        Some(Value::String(cursor)) => cursor
            .strip_prefix("v1:")
            .and_then(|rest| rest.parse::<usize>().ok())
            .ok_or_else(|| ApiError::new("invalid_input", "分页游标无效"))?,
        Some(_) => return Err(ApiError::new("invalid_input", "cursor 须为字符串")),
    };
    let level = match optional_string(input, "level")? {
        None => None,
        Some(level) => {
            if !["trace", "debug", "info", "warn", "error"].contains(&level.as_str()) {
                return Err(ApiError::new("invalid_input", "级别须为 trace/debug/info/warn/error"));
            }
            Some(level)
        }
    };
    Ok(LogFilter {
        level,
        min_level: None,
        since_ms: parse_time_ms(input, "since")?,
        until_ms: parse_time_ms(input, "until")?,
        trace_id: optional_string(input, "trace_id")?,
        run_id: optional_string(input, "run_id")?,
        op_prefix: optional_string(input, "op_prefix")?,
        contains: optional_string(input, "contains")?,
        limit,
        offset,
    })
}

fn logs_query_operation(app: &Application, input: Value) -> Result<Value, ApiError> {
    let filter = parse_filter(&input)?;
    Ok(query_result(app, filter))
}

/// CLI logs 与 GUI 共用的过滤查询；新→旧，cursor 续页。
pub fn query_result(app: &Application, filter: LogFilter) -> Value {
    let events = logging::read_events(&logger(app), &filter);
    build_query_output(events, filter)
}

/// CLI `logs` 子命令入口：不依赖数据库可用，直接读日志文件。
pub fn cli_logs_query(data_dir: &Path, input: Value) -> Result<Value, ApiError> {
    let filter = parse_filter(&input)?;
    let logger = logging::existing_logger(data_dir).unwrap_or_else(|| {
        std::sync::Arc::new(Logger::new(
            data_dir.join("logs"),
            logging::DiagnosticsConfig::default(),
        ))
    });
    let events = logging::read_events(&logger, &filter);
    Ok(build_query_output(events, filter))
}

fn build_query_output(events: Vec<Value>, filter: LogFilter) -> Value {
    let next_cursor = if events.len() == filter.limit.max(1) {
        Value::String(format!("v1:{}", filter.offset + events.len()))
    } else {
        Value::Null
    };
    json!({"schema_version":1,"items":events,"next_cursor":next_cursor})
}

// ---------- diagnostics.summary ----------

fn summary_operation(app: &Application, input: Value) -> Result<Value, ApiError> {
    fields(&input, &[])?;
    Ok(build_summary(Some(&app.connection), &logger(app)))
}

pub(crate) fn build_summary(conn: Option<&Connection>, logger: &Logger) -> Value {
    let (log_bytes, log_files, _) = logger.usage();
    let mut summary = json!({
        "schema_version":1,
        "run_id": logging::current_run(),
        "database": {"schema_version": DATABASE_SCHEMA_VERSION, "opened": conn.is_some()},
        "logs": {
            "total_bytes": log_bytes,
            "files": log_files,
            "max_total_bytes": logger.config().max_log_total_bytes,
            "retain_days": logger.config().log_retain_days,
            "dropped": logger.dropped(),
        },
    });
    if let Some(conn) = conn {
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);
        let generation: Option<i64> = conn
            .query_row(
                "SELECT generation FROM local_installation_state WHERE id=1",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap_or(None);
        let job_counts = job_state_counts(conn);
        let provider_unknown: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE json_extract(document,'$.error.code') IN ('submission_unknown','provider_outcome_unknown')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        summary["database"]["user_version"] = json!(version);
        summary["database"]["matches_application"] = json!(version == DATABASE_SCHEMA_VERSION);
        summary["installation_generation"] = json!(generation);
        summary["jobs"] = job_counts;
        summary["provider_unknown"] = json!(provider_unknown);
    } else {
        summary["database"]["user_version"] = Value::Null;
        summary["database"]["matches_application"] = Value::Null;
        summary["installation_generation"] = Value::Null;
        summary["jobs"] = json!({});
        summary["provider_unknown"] = Value::Null;
    }
    // 最近错误：读日志文件（数据库损坏也不影响这一投影）。
    // 最近错误：warn 及以上（失败操作与系统故障），读日志文件，数据库损坏也不影响。
    let errors = logging::read_events(
        logger,
        &LogFilter {
            min_level: Some(3),
            limit: 10,
            ..Default::default()
        },
    );
    summary["recent_errors"] = Value::Array(errors);
    summary
}

fn job_state_counts(conn: &Connection) -> Value {
    let mut counts = serde_json::Map::new();
    for state in ["queued", "running", "waiting_input", "succeeded", "failed", "cancelled"] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE json_extract(document,'$.state')=?1",
                [state],
                |row| row.get(0),
            )
            .unwrap_or(0);
        counts.insert(state.to_string(), json!(count));
    }
    Value::Object(counts)
}

// ---------- diagnostics.checks ----------

fn checks_operation(app: &Application, input: Value) -> Result<Value, ApiError> {
    fields(&input, &[])?;
    Ok(checks_result(
        Some(&app.connection),
        &app.data_dir(),
        Path::new(app.connection.path().unwrap_or("wardrobe.sqlite")),
    ))
}

/// CLI doctor 与 GUI checks 的唯一实现；conn 缺省时自行只读打开数据库。
pub fn checks_result(conn: Option<&Connection>, data_dir: &Path, db_path: &Path) -> Value {
    let items = run_checks(conn, data_dir, db_path);
    let generated_at = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    json!({"schema_version":1,"items":items,"generated_at":generated_at})
}

fn item(id: &str, status: &str, message: &str, hint: &str, detail: Value) -> Value {
    json!({"id":id,"status":status,"message":message,"hint":hint,"detail":detail})
}

/// CLI doctor：summary + checks 同源输出；数据库打不开时降级为 fail 检查项而非中止。
pub fn doctor_report(data_dir: &Path, db_path: &Path) -> Value {
    let opened = Application::open(db_path).ok();
    let (conn, logger) = match &opened {
        Some(app) => (Some(&app.connection), app.logger()),
        None => (
            None,
            logging::existing_logger(data_dir).unwrap_or_else(|| {
                std::sync::Arc::new(Logger::new(
                    data_dir.join("logs"),
                    logging::DiagnosticsConfig::default(),
                ))
            }),
        ),
    };
    json!({
        "schema_version":1,
        "summary": build_summary(conn, &logger),
        "checks": checks_result(conn, data_dir, db_path),
    })
}

pub(crate) fn run_checks(conn: Option<&Connection>, data_dir: &Path, db_path: &Path) -> Vec<Value> {
    let mut items = Vec::new();
    // 1. 数据目录可写
    let probe = data_dir.join(format!(".wardrobe-check-{}", uuid::Uuid::now_v7()));
    match std::fs::write(&probe, b"ok").and_then(|_| std::fs::remove_file(&probe)) {
        Ok(()) => items.push(item(
            "data_directory",
            "pass",
            "数据目录可以写入，衣橱资料读写正常。",
            "",
            json!({"directory": data_dir.to_string_lossy()}),
        )),
        Err(error) => items.push(item(
            "data_directory",
            "fail",
            "数据目录无法写入，新数据可能保存失败。",
            "请检查磁盘与文件夹权限，或在「备份与恢复」中更换数据位置。",
            json!({"directory": data_dir.to_string_lossy(), "error": error.to_string()}),
        )),
    }
    // 2/3. 数据库可打开、模式版本一致、WAL 生效
    let owned;
    let conn = match conn {
        Some(conn) => Some(conn),
        None => {
            owned = Connection::open_with_flags(
                db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .ok();
            owned.as_ref()
        }
    };
    match conn {
        Some(conn) => {
            let version: u32 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap_or(0);
            if version > DATABASE_SCHEMA_VERSION {
                items.push(item(
                    "database",
                    "fail",
                    "数据库由更新版本的应用创建，当前应用无法使用。",
                    "请升级衣橱应用后再打开。",
                    json!({"user_version": version, "application_version": DATABASE_SCHEMA_VERSION}),
                ));
            } else if version == DATABASE_SCHEMA_VERSION {
                items.push(item(
                    "database",
                    "pass",
                    "本地数据库可以打开，数据结构与当前应用一致。",
                    "",
                    json!({"user_version": version}),
                ));
            } else {
                items.push(item(
                    "database",
                    "warn",
                    "数据库结构比当前应用旧，下次打开时会自动升级。",
                    "正常使用即可，升级会自动完成。",
                    json!({"user_version": version, "application_version": DATABASE_SCHEMA_VERSION}),
                ));
            }
            let journal: String = conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap_or_default();
            if journal.eq_ignore_ascii_case("wal") {
                items.push(item(
                    "database_wal",
                    "pass",
                    "数据库已启用可靠的写入方式（WAL）。",
                    "",
                    json!({"journal_mode": journal}),
                ));
            } else {
                items.push(item(
                    "database_wal",
                    "warn",
                    "数据库未启用推荐的 WAL 写入方式。",
                    "重启应用通常会自动恢复；若反复出现请联系维护者。",
                    json!({"journal_mode": journal}),
                ));
            }
            // 6. 本机安装代际
            let generation: Option<i64> = conn
                .query_row(
                    "SELECT generation FROM local_installation_state WHERE id=1",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .unwrap_or(None);
            match generation {
                Some(_) => items.push(item(
                    "installation_generation",
                    "pass",
                    "本机安装信息完整。",
                    "",
                    json!({"generation": generation}),
                )),
                None => items.push(item(
                    "installation_generation",
                    "warn",
                    "本机安装信息缺失，可能来自不完整的恢复。",
                    "请在「备份与恢复」重新恢复一次数据。",
                    json!({"generation": null}),
                )),
            }
            // 7. 加密状态（不泄露任何密钥材料）
            let state: Option<String> = conn
                .query_row(
                    "SELECT document FROM encryption_state WHERE id=1",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .unwrap_or(None);
            match state {
                Some(_) => items.push(item(
                    "encryption",
                    "pass",
                    "已启用加密空间；加密状态由本机系统安全存储保护。",
                    "",
                    json!({"configured": true}),
                )),
                None => items.push(item(
                    "encryption",
                    "pass",
                    "尚未开启加密。衣橱资料以本机明文保存。",
                    "如需保护隐私，可在「加密与设备」中开启加密。",
                    json!({"configured": false}),
                )),
            }
            // 8. 模型凭据是否配置（只报是/否）
            let provider_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))
                .unwrap_or(0);
            if provider_count > 0 {
                items.push(item(
                    "model_credentials",
                    "pass",
                    "已配置模型连接，智能功能可以调用。",
                    "",
                    json!({"providers": provider_count}),
                ));
            } else {
                items.push(item(
                    "model_credentials",
                    "warn",
                    "还没有配置模型连接，AI 推荐与识别无法使用。",
                    "请到「模型与连接」添加模型。",
                    json!({"providers": 0}),
                ));
            }
            // 9. 卡住任务（租约过期仍 running）
            let now = chrono::Utc::now().timestamp_millis();
            let stuck: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM jobs WHERE json_extract(document,'$.state')='running' AND json_extract(document,'$.lease_expires') < ?1",
                    [now],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            if stuck == 0 {
                items.push(item(
                    "stuck_jobs",
                    "pass",
                    "没有卡住的后台任务。",
                    "",
                    json!({"stuck": 0}),
                ));
            } else {
                items.push(item(
                    "stuck_jobs",
                    "warn",
                    format!("有 {stuck} 个后台任务卡住了，等待中的工作不会被处理。").as_str(),
                    "请在「任务中心」重试或取消这些任务；也可以重启应用让任务接管。",
                    json!({"stuck": stuck}),
                ));
            }
        }
        None => {
            items.push(item(
                "database",
                "fail",
                "本地数据库无法打开，衣橱数据暂时不可用。",
                "请把诊断文件发给维护者排查；不要删除数据目录。",
                json!({"database": db_path.to_string_lossy()}),
            ));
            for (id, message, hint) in [
                ("database_wal", "数据库无法打开，写入方式无法检查。", "先解决数据库打开问题。"),
                ("installation_generation", "数据库无法打开，安装信息无法检查。", "先解决数据库打开问题。"),
                ("encryption", "数据库无法打开，加密状态无法检查。", "先解决数据库打开问题。"),
                ("model_credentials", "数据库无法打开，模型配置无法检查。", "先解决数据库打开问题。"),
                ("stuck_jobs", "数据库无法打开，任务状态无法检查。", "先解决数据库打开问题。"),
            ] {
                items.push(item(id, "warn", message, hint, json!({})));
            }
        }
    }
    // 4. 日志可写（试写）
    let logs_dir = data_dir.join("logs");
    let probe = logs_dir.join(format!(".check-{}", uuid::Uuid::now_v7()));
    match std::fs::write(&probe, b"ok").and_then(|_| std::fs::remove_file(&probe)) {
        Ok(()) => items.push(item(
            "logs",
            "pass",
            "日志可以正常记录。",
            "",
            json!({"directory": logs_dir.to_string_lossy()}),
        )),
        Err(error) => items.push(item(
            "logs",
            "warn",
            "日志无法写入，出问题时将没有记录可查。",
            "请检查磁盘空间与数据目录权限。",
            json!({"directory": logs_dir.to_string_lossy(), "error": error.to_string()}),
        )),
    }
    // 5. 磁盘剩余空间
    match free_space_bytes(data_dir) {
        Some(free) => {
            let (status, message, hint) = if free < 100 * 1024 * 1024 {
                (
                    "fail",
                    "磁盘空间几乎用尽，衣橱可能无法保存新数据。",
                    "请清理磁盘，或在「备份与恢复」中更换数据位置。",
                )
            } else if free < 2 * 1024 * 1024 * 1024 {
                (
                    "warn",
                    "磁盘剩余空间不足 2 GB，建议尽快清理。",
                    "请清理磁盘，或在「备份与恢复」中更换数据位置。",
                )
            } else {
                ("pass", "磁盘剩余空间充足。", "")
            };
            items.push(item(
                "disk_space",
                status,
                message,
                hint,
                json!({"free_bytes": free}),
            ));
        }
        None => items.push(item(
            "disk_space",
            "warn",
            "无法读取磁盘剩余空间。",
            "如遇保存失败，请检查磁盘。",
            json!({}),
        )),
    }
    // 10. 待发布数据位置标记残留
    let marker = data_dir.join(".wardrobe-location.json");
    if marker.exists() {
        let phase = std::fs::read_to_string(&marker)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|record| record["phase"].as_str().map(str::to_owned));
        if phase.as_deref() == Some("pending") {
            items.push(item(
                "pending_move_marker",
                "fail",
                "一次数据位置切换没有完成，数据可能在两个位置之间不一致。",
                "请把诊断文件发给维护者处理，不要手动删除标记文件。",
                json!({"phase": "pending"}),
            ));
        } else {
            items.push(item(
                "pending_move_marker",
                "pass",
                "数据位置切换记录正常。",
                "",
                json!({"phase": phase.unwrap_or_default()}),
            ));
        }
    } else {
        items.push(item(
            "pending_move_marker",
            "pass",
            "没有未完成的数据位置切换。",
            "",
            json!({}),
        ));
    }
    items
}

fn free_space_bytes(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let c_path = std::ffi::CString::new(path.to_str()?).ok()?;
        // SAFETY: statvfs 仅读取 c_path 并填充调用方提供的结构体。
        let result = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
        if result != 0 {
            return None;
        }
        Some(stat.f_bavail as u64 * stat.f_frsize as u64)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

// ---------- diagnostics.export ----------

fn export_operation(app: &Application, input: Value) -> Result<Value, ApiError> {
    fields(
        &input,
        &["path", "filename", "range", "from", "to", "kind"],
    )?;
    let range = optional_string(&input, "range")?.unwrap_or_else(|| "recent".to_string());
    if !["session", "recent", "all", "window"].contains(&range.as_str()) {
        return Err(ApiError::new(
            "invalid_input",
            "范围须为 session/recent/all/window",
        ));
    }
    let kind = optional_string(&input, "kind")?.unwrap_or_else(|| "diagnostics".to_string());
    if !["diagnostics", "reproduction"].contains(&kind.as_str()) {
        return Err(ApiError::new("invalid_input", "导出类型须为 diagnostics/reproduction"));
    }
    let destination = export_destination(app, &input)?;
    let mut extra = json!({"kind": kind});
    if kind == "reproduction" {
        extra["checks"] = checks_result(
            Some(&app.connection),
            &app.data_dir(),
            Path::new(app.connection.path().unwrap_or("wardrobe.sqlite")),
        )["items"].clone();
    }
    export_window(
        &logger(app),
        &range,
        parse_time_ms(&input, "from")?,
        parse_time_ms(&input, "to")?,
        extra,
        &destination,
    )
}

fn export_destination(app: &Application, input: &Value) -> Result<PathBuf, ApiError> {
    if let Some(path) = optional_string(input, "path")? {
        let path = PathBuf::from(&path);
        if !path.is_absolute() {
            return Err(ApiError::new("invalid_input", "导出路径须为绝对路径"));
        }
        return Ok(path);
    }
    let filename = optional_string(input, "filename")?
        .unwrap_or_else(logging::default_export_name);
    let filename = filename.replace(['/', '\\', ':'], "-");
    Ok(app.data_dir().join("exports").join(filename))
}

/// 范围解析与文件写出的共享实现（复现记录也走这里）。
pub(crate) fn export_window(
    logger: &std::sync::Arc<Logger>,
    range: &str,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    meta_extra: Value,
    destination: &Path,
) -> Result<Value, ApiError> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let (since_ms, until_ms) = match range {
        "session" => (None, None),
        "recent" => (Some(now_ms - 24 * 3600 * 1000), None),
        "all" => (None, None),
        _ => {
            if from_ms.is_none() || to_ms.is_none() {
                return Err(ApiError::new(
                    "invalid_input",
                    "显式时间窗需要 from 与 to（RFC3339）",
                ));
            }
            (from_ms, to_ms)
        }
    };
    // session 只取当前运行会话；recent/all/window 用时间（跨重启）。
    let events = {
        let session_run = if range == "session" {
            Some(logging::current_run())
        } else {
            None
        };
        collect_window(logger, since_ms, until_ms, session_run.as_deref())
    };
    logging::write_export(logger, events, meta_extra, destination)
}

fn collect_window(
    logger: &std::sync::Arc<Logger>,
    since_ms: Option<i64>,
    until_ms: Option<i64>,
    run_id: Option<&str>,
) -> Vec<Value> {
    logging::read_events(
        logger,
        &LogFilter {
            since_ms,
            until_ms,
            run_id: run_id.map(str::to_string),
            limit: usize::MAX,
            ..Default::default()
        },
    )
}

// ---------- diagnostics.ui_event ----------

fn ui_event_operation(app: &Application, input: Value) -> Result<Value, ApiError> {
    fields(&input, &["type", "label"])?;
    let event_type = string(&input, "type")?;
    if !UI_EVENT_TYPES.contains(&event_type) {
        return Err(ApiError::new(
            "invalid_input",
            "不支持的操作记录类型；仅接受 view/dialog_open/action/error",
        ));
    }
    let label = match optional_string(&input, "label")? {
        Some(label) => {
            let cleaned: String = label.chars().filter(|c| !c.is_control()).take(200).collect();
            if cleaned.is_empty() {
                return Err(ApiError::new("invalid_input", "操作记录标签不能为空"));
            }
            cleaned
        }
        None => String::new(),
    };
    logger(app).write_event(json!({
        "ts": chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "level": "info",
        "kind": "ui",
        "trace_id": logging::current_trace(),
        "op": format!("ui.{event_type}"),
        "result": "ok",
        "detail": {"label": label},
    }));
    Ok(json!({"schema_version":1,"recorded":true}))
}

// ---------- 记录复现 ----------

const REPRODUCTION_FILE: &str = "reproduction.json";

fn reproduction_path(app: &Application) -> PathBuf {
    app.data_dir().join("logs").join(REPRODUCTION_FILE)
}

fn reproduction_start(app: &Application, input: Value) -> Result<Value, ApiError> {
    fields(&input, &[])?;
    let path = reproduction_path(app);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|_| {
        ApiError::new("directory_io", "日志目录无法写入，复现记录未开始")
    })?;
    let config = logger(app).config();
    let started_at = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    std::fs::write(
        &path,
        json!({"started_at": started_at, "max_seconds": config.reproduction_max_seconds}).to_string(),
    )
    .map_err(|_| ApiError::new("directory_io", "复现记录状态无法保存，未开始"))?;
    logger(app).emit(
        "info",
        "reproduction",
        "diagnostics.reproduction.start",
        None,
        "ok",
        None,
        json!({"phase": "start", "max_seconds": config.reproduction_max_seconds}),
    );
    Ok(json!({"schema_version":1,"started_at":started_at,"max_seconds":config.reproduction_max_seconds}))
}

fn reproduction_stop(app: &Application, input: Value) -> Result<Value, ApiError> {
    fields(&input, &[])?;
    let path = reproduction_path(app);
    let raw = std::fs::read_to_string(&path).map_err(|_| {
        ApiError::new("invalid_state", "没有进行中的复现记录，请先开始记录")
    })?;
    let state: Value = serde_json::from_str(&raw)
        .map_err(|_| ApiError::new("invalid_state", "复现记录状态损坏，请重新开始记录"))?;
    let started_text = state["started_at"].as_str().ok_or_else(|| {
        ApiError::new("invalid_state", "复现记录状态损坏，请重新开始记录")
    })?;
    let started = chrono::DateTime::parse_from_rfc3339(started_text)
        .map_err(|_| ApiError::new("invalid_state", "复现记录开始时间损坏，请重新开始记录"))?;
    let max_seconds = state["max_seconds"].as_u64().unwrap_or(600);
    let now = chrono::Local::now();
    let started_local = started.with_timezone(&chrono::Local);
    let mut to = now;
    let mut auto_stopped = false;
    if (now.timestamp_millis() - started.timestamp_millis()) as f64 / 1000.0 > max_seconds as f64 {
        to = started_local + chrono::Duration::seconds(max_seconds as i64);
        auto_stopped = true;
    }
    let stopped_at = to.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    std::fs::remove_file(&path).ok();
    logger(app).emit(
        "info",
        "reproduction",
        "diagnostics.reproduction.stop",
        None,
        if auto_stopped { "auto_stopped" } else { "ok" },
        None,
        json!({"phase": "stop", "started_at": started_text, "stopped_at": stopped_at}),
    );
    let destination = app.data_dir().join("exports").join(format!(
        "复现记录-{}.jsonl",
        now.format("%Y%m%d-%H%M")
    ));
    let mut extra = json!({"kind": "reproduction"});
    extra["checks"] = checks_result(
        Some(&app.connection),
        &app.data_dir(),
        Path::new(app.connection.path().unwrap_or("wardrobe.sqlite")),
    )["items"].clone();
    extra["reproduction"] = json!({"started_at": started_text, "stopped_at": stopped_at, "auto_stopped": auto_stopped});
    let mut export = export_window(
        &logger(app),
        "window",
        Some(started.timestamp_millis()),
        Some(to.timestamp_millis()),
        extra,
        &destination,
    )?;
    export["schema_version"] = json!(1);
    export["started_at"] = json!(started_text);
    export["stopped_at"] = json!(stopped_at);
    export["auto_stopped"] = json!(auto_stopped);
    Ok(export)
}

// ---------- 描述符 ----------

fn filter_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"properties":{
        "level":{"enum":["trace","debug","info","warn","error"]},
        "since":{"type":"string","description":"RFC3339 起始时间"},
        "until":{"type":"string","description":"RFC3339 截止时间"},
        "trace_id":{"type":"string"},
        "run_id":{"type":"string"},
        "op_prefix":{"type":"string"},
        "contains":{"type":"string"},
        "cursor":{"type":"string"},
        "limit":{"type":"integer","minimum":1,"maximum":500}
    }})
}

fn log_event_schema() -> Value {
    json!({"type":"object","required":["ts","level","kind","run_id","op","result"],"properties":{
        "ts":{"type":"string"},"level":{"type":"string"},"kind":{"type":"string"},
        "run_id":{"type":"string"},"trace_id":{"type":"string"},"op":{"type":"string"},
        "result":{"type":"string"},"duration_ms":{"type":"integer"},"detail":{"type":"object"}
    }})
}

pub(super) fn descriptors() -> Vec<Value> {
    let query_output = json!({"type":"object","required":["schema_version","items","next_cursor"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":log_event_schema()},"next_cursor":{"type":["string","null"]}}});
    let summary_output = json!({"type":"object","required":["schema_version","run_id","database","logs","recent_errors"],"properties":{"schema_version":{"const":1},"run_id":{"type":"string"},"database":{"type":"object"},"installation_generation":{"type":["integer","null"]},"jobs":{"type":"object"},"provider_unknown":{"type":["integer","null"]},"logs":{"type":"object"},"recent_errors":{"type":"array","items":log_event_schema()}}});
    let check_item = json!({"type":"object","required":["id","status","message","hint","detail"],"properties":{"id":{"type":"string"},"status":{"enum":["pass","warn","fail"]},"message":{"type":"string"},"hint":{"type":"string"},"detail":{"type":"object"}}});
    let checks_output = json!({"type":"object","required":["schema_version","items","generated_at"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":check_item},"generated_at":{"type":"string"}}});
    let export_output = json!({"type":"object","required":["schema_version","path","events","bytes","from","to","kind"],"properties":{"schema_version":{"const":1},"path":{"type":"string"},"events":{"type":"integer"},"bytes":{"type":"integer"},"from":{"type":"string"},"to":{"type":"string"},"kind":{"type":"string"},"started_at":{"type":"string"},"stopped_at":{"type":"string"},"auto_stopped":{"type":"boolean"}}});
    vec![
        json!({"name":"diagnostics.logs.query","description":"仅可信本机入口按级别/时间/链路/操作过滤查询全链路日志","effect":{"kind":"query","network":"none","local_files":false,"async":null},"examples":[{"request":{"level":"error","limit":50},"prerequisites":["仅可信本机入口可用；读取数据目录 logs/ 下的 JSONL 日志文件","按时间倒序返回，cursor 续页"]}],"inputSchema":filter_schema(),"outputSchema":query_output}),
        json!({"name":"diagnostics.summary","description":"仅可信本机入口返回只读健康摘要（数据库/任务计数/最近错误/日志占用）","effect":{"kind":"query","network":"none","local_files":false,"async":null},"examples":[{"request":{},"prerequisites":["仅可信本机入口可用；只读投影，不修改任何数据"]}],"inputSchema":{"type":"object","additionalProperties":false,"properties":{}},"outputSchema":summary_output}),
        json!({"name":"diagnostics.checks","description":"仅可信本机入口运行基础检查清单（只读诊断，不自动修复）","effect":{"kind":"query","network":"none","local_files":false,"async":null},"examples":[{"request":{},"prerequisites":["仅可信本机入口可用；每项检查返回 pass/warn/fail、通俗说明与建议去向"]}],"inputSchema":{"type":"object","additionalProperties":false,"properties":{}},"outputSchema":checks_output}),
        json!({"name":"diagnostics.export","description":"仅可信本机入口导出单文件 JSONL 诊断（默认最近24小时，跨重启；不含数据库/图片/凭据）","effect":{"kind":"mutation","network":"none","local_files":true,"async":null},"examples":[{"request":{"range":"recent"},"prerequisites":["仅可信本机入口可用；path 为绝对路径时写到该位置，否则写入数据目录 exports/","产物首行为 meta（版本/系统/模式版本/时间范围/级别计数），其后为按时间排序的日志事件"]}],"inputSchema":{"type":"object","additionalProperties":false,"properties":{"path":{"type":"string"},"filename":{"type":"string"},"range":{"enum":["session","recent","all","window"]},"from":{"type":"string"},"to":{"type":"string"},"kind":{"enum":["diagnostics","reproduction"]}}},"outputSchema":export_output}),
        json!({"name":"diagnostics.ui_event","description":"仅可信本机入口把前端关键交互写入日志流，形成用户操作时间线","effect":{"kind":"mutation","network":"none","local_files":true,"async":null},"examples":[{"request":{"type":"view","label":"设置/诊断"},"prerequisites":["仅可信本机入口可用；只接受白名单类型 view/dialog_open/action/error 与 ≤200 字符标签"]}],"inputSchema":{"type":"object","required":["type"],"additionalProperties":false,"properties":{"type":{"enum":["view","dialog_open","action","error"]},"label":{"type":"string","maxLength":200}}},"outputSchema":{"type":"object","required":["schema_version","recorded"],"properties":{"schema_version":{"const":1},"recorded":{"const":true}}}}),
        json!({"name":"diagnostics.reproduction.start","description":"仅可信本机入口开始记录复现（写入起止标记，正常使用后停止并导出）","effect":{"kind":"mutation","network":"none","local_files":true,"async":null},"examples":[{"request":{},"prerequisites":["仅可信本机入口可用；记录不改变日志级别或脱敏行为","最长 10 分钟自动停止"]}],"inputSchema":{"type":"object","additionalProperties":false,"properties":{}},"outputSchema":{"type":"object","required":["schema_version","started_at","max_seconds"],"properties":{"schema_version":{"const":1},"started_at":{"type":"string"},"max_seconds":{"type":"integer"}}}}),
        json!({"name":"diagnostics.reproduction.stop","description":"仅可信本机入口停止复现记录并导出窗口内日志与操作时间线（meta 标记 kind=reproduction）","effect":{"kind":"mutation","network":"none","local_files":true,"async":null},"examples":[{"request":{},"prerequisites":["仅可信本机入口可用；须先开始记录","产物含窗口内全部日志事件与检查结果"]}],"inputSchema":{"type":"object","additionalProperties":false,"properties":{}},"outputSchema":export_output}),
    ]
}
