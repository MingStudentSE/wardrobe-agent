//! #41 全链路日志与诊断系统 — Application 级行为验收。
//! 只测外部行为：操作输入 → 日志文件 / diagnostics 输出 / 导出产物。

use serde_json::{json, Value};
use std::path::Path;
use wardrobe_core::{logging, Application};

fn read_all_events(data_dir: &Path) -> Vec<Value> {
    let logs = data_dir.join("logs");
    let mut events = Vec::new();
    let Ok(entries) = std::fs::read_dir(&logs) else {
        return events;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string());
            name.map(|n| n.starts_with("wardrobe-") && n.contains(".jsonl"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in text.lines() {
            if let Ok(event) = serde_json::from_str::<Value>(line) {
                events.push(event);
            }
        }
    }
    events
}

fn events_for_op(events: &[Value], op: &str) -> Vec<Value> {
    events
        .iter()
        .filter(|event| event["op"].as_str() == Some(op))
        .cloned()
        .collect()
}

fn open_default(data_dir: &Path) -> Application {
    Application::open(&data_dir.join("wardrobe.sqlite")).unwrap()
}

#[test]
fn every_api_call_leaves_a_trace_and_run_in_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let app = open_default(&data_dir);
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();
    let created = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"日志验收衬衫","category":"top"}),
        )
        .unwrap();
    app.execute("clothing.get", json!({"member_id":member,"id":created["id"]}))
        .unwrap();

    let events = read_all_events(&data_dir);
    let create_events = events_for_op(&events, "clothing.create");
    assert!(!create_events.is_empty(), "clothing.create 事件必须落盘");
    let event = &create_events[0];
    // trace_id 是 UUIDv7，run_id 非空，时间戳可解析，结果成功
    let trace = event["trace_id"].as_str().unwrap();
    assert_eq!(trace.len(), 36);
    assert!(!event["run_id"].as_str().unwrap().is_empty());
    chrono::DateTime::parse_from_rfc3339(event["ts"].as_str().unwrap()).unwrap();
    assert_eq!(event["result"], "ok");
    assert_eq!(event["kind"], "operation");
    // 同一次调用内 trace 一致；不同调用各有独立 trace
    let get_events = events_for_op(&events, "clothing.get");
    assert_eq!(get_events.len(), 1);
    assert_ne!(get_events[0]["trace_id"], create_events[0]["trace_id"]);
    // 同一进程内 run_id 一致
    let runs: std::collections::HashSet<_> = events
        .iter()
        .map(|event| event["run_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(runs.len(), 1, "同一进程所有事件共享一个 run_id");
}

#[test]
fn storage_failure_keeps_user_message_and_logs_real_reason() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let db = data_dir.join("wardrobe.sqlite");
    let app = open_default(&data_dir);
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();

    // 外部连接破坏表结构，触发真实的 rusqlite 错误
    {
        let saboteur = rusqlite::Connection::open(&db).unwrap();
        saboteur.execute_batch("DROP TABLE clothing;").unwrap();
    }
    let error = app
        .execute(
            "clothing.list",
            json!({"member_id":member}),
        )
        .unwrap_err();
    // 用户可见消息保持不变，附带 trace_id
    assert_eq!(error.code, "storage_error");
    assert_eq!(error.message, "本地数据库操作失败");
    let trace = error.trace_id.as_deref().expect("错误输出附带 trace_id");
    assert_eq!(trace.len(), 36);

    // 真实原因只写日志
    let events = read_all_events(&data_dir);
    let failure = events
        .iter()
        .find(|event| event["trace_id"].as_str() == Some(trace))
        .expect("失败操作事件按 trace 可查");
    assert_eq!(failure["result"], "storage_error");
    let reason = failure["detail"]["reason"].as_str().expect("日志保留真实原因");
    assert!(
        reason.to_lowercase().contains("no such table") || reason.to_lowercase().contains("no such column"),
        "真实原因应含 SQLite 细节，实际 {reason}"
    );
    let _ = member;
}

