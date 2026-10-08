//! 日志核心集成测试：只观察外部行为（执行操作后的 logs/ JSONL 文件与公开错误输出），
//! 不直接测 logger 内部 API。先例：application.rs、jobs.rs。

use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use wardrobe_core::Application;

/// 读取数据目录 logs/ 下全部 JSONL 事件（按构造只应存在 wardrobe-*.jsonl）。
fn log_events(dir: &std::path::Path) -> Vec<Value> {
    let mut events = Vec::new();
    for entry in std::fs::read_dir(dir.join("logs")).expect("logs 目录应存在") {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(
            name.starts_with("wardrobe-") && name.ends_with(".jsonl"),
            "日志目录只应包含按天命名的事件文件，实际 {name}"
        );
        let content = std::fs::read_to_string(entry.path()).unwrap();
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            events.push(serde_json::from_str(line).expect("每行必须是合法 JSON 事件"));
        }
    }
    events
}

#[test]
fn successful_operations_are_recorded_with_trace_run_and_duration() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();

    let events = log_events(dir.path());
    let mut operations: Vec<&Value> = events
        .iter()
        .filter(|e| e["event_type"] == "operation" && e["operation"] == "space.default")
        .collect();
    assert_eq!(operations.len(), 1, "每次调用恰好一条操作事件：{events:?}");
    let event = operations.pop().unwrap();
    assert_eq!(event["result"], "ok", "{event}");
    assert_eq!(event["level"], "info", "{event}");
    assert!(event["duration_ms"].is_number(), "{event}");
    // 时间戳是可解析的 RFC3339 UTC
    assert!(
        chrono::DateTime::parse_from_rfc3339(event["ts"].as_str().unwrap()).is_ok(),
        "{event}"
    );
    // trace_id 是 UUIDv7
    let trace = uuid::Uuid::parse_str(event["trace_id"].as_str().unwrap()).unwrap();
    assert_eq!(trace.get_version_num(), 7, "{event}");
    // run_id 每进程一致，且同 run 全部事件共享
    for e in &events {
        assert_eq!(e["run_id"], wardrobe_core::diagnostics::run_id(), "{e}");
    }
}

#[test]
fn each_call_gets_a_fresh_trace_id() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let traces: Vec<String> = log_events(dir.path())
        .iter()
        .filter(|e| e["event_type"] == "operation")
        .map(|e| e["trace_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(traces.len(), 2);
    assert_ne!(traces[0], traces[1], "每次调用分配独立 trace_id");
}

#[test]
fn failed_execute_errors_carry_trace_id_without_exposing_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let error = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":" ","category":"top"}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");
    // 用户可见错误消息语义不变；trace_id 为加法字段
    let public = serde_json::to_value(&error).unwrap();
    assert_eq!(public["code"], "invalid_input");
    assert_eq!(public["message"], "名称须为 1–120 个字符");
    assert!(public["trace_id"].is_string(), "{public}");
    assert!(public.get("reason").is_none(), "原因不得进入用户可见输出 {public}");
    // 日志记录失败事件与同一 trace_id
    let events = log_events(dir.path());
    let failures: Vec<&Value> = events
        .iter()
        .filter(|e| e["event_type"] == "operation" && e["result"] == "invalid_input")
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["trace_id"], public["trace_id"]);
    assert_eq!(failures[0]["level"], "error");
}

#[test]
fn open_failures_keep_message_but_preserve_the_real_reason_in_logs() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    std::fs::write(&db, b"this is definitely not sqlite").unwrap();
    let error = Application::open(&db).err().expect("打开损坏数据库应失败");
    // 用户可见错误保持既有错误码与中文消息
    assert_eq!(error.code, "storage_error");
    assert_eq!(error.message, "本地数据库操作失败");
    // 公开序列化不含原因，只含 code/message/trace_id
    let public = serde_json::to_value(&error).unwrap();
    let keys: Vec<&str> = public.as_object().unwrap().keys().map(String::as_str).collect();
    assert!(
        keys.iter().all(|k| ["code", "message", "trace_id"].contains(k)),
        "{public}"
    );
    // 底层真实原因保留并写入日志
    let reason = error.reason().expect("底层原因必须被保留");
    assert!(!reason.is_empty() && reason != error.message, "{reason}");
    let events = log_events(dir.path());
    let failures: Vec<&Value> = events
        .iter()
        .filter(|e| e["result"] == "storage_error")
        .collect();
    assert!(!failures.is_empty(), "打开失败也应留有日志：{events:?}");
    assert_eq!(failures[0]["reason"], json!(reason), "日志须保留底层真实原因");
}

#[test]
fn rusqlite_errors_no_longer_lose_the_underlying_cause() {
    // 直接钉住 From<rusqlite::Error> 的原因保留行为
    let error: wardrobe_core::ApiError = rusqlite::Error::QueryReturnedNoRows.into();
    assert_eq!(error.code, "storage_error");
    assert_eq!(error.message, "本地数据库操作失败");
    assert!(error.reason().is_some());
}

