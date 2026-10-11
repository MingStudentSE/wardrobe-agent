//! 日志核心集成测试：只观察外部行为（执行操作后的 logs/ JSONL 文件与公开错误输出），
//! 不直接测 logger 内部 API。先例：application.rs、jobs.rs。

use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex, MutexGuard};
use wardrobe_core::{operations, Application};

/// panic 钩子是进程级的：触发时向本进程注册过的全部日志汇写 panic 事件，
/// 进而驱动其他测试 logger 的轮转与惰性清理（删旧文件）。用进程内互斥把
/// "会触发 panic 写入"的测试与"对 logs/ 做文件断言"的测试隔开，
/// 保证 `--test-threads` 并行下确定性（#42）。
static LOG_FS_LOCK: Mutex<()> = Mutex::new(());
fn fs_lock() -> MutexGuard<'static, ()> {
    LOG_FS_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 文件在列出后恰好被并发清理删掉：视为"已被清理"，跳过而非报错。
/// 与源码 `cleanup`/`scan` 对 NotFound 的容忍同语义（#42，CLI 与 GUI 两进程共享
/// 同一 logs 目录时是真实场景）。
fn stat_or_cleaned(path: &std::path::Path) -> Option<u64> {
    match std::fs::metadata(path) {
        Ok(meta) => Some(meta.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("stat {path:?} 失败：{error}"),
    }
}

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
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(content) => content,
            // 列出后被并发清理删掉：视为"已被清理"，跳过
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("读取 {:?} 失败：{error}", entry.path()),
        };
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
    assert!(
        public.get("reason").is_none(),
        "原因不得进入用户可见输出 {public}"
    );
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
    let keys: Vec<&str> = public
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert!(
        keys.iter()
            .all(|k| ["code", "message", "trace_id"].contains(k)),
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
    assert_eq!(
        failures[0]["reason"],
        json!(reason),
        "日志须保留底层真实原因"
    );
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
    let rejected = app.execute(
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
    assert!(
        !all_text.contains(&token_hash),
        "连接令牌 hash 不得写入日志"
    );
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
    // 本测试的 logger 上限极小，其他测试触发的 panic 钩子写入会驱动它清理；
    // 与触发 panic 写入的测试互斥，避免并发断言文件时被删（#42）
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    // 注入保留 0 天：昨日文件必须被保留清理删除
    let yesterday =
        (chrono::Local::now().date_naive() - chrono::Duration::days(1)).format("%Y-%m-%d");
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
        !logs.join(format!("wardrobe-{yesterday}.jsonl")).exists(),
        "超过保留期的文件必须被清理"
    );
    let mut total = 0;
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&logs).unwrap() {
        let path = entry.unwrap().path();
        // 并发清理者可能恰在此刻删掉该文件：视为"已被清理"，跳过
        let Some(size) = stat_or_cleaned(&path) else {
            continue;
        };
        total += size;
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
    assert!(
        created.get("input_keys").is_none(),
        "info 级别不含输入摘要：{created}"
    );

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

/// #42 竞态回归现场：panic 钩子是进程级的，触发时向进程内全部日志汇写 panic 事件，
/// 驱动极小上限 logger 的轮转与惰性清理（删旧文件）；与外部观察者的
/// `read_dir → metadata` 断言循环并发时构成 TOCTOU。本测试把"并发清理者"与
/// "文件观察者"放进同一个测试的两个线程，确定性复现该交错。
#[test]
fn concurrent_cleanup_keeps_observers_and_business_running() {
    // 触发 panic 钩子（向进程内全部日志汇写入并驱动清理）期间，
    // 不得与其他测试的文件断言并发（#42）
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    // 注入保留 0 天：清理者除总量驱逐外还会持续删过期文件，制造高频删除
    let yesterday =
        (chrono::Local::now().date_naive() - chrono::Duration::days(1)).format("%Y-%m-%d");
    std::fs::write(
        logs.join(format!("wardrobe-{yesterday}.jsonl")),
        "{\"event_type\":\"stale\"}\n",
    )
    .unwrap();
    let app = wardrobe_core::Application::open_with_limits(
        &dir.path().join("wardrobe.sqlite"),
        wardrobe_core::diagnostics::LogLimits {
            max_file_bytes: 300,
            max_total_bytes: 500,
            retention_days: 0,
        },
    )
    .unwrap();
    wardrobe_core::diagnostics::install_panic_hook();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let scanner = {
        let logs = logs.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut seen_bytes = 0u64;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                for entry in std::fs::read_dir(&logs).unwrap() {
                    let path = entry.unwrap().path();
                    let name = path.file_name().unwrap().to_string_lossy().to_string();
                    assert!(
                        name.starts_with("wardrobe-") && name.ends_with(".jsonl"),
                        "日志目录只应包含按天命名的事件文件，实际 {name}"
                    );
                    // 与 total_cap 断言循环同型的观察：文件被并发清理删掉即 NotFound，
                    // 视为"已被清理"跳过，不得报错（#42 修复的验收口径）
                    let Some(size) = stat_or_cleaned(&path) else {
                        continue;
                    };
                    seen_bytes += size;
                    match std::fs::read_to_string(&path) {
                        Ok(_) => {}
                        // stat 与 read 之间被并发清理删掉：同样视为"已被清理"
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(error) => panic!("读取 {path:?} 失败：{error}"),
                    }
                }
            }
            seen_bytes
        })
    };
    let marker = "并发清理竞态标记-4c2e";
    for _ in 0..80 {
        // 触发 panic 钩子：向本进程全部日志汇（含本测试的极小上限 logger）写 error 事件
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("{marker}");
        }));
        app.execute("space.default", json!({})).unwrap();
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    scanner.join().expect("观察循环不得因并发清理而报错");
    // 过期文件已被清理删除
    assert!(
        !logs.join(format!("wardrobe-{yesterday}.jsonl")).exists(),
        "超过保留期的文件必须被清理"
    );
    // 业务操作全部成功（上方 unwrap 已证），且最新事件仍在（活动文件不被清理）
    let operations = log_events(dir.path())
        .iter()
        .filter(|e| e["event_type"] == "operation")
        .count();
    assert!(operations >= 1, "总量清理不得吞掉当前事件");
}

/// 把一个任务置为「供应商结果未知」的持久形态（waiting_input + submitted_at），
/// 与真实流程落盘的文档形状一致（先例：ai_recommendations.rs 的 due_clock_seam 直连 SQL 接缝）。
fn mark_provider_outcome_unknown(db: &std::path::Path, id: &str) {
    let connection = rusqlite::Connection::open(db).unwrap();
    let raw: String = connection
        .query_row("SELECT document FROM jobs WHERE id=?1", [id], |r| r.get(0))
        .unwrap();
    let mut job: Value = serde_json::from_str(&raw).unwrap();
    job["state"] = json!("waiting_input");
    job["submitted_at"] = json!(chrono::Utc::now().to_rfc3339());
    job["error"] = json!({"code":"provider_outcome_unknown","retryable":false});
    connection
        .execute(
            "UPDATE jobs SET document=?1 WHERE id=?2",
            rusqlite::params![job.to_string(), id],
        )
        .unwrap();
}

#[test]
fn diagnostics_summary_matches_persisted_job_states_and_reports_log_health() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&db).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    // 七个任务覆盖全部状态：queued / running / failed / waiting_input / succeeded / cancelled
    // /供应商结果未知
    app.job_enqueue(&member, "local-test", "s-q", json!({}))
        .unwrap();
    let running = app
        .job_enqueue(&member, "local-test", "s-r", json!({}))
        .unwrap();
    app.job_claim(&member, running["id"].as_str().unwrap(), 30)
        .unwrap();
    let failed = app
        .job_enqueue(&member, "local-test", "s-f", json!({}))
        .unwrap();
    let lease = app
        .job_claim(&member, failed["id"].as_str().unwrap(), 30)
        .unwrap();
    app.job_finish(
        &member,
        failed["id"].as_str().unwrap(),
        lease["lease_token"].as_str().unwrap(),
        "failed",
        Value::Null,
        json!({"code":"permanent","retryable":false}),
    )
    .unwrap();
    // 本地待输入（无提交）：waiting_input 但不计入供应商结果未知
    let waiting = app
        .job_enqueue(&member, "local-test", "s-w", json!({}))
        .unwrap();
    let lease = app
        .job_claim(&member, waiting["id"].as_str().unwrap(), 30)
        .unwrap();
    app.job_finish(
        &member,
        waiting["id"].as_str().unwrap(),
        lease["lease_token"].as_str().unwrap(),
        "waiting_input",
        json!({"preview":"local"}),
        Value::Null,
    )
    .unwrap();
    let done = app
        .job_enqueue(&member, "local-test", "s-s", json!({}))
        .unwrap();
    let lease = app
        .job_claim(&member, done["id"].as_str().unwrap(), 30)
        .unwrap();
    app.job_finish(
        &member,
        done["id"].as_str().unwrap(),
        lease["lease_token"].as_str().unwrap(),
        "succeeded",
        json!({}),
        Value::Null,
    )
    .unwrap();
    let cancelled = app
        .job_enqueue(&member, "local-test", "s-c", json!({}))
        .unwrap();
    app.execute(
        "jobs.cancel",
        json!({"member_id":member,"id":cancelled["id"]}),
    )
    .unwrap();
    let unknown = app
        .job_enqueue(&member, "recognition", "s-u", json!({}))
        .unwrap();
    mark_provider_outcome_unknown(&db, unknown["id"].as_str().unwrap());
    // 一次失败操作：健康摘要的最近错误须能反映它
    let failed_call = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":" ","category":"top"}),
        )
        .unwrap_err();

    let summary = app.execute("diagnostics.summary", json!({})).unwrap();
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(
        summary["database_schema_version"],
        json!(wardrobe_core::DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        summary["database_user_version"],
        json!(wardrobe_core::DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(summary["installation_generation"], 0, "全新安装代际为 0");
    assert_eq!(
        summary["current_run_id"],
        wardrobe_core::diagnostics::run_id()
    );
    assert_eq!(
        summary["jobs_by_state"],
        json!({"queued":1,"running":1,"waiting_input":2,"succeeded":1,"failed":1,"cancelled":1}),
        "按状态计数须与持久任务实际状态一致：{summary}"
    );
    assert_eq!(
        summary["provider_outcome_unknown"], 1,
        "仅已提交且结果未知的 waiting_input 计入：{summary}"
    );
    // 一致性不变量：对持久任务文档逐个套领域谓词重算，与摘要计数一致
    let listed = app
        .execute("jobs.list", json!({"member_id":member}))
        .unwrap();
    let derived = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|job| {
            job["state"] == "waiting_input"
                && (job["submitted_at"].is_string()
                    || (job["kind"] == "recognition"
                        && ["submitted", "received", "unknown"]
                            .contains(&job["payload"]["submission"].as_str().unwrap_or(""))))
        })
        .count();
    assert_eq!(summary["provider_outcome_unknown"], json!(derived));

    // 最近错误：只含 error 级事件、时间升序、带 trace_id 与错误码
    let errors = summary["recent_errors"].as_array().unwrap();
    assert!(
        errors.iter().all(|e| e["level"] == "error"),
        "最近错误只收 error 级事件：{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e["operation"] == "clothing.create"
            && e["result"] == "invalid_input"
            && e["trace_id"].as_str() == failed_call.trace_id.as_deref()),
        "失败操作须出现在最近错误中：{errors:?}"
    );
    let stamps: Vec<i64> = errors
        .iter()
        .map(|e| {
            chrono::DateTime::parse_from_rfc3339(e["ts"].as_str().unwrap())
                .unwrap()
                .timestamp_millis()
        })
        .collect();
    let mut sorted = stamps.clone();
    sorted.sort();
    assert_eq!(stamps, sorted, "最近错误按时间升序");
    assert!(errors.len() <= 20, "最近错误有窗口上限");

    // 日志占用与保留信息、丢弃计数
    let logs = &summary["logs"];
    assert!(logs["total_bytes"].as_u64().unwrap() > 0, "{summary}");
    assert!(logs["file_count"].as_u64().unwrap() >= 1, "{summary}");
    assert_eq!(logs["retention_days"], 14);
    assert_eq!(logs["max_total_bytes"], json!(256 * 1024 * 1024));
    assert_eq!(logs["dropped_events"], 0, "可写目录下无丢弃");
}