#[test]
fn secrets_never_reach_log_files_or_exports() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let app = open_default(&data_dir);
    app.execute("space.default", json!({})).unwrap();
    let grant = app
        .execute(
            "client.authorize",
            json!({"name":"脱敏验收","mode":"full","operations":[],"member_ids":null,"expires_at":null}),
        )
        .unwrap();
    let token = grant["token"].as_str().unwrap().to_string();
    let token_hash = {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(token.as_bytes());
        digest.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    // 泄漏高危的操作后再导出
    let error = app
        .execute_as_client(&token, "clothing.list", json!({"member_id":"default"}))
        .err();
    let _ = error;
    let export = app
        .execute(
            "diagnostics.export",
            json!({"path": data_dir.join("export-secret-check.jsonl")}),
        )
        .unwrap();
    assert!(export["events"].as_u64().unwrap() > 0);

    for artifact in [
        data_dir.join("export-secret-check.jsonl"),
    ] {
        let text = std::fs::read_to_string(&artifact).unwrap();
        assert!(!text.contains(&token), "导出产物不得包含连接令牌");
        assert!(!text.contains(&token_hash), "导出产物不得包含令牌hash");
    }
    let logs = collect_logs_text(&data_dir);
    assert!(!logs.contains(&token), "日志文件不得包含连接令牌");
    assert!(!logs.contains(&token_hash), "日志文件不得包含令牌hash");
}