#[test]
fn connection_tokens_and_provider_keys_never_reach_the_log_files() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    // client.authorize：一次性连接令牌只在本次响应中出现
    let grant = app
        .execute(
            "client.authorize",
            json!({"name":"脱敏审计客户端","mode":"limited","operations":["clothing.list"],"member_ids":null,"expires_at":null}),
        )
        .unwrap();
    let token = grant["token"].as_str().unwrap();
    // 供应商密钥配置操作：输入携带密钥（此供应商无认证，操作在触碰钥匙串前拒绝，
    // 但密钥内容无论成败都绝不能进日志——脱敏按构造）
    let api_key = "sk-sensitive-9f8e7d6c5b4a3210";
    let rejected = app
        .execute(
            "provider.credential.set",
            json!({"id":"any-provider","api_key":api_key}),
        );
    assert!(rejected.is_err(), "无认证供应商应拒绝配置密钥");
    // 令牌 hash 同样不得出现（授权表里只存 hash，日志连 hash 都不该有）
    use sha2::{Digest, Sha256};
    let token_hash: String = Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut all_text = String::new();
    for entry in std::fs::read_dir(dir.path().join("logs")).unwrap() {
        all_text.push_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap());
    }
    assert!(!all_text.contains(token), "连接令牌不得写入日志");
    assert!(!all_text.contains(&token_hash), "连接令牌 hash 不得写入日志");
    assert!(!all_text.contains(api_key), "供应商密钥不得写入日志");
    // 事件仍完整：client.authorize 成功事件存在
    let events = log_events(dir.path());
    assert!(
        events
            .iter()
            .any(|e| e["operation"] == "client.authorize" && e["result"] == "ok"),
        "{events:?}"
    );
}

#[test]
fn tiny_file_limit_rotates_daily_files_without_losing_events() {
    let dir = tempfile::tempdir().unwrap();
    let app = wardrobe_core::Application::open_with_limits(
        &dir.path().join("wardrobe.sqlite"),
        wardrobe_core::diagnostics::LogLimits {
            max_file_bytes: 300,
            max_total_bytes: 1 << 30,
            retention_days: 14,
        },
    )
    .unwrap();
    for _ in 0..6 {
        app.execute("space.default", json!({})).unwrap();
    }
    let files: Vec<(String, u64)> = std::fs::read_dir(dir.path().join("logs"))
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            (name, std::fs::metadata(&path).unwrap().len())
        })
        .collect();
    assert!(files.len() >= 2, "极小上限应触发按文件轮转：{files:?}");
    for (name, size) in &files {
        assert!(
            *size <= 300,
            "轮转后每个文件都不得超过单文件上限：{name}={size}"
        );
    }
    // 轮转不丢事件：6 次调用全部可读
    let operations = log_events(dir.path())
        .iter()
        .filter(|e| e["event_type"] == "operation" && e["operation"] == "space.default")
        .count();
    assert_eq!(operations, 6);
}

#[test]
fn total_cap_and_retention_evict_old_files_keeping_recent_events() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    // 注入保留 0 天：昨日文件必须被保留清理删除
    let yesterday = (chrono::Local::now().date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d");
    std::fs::write(
        logs.join(format!("wardrobe-{yesterday}.jsonl")),
        "{\"event_type\":\"stale\"}\n",
    )
    .unwrap();
    let app = wardrobe_core::Application::open_with_limits(
        &dir.path().join("wardrobe.sqlite"),
        wardrobe_core::diagnostics::LogLimits {
            max_file_bytes: 300,
            max_total_bytes: 700,
            retention_days: 0,
        },
    )
    .unwrap();
    for _ in 0..8 {
        app.execute("space.default", json!({})).unwrap();
    }
    assert!(
        !logs
            .join(format!("wardrobe-{yesterday}.jsonl"))
            .exists(),
        "超过保留期的文件必须被清理"
    );
    let mut total = 0;
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&logs).unwrap() {
        let path = entry.unwrap().path();
        total += std::fs::metadata(&path).unwrap().len();
        names.push(path.file_name().unwrap().to_string_lossy().to_string());
    }
    assert!(
        total <= 700 + 300,
        "总占用须回落到上限附近（活动文件单行除外）：{total} {names:?}"
    );
    // 最新事件仍在（活动文件不被清理）
    let operations = log_events(dir.path())
        .iter()
        .filter(|e| e["event_type"] == "operation")
        .count();
    assert!(operations >= 1, "总量清理不得吞掉当前事件");
}

#[test]
fn read_only_log_directory_keeps_business_operations_succeeding() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let logs = dir.path().join("logs");
    let mut permissions = std::fs::metadata(&logs).unwrap().permissions();
    permissions.set_mode(0o555);
    std::fs::set_permissions(&logs, permissions).unwrap();
    // 日志目录只读：业务操作照常成功，日志写入静默降级
    let member = space["member_id"].as_str().unwrap();
    let created = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"只读日志衬衫","category":"top"}),
        )
        .unwrap();
    assert!(created["id"].is_string(), "业务操作不受日志故障影响");
    let mut permissions = std::fs::metadata(&logs).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&logs, permissions).unwrap();
}

#[test]
fn debug_level_adds_input_keys_while_default_info_stays_lean() {
    let quiet = tempfile::tempdir().unwrap();
    let app = Application::open(&quiet.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    app.execute(
        "clothing.create",
        json!({"member_id":"default","name":"级别衬衫","category":"top"}),
    )
    .unwrap();
    let events = log_events(quiet.path());
    let created = events
        .iter()
        .find(|e| e["operation"] == "clothing.create")
        .unwrap();
    assert!(created.get("input_keys").is_none(), "info 级别不含输入摘要：{created}");

    std::env::set_var("WARDROBE_LOG_LEVEL", "debug");
    let verbose_dir = tempfile::tempdir().unwrap();
    let verbose = Application::open(&verbose_dir.path().join("wardrobe.sqlite")).unwrap();
    verbose.execute("space.default", json!({})).unwrap();
    verbose
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"级别衬衫","category":"top"}),
        )
        .unwrap();
    std::env::remove_var("WARDROBE_LOG_LEVEL");
    let events = log_events(verbose_dir.path());
    let created = events
        .iter()
        .find(|e| e["operation"] == "clothing.create")
        .unwrap();
    let keys: Vec<&str> = created["input_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        keys.contains(&"member_id") && keys.contains(&"name") && keys.contains(&"category"),
        "debug 级别记录输入字段名（仅字段名，不含值）：{created}"
    );
}