#[test]
fn provider_outcome_unknown_projection_covers_every_vision_submission_kind() {
    // 诊断投影的「已提交」谓词与恢复守卫同源（提交台账）：视觉提交类任务
    // 不论 recognition 还是 purchase_recognition 都计入，台账字段在提交载荷里。
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&db).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let vision = app
        .job_enqueue(&member, "purchase_recognition", "vision-unknown", json!({}))
        .unwrap();
    // 直连 SQL 接缝（先例 mark_provider_outcome_unknown）：持久化「已提交、
    // 结果未知」的视觉任务形态（无 submitted_at，台账在提交载荷里）。
    {
        let connection = rusqlite::Connection::open(&db).unwrap();
        let id = vision["id"].as_str().unwrap();
        let raw: String = connection
            .query_row("SELECT document FROM jobs WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        let mut job: Value = serde_json::from_str(&raw).unwrap();
        job["state"] = json!("waiting_input");
        job["payload"]["submission"] = json!("submitted");
        connection
            .execute(
                "UPDATE jobs SET document=?1 WHERE id=?2",
                rusqlite::params![job.to_string(), id],
            )
            .unwrap();
    }
    let summary = app.execute("diagnostics.summary", json!({})).unwrap();
    assert_eq!(
        summary["provider_outcome_unknown"], 1,
        "购买候选识别的已提交未知任务同样计入：{summary}"
    );
}

#[test]
fn external_clients_can_never_gain_diagnostics_scope() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    // 完整授权也拿不到 diagnostics：仅本机可信入口
    let grant = app
        .execute(
            "client.authorize",
            json!({"name":"全量客户端","mode":"full","operations":[],"member_ids":null,"expires_at":null}),
        )
        .unwrap();
    let token = grant["token"].as_str().unwrap();
    for operation in [
        "diagnostics.summary",
        "diagnostics.checks",
        "diagnostics.logs.query",
        "diagnostics.export",
        "diagnostics.ui_event",
        "diagnostics.reproduction.start",
        "diagnostics.reproduction.stop",
    ] {
        let denied = app
            .execute_as_client(token, operation, json!({}))
            .unwrap_err();
        assert_eq!(denied.code, "scope_denied", "{operation}: {denied:?}");
    }
    // 限定授权登记包含 diagnostics：登记即拒绝，令牌无从产生
    let rejected = app
        .execute(
            "client.authorize",
            json!({"name":"诊断越权","mode":"limited","operations":["diagnostics.summary","diagnostics.export","clothing.list"],"member_ids":null,"expires_at":null}),
        )
        .unwrap_err();
    assert_eq!(rejected.code, "invalid_input", "{rejected:?}");
    // capabilities 对外部客户端隐藏 diagnostics 描述符
    let caps = app
        .execute_as_client(token, "capabilities", json!({}))
        .unwrap();
    let names: Vec<&str> = caps["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert!(
        !names.iter().any(|name| name.starts_with("diagnostics.")),
        "外部 capabilities 不得出现 diagnostics 操作：{names:?}"
    );
    // 可信本机入口不受影响
    assert_eq!(
        app.execute("diagnostics.summary", json!({})).unwrap()["schema_version"],
        1
    );
}

#[test]
fn diagnostics_log_query_filters_level_time_trace_run_operation_and_keyword() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let failed = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":" ","category":"top"}),
        )
        .unwrap_err();
    app.execute("member.list", json!({})).unwrap();

    // 级别过滤：error 级 + 操作前缀钉住失败事件（外部 panic 事件无 operation，天然被排除）
    let errors = app
        .execute(
            "diagnostics.logs.query",
            json!({"level":"error","operation":"clothing."}),
        )
        .unwrap();
    assert_eq!(errors["schema_version"], 1);
    let items = errors["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{errors}");
    assert_eq!(items[0]["result"], "invalid_input");
    assert_eq!(items[0]["trace_id"], failed.trace_id.clone().unwrap());
    assert_eq!(items[0]["event_type"], "operation");

    // 操作前缀过滤：恰好一次 space.default，且查询自身的事件（diagnostics.*）不被计入
    let spaces = app
        .execute("diagnostics.logs.query", json!({"operation":"space."}))
        .unwrap();
    let items = spaces["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{spaces}");
    assert_eq!(items[0]["operation"], "space.default");
    assert_eq!(items[0]["result"], "ok");
    let trace = items[0]["trace_id"].as_str().unwrap().to_string();
    let space_ts = items[0]["ts"].as_str().unwrap().to_string();

    // trace_id 过滤：同一次调用的事件（此处恰一条）
    let traced = app
        .execute("diagnostics.logs.query", json!({"trace_id":trace}))
        .unwrap();
    let items = traced["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{traced}");
    assert_eq!(items[0]["operation"], "space.default");

    // run_id 过滤：本进程全部事件共享 run_id
    let run = wardrobe_core::diagnostics::run_id().to_string();
    let same_run = app
        .execute("diagnostics.logs.query", json!({"run_id":run}))
        .unwrap();
    let items = same_run["items"].as_array().unwrap();
    assert!(items.len() >= 3, "{same_run}");
    assert!(items.iter().all(|e| e["run_id"] == json!(run)));
    // 标记操作都在
    for operation in ["space.default", "clothing.create", "member.list"] {
        assert!(
            items.iter().any(|e| e["operation"] == operation),
            "缺少 {operation}：{items:?}"
        );
    }

    // 时间范围：since/until 皆含端点；以前缀过滤钉住
    let after = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"space.","since":space_ts}),
        )
        .unwrap();
    assert_eq!(after["items"].as_array().unwrap().len(), 1, "since 含端点");
    let later = format!(
        "{}",
        (chrono::DateTime::parse_from_rfc3339(&space_ts).unwrap()
            + chrono::Duration::milliseconds(1))
        .to_rfc3339()
    );
    let none = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"space.","since":later}),
        )
        .unwrap();
    assert_eq!(
        none["items"].as_array().unwrap().len(),
        0,
        "起点之后无 space. 事件"
    );
    let until_now = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"space.","until":space_ts}),
        )
        .unwrap();
    assert_eq!(
        until_now["items"].as_array().unwrap().len(),
        1,
        "until 含端点"
    );

    // 关键词：对事件序列化文本做大小写不敏感子串匹配
    let keyword = app
        .execute(
            "diagnostics.logs.query",
            json!({"keyword":"invalid_input","operation":"clothing."}),
        )
        .unwrap();
    assert_eq!(keyword["items"].as_array().unwrap().len(), 1, "{keyword}");
    let none = app
        .execute(
            "diagnostics.logs.query",
            json!({"keyword":"彻底不存在的词"}),
        )
        .unwrap();
    assert_eq!(none["items"].as_array().unwrap().len(), 0);

    // 输入校验：未知字段、非法级别、非法时间、非法 limit 都拒绝
    for bad in [
        json!({"member_id":"x"}),
        json!({"level":"verbose"}),
        json!({"since":"不是时间"}),
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"cursor":"not-a-cursor"}),
    ] {
        let error = app
            .execute("diagnostics.logs.query", bad.clone())
            .unwrap_err();
        assert_eq!(error.code, "invalid_input", "{bad}: {error:?}");
    }
}

#[test]
fn diagnostics_log_query_paginates_with_cursor_until_exhausted() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    // 4 条 clothing.create 事件 + 1 条失败校验事件；前缀钉住集合不受并发事件干扰
    for name in ["甲", "乙", "丙", "丁"] {
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":"top"}),
        )
        .unwrap();
    }
    let all = app
        .execute("diagnostics.logs.query", json!({"operation":"clothing."}))
        .unwrap();
    let expected: Vec<Value> = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["operation"] == "clothing.create")
        .cloned()
        .collect();
    assert_eq!(expected.len(), 4, "{all}");

    // limit=1 逐页续读：顺序、无重复、末页 next_cursor 为 null
    let mut page_cursor: Option<String> = None;
    let mut collected = vec![];
    for _ in 0..10 {
        let mut input = json!({"operation":"clothing.create","limit":1});
        if let Some(cursor) = &page_cursor {
            input["cursor"] = json!(cursor);
        }
        let page = app.execute("diagnostics.logs.query", input).unwrap();
        let items = page["items"].as_array().unwrap();
        assert!(items.len() <= 1, "{page}");
        collected.extend(items.iter().cloned());
        match page["next_cursor"].as_str() {
            Some(cursor) => page_cursor = Some(cursor.to_string()),
            None => break,
        }
    }
    assert_eq!(collected.len(), 4, "{collected:?}");
    assert_eq!(collected, expected, "分页结果须与全量一致且按时间序");
    // 游标稳定可重放：同末页游标重放返回同一条（不重复、不漂移）
    let replay = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"clothing.create","limit":1,"cursor":page_cursor.unwrap()}),
        )
        .unwrap();
    assert_eq!(
        replay["items"].as_array().unwrap(),
        &[collected.last().unwrap().clone()],
        "{replay}"
    );
    assert!(
        replay["next_cursor"].is_null(),
        "末页 next_cursor 为 null：{replay}"
    );
    // 越过末尾的游标：空页且 next_cursor 为 null
    let exhausted = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"clothing.create","cursor":"99999999999999-0"}),
        )
        .unwrap();
    assert_eq!(
        exhausted["items"].as_array().unwrap().len(),
        0,
        "{exhausted}"
    );
    assert!(exhausted["next_cursor"].is_null());
}