fn collect_logs_text(data_dir: &Path) -> String {
    read_all_events(data_dir)
        .iter()
        .map(|event| event.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn logs_query_filters_by_level_trace_and_paginates() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let app = open_default(&data_dir);
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();
    app.execute("clothing.create", json!({"member_id":member,"name":"查询验收","category":"top"}))
        .unwrap();
    // 制造一个 warn（用户输入错误）与一个 error 级别事件（数据库故障）
    let invalid = app
        .execute("clothing.create", json!({"member_id":member,"name":" ","category":"top"}))
        .unwrap_err();
    assert_eq!(invalid.code, "invalid_input");
    let failed_trace = invalid.trace_id.clone().unwrap();

    let ok_trace = {
        let events = read_all_events(&data_dir);
        events_for_op(&events, "clothing.create")
            .iter()
            .find(|event| event["result"] == "ok")
            .unwrap()["trace_id"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // 按 trace 过滤
    let by_trace = app
        .execute(
            "diagnostics.logs.query",
            json!({"trace_id": ok_trace, "limit": 50}),
        )
        .unwrap();
    let items = by_trace["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(items
        .iter()
        .all(|event| event["trace_id"].as_str() == Some(ok_trace.as_str())));

    // 级别过滤：warn 命中 invalid_input，error 不含它
    let warns = app
        .execute("diagnostics.logs.query", json!({"level":"warn","limit":500}))
        .unwrap();
    assert!(warns["items"].as_array().unwrap().iter().any(|event| {
        event["trace_id"].as_str() == Some(failed_trace.as_str())
    }));
    let errors = app
        .execute("diagnostics.logs.query", json!({"level":"error","limit":500}))
        .unwrap();
    assert!(errors["items"].as_array().unwrap().iter().all(|event| event["level"] == "error"));

    // 游标续页：limit=1 逐页拿完且不重复
    let mut seen = Vec::new();
    let mut cursor = Value::Null;
    loop {
        let mut input = json!({"op_prefix":"clothing.","limit":1});
        if !cursor.is_null() {
            input["cursor"] = cursor.clone();
        }
        let page = app.execute("diagnostics.logs.query", input).unwrap();
        let items = page["items"].as_array().unwrap().clone();
        if items.is_empty() {
            break;
        }
        assert_eq!(items.len(), 1);
        seen.push(items[0].clone());
        cursor = page["next_cursor"].clone();
        if seen.len() > 20 {
            panic!("游标分页未收敛");
        }
        if cursor.is_null() {
            break;
        }
    }
    assert!(seen.len() >= 2, "应能翻页取得多条 clothing 事件");
    let ids: std::collections::HashSet<_> = seen
        .iter()
        .map(|event| event["trace_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), seen.len(), "分页不得重复或遗漏事件");
}

#[test]
fn export_ranges_cover_cross_run_window_with_meta_first_line() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    {
        let app = open_default(&data_dir);
        app.execute("space.default", json!({})).unwrap();
    }
    // 模拟重启：新的运行会话
    logging::begin_run();
    let app = open_default(&data_dir);
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();
    app.execute("clothing.create", json!({"member_id":member,"name":"导出验收","category":"top"}))
        .unwrap();

    let recent = app
        .execute("diagnostics.export", json!({"range":"recent","filename":"recent.jsonl"}))
        .unwrap();
    let recent_path = recent["path"].as_str().unwrap();
    assert!(recent_path.contains("exports"), "缺省导出到数据目录 exports/");
    let text = std::fs::read_to_string(recent_path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let meta: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(meta["kind"], "diagnostics");
    assert!(meta["app_version"].as_str().unwrap().contains('.'));
    assert_eq!(meta["database_schema_version"], wardrobe_core::DATABASE_SCHEMA_VERSION);
    assert!(meta["exported_at"].as_str().is_some());
    assert!(meta["counts_by_level"].as_object().is_some());
    // recent（24小时）跨重启：包含两个 run 的事件
    let runs = meta["run_ids"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "recent 导出应覆盖崩溃/重启前的上次运行");
    let event_lines = &lines[1..];
    assert!(event_lines.len() >= 3);
    let first_event: Value = serde_json::from_str(event_lines[0]).unwrap();
    assert!(first_event.get("run_id").is_some(), "事件行带 run_id");

    // session 只含当前 run
    let session = app
        .execute("diagnostics.export", json!({"range":"session","filename":"session.jsonl"}))
        .unwrap();
    let session_text = std::fs::read_to_string(session["path"].as_str().unwrap()).unwrap();
    let session_meta: Value = serde_json::from_str(session_text.lines().next().unwrap()).unwrap();
    assert_eq!(session_meta["run_ids"].as_array().unwrap().len(), 1);

    // 显式时间窗：只含窗口内事件
    let all_events = read_all_events(&data_dir);
    let first_ts = all_events
        .iter()
        .map(|event| event["ts"].as_str().unwrap())
        .min()
        .unwrap()
        .to_string();
    let window = app
        .execute(
            "diagnostics.export",
            json!({"range":"window","from":first_ts,"to":first_ts,"filename":"window.jsonl"}),
        )
        .unwrap();
    let window_text = std::fs::read_to_string(window["path"].as_str().unwrap()).unwrap();
    let window_events = window_text.lines().count() - 1;
    assert!(window_events < event_lines.len(), "时间窗导出小于全量");
    // 非法范围
    assert_eq!(
        app.execute("diagnostics.export", json!({"range":"today"}))
            .unwrap_err()
            .code,
        "invalid_input"
    );
    assert_eq!(
        app.execute("diagnostics.export", json!({"path":"relative/path.jsonl"}))
            .unwrap_err()
            .code,
        "invalid_input"
    );
}

#[test]
fn rotation_and_retention_caps_keep_logs_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let config = logging::DiagnosticsConfig {
        max_log_file_bytes: 600,
        max_log_total_bytes: 2600,
        log_retain_days: 0,
        reproduction_max_seconds: 600,
    };
    let app =
        Application::open_with(&data_dir.join("wardrobe.sqlite"), config.clone()).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();
    for index in 0..40 {
        app.execute(
            "clothing.create",
            json!({"member_id":member,"name":format!("轮转验收{index:03}"),"_pad":"x".repeat(40)}),
        )
        .unwrap_err();
    }
    // 注入过期旧文件与超大旧文件，驱动保留清理与总量封顶
    let logs = data_dir.join("logs");
    std::fs::write(logs.join("wardrobe-2020-01-01.jsonl"), "x\n".repeat(50)).unwrap();
    std::fs::write(logs.join("wardrobe-2020-01-02.jsonl"), "y".repeat(2048)).unwrap();
    // 重新打开应用：启动时惰性清理
    drop(app);
    let app = Application::open_with(&data_dir.join("wardrobe.sqlite"), config.clone()).unwrap();
    app.execute("space.default", json!({})).unwrap();

    let (total, files, _) = {
        let entries = std::fs::read_dir(&logs).unwrap();
        let mut total = 0;
        let mut names = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("wardrobe-") && name.contains(".jsonl") {
                total += entry.metadata().unwrap().len();
                names.push(name);
            }
        }
        (total, names.len(), names)
    };
    assert!(
        !Path::new(&logs.join("wardrobe-2020-01-01.jsonl")).exists(),
        "超过保留天数的旧日志应被清理"
    );
    assert!(
        total <= 2600 + 4096,
        "日志总占用应受封顶约束（当前 {total}，文件数 {files}）"
    );
    // 轮转确实发生：存在带时刻戳的轮转文件
    let rotated = std::fs::read_dir(&logs)
        .unwrap()
        .flatten()
        .any(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.starts_with("wardrobe-") && name.contains('T') && name.ends_with(".jsonl")
        });
    assert!(rotated, "超过单文件上限后应出现轮转文件");
}

#[test]
fn summary_matches_job_states_and_reports_log_usage() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let app = open_default(&data_dir);
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();
    // 制造 failed / waiting_input 任务
    let failed = app
        .job_enqueue(&member, "text_recommendation", "summary-failed", json!({"probe":1}))
        .unwrap();
    let claimed = app.job_claim(&member, failed["id"].as_str().unwrap(), 60).unwrap();
    app.job_finish(
        &member,
        failed["id"].as_str().unwrap(),
        claimed["lease_token"].as_str().unwrap(),
        "failed",
        Value::Null,
        json!({"code":"provider_network","retryable":false}),
    )
    .unwrap();
    let waiting = app
        .job_enqueue(&member, "text_recommendation", "summary-waiting", json!({"probe":2}))
        .unwrap();
    let claimed = app.job_claim(&member, waiting["id"].as_str().unwrap(), 60).unwrap();
    app.job_finish(
        &member,
        waiting["id"].as_str().unwrap(),
        claimed["lease_token"].as_str().unwrap(),
        "waiting_input",
        Value::Null,
        json!({"code":"submission_unknown","retryable":false}),
    )
    .unwrap();

    let summary = app.execute("diagnostics.summary", json!({})).unwrap();
    assert_eq!(summary["schema_version"], 1);
    assert!(!summary["run_id"].as_str().unwrap().is_empty());
    assert_eq!(summary["database"]["matches_application"], true);
    assert_eq!(summary["jobs"]["failed"], 1);
    assert_eq!(summary["jobs"]["waiting_input"], 1);
    assert_eq!(summary["provider_unknown"], 1);
    assert!(summary["logs"]["total_bytes"].as_u64().unwrap() > 0);
    assert!(summary["recent_errors"].is_array());
}

#[test]
fn checks_pass_in_healthy_install_and_flag_faults() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let app = open_default(&data_dir);
    app.execute("space.default", json!({})).unwrap();
    let checks = app.execute("diagnostics.checks", json!({})).unwrap();
    let items = checks["items"].as_array().unwrap();
    let ids: Vec<&str> = items.iter().map(|item| item["id"].as_str().unwrap()).collect();
    for expected in [
        "data_directory",
        "database",
        "database_wal",
        "logs",
        "disk_space",
        "installation_generation",
        "encryption",
        "model_credentials",
        "stuck_jobs",
        "pending_move_marker",
    ] {
        assert!(ids.contains(&expected), "缺少检查项 {expected}");
    }
    // 健康环境：无 fail
    let failing: Vec<_> = items
        .iter()
        .filter(|item| item["status"] == "fail")
        .collect();
    assert!(failing.is_empty(), "健康环境不应有 fail 项：{failing:?}");
    // 每项都有通俗中文说明
    for item in items {
        assert!(!item["message"].as_str().unwrap().trim().is_empty());
    }

    // 故障注入：过高的模式版本 → doctor 路径（无打开的 Application）报 fail
    let broken = dir.path().join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    {
        let fixture = rusqlite::Connection::open(broken.join("wardrobe.sqlite")).unwrap();
        fixture.execute_batch("PRAGMA user_version=99;").unwrap();
    }
    let report = wardrobe_core::diagnostics_doctor(&broken, &broken.join("wardrobe.sqlite"));
    let doctor_items = report["checks"]["items"].as_array().unwrap();
    let database = doctor_items
        .iter()
        .find(|item| item["id"] == "database")
        .unwrap();
    assert_eq!(database["status"], "fail");

    // 故障注入：租约过期的卡住任务
    let stuck_app_dir = dir.path().join("stuck");
    let stuck_app = Application::open(&stuck_app_dir.join("wardrobe.sqlite")).unwrap();
    stuck_app.execute("space.default", json!({})).unwrap();
    let space = stuck_app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();
    let job = stuck_app
        .job_enqueue(&member, "text_recommendation", "stuck-probe", json!({}))
        .unwrap();
    {
        let conn = rusqlite::Connection::open(&stuck_app_dir.join("wardrobe.sqlite")).unwrap();
        conn.execute(
            "UPDATE jobs SET document=json_set(document,'$.state','running','$.lease_expires',1) WHERE id=?1",
            [job["id"].as_str().unwrap()],
        )
        .unwrap();
    }
    let stuck_checks = stuck_app.execute("diagnostics.checks", json!({})).unwrap();
    let stuck = stuck_checks["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "stuck_jobs")
        .unwrap();
    assert_eq!(stuck["status"], "warn");
    assert!(!stuck["hint"].as_str().unwrap().is_empty(), "失败项必须给出去向");

    // 故障注入：待发布数据位置标记残留（迁移进行中：持有切换锁）
    let marker_dir = dir.path().join("marker");
    let marker_app = Application::open(&marker_dir.join("wardrobe.sqlite")).unwrap();
    std::fs::write(
        marker_dir.join(".wardrobe-location.json"),
        json!({"version":1,"phase":"pending","source_file":"wardrobe.sqlite","target_database":"/tmp/nope.sqlite"}).to_string(),
    )
    .unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(marker_dir.join(".wardrobe-move.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    // 持锁期间 Application 路径会报 data_directory_moving；doctor（无 Application）仍能体检
    assert_eq!(
        marker_app
            .execute("diagnostics.checks", json!({}))
            .unwrap_err()
            .code,
        "data_directory_moving"
    );
    let report = wardrobe_core::diagnostics_doctor(&marker_dir, &marker_dir.join("wardrobe.sqlite"));
    let marker = report["checks"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "pending_move_marker")
        .expect("doctor 报告含数据位置检查项");
    assert_eq!(marker["status"], "fail");
    drop(lock);
}

#[test]
fn reproduction_records_window_with_ui_timeline_and_auto_stops() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let app = open_default(&data_dir);
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();

    // 未开始就停止 → 明确报错
    assert_eq!(
        app.execute("diagnostics.reproduction.stop", json!({}))
            .unwrap_err()
            .code,
        "invalid_state"
    );
    let started = app.execute("diagnostics.reproduction.start", json!({})).unwrap();
    assert!(started["started_at"].as_str().is_some());
    // 白名单外的 ui_event 拒绝且不落盘
    assert_eq!(
        app.execute(
            "diagnostics.ui_event",
            json!({"type":"freeform","label":"不该记录"})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
    let before = read_all_events(&data_dir).len();
    // 记录窗口内的正常使用：视图切换 + 操作
    app.execute(
        "diagnostics.ui_event",
        json!({"type":"view","label":"设置/诊断"}),
    )
    .unwrap();
    app.execute(
        "clothing.create",
        json!({"member_id":member,"name":"复现验收单品","category":"top"}),
    )
    .unwrap();
    app.execute(
        "diagnostics.ui_event",
        json!({"type":"dialog_open","label":"新增衣物"}),
    )
    .unwrap();
    let stopped = app.execute("diagnostics.reproduction.stop", json!({})).unwrap();
    assert_eq!(stopped["auto_stopped"], false);
    assert!(stopped["started_at"].as_str().is_some());
    let path = stopped["path"].as_str().unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    let meta: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(meta["kind"], "reproduction");
    assert!(meta.get("checks").is_some(), "复现导出附检查结果");
    let window_events: Vec<Value> = text
        .lines()
        .skip(1)
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    // 窗口含操作时间线（ui 事件）与业务操作
    assert!(window_events
        .iter()
        .any(|event| event["op"] == "ui.view" && event["detail"]["label"] == "设置/诊断"));
    assert!(window_events.iter().any(|event| event["op"] == "ui.dialog_open"));
    assert!(window_events
        .iter()
        .any(|event| event["op"] == "clothing.create" && event["result"] == "ok"));
    // 复现开始标记在窗口内
    assert!(window_events
        .iter()
        .any(|event| event["op"] == "diagnostics.reproduction.start"));
    // 白名单外事件确实没落盘
    assert!(read_all_events(&data_dir)[before..]
        .iter()
        .all(|event| event["op"] != "ui.freeform"));

    // 注入 1 秒上限：停止时自动截断并标记
    let short_dir = dir.path().join("short");
    let config = logging::DiagnosticsConfig {
        reproduction_max_seconds: 1,
        ..logging::DiagnosticsConfig::default()
    };
    let short_app = Application::open_with(&short_dir.join("wardrobe.sqlite"), config).unwrap();
    short_app.execute("space.default", json!({})).unwrap();
    short_app.execute("diagnostics.reproduction.start", json!({})).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let auto = short_app.execute("diagnostics.reproduction.stop", json!({})).unwrap();
    assert_eq!(auto["auto_stopped"], true);
}

#[test]
fn external_client_authorization_never_reaches_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let app = open_default(&data_dir);
    app.execute("space.default", json!({})).unwrap();
    // full 授权也不能调用 diagnostics
    let full = app
        .execute(
            "client.authorize",
            json!({"name":"外部拒绝验收","mode":"full","operations":[],"member_ids":null,"expires_at":null}),
        )
        .unwrap();
    let token = full["token"].as_str().unwrap();
    for operation in [
        "diagnostics.summary",
        "diagnostics.checks",
        "diagnostics.logs.query",
        "diagnostics.export",
        "diagnostics.ui_event",
        "diagnostics.reproduction.start",
        "diagnostics.reproduction.stop",
    ] {
        let error = app.execute_as_client(token, operation, json!({})).unwrap_err();
        assert_eq!(error.code, "scope_denied", "{operation} 应拒绝外部客户端");
    }
    // 登记授权时 scope 含 diagnostics 直接拒绝
    let error = app
        .execute(
            "client.authorize",
            json!({"name":"越权scope","mode":"limited","operations":["diagnostics.summary"],"member_ids":null,"expires_at":null}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");
    // capabilities 对外部客户端不暴露 diagnostics 描述符
    let capabilities = app.execute_as_client(token, "capabilities", json!({})).unwrap();
    let names: Vec<&str> = capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| op["name"].as_str().unwrap())
        .collect();
    assert!(names.iter().all(|name| !name.starts_with("diagnostics.")));
}

#[test]
fn trace_flows_from_operation_into_job_worker_and_provider() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let app = open_default(&data_dir);
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();
    let provider = app
        .execute(
            "provider.save",
            json!({
                "name":"追踪验收模型",
                "endpoint":"http://127.0.0.1:9/v1",
                "model":"trace-test",
                "capabilities":["text"],
                "authentication":"none",
                "adapter":"openai_compatible"
            }),
        )
        .unwrap();
    let provider_id = provider["id"].as_str().unwrap().to_string();
    // 空衣橱会在候选生成前短路、不发起供应商请求；先放入可搭配的单品
    for (name, category) in [
        ("链路上衣", "top"),
        ("链路长裤", "bottom"),
        ("链路鞋子", "shoes"),
    ] {
        app.execute(
            "clothing.create",
            json!({"member_id":member,"name":name,"category":category}),
        )
        .unwrap();
    }
    let receipt = app
        .execute(
            "recommendation.generate",
            json!({
                "member_id":member,
                "operation_key":"trace-chain-1",
                "strategy":"ai",
                "provider_id":provider_id,
                "timezone":"Asia/Shanghai",
                "limit":1
            }),
        )
        .unwrap();
    let job_id = receipt["job_id"].as_str().unwrap().to_string();
    // 任务文档携带创建链路的 trace_id
    let create_trace = {
        let events = read_all_events(&data_dir);
        events_for_op(&events, "recommendation.generate")
            .last()
            .unwrap()["trace_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let job_doc = app
        .execute("jobs.get", json!({"member_id":member,"id":job_id}))
        .unwrap();
    assert_eq!(job_doc["trace_id"].as_str(), Some(create_trace.as_str()));

    // worker 运行（供应商 127.0.0.1:9 立即拒绝连接）→ 同一 trace 贯穿
    let worker = app
        .execute(
            "recommendation.worker.run",
            json!({"member_id":member,"id":job_id}),
        )
        .unwrap();
    // 供应商不可达 → 结果未知 → waiting_input（不盲重发的持久回执）
    assert_eq!(worker["state"], "waiting_input");

    let events = read_all_events(&data_dir);
    let chain: Vec<&Value> = events
        .iter()
        .filter(|event| event["trace_id"].as_str() == Some(create_trace.as_str()))
        .collect();
    let ops: Vec<&str> = chain.iter().map(|event| event["op"].as_str().unwrap()).collect();
    assert!(ops.contains(&"recommendation.generate"), "创建操作在链路上");
    assert!(ops.contains(&"job.enqueue"), "任务入队在链路上");
    assert!(ops.contains(&"job.claim"), "任务认领在链路上");
    assert!(ops.contains(&"job.finish"), "任务结束在链路上");
    assert!(
        ops.iter().any(|op| *op == "provider.chat"),
        "供应商请求在链路上：{ops:?}"
    );
    // 供应商事件记录模型名与结果类别，不含密钥
    let provider_event = chain
        .iter()
        .find(|event| event["op"] == "provider.chat")
        .unwrap();
    assert_eq!(provider_event["detail"]["model"], "trace-test");
    assert_eq!(provider_event["result"], "send_failed");
    assert!(provider_event["detail"].get("endpoint").is_none());
    // 任务结束记录结果未知与错误码
    let finish = chain.iter().find(|event| event["op"] == "job.finish").unwrap();
    assert_eq!(finish["result"], "waiting_input");
    assert_eq!(finish["detail"]["error_code"], "provider_outcome_unknown");
}