#[test]
fn corrupt_log_lines_do_not_block_diagnostics_queries_or_summary() {
    // 尾行截断等单文件损坏不阻断查询（父规格健壮性要求）
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let marker = app.execute("space.default", json!({})).unwrap();
    assert!(marker["member_id"].is_string());
    let logs = dir.path().join("logs");
    // 向活动文件追加损坏行：截断 JSON、非 JSON、缺公共字段的良构 JSON
    let today = chrono::Local::now().date_naive().format("%Y-%m-%d");
    let mut path = logs.join(format!("wardrobe-{today}.jsonl"));
    if !path.exists() {
        // 跨日边界兜底：取目录里最新的 wardrobe-*.jsonl
        let mut names: Vec<String> = std::fs::read_dir(&logs)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("wardrobe-") && n.ends_with(".jsonl"))
            .collect();
        names.sort();
        path = logs.join(names.pop().unwrap());
    }
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{{\"ts\":\"截断的JSON").unwrap();
        writeln!(file, "这不是JSON").unwrap();
        writeln!(file, "{{\"event_type\":\"缺公共字段\"}}").unwrap();
    }
    let queried = app
        .execute("diagnostics.logs.query", json!({"operation":"space."}))
        .unwrap();
    let items = queried["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "损坏行不得吞掉合法事件：{queried}");
    assert_eq!(items[0]["operation"], "space.default");
    let summary = app.execute("diagnostics.summary", json!({})).unwrap();
    assert_eq!(summary["schema_version"], 1, "摘要同样不受损坏行影响");
}

/// #44：创建持久任务的调用把本次调用 trace_id 写入任务文档（可选字段，加法变更）。
/// trace 只能经任务文档跨进程传递：worker 直接入队（trusted seam，不经 Application
/// API 调用）时没有调用链路，文档缺省该字段。
#[test]
fn created_jobs_carry_the_submitting_call_trace_id() {
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let image = dir.path().join("trace-batch.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([255, 255, 255]))
        .save(&image)
        .unwrap();
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":member,"operation_key":"trace-batch","paths":[image.to_string_lossy()]}),
        )
        .unwrap();
    // 创建调用的 trace_id 从日志查询得出（与文档对照的独立事实来源）
    let queried = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"imports.create","level":"info"}),
        )
        .unwrap();
    let created_trace = queried["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["result"] == "ok")
        .unwrap()["trace_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!batch["job_ids"].as_array().unwrap().is_empty());
    for jid in batch["job_ids"].as_array().unwrap() {
        let job = app
            .execute("jobs.get", json!({"member_id":member,"id":jid}))
            .unwrap();
        assert_eq!(
            job["trace_id"], created_trace,
            "任务文档必须携带创建调用的 trace_id：{job}"
        );
    }
    let list = app
        .execute("jobs.list", json!({"member_id":member}))
        .unwrap();
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|j| j["trace_id"] == created_trace),
        "jobs.list 返回的文档同样携带 trace_id：{list}"
    );
    // trusted worker seam 直接入队：无调用链路，trace_id 缺省（可选字段兼容旧任务）
    let orphan = app
        .job_enqueue(&member, "local-test", "orphan", json!({}))
        .unwrap();
    assert!(
        orphan.get("trace_id").is_none(),
        "worker 直接入队没有调用 trace，文档须缺省该字段：{orphan}"
    );
    // 幂等重入返回既有文档：trace 保持创建时的值，不被后续调用覆盖
    let again = app
        .job_enqueue(&member, "local-test", "orphan", json!({}))
        .unwrap();
    assert_eq!(again, orphan, "幂等重入不得改动既有任务文档");
    // 任务 schema 契约：outputSchema 声明可选 trace_id（加法变更，不进 required）
    let capabilities = app.execute("capabilities", json!({})).unwrap();
    let jobs_get = capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "jobs.get")
        .expect("registry 须含 jobs.get");
    assert!(
        jobs_get["outputSchema"]["properties"]["trace_id"].is_object(),
        "任务 schema 须声明 trace_id 字段：{}",
        jobs_get["outputSchema"]
    );
    assert!(
        !jobs_get["outputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "trace_id"),
        "trace_id 属加法变更，不得进入 required"
    );
}

/// #44：worker 执行与任务状态迁移（排队、运行、租约接管、失败、重试、终态）
/// 以任务文档里的 trace_id 写日志事件。worker 可能在另一进程运行：
/// 这里的 worker 步骤全部走 trusted seam 直连（不经 Application API 调用），
/// 事件 trace 只能来自任务文档，不依赖进程状态。
#[test]
fn job_lifecycle_events_are_queryable_by_the_job_trace_id() {
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&db).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let image = dir.path().join("lifecycle.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([255, 255, 255]))
        .save(&image)
        .unwrap();
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":member,"operation_key":"lifecycle","paths":[image.to_string_lossy()]}),
        )
        .unwrap();
    let id = batch["job_ids"][0].as_str().unwrap().to_string();
    let created_trace = app
        .execute("jobs.get", json!({"member_id":member,"id":&id}))
        .unwrap()["trace_id"]
        .as_str()
        .unwrap()
        .to_string();

    // worker 直连接缝：排队任务被领取（queued→running）
    let claimed = app.job_claim(&member, &id, 300).unwrap();
    assert_eq!(claimed["state"], "running");
    // 租约接管：把租约过期时刻改到过去（直连 SQL 接缝，先例 mark_provider_outcome_unknown）
    {
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .execute(
                "UPDATE jobs SET document=json_set(document,'$.lease_expires',1) WHERE id=?1",
                [&id],
            )
            .unwrap();
    }
    let takeover = app.job_claim(&member, &id, 300).unwrap();
    assert_eq!(takeover["state"], "running");
    // 失败且可重试：回 queued 并安排延迟（attempts<3）
    let retried = app
        .job_finish(
            &member,
            &id,
            takeover["lease_token"].as_str().unwrap(),
            "failed",
            Value::Null,
            json!({"code":"provider_rate_limit","retryable":true}),
        )
        .unwrap();
    assert_eq!(retried["state"], "queued");
    assert!(retried["next_due"].as_i64().unwrap() > 0, "重试须安排延迟");
    // 取消排队中的任务：终态 cancelled（succeeded 不能取消，其余状态可）
    app.execute("jobs.cancel", json!({"member_id":member,"id":&id}))
        .unwrap();

    // 单一 trace_id 还原整条迁移链
    let queried = app
        .execute(
            "diagnostics.logs.query",
            json!({"trace_id":created_trace,"operation":"job.","limit":100}),
        )
        .unwrap();
    let events = queried["items"].as_array().unwrap();
    let transitions: Vec<(String, String)> = events
        .iter()
        .filter(|e| e["event_type"] == "job")
        .map(|e| {
            (
                e["operation"].as_str().unwrap().to_string(),
                e["state_to"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    assert_eq!(
        transitions,
        vec![
            ("job.enqueue".to_string(), "queued".to_string()),
            ("job.state".to_string(), "running".to_string()),
            ("job.state".to_string(), "running".to_string()),
            ("job.state".to_string(), "queued".to_string()),
            ("job.state".to_string(), "cancelled".to_string()),
        ],
        "同 trace 下必须能还原排队→运行→租约接管→失败重试→取消：{events:?}"
    );
    let by_state_from = |from: &str, to: &str| {
        events
            .iter()
            .find(|e| {
                e["state_from"].as_str().unwrap_or("") == from
                    && e["state_to"].as_str().unwrap_or("") == to
            })
            .unwrap_or_else(|| panic!("缺 state_from={from} state_to={to} 的事件：{events:?}"))
    };
    let enqueue = by_state_from("", "queued");
    assert_eq!(enqueue["operation"], "job.enqueue");
    assert_eq!(enqueue["job_kind"], "import");
    let first_claim = events
        .iter()
        .filter(|e| e["operation"] == "job.state" && e["state_to"] == "running")
        .find(|e| e["state_from"] == "queued")
        .expect("首次领取应来自 queued");
    assert_eq!(first_claim["attempts"], 1);
    let takeover_event = events
        .iter()
        .find(|e| e["state_from"] == "running" && e["state_to"] == "running")
        .expect("租约接管必须可辨（state_from=running）：{events:?}");
    assert_eq!(takeover_event["attempts"], 2);
    // 可重试失败在 job_finish 内一次持久化完成（failed+回队），事件合并为
    // running→queued 并携带错误码与重试时刻——失败原因与重试安排同一条事件可读
    let retry_event = by_state_from("running", "queued");
    assert_eq!(retry_event["operation"], "job.state");
    assert_eq!(
        retry_event["error_code"], "provider_rate_limit",
        "失败重试事件须带错误码"
    );
    assert!(retry_event["next_due"].as_i64().unwrap() > 0);
    assert_eq!(by_state_from("queued", "cancelled")["job_id"], id);

    // worker 直接入队的任务（无 trace）：迁移事件仍写入，只是缺省 trace_id
    let orphan = app
        .job_enqueue(&member, "local-test", "orphan-lifecycle", json!({}))
        .unwrap();
    let orphan_id = orphan["id"].as_str().unwrap().to_string();
    app.job_claim(&member, &orphan_id, 300).unwrap();
    let orphan_events = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"job.","keyword":&orphan_id,"limit":100}),
        )
        .unwrap();
    let orphan_running = orphan_events["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["job_id"] == orphan_id && e["state_to"] == "running")
        .expect("无 trace 任务的迁移事件仍须写入");
    assert!(
        orphan_running.get("trace_id").is_none(),
        "无链路任务不得伪造 trace_id：{orphan_running}"
    );
    assert_eq!(orphan_running["event_type"], "job");
    assert!(orphan_running["run_id"].is_string());
}

/// #44：维护任务（独立任务系统）同样贯穿——创建调用写入 trace_id，
/// enqueue/领取/终态迁移事件以文档 trace 可查。
#[test]
fn maintenance_job_lifecycle_events_share_the_submitted_trace_id() {
    let _guard = fs_lock();
    let installation = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let app = Application::open(&installation.path().join("wardrobe.sqlite")).unwrap();
    let directory = archive.path().join("backup");
    let submitted = app
        .execute("backup.create", json!({"directory":directory}))
        .unwrap();
    let id = submitted["job_id"].as_str().unwrap().to_string();
    // public 投影保留 trace_id（下划线内部字段被剥离，业务字段不动）
    let doc = app.execute("maintenance.get", json!({"id":&id})).unwrap();
    let trace = doc["trace_id"].as_str().unwrap().to_string();
    app.execute("maintenance.worker.run", json!({"id":&id}))
        .unwrap();
    let queried = app
        .execute(
            "diagnostics.logs.query",
            json!({"trace_id":trace,"operation":"job.","limit":100}),
        )
        .unwrap();
    let events = queried["items"].as_array().unwrap();
    let transitions: Vec<(String, String, String)> = events
        .iter()
        .filter(|e| e["event_type"] == "job")
        .map(|e| {
            (
                e["operation"].as_str().unwrap().to_string(),
                e["state_from"].as_str().unwrap_or("").to_string(),
                e["state_to"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    assert_eq!(
        transitions,
        vec![
            (
                "job.enqueue".to_string(),
                "".to_string(),
                "queued".to_string()
            ),
            (
                "job.state".to_string(),
                "queued".to_string(),
                "running".to_string()
            ),
            (
                "job.state".to_string(),
                "running".to_string(),
                "succeeded".to_string()
            ),
        ],
        "维护任务须以创建调用 trace 还原排队→运行→成功：{events:?}"
    );
    assert!(
        events.iter().all(|e| e["job_kind"] == "backup.create"),
        "维护事件以操作名作类别：{events:?}"
    );
}

/// 读取一个完整 HTTP 请求（头 + 体），返回原始文本（用于捕获 mock 收到的请求头）。
fn read_request_text(stream: &mut std::net::TcpStream) -> String {
    use std::io::Read;
    let mut bytes = vec![];
    loop {
        let mut chunk = [0; 4096];
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(split) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let len: usize = String::from_utf8_lossy(&bytes[..split])
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length: ")
                        .map(|v| v.parse().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= split + 4 + len {
                return String::from_utf8_lossy(&bytes).to_string();
            }
        }
    }
}

/// #44：供应商请求事件——端点类别、模型名、结果类别、耗时，trace 与任务文档一致；
/// 密钥、请求头、完整响应体绝不入日志（脱敏按构造：事件只含白名单字段）。
/// worker 的供应商调用发生在 recognition.process（本例经 execute，另一进程同构）。
#[test]
fn provider_requests_are_logged_sanitized_and_share_the_job_trace() {
    use std::io::Write;
    for (status, expected_outcome, expected_state) in [
        ("200 OK", "ok", "succeeded"),
        ("429 Too Many Requests", "http_error", "queued"),
    ] {
        let _guard = fs_lock();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let (headers_tx, headers_rx) = std::sync::mpsc::channel::<String>();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let request = read_request_text(&mut s);
            headers_tx.send(request).unwrap();
            let body = r#"{"choices":[{"message":{"content":"a white shirt"}}]}"#;
            write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let app = Application::open(&dir.path().join("db")).unwrap();
        let p = app
            .execute(
                "provider.save",
                json!({"name":"脱敏边界","endpoint":endpoint,"model":"trace-model","capabilities":["vision"],"authentication":"bearer"}),
            )
            .unwrap();
        let api_key = format!("sk-trace-secret-{}", uuid::Uuid::now_v7().simple());
        app.execute(
            "provider.credential.set",
            json!({"id":p["id"],"api_key":api_key}),
        )
        .unwrap();
        let path = dir.path().join("fixture.png");
        image::RgbImage::new(16, 16).save(&path).unwrap();
        let c = app
            .execute(
                "clothing.create",
                json!({"member_id":"default","name":"手工","category":"top"}),
            )
            .unwrap();
        let a = app
            .execute("assets.import", json!({"member_id":"default","path":path}))
            .unwrap();
        app.execute(
            "assets.adopt",
            json!({"member_id":"default","clothing_id":c["id"],"asset_id":a["id"]}),
        )
        .unwrap();
        let job = app
            .execute(
                "recognition.create",
                json!({"member_id":"default","operation_key":"sanitize","clothing_id":c["id"],"provider_id":p["id"],"asset_ids":[a["id"]],"upload_role":"product_display","confirm_upload":true}),
            )
            .unwrap();
        let job_trace = job["trace_id"].as_str().unwrap().to_string();
        let out = app
            .execute(
                "recognition.process",
                json!({"member_id":"default","id":job["id"]}),
            )
            .unwrap();
        server.join().unwrap();
        assert_eq!(out["state"], expected_state, "{status}");
        let sent = headers_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(
            sent.to_lowercase().contains("authorization: bearer "),
            "测试有效性：mock 必须真的收到 bearer 请求头"
        );

        let queried = app
            .execute(
                "diagnostics.logs.query",
                json!({"trace_id":job_trace,"operation":"provider.","limit":100}),
            )
            .unwrap();
        let items = queried["items"].as_array().unwrap();
        let request_event = items
            .iter()
            .find(|e| e["event_type"] == "provider_request")
            .expect("供应商请求事件必须落在任务文档的 trace 上");
        assert_eq!(request_event["operation"], "provider.request");
        assert_eq!(request_event["endpoint_kind"], "chat_completions");
        assert_eq!(request_event["model"], "trace-model");
        assert_eq!(request_event["outcome"], expected_outcome, "{status}");
        assert!(request_event["duration_ms"].is_number());
        // 脱敏按构造：事件只含白名单字段，密钥/请求头/请求与响应体没有容身之处
        let allowed = [
            "ts",
            "level",
            "event_type",
            "run_id",
            "trace_id",
            "operation",
            "endpoint_kind",
            "model",
            "outcome",
            "duration_ms",
            "http_status",
        ];
        for key in request_event.as_object().unwrap().keys() {
            assert!(
                allowed.contains(&key.as_str()),
                "供应商请求事件出现白名单外字段 {key}：{request_event}"
            );
        }
        if expected_outcome == "ok" {
            assert!(
                request_event.get("http_status").is_none(),
                "{request_event}"
            );
        } else {
            assert_eq!(request_event["http_status"], 429, "{request_event}");
        }
        // 全日志文件扫描：密钥与响应内容绝不出现
        let mut all_text = String::new();
        for entry in std::fs::read_dir(dir.path().join("logs")).unwrap() {
            all_text.push_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap());
        }
        assert!(
            !all_text.contains(api_key.as_str()),
            "供应商密钥不得写入日志"
        );
        assert!(
            !all_text.contains("a white shirt"),
            "完整响应体不得写入日志"
        );
        app.execute("provider.credential.remove", json!({"id":p["id"]}))
            .unwrap();
    }
}

/// #44 验收：从提交到 worker/供应商结果的全部事件可用单一 trace_id 经
/// diagnostics.logs.query 查出，覆盖 waiting_input 场景——
/// 「这个任务为什么停在 waiting_input」「供应商请求到底发出去没有」一条查询还原。
#[test]
fn full_async_chain_is_queryable_by_a_single_trace_id() {
    use std::io::Write;
    let _guard = fs_lock();
    // 供应商返回 5xx：提交已到达但结果未知 → 任务 waiting_input
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request_text(&mut s);
        let body = r#"{"error":"internal"}"#;
        s.write_all(
            format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .as_bytes(),
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let p = app
        .execute(
            "provider.save",
            json!({"name":"批次链路","endpoint":endpoint,"model":"trace-model","capabilities":["vision"],"authentication":"none"}),
        )
        .unwrap();
    let path = dir.path().join("fixture.png");
    image::RgbImage::new(16, 16).save(&path).unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"手工","category":"top"}),
        )
        .unwrap();
    let a = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":c["id"],"asset_id":a["id"]}),
    )
    .unwrap();
    let job = app
        .execute(
            "recognition.create",
            json!({"member_id":"default","operation_key":"chain","clothing_id":c["id"],"provider_id":p["id"],"asset_ids":[a["id"]],"upload_role":"product_display","confirm_upload":true}),
        )
        .unwrap();
    let trace = job["trace_id"].as_str().unwrap().to_string();
    // worker 侧以一次全新的 Application 调用推进任务（另一进程同构）：
    // 这次调用有自己的新 trace，任务链事件仍归入文档里的创建 trace
    let out = app
        .execute(
            "recognition.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    server.join().unwrap();
    assert_eq!(out["state"], "waiting_input");
    assert_eq!(out["error"]["code"], "submission_unknown");

    // 单一 trace_id 一次查询还原整条链
    let queried = app
        .execute(
            "diagnostics.logs.query",
            json!({"trace_id":trace,"limit":100}),
        )
        .unwrap();
    let items = queried["items"].as_array().unwrap();
    let chain: Vec<(String, String)> = items
        .iter()
        .map(|e| {
            (
                e["event_type"].as_str().unwrap_or("").to_string(),
                e["operation"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    assert_eq!(
        chain,
        vec![
            // enqueue 在任务落盘时写入；operation 事件按 #42 约定在调用完成后写
            ("job".to_string(), "job.enqueue".to_string()),
            ("operation".to_string(), "recognition.create".to_string()),
            ("job".to_string(), "job.state".to_string()),
            (
                "provider_request".to_string(),
                "provider.request".to_string()
            ),
            ("job".to_string(), "job.state".to_string()),
        ],
        "提交→排队→运行→供应商请求→waiting_input 必须在同一 trace 下：{items:?}"
    );
    // 「供应商请求到底发出去没有」：事件证明请求真实发出且到达，5xx 结果未知
    let request = items
        .iter()
        .find(|e| e["event_type"] == "provider_request")
        .unwrap();
    assert_eq!(request["outcome"], "outcome_unknown");
    assert_eq!(request["http_status"], 500);
    assert_eq!(request["model"], "trace-model");
    // 「为什么停在 waiting_input」：迁移事件带最终状态与原因
    let stuck = items.last().unwrap();
    assert_eq!(stuck["state_from"], "running");
    assert_eq!(stuck["state_to"], "waiting_input");
    assert_eq!(stuck["error_code"], "submission_unknown");
    // worker 调用自身的 trace 是新分配的：不入任务链（trace 经文档统一，不经调用栈）
    assert!(
        !items
            .iter()
            .any(|e| e["event_type"] == "operation" && e["operation"] == "recognition.process"),
        "worker 调用的 operation 事件属于调用自身的 trace，不属于任务链：{items:?}"
    );
}

/// #45 diagnostics.export：把 fabricated 事件追加进数据目录 logs/ 的当日文件，
/// 供范围选择测试控制事件内容（与 corrupt 测试同一手法）。
fn append_events(dir: &std::path::Path, events: &[Value]) {
    let logs = dir.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    let today = chrono::Local::now().date_naive().format("%Y-%m-%d");
    let path = logs.join(format!("wardrobe-{today}.jsonl"));
    // 全部行连同换行合并为一次 write（与源码 append_line 同一原子性约定，#53）：
    // writeln! 把行与换行拆成多次 write，并发的 panic 钩子写入（进程级，向全部
    // 日志汇写）可落在两段之间，把伪造事件与钩子事件拼成一行损坏行，两个事件
    // 一起被读取端按损坏行跳过——导出随即缺事件。
    let mut record = String::new();
    for event in events {
        record.push_str(&event.to_string());
        record.push('\n');
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(record.as_bytes()).unwrap();
}

#[test]
fn export_defaults_to_recent_window_with_complete_meta_first_line() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let target = dir.path().join("衣橱日志-导出.jsonl");
    let output = app
        .execute(
            "diagnostics.export",
            json!({"path": target.to_string_lossy()}),
        )
        .unwrap();
    // 输出：路径与导出统计（事件数、字节、范围）
    assert_eq!(output["schema_version"], 1, "{output}");
    assert_eq!(output["path"], json!(target.to_string_lossy()), "{output}");
    let content = std::fs::read_to_string(&target).expect("导出文件应存在");
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(lines.len() >= 2, "首行 meta + 至少一条事件：{content}");
    // 首行 meta 内容完整准确
    let meta: Value = serde_json::from_str(lines[0]).expect("首行必须是 JSON meta");
    assert_eq!(meta["kind"], "diagnostics", "{meta}");
    assert_eq!(meta["schema_version"], 1, "{meta}");
    assert_eq!(meta["app_version"], env!("CARGO_PKG_VERSION"), "{meta}");
    assert_eq!(meta["os"], std::env::consts::OS, "{meta}");
    assert_eq!(
        meta["database_schema_version"],
        json!(wardrobe_core::DATABASE_SCHEMA_VERSION),
        "{meta}"
    );
    assert!(
        chrono::DateTime::parse_from_rfc3339(meta["exported_at"].as_str().unwrap()).is_ok(),
        "{meta}"
    );
    assert_eq!(meta["scope"], "recent", "{meta}");
    assert_eq!(
        meta["run_ids"]
            .as_array()
            .expect("run_ids 须为数组")
            .iter()
            .any(|r| r == wardrobe_core::diagnostics::run_id()),
        true,
        "run_ids 须含当前运行会话：{meta}"
    );
    // 其后为按时间排序的日志事件
    let events: Vec<Value> = lines[1..]
        .iter()
        .map(|l| serde_json::from_str(l).expect("事件行必须是 JSON"))
        .collect();
    assert!(
        events
            .iter()
            .any(|e| e["operation"] == "space.default" && e["result"] == "ok"),
        "导出须含本次操作事件：{events:?}"
    );
    let stamps: Vec<i64> = events
        .iter()
        .map(|e| {
            chrono::DateTime::parse_from_rfc3339(e["ts"].as_str().unwrap())
                .unwrap()
                .timestamp_millis()
        })
        .collect();
    let mut sorted = stamps.clone();
    sorted.sort();
    assert_eq!(stamps, sorted, "导出事件按时间升序");
    // 按级别计数：恰好四个级别键，总数与事件数一致
    let counts = &meta["level_counts"];
    for level in ["info", "debug", "trace", "error"] {
        assert!(counts[level].is_u64(), "缺少 {level} 计数：{meta}");
    }
    let total: u64 = ["info", "debug", "trace", "error"]
        .iter()
        .map(|l| counts[*l].as_u64().unwrap())
        .sum();
    assert_eq!(total, events.len() as u64, "{meta}");
    // 最近错误摘要：只含 error 级投影（同 summary 口径；并发 panic 事件允许出现）
    let recent_errors = meta["recent_errors"].as_array().unwrap();
    assert!(
        recent_errors.iter().all(|e| e["level"] == "error"),
        "{recent_errors:?}"
    );
    // 输出统计与文件实际内容一致
    assert_eq!(output["event_count"], json!(events.len()), "{output}");
    assert_eq!(
        output["bytes"],
        json!(std::fs::metadata(&target).unwrap().len()),
        "{output}"
    );
    assert_eq!(
        output["covered_from"],
        events.first().unwrap()["ts"],
        "{output}"
    );
    assert_eq!(
        output["covered_to"],
        events.last().unwrap()["ts"],
        "{output}"
    );
}

#[test]
fn export_rejects_missing_empty_or_unwritable_target_path() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    for bad in [json!({}), json!({"path": ""}), json!({"path": 123})] {
        let error = app.execute("diagnostics.export", bad.clone()).unwrap_err();
        assert_eq!(error.code, "invalid_input", "{bad}: {error:?}");
    }
    // 父目录不存在：无法写入，属存储故障而非输入格式问题
    let unwritable = app
        .execute(
            "diagnostics.export",
            json!({"path": dir.path().join("no-such-dir/out.jsonl").to_string_lossy()}),
        )
        .unwrap_err();
    assert_eq!(unwritable.code, "storage_error", "{unwritable:?}");
}

/// 执行一次导出并解析产物：返回（输出、meta 首行、事件行）。
fn export_lines(
    app: &Application,
    target: &std::path::Path,
    mut input: Value,
) -> (Value, Value, Vec<Value>) {
    input["path"] = json!(target.to_string_lossy());
    let output = app.execute("diagnostics.export", input).unwrap();
    let content = std::fs::read_to_string(target).unwrap();
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let meta: Value = serde_json::from_str(lines[0]).unwrap();
    let events: Vec<Value> = lines[1..]
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (output, meta, events)
}

#[test]
fn export_scope_selects_session_recent_all_and_explicit_window() {
    // 本测试向活动日志文件追加伪造事件并经导出断言其在场/不在场；进程级 panic 钩子
    // 会向全部日志汇并发写（#42 通道），不得与追加及导出断言并发（#53）
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    // 两个“其他运行会话”的外部事件：25 小时前（只有 all 含）与 2 小时前（recent 含）
    let now = chrono::Utc::now();
    let stamp = |hours: i64| {
        (now - chrono::Duration::hours(hours)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    let foreign_run = "018f0000-0000-7000-8000-00000000dead";
    append_events(
        dir.path(),
        &[
            json!({"ts": stamp(25), "level":"info", "event_type":"operation", "run_id": foreign_run, "operation":"import.old", "result":"ok"}),
            json!({"ts": stamp(2), "level":"error", "event_type":"operation", "run_id": foreign_run, "operation":"recognition.crash", "result":"storage_error"}),
        ],
    );
    let target = dir.path().join("scope.jsonl");
    let contains = |events: &[Value], operation: &str| {
        events.iter().any(|e| e["operation"] == json!(operation))
    };

    // session：仅本次运行会话，排除其他 run_id 的全部事件
    let (_, meta, events) = export_lines(&app, &target, json!({"scope":"session"}));
    assert_eq!(meta["scope"], "session", "{meta}");
    assert!(
        events
            .iter()
            .all(|e| e["run_id"] == json!(wardrobe_core::diagnostics::run_id())),
        "session 只含当前运行会话：{events:?}"
    );
    assert!(contains(&events, "space.default"), "{events:?}");
    assert!(!contains(&events, "import.old") && !contains(&events, "recognition.crash"));

    // recent（显式传参）：最近 24 小时跨重启，含 2 小时前、不含 25 小时前
    let (_, meta, events) = export_lines(&app, &target, json!({"scope":"recent"}));
    assert_eq!(meta["scope"], "recent", "{meta}");
    assert!(
        contains(&events, "recognition.crash"),
        "recent 含 24h 内其他运行：{events:?}"
    );
    assert!(contains(&events, "space.default"));
    assert!(
        !contains(&events, "import.old"),
        "recent 不含 24h 前事件：{events:?}"
    );

    // all：保留期内全部，含最旧事件
    let (_, meta, events) = export_lines(&app, &target, json!({"scope":"all"}));
    assert_eq!(meta["scope"], "all", "{meta}");
    assert!(
        contains(&events, "import.old"),
        "all 含保留期内最旧事件：{events:?}"
    );
    assert!(contains(&events, "recognition.crash") && contains(&events, "space.default"));

    // 显式时间窗 [now-3h, now-1h]：恰含 2 小时前事件，排除当前与 25 小时前
    let window = json!({"from": stamp(3), "to": stamp(1)});
    let (output, meta, events) = export_lines(&app, &target, window);
    assert_eq!(meta["scope"], "window", "{meta}");
    assert_eq!(events.len(), 1, "时间窗恰含 2 小时前事件：{events:?}");
    assert!(contains(&events, "recognition.crash"), "{events:?}");
    assert_eq!(output["event_count"], json!(1), "{output}");

    // 非法范围：起止颠倒、缺一端、与 scope 混用、非法 scope 值
    for bad in [
        json!({"from": stamp(1), "to": stamp(3)}),
        json!({"from": stamp(3)}),
        json!({"to": stamp(1)}),
        json!({"scope":"all","from": stamp(3), "to": stamp(1)}),
        json!({"scope":"today"}),
    ] {
        let error = app.execute("diagnostics.export", bad.clone()).unwrap_err();
        assert_eq!(error.code, "invalid_input", "{bad}: {error:?}");
    }
}

#[test]
fn export_recent_includes_previous_run_after_reopening_the_application() {
    // 上次运行崩溃后重新打开应用再导出：recent 按时间窗口选取、跨重启，
    // 崩溃前上次运行（独立 run_id）的事件仍在文件里（run_id 每进程一次，
    // 上次进程的会话以独立 run_id 的日志事件代表——与崩溃现场同形）
    // 追加伪造崩溃事件并断言其可导出：与 panic 钩子并发写互斥（#42 通道，#53）
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let first = Application::open(&db).unwrap();
    first.execute("space.default", json!({})).unwrap();
    // 上次运行的最后现场：panic 事件带它自己的 run_id
    let crashed_run = "018f0000-0000-7000-8000-00000000cafe";
    let crash_ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    append_events(
        dir.path(),
        &[json!({
            "ts": crash_ts,
            "level": "error",
            "event_type": "panic",
            "run_id": crashed_run,
            "message": "崩溃现场-上次运行",
        })],
    );
    drop(first);
    // 重新打开应用（第二次运行），默认导出（recent）
    let second = Application::open(&db).unwrap();
    let (_, meta, events) = export_lines(&second, &dir.path().join("recent.jsonl"), json!({}));
    assert!(
        events
            .iter()
            .any(|e| e["run_id"] == json!(crashed_run) && e["event_type"] == "panic"),
        "recent 必须含崩溃前上次运行的事件：{events:?}"
    );
    assert!(
        events.iter().any(|e| e["operation"] == "space.default"
            && e["run_id"] == json!(wardrobe_core::diagnostics::run_id())),
        "recent 也含本次运行事件：{events:?}"
    );
    let run_ids = meta["run_ids"].as_array().unwrap();
    assert!(run_ids.contains(&json!(crashed_run)), "{meta}");
    assert!(
        run_ids.contains(&json!(wardrobe_core::diagnostics::run_id())),
        "{meta}"
    );
    assert!(
        meta["recent_errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["run_id"] == json!(crashed_run)),
        "meta 最近错误含上次运行的 panic：{meta}"
    );
    // 对照：session 只含第二次运行——跨重启的包含关系确实由时间窗决定
    let (_, meta, events) = export_lines(
        &second,
        &dir.path().join("session.jsonl"),
        json!({"scope":"session"}),
    );
    assert!(
        events.iter().all(|e| e["run_id"] != json!(crashed_run)),
        "session 不得含上次运行：{events:?}"
    );
    assert!(
        !meta["run_ids"]
            .as_array()
            .unwrap()
            .contains(&json!(crashed_run)),
        "{meta}"
    );
}

#[test]
fn export_skips_corrupt_lines_without_blocking() {
    // 尾行截断等单文件损坏不阻断导出（与查询/摘要同一容错约定）
    // 向活动日志文件追加行并断言导出内容：与 panic 钩子并发写互斥（#42 通道，#53）
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    append_events(
        dir.path(),
        &[
            json!({"ts": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true), "level":"info", "event_type":"operation", "run_id": wardrobe_core::diagnostics::run_id(), "operation":"fabricated.valid", "result":"ok"}),
        ],
    );
    // 向同一文件追加损坏行：截断 JSON、非 JSON、缺公共字段的良构 JSON
    let logs = dir.path().join("logs");
    let today = chrono::Local::now().date_naive().format("%Y-%m-%d");
    let mut path = logs.join(format!("wardrobe-{today}.jsonl"));
    if !path.exists() {
        let mut names: Vec<String> = std::fs::read_dir(&logs)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("wardrobe-") && n.ends_with(".jsonl"))
            .collect();
        names.sort();
        path = logs.join(names.pop().unwrap());
    }
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{{\"ts\":\"截断的JSON").unwrap();
        writeln!(file, "这不是JSON").unwrap();
        writeln!(file, "{{\"event_type\":\"缺公共字段\"}}").unwrap();
    }
    let (_, meta, events) =
        export_lines(&app, &dir.path().join("all.jsonl"), json!({"scope":"all"}));
    let content = meta.to_string();
    for fragment in ["截断的JSON", "这不是JSON", "缺公共字段"] {
        assert!(
            !content.contains(fragment) && !events.iter().any(|e| e.to_string().contains(fragment)),
            "损坏行不得进入导出：{events:?}"
        );
    }
    assert!(
        events.iter().any(|e| e["operation"] == "space.default")
            && events.iter().any(|e| e["operation"] == "fabricated.valid"),
        "损坏行不得吞掉合法事件：{events:?}"
    );
    assert_eq!(
        meta["event_count"],
        json!(events.len()),
        "meta 事件计数与实际行数一致：{meta}"
    );
}

#[test]
fn export_explicit_window_endpoints_are_inclusive() {
    // 本测试向活动日志文件追加端点事件并断言恰被导出一次；进程级 panic 钩子
    // 会向全部日志汇并发写（#42 通道），不得与追加及导出断言并发（#53）
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let edge = "2026-06-01T12:00:00.000Z";
    append_events(
        dir.path(),
        &[
            json!({"ts": edge, "level":"info", "event_type":"operation", "run_id": "018f0000-0000-7000-8000-00000000edge", "operation":"window.edge", "result":"ok"}),
        ],
    );
    let target = dir.path().join("edge.jsonl");
    // from = to = 事件时刻：端点含入
    let (output, _, events) = export_lines(&app, &target, json!({"from": edge, "to": edge}));
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["operation"], "window.edge", "{events:?}");
    assert_eq!(output["covered_from"], json!(edge), "{output}");
    assert_eq!(output["covered_to"], json!(edge), "{output}");
    // 端点外 1 毫秒：排除
    let (_, _, events) = export_lines(
        &app,
        &target,
        json!({"from": "2026-06-01T12:00:00.001Z", "to": "2026-06-01T13:00:00.000Z"}),
    );
    assert_eq!(events.len(), 0, "{events:?}");
    let (_, _, events) = export_lines(
        &app,
        &target,
        json!({"from": "2026-06-01T11:00:00.000Z", "to": "2026-06-01T11:59:59.999Z"}),
    );
    assert_eq!(events.len(), 0, "{events:?}");
}

#[test]
fn export_artifact_never_contains_tokens_keys_database_or_images() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    // client.authorize：一次性连接令牌只在本次响应中出现
    let grant = app
        .execute(
            "client.authorize",
            json!({"name":"导出脱敏客户端","mode":"limited","operations":["clothing.list"],"member_ids":null,"expires_at":null}),
        )
        .unwrap();
    let token = grant["token"].as_str().unwrap();
    // 供应商密钥：输入携带密钥（无认证供应商在触碰钥匙串前拒绝，密钥无论成败绝不落盘）
    let api_key = "sk-export-9f8e7d6c5b4a3210feed";
    assert!(app
        .execute(
            "provider.credential.set",
            json!({"id":"any-provider","api_key":api_key}),
        )
        .is_err());
    use sha2::{Digest, Sha256};
    let token_hash: String = Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let (_, meta, events) = export_lines(&app, &dir.path().join("export.jsonl"), json!({}));
    let meta_text = meta.to_string();
    let events_text: Vec<String> = events.iter().map(|e| e.to_string()).collect();
    for secret in [token, &token_hash, api_key] {
        assert!(
            !meta_text.contains(secret),
            "meta 不得含 {secret}：{meta_text}"
        );
        for (index, text) in events_text.iter().enumerate() {
            assert!(!text.contains(secret), "事件行 {index} 不得含凭据：{text}");
        }
    }
    // 产物只来自日志文件与摘要投影：不含数据库字节与图片内容。
    // 结构性证明——导出每行都是合法 JSON 事件，数据库魔数与二进制图像不可能出现。
    assert!(!meta_text.contains("SQLite format 3"), "{meta_text}");
    assert!(
        events
            .iter()
            .all(|e| e["event_type"].is_string() && e["ts"].is_string() && e["level"].is_string()),
        "{events:?}"
    );
    // 事件本身仍完整：client.authorize 成功事件在导出中（值脱敏、事件在场）
    assert!(
        events
            .iter()
            .any(|e| e["operation"] == "client.authorize" && e["result"] == "ok"),
        "{events:?}"
    );
}

#[test]
fn process_panic_leaves_a_scene_in_the_log() {
    // 钩子触发时会向进程内全部日志汇写入并可能驱动它们清理，
    // 期间不得与其他测试的文件断言并发（#42）
    let _guard = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let _app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    wardrobe_core::diagnostics::install_panic_hook();
    let marker = "崩溃现场标记-9a3f";
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        panic!("{marker}");
    }));
    let events = log_events(dir.path());
    let panics: Vec<&Value> = events
        .iter()
        .filter(|e| e["event_type"] == "panic")
        .collect();
    assert!(
        panics
            .iter()
            .any(|e| e["message"].as_str().unwrap_or("").contains(marker)),
        "panic 必须留下带原始消息的日志现场：{panics:?}"
    );
    let scene = panics
        .iter()
        .find(|e| e["message"].as_str().unwrap_or("").contains(marker))
        .unwrap();
    assert_eq!(scene["level"], "error");
    assert_eq!(scene["run_id"], wardrobe_core::diagnostics::run_id());
    assert!(
        scene["location"]
            .as_str()
            .unwrap()
            .contains("diagnostics.rs"),
        "{scene}"
    );
}

/// #48 基础检查：固定十项、顺序稳定，正常环境（全新数据目录）全部 pass。
#[test]
fn diagnostics_checks_report_all_items_passing_in_a_normal_environment() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let checks = app.execute("diagnostics.checks", json!({})).unwrap();
    assert_eq!(checks["schema_version"], 1, "{checks}");
    let items = checks["checks"].as_array().unwrap();
    let ids: Vec<&str> = items
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![
            "data_directory",
            "database_schema",
            "database_wal",
            "logs_writable",
            "disk_space",
            "installation_generation",
            "encryption_wallet",
            "model_credentials",
            "stuck_jobs",
            "pending_location_marker",
        ],
        "检查项固定顺序：{checks}"
    );
    for item in items {
        assert_eq!(item["status"], "pass", "正常环境全部通过：{item}");
        assert!(item["message"].is_string(), "通俗消息：{item}");
        assert!(
            !item["message"].as_str().unwrap().trim().is_empty(),
            "{item}"
        );
        assert!(item["detail"].is_string(), "技术明细：{item}");
        assert!(
            item["setting"].is_null() || item["setting"].is_string(),
            "去向为设置页标识或 null：{item}"
        );
    }
    assert_eq!(
        checks["counts"],
        json!({"pass":10,"warn":0,"fail":0}),
        "{checks}"
    );
    // WAL 生效的技术明细可读（只读 pragma 查询）
    let wal = items.iter().find(|i| i["id"] == "database_wal").unwrap();
    assert!(wal["detail"].as_str().unwrap().contains("wal"), "{wal}");
    // 未配置模型/加密属于正常状态：pass 且通俗消息说明现状
    let credentials = items
        .iter()
        .find(|i| i["id"] == "model_credentials")
        .unwrap();
    assert!(
        credentials["message"].as_str().unwrap().contains("未配置"),
        "{credentials}"
    );
}

/// #48 故障注入：数据目录只读 → data_directory fail；业务操作不被检查阻塞。
#[test]
fn read_only_data_directory_fails_the_data_directory_check() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let restore = |mode: u32| {
        let mut permissions = std::fs::metadata(dir.path()).unwrap().permissions();
        permissions.set_mode(mode);
        std::fs::set_permissions(dir.path(), permissions).unwrap();
    };
    restore(0o555);
    let checks = app.execute("diagnostics.checks", json!({})).unwrap();
    restore(0o755);
    let item = checks["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "data_directory")
        .unwrap()
        .clone();
    assert_eq!(item["status"], "fail", "{item}");
    assert_eq!(item["setting"], "backup", "{item}");
    // 其余检查不受影响（数据库等仍可读）
    assert_eq!(checks["counts"]["fail"], 1, "{checks}");
}

/// #48 故障注入：租约过期仍 running 的卡住任务 → stuck_jobs warn 带计数与去向。
#[test]
fn expired_lease_running_jobs_are_reported_as_stuck() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&db).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let job = app
        .job_enqueue(&member, "local-test", "stuck-1", json!({}))
        .unwrap();
    let id = job["id"].as_str().unwrap().to_string();
    app.job_claim(&member, &id, 30).unwrap();
    // 直连接缝（先例 #43 mark_provider_outcome_unknown）：把租约拨到过去，制造「卡住」
    let connection = rusqlite::Connection::open(&db).unwrap();
    let raw: String = connection
        .query_row("SELECT document FROM jobs WHERE id=?1", [&id], |r| r.get(0))
        .unwrap();
    let mut document: Value = serde_json::from_str(&raw).unwrap();
    document["lease_expires"] = json!(chrono::Utc::now().timestamp_millis() - 60_000);
    connection
        .execute(
            "UPDATE jobs SET document=?1 WHERE id=?2",
            rusqlite::params![document.to_string(), id],
        )
        .unwrap();
    drop(connection);

    let checks = app.execute("diagnostics.checks", json!({})).unwrap();
    let item = checks["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "stuck_jobs")
        .unwrap()
        .clone();
    assert_eq!(item["status"], "warn", "{item}");
    assert_eq!(item["setting"], "tasks", "{item}");
    assert!(item["message"].as_str().unwrap().contains('1'), "{item}");
    assert!(
        item["detail"].as_str().unwrap().contains("stuck_jobs=1"),
        "{item}"
    );
}

/// #48 故障注入：残留的待发布位置标记（source_file 与当前库不符）→ warn；
/// 检查只读：标记原样保留，绝不被检查消费。
#[test]
fn residual_pending_location_marker_is_reported_without_being_consumed() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let marker = dir.path().join(".wardrobe-location.json");
    std::fs::write(
        &marker,
        json!({"version":1,"phase":"pending","source_file":"wardrobe-old.sqlite","target_database":"/elsewhere/wardrobe.sqlite"})
            .to_string(),
    )
    .unwrap();

    let checks = app.execute("diagnostics.checks", json!({})).unwrap();
    let item = checks["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "pending_location_marker")
        .unwrap()
        .clone();
    assert_eq!(item["status"], "warn", "{item}");
    assert_eq!(item["setting"], "backup", "{item}");
    assert!(
        item["detail"].as_str().unwrap().contains("pending"),
        "技术明细应说明残留：{item}"
    );
    let content = std::fs::read_to_string(&marker).unwrap();
    assert!(
        content.contains("\"phase\":\"pending\""),
        "检查只读：标记必须原样保留：{content}"
    );
}

/// #48 红线：检查过程不修改任何配置或数据——审计/outbox/任务/单品/模型/加密状态
/// 行数不变，数据目录不留下探测文件（logs/ 事件是日志系统自身行为，不计入）。
#[test]
fn checks_never_modify_configuration_or_data() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&db).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    app.execute(
        "clothing.create",
        json!({"member_id":member,"name":"体检衬衫","category":"top"}),
    )
    .unwrap();
    app.job_enqueue(&member, "local-test", "check-j", json!({}))
        .unwrap();

    let table_counts = |connection: &rusqlite::Connection| -> Vec<(String, i64)> {
        [
            "audit",
            "outbox",
            "jobs",
            "clothing",
            "providers",
            "encryption_state",
            "clients",
            "members",
        ]
        .iter()
        .map(|table| {
            let count: i64 = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            (table.to_string(), count)
        })
        .collect()
    };
    let data_files = || -> Vec<std::path::PathBuf> {
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.file_name().and_then(|n| n.to_str()) != Some("logs"))
            .collect();
        files.sort();
        files
    };

    let connection = rusqlite::Connection::open(&db).unwrap();
    let before_tables = table_counts(&connection);
    let before_files = data_files();
    drop(connection);

    for _ in 0..3 {
        app.execute("diagnostics.checks", json!({})).unwrap();
    }

    let connection = rusqlite::Connection::open(&db).unwrap();
    assert_eq!(
        table_counts(&connection),
        before_tables,
        "检查不得改动任何表"
    );
    drop(connection);
    assert_eq!(data_files(), before_files, "检查不得在数据目录留下探测文件");
}

/// #48 验收「过高模式版本」注入：数据库无法打开时 doctor 降级——
/// database_schema fail 带失败原因，文件系统级检查照常给出，依赖数据库的项 warn。
#[test]
fn doctor_report_degrades_to_filesystem_checks_when_database_cannot_open() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    // 直连接缝：制造「由更新版本应用创建」的数据库
    {
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .pragma_update(
                None,
                "user_version",
                wardrobe_core::DATABASE_SCHEMA_VERSION + 1,
            )
            .unwrap();
    }
    let report = wardrobe_core::diagnostics::doctor_report(&db);
    assert_eq!(report["schema_version"], 1, "{report}");
    assert_eq!(report["error"]["code"], "unsupported_schema", "{report}");
    assert!(report["summary"].is_null(), "摘要不可用：{report}");
    let items = report["checks"]["checks"].as_array().unwrap();
    assert_eq!(items.len(), 10, "检查项清单保持完整：{report}");
    let schema = items.iter().find(|i| i["id"] == "database_schema").unwrap();
    assert_eq!(schema["status"], "fail", "{schema}");
    assert!(
        schema["detail"]
            .as_str()
            .unwrap()
            .contains("unsupported_schema"),
        "技术明细带打开失败原因：{schema}"
    );
    let data_directory = items.iter().find(|i| i["id"] == "data_directory").unwrap();
    assert_eq!(
        data_directory["status"], "pass",
        "文件系统级检查照常：{data_directory}"
    );
    let wal = items.iter().find(|i| i["id"] == "database_wal").unwrap();
    assert_eq!(
        wal["status"], "warn",
        "依赖数据库的项如实说明无法检查：{wal}"
    );
    assert_eq!(report["checks"]["counts"]["fail"], 1, "{report}");
}

/// #50：`diagnostics.ui_event` 封闭白名单——合法类型落盘为 event_type=ui 的事件
///（只含公共字段与 type/label），白名单外类型、缺失/超长标签与未知字段被拒绝，
/// 且被拒绝的 ui 事件不落盘（脱敏按构造不变：事件没有携带任意内容的容身之处）。
#[test]
fn ui_event_accepts_only_whitelisted_types_and_short_labels() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let samples = [
        ("view_switch", "穿搭库"),
        ("dialog_open", "新增衣物"),
        ("dialog_submit", "记录穿着"),
    ];
    for (kind, label) in samples {
        app.execute("diagnostics.ui_event", json!({"type":kind,"label":label}))
            .unwrap();
    }
    let events = log_events(dir.path());
    let ui: Vec<&Value> = events.iter().filter(|e| e["event_type"] == "ui").collect();
    assert_eq!(ui.len(), 3, "恰好三条 ui 事件：{events:?}");
    for (event, (kind, label)) in ui.iter().zip(samples) {
        assert_eq!(event["type"], json!(kind), "{event}");
        assert_eq!(event["label"], json!(label), "{event}");
        assert_eq!(event["level"], "info", "{event}");
        assert_eq!(
            event["run_id"],
            wardrobe_core::diagnostics::run_id(),
            "{event}"
        );
        assert!(
            chrono::DateTime::parse_from_rfc3339(event["ts"].as_str().unwrap()).is_ok(),
            "{event}"
        );
        let allowed = ["ts", "level", "event_type", "run_id", "type", "label"];
        for key in event.as_object().unwrap().keys() {
            assert!(
                allowed.contains(&key.as_str()),
                "ui 事件出现白名单外字段 {key}：{event}"
            );
        }
    }
    // 白名单外类型、缺 label、空 label、超长 label、未知字段、非法类型值：全部拒绝
    for bad in [
        json!({"type":"free_text","label":"任意内容"}),
        json!({"type":"view_switch"}),
        json!({"type":"view_switch","label":""}),
        json!({"type":"view_switch","label":"x".repeat(41)}),
        json!({"type":"view_switch","label":"穿搭库","extra":"字段"}),
        json!({"type":123,"label":"穿搭库"}),
    ] {
        let error = app
            .execute("diagnostics.ui_event", bad.clone())
            .unwrap_err();
        assert_eq!(error.code, "invalid_input", "{bad}: {error:?}");
    }
    let after = log_events(dir.path());
    assert_eq!(
        after.iter().filter(|e| e["event_type"] == "ui").count(),
        3,
        "被拒绝的 ui 事件不得落盘：{after:?}"
    );
}

/// #50：记录复现——开始/停止标记事件落盘（event_type=reproduction，phase=start/stop，
/// 返回的时间与落盘 ts 一致）；以窗口起止为界的导出只含窗口内事件，meta 标
/// kind=reproduction 并附记录时刻的 checks 结果；不传新参的导出与 #45 完全一致
///（meta.kind=diagnostics 且无 checks 键）。
#[test]
fn reproduction_markers_bound_the_window_and_export_carries_reproduction_meta_with_checks() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    // 窗口前的操作：窗口导出不得包含（隔开 2ms 避免毫秒边界同点）
    app.execute("member.list", json!({})).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let start = app
        .execute("diagnostics.reproduction.start", json!({}))
        .unwrap();
    assert_eq!(start["schema_version"], 1, "{start}");
    let started_at = start["started_at"]
        .as_str()
        .expect("started_at 须为 RFC3339")
        .to_string();
    // 窗口内：一次真实操作 + 一条用户操作时间线事件
    app.execute("space.default", json!({})).unwrap();
    app.execute(
        "diagnostics.ui_event",
        json!({"type":"view_switch","label":"穿搭库"}),
    )
    .unwrap();
    let stop = app
        .execute("diagnostics.reproduction.stop", json!({}))
        .unwrap();
    assert_eq!(stop["schema_version"], 1, "{stop}");
    let stopped_at = stop["stopped_at"]
        .as_str()
        .expect("stopped_at 须为 RFC3339")
        .to_string();

    // 标记事件落盘，且落盘 ts 与返回的窗口端点完全一致
    let events = log_events(dir.path());
    let markers: Vec<&Value> = events
        .iter()
        .filter(|e| e["event_type"] == "reproduction")
        .collect();
    assert_eq!(markers.len(), 2, "开始/停止标记各一条：{events:?}");
    assert_eq!(markers[0]["phase"], "start", "{markers:?}");
    assert_eq!(
        markers[0]["ts"],
        json!(started_at),
        "落盘 ts 须与 started_at 一致：{markers:?}"
    );
    assert_eq!(markers[1]["phase"], "stop", "{markers:?}");
    assert_eq!(
        markers[1]["ts"],
        json!(stopped_at),
        "落盘 ts 须与 stopped_at 一致：{markers:?}"
    );

    // 以标记为界的窗口导出（含端点）：含两端标记、窗口内操作与 ui 事件，不含窗口前操作
    let target = dir.path().join("复现记录.jsonl");
    let (output, meta, exported) = export_lines(
        &app,
        &target,
        json!({"from":started_at,"to":stopped_at,"kind":"reproduction","attach_checks":true}),
    );
    assert!(
        exported
            .iter()
            .any(|e| e["event_type"] == "reproduction" && e["phase"] == "start"),
        "{exported:?}"
    );
    assert!(
        exported
            .iter()
            .any(|e| e["event_type"] == "reproduction" && e["phase"] == "stop"),
        "{exported:?}"
    );
    assert!(
        exported
            .iter()
            .any(|e| e["event_type"] == "ui" && e["type"] == "view_switch"),
        "{exported:?}"
    );
    assert!(
        exported.iter().any(|e| e["operation"] == "space.default"),
        "{exported:?}"
    );
    assert!(
        !exported.iter().any(|e| e["operation"] == "member.list"),
        "窗口前操作不得进入复现导出：{exported:?}"
    );
    assert_eq!(meta["kind"], "reproduction", "{meta}");
    let checks = &meta["checks"];
    assert_eq!(checks["schema_version"], 1, "{meta}");
    assert_eq!(
        checks["checks"].as_array().unwrap().len(),
        10,
        "附记录时刻的十项检查：{meta}"
    );
    assert_eq!(
        checks["counts"],
        json!({"pass":10,"warn":0,"fail":0}),
        "{meta}"
    );
    // 输出契约与 #45 同形：路径与统计字段不变
    assert_eq!(output["schema_version"], 1, "{output}");
    assert_eq!(output["path"], json!(target.to_string_lossy()), "{output}");
    assert_eq!(output["event_count"], json!(exported.len()), "{output}");

    // #45 回归钉住：不传新参，meta.kind 仍为 diagnostics 且没有 checks 键
    let (_, plain_meta, _) = export_lines(
        &app,
        &dir.path().join("plain.jsonl"),
        json!({"scope":"session"}),
    );
    assert_eq!(plain_meta["kind"], "diagnostics", "{plain_meta}");
    assert!(
        plain_meta.get("checks").is_none(),
        "不传新参不得附 checks：{plain_meta}"
    );
    // 新参数自身校验：非法 kind、非布尔 attach_checks、未知字段
    for bad in [
        json!({"path":"x.jsonl","kind":"video"}),
        json!({"path":"x.jsonl","attach_checks":"yes"}),
        json!({"path":"x.jsonl","kind":"reproduction","extra":1}),
    ] {
        let error = app.execute("diagnostics.export", bad.clone()).unwrap_err();
        assert_eq!(error.code, "invalid_input", "{bad}: {error:?}");
    }
}

/// #50：标记操作输入严格（不接受任何字段），开始/停止标记不携带可注入内容。
#[test]
fn reproduction_markers_accept_no_input_fields() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    for operation in [
        "diagnostics.reproduction.start",
        "diagnostics.reproduction.stop",
    ] {
        let error = app
            .execute(operation, json!({"phase":"forged"}))
            .unwrap_err();
        assert_eq!(error.code, "invalid_input", "{operation}: {error:?}");
        app.execute(operation, json!({})).unwrap();
    }
    let events = log_events(dir.path());
    let markers: Vec<&Value> = events
        .iter()
        .filter(|e| e["event_type"] == "reproduction")
        .collect();
    assert_eq!(markers.len(), 2, "{events:?}");
    assert_eq!(markers[0]["phase"], "start", "{markers:?}");
    assert_eq!(markers[1]["phase"], "stop", "{markers:?}");
}

/// #48 doctor 降级：位置标记 pending 且迁移锁被占用（切换进行中）→ 打开被拒，
/// 降级体检把 pending_location_marker 判为 fail。
#[test]
fn doctor_report_flags_an_incomplete_switch_while_the_move_lock_is_held() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    std::fs::write(&db, []).unwrap();
    let marker = dir.path().join(".wardrobe-location.json");
    std::fs::write(
        &marker,
        json!({"version":1,"phase":"pending","source_file":"wardrobe.sqlite","target_database":"/elsewhere/wardrobe.sqlite"})
            .to_string(),
    )
    .unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.path().join(".wardrobe-move.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let report = wardrobe_core::diagnostics::doctor_report(&db);
    assert_eq!(report["error"]["code"], "data_directory_moving", "{report}");
    let item = report["checks"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "pending_location_marker")
        .unwrap()
        .clone();
    assert_eq!(item["status"], "fail", "{item}");
    assert_eq!(item["setting"], "backup", "{item}");
}

// ==== 自 registry_contracts.rs 迁入的运行时/内容契约（issue #70 增量5）====

fn assert_result_matches_schema(schema: &Value, result: &Value, context: &str) {
    assert_eq!(
        schema["type"].as_str(),
        Some("object"),
        "{context}: outputSchema 须为 object"
    );
    let props = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{context}: outputSchema 须声明 properties"));
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|r| r.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    let actual = result
        .as_object()
        .unwrap_or_else(|| panic!("{context} 结果须为对象"));
    for (key, value) in actual {
        let prop = props
            .get(key)
            .unwrap_or_else(|| panic!("{context}: 实际结果字段 {key} 未在 outputSchema 声明"));
        if let Some(const_value) = prop.get("const") {
            assert_eq!(value, const_value, "{context}: 字段 {key} 须等于 const");
        }
        if let Some(allowed) = prop["enum"].as_array() {
            assert!(allowed.contains(value), "{context}: 字段 {key} 须为枚举值");
        }
        let declared_types: Vec<String> = match prop.get("type") {
            Some(Value::String(t)) => vec![t.clone()],
            Some(Value::Array(ts)) => ts.iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            _ => vec![],
        };
        let actual_type = match value {
            Value::String(_) => "string",
            Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Bool(_) => "boolean",
            Value::Null => "null",
        };
        assert!(
            declared_types.is_empty()
                || declared_types.iter().any(|t| {
                    t == actual_type
                        || (t == "number" && value.is_number())
                        || (t == "integer" && value.as_f64().is_some_and(|n| n.fract() == 0.0))
                }),
            "{context}: 字段 {key} 声明类型不符"
        );
    }
    for key in required {
        assert!(
            actual.contains_key(key),
            "{context}: 缺少 required 字段 {key}"
        );
    }
}

#[test]
fn diagnostics_family_contract_trusted_local_only_with_strict_schemas() {
    // 真实运行时输出钉住 outputSchema；scope 元数据与授权执行一致由上方
    // scope_metadata_reflects… 与 capabilities 过滤（privileged 家族）共同钉住
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("diag.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let descriptors = operations().as_array().unwrap().clone();
    let schema_of = |name: &str| -> Value {
        let d = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        Value::Object(
            d["outputSchema"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} 必须声明 outputSchema"))
                .clone(),
        )
    };

    let summary = app.execute("diagnostics.summary", json!({})).unwrap();
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(
        summary["database_schema_version"],
        json!(wardrobe_core::DATABASE_SCHEMA_VERSION)
    );
    assert_result_matches_schema(
        &schema_of("diagnostics.summary"),
        &summary,
        "diagnostics.summary",
    );
    // 输入严格：未知字段拒绝
    let error = app
        .execute("diagnostics.summary", json!({"member_id":"x"}))
        .unwrap_err();
    assert_eq!(error.code, "invalid_input", "{error:?}");

    // 基础检查：真实运行时输出钉住 outputSchema；scope 同为仅本机可信入口
    let checks = app.execute("diagnostics.checks", json!({})).unwrap();
    assert_eq!(checks["schema_version"], 1);
    assert_result_matches_schema(
        &schema_of("diagnostics.checks"),
        &checks,
        "diagnostics.checks",
    );
    let items = checks["checks"].as_array().unwrap();
    assert_eq!(items.len(), 10, "固定十项：{checks}");
    for item in items {
        assert!(
            item["message"].is_string() && item["detail"].is_string(),
            "{item}"
        );
        assert!(
            item["setting"].is_null() || item["setting"].is_string(),
            "{item}"
        );
    }
    // 输入严格：未知字段拒绝
    let error = app
        .execute("diagnostics.checks", json!({"repair":true}))
        .unwrap_err();
    assert_eq!(error.code, "invalid_input", "{error:?}");

    // 日志查询：过滤条件与游标严格声明，运行时输出钉住 outputSchema
    let queried = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"space.","limit":1}),
        )
        .unwrap();
    assert_result_matches_schema(
        &schema_of("diagnostics.logs.query"),
        &queried,
        "diagnostics.logs.query",
    );
    assert!(
        !queried["items"].as_array().unwrap().is_empty(),
        "{queried}"
    );
    let item = &queried["items"][0];
    for key in ["ts", "level", "event_type", "run_id"] {
        assert!(
            item[key].is_string(),
            "事件公共字段 {key} 须为字符串：{item}"
        );
    }
    let empty = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"space.","until":"2020-01-01T00:00:00Z"}),
        )
        .unwrap();
    assert!(empty["items"].as_array().unwrap().is_empty());
    assert!(empty["next_cursor"].is_null(), "末页游标为 null：{empty}");
    let error = app
        .execute("diagnostics.logs.query", json!({"level":"verbose"}))
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");

    // 诊断导出：真实导出钉住 outputSchema；scope 元数据同为仅本机可信入口
    let export_dir = tempfile::tempdir().unwrap();
    let target = export_dir.path().join("contract-export.jsonl");
    let exported = app
        .execute(
            "diagnostics.export",
            json!({"path": target.to_string_lossy(), "scope":"recent"}),
        )
        .unwrap();
    assert_result_matches_schema(
        &schema_of("diagnostics.export"),
        &exported,
        "diagnostics.export",
    );
    assert!(target.exists(), "导出文件应已写入");
    // 输入严格：未知字段拒绝
    let error = app
        .execute(
            "diagnostics.export",
            json!({"path":"x.jsonl","member_id":"x"}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input", "{error:?}");
    // 范围参数互斥与校验
    for bad in [
        json!({"path":"x.jsonl","scope":"today"}),
        json!({"path":"x.jsonl","from":"2026-01-01T00:00:00Z"}),
        json!({"path":"x.jsonl","scope":"all","to":"2026-01-01T00:00:00Z"}),
    ] {
        let error = app.execute("diagnostics.export", bad.clone()).unwrap_err();
        assert_eq!(error.code, "invalid_input", "{bad}: {error:?}");
    }

    // #50 记录复现：ui_event 白名单拒绝、reproduction 标记与复现导出扩展的运行时契约
    let ui = app
        .execute(
            "diagnostics.ui_event",
            json!({"type":"view_switch","label":"穿搭库"}),
        )
        .unwrap();
    assert_result_matches_schema(
        &schema_of("diagnostics.ui_event"),
        &ui,
        "diagnostics.ui_event",
    );
    let error = app
        .execute(
            "diagnostics.ui_event",
            json!({"type":"free_text","label":"任意"}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input", "{error:?}");
    let start = app
        .execute("diagnostics.reproduction.start", json!({}))
        .unwrap();
    assert_result_matches_schema(
        &schema_of("diagnostics.reproduction.start"),
        &start,
        "diagnostics.reproduction.start",
    );
    assert!(start["started_at"].is_string(), "{start}");
    let stop = app
        .execute("diagnostics.reproduction.stop", json!({}))
        .unwrap();
    assert_result_matches_schema(
        &schema_of("diagnostics.reproduction.stop"),
        &stop,
        "diagnostics.reproduction.stop",
    );
    assert!(stop["stopped_at"].is_string(), "{stop}");
    let error = app
        .execute("diagnostics.reproduction.start", json!({"phase":"forged"}))
        .unwrap_err();
    assert_eq!(error.code, "invalid_input", "{error:?}");
    // 复现导出（kind=reproduction + attach_checks）：输出契约与普通导出同形
    let reproduction_target = export_dir.path().join("contract-reproduction.jsonl");
    let reproduction = app
        .execute(
            "diagnostics.export",
            json!({
                "path": reproduction_target.to_string_lossy(),
                "from": start["started_at"],
                "to": stop["stopped_at"],
                "kind":"reproduction",
                "attach_checks":true,
            }),
        )
        .unwrap();
    assert_result_matches_schema(
        &schema_of("diagnostics.export"),
        &reproduction,
        "diagnostics.export(reproduction)",
    );
    let first_line = std::fs::read_to_string(&reproduction_target)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_string();
    let meta: Value = serde_json::from_str(&first_line).unwrap();
    assert_eq!(meta["kind"], json!("reproduction"), "{meta}");
    assert_eq!(meta["checks"]["schema_version"], json!(1), "{meta}");

}
