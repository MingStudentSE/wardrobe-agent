//! 日志核心集成测试：只观察外部行为（执行操作后的 logs/ JSONL 文件与公开错误输出），
//! 不直接测 logger 内部 API。先例：application.rs、jobs.rs。

use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex, MutexGuard};
use wardrobe_core::Application;

/// panic 钩子是进程级的：触发时向本进程注册过的全部日志汇写 panic 事件，
/// 进而驱动其他测试 logger 的轮转与惰性清理（删旧文件）。用进程内互斥把
/// "会触发 panic 写入"的测试与"对 logs/ 做文件断言"的测试隔开，
/// 保证 `--test-threads` 并行下确定性（#42）。
static LOG_FS_LOCK: Mutex<()> = Mutex::new(());
fn fs_lock() -> MutexGuard<'static, ()> {
    LOG_FS_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
    // 本测试的 logger 上限极小，其他测试触发的 panic 钩子写入会驱动它清理；
    // 与触发 panic 写入的测试互斥，避免并发断言文件时被删（#42）
    let _guard = fs_lock();
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
    app.execute("jobs.cancel", json!({"member_id":member,"id":cancelled["id"]}))
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
    let listed = app.execute("jobs.list", json!({"member_id":member})).unwrap();
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
    for operation in ["diagnostics.summary", "diagnostics.logs.query", "diagnostics.export"] {
        let denied = app.execute_as_client(token, operation, json!({})).unwrap_err();
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
    let caps = app.execute_as_client(token, "capabilities", json!({})).unwrap();
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
    assert_eq!(until_now["items"].as_array().unwrap().len(), 1, "until 含端点");

    // 关键词：对事件序列化文本做大小写不敏感子串匹配
    let keyword = app
        .execute(
            "diagnostics.logs.query",
            json!({"keyword":"invalid_input","operation":"clothing."}),
        )
        .unwrap();
    assert_eq!(keyword["items"].as_array().unwrap().len(), 1, "{keyword}");
    let none = app
        .execute("diagnostics.logs.query", json!({"keyword":"彻底不存在的词"}))
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
        let error = app.execute("diagnostics.logs.query", bad.clone()).unwrap_err();
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
    assert!(replay["next_cursor"].is_null(), "末页 next_cursor 为 null：{replay}");
    // 越过末尾的游标：空页且 next_cursor 为 null
    let exhausted = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"clothing.create","cursor":"99999999999999-0"}),
        )
        .unwrap();
    assert_eq!(exhausted["items"].as_array().unwrap().len(), 0, "{exhausted}");
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
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
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

/// #45 diagnostics.export：把 fabricated 事件追加进数据目录 logs/ 的当日文件，
/// 供范围选择测试控制事件内容（与 corrupt 测试同一手法）。
fn append_events(dir: &std::path::Path, events: &[Value]) {
    let logs = dir.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    let today = chrono::Local::now().date_naive().format("%Y-%m-%d");
    let path = logs.join(format!("wardrobe-{today}.jsonl"));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    for event in events {
        writeln!(file, "{event}").unwrap();
    }
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
    assert_eq!(output["covered_to"], events.last().unwrap()["ts"], "{output}");
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
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    // 两个“其他运行会话”的外部事件：25 小时前（只有 all 含）与 2 小时前（recent 含）
    let now = chrono::Utc::now();
    let stamp = |hours: i64| {
        (now - chrono::Duration::hours(hours)).to_rfc3339_opts(
            chrono::SecondsFormat::Millis,
            true,
        )
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
        events.iter().all(|e| e["run_id"] == json!(wardrobe_core::diagnostics::run_id())),
        "session 只含当前运行会话：{events:?}"
    );
    assert!(contains(&events, "space.default"), "{events:?}");
    assert!(!contains(&events, "import.old") && !contains(&events, "recognition.crash"));

    // recent（显式传参）：最近 24 小时跨重启，含 2 小时前、不含 25 小时前
    let (_, meta, events) = export_lines(&app, &target, json!({"scope":"recent"}));
    assert_eq!(meta["scope"], "recent", "{meta}");
    assert!(contains(&events, "recognition.crash"), "recent 含 24h 内其他运行：{events:?}");
    assert!(contains(&events, "space.default"));
    assert!(!contains(&events, "import.old"), "recent 不含 24h 前事件：{events:?}");

    // all：保留期内全部，含最旧事件
    let (_, meta, events) = export_lines(&app, &target, json!({"scope":"all"}));
    assert_eq!(meta["scope"], "all", "{meta}");
    assert!(contains(&events, "import.old"), "all 含保留期内最旧事件：{events:?}");
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
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let first = Application::open(&db).unwrap();
    first.execute("space.default", json!({})).unwrap();
    // 上次运行的最后现场：panic 事件带它自己的 run_id
    let crashed_run = "018f0000-0000-7000-8000-00000000cafe";
    let crash_ts = chrono::Utc::now()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
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
    let (_, meta, events) =
        export_lines(&second, &dir.path().join("recent.jsonl"), json!({}));
    assert!(
        events
            .iter()
            .any(|e| e["run_id"] == json!(crashed_run) && e["event_type"] == "panic"),
        "recent 必须含崩溃前上次运行的事件：{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| e["operation"] == "space.default" && e["run_id"] == json!(wardrobe_core::diagnostics::run_id())),
        "recent 也含本次运行事件：{events:?}"
    );
    let run_ids = meta["run_ids"].as_array().unwrap();
    assert!(run_ids.contains(&json!(crashed_run)), "{meta}");
    assert!(run_ids.contains(&json!(wardrobe_core::diagnostics::run_id())), "{meta}");
    assert!(
        meta["recent_errors"].as_array().unwrap().iter().any(|e| e["run_id"] == json!(crashed_run)),
        "meta 最近错误含上次运行的 panic：{meta}"
    );
    // 对照：session 只含第二次运行——跨重启的包含关系确实由时间窗决定
    let (_, meta, events) =
        export_lines(&second, &dir.path().join("session.jsonl"), json!({"scope":"session"}));
    assert!(
        events.iter().all(|e| e["run_id"] != json!(crashed_run)),
        "session 不得含上次运行：{events:?}"
    );
    assert!(!meta["run_ids"].as_array().unwrap().contains(&json!(crashed_run)), "{meta}");
}

#[test]
fn export_skips_corrupt_lines_without_blocking() {
    // 尾行截断等单文件损坏不阻断导出（与查询/摘要同一容错约定）
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    append_events(
        dir.path(),
        &[json!({"ts": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true), "level":"info", "event_type":"operation", "run_id": wardrobe_core::diagnostics::run_id(), "operation":"fabricated.valid", "result":"ok"})],
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
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{\"ts\":\"截断的JSON").unwrap();
        writeln!(file, "这不是JSON").unwrap();
        writeln!(file, "{{\"event_type\":\"缺公共字段\"}}").unwrap();
    }
    let (_, meta, events) = export_lines(&app, &dir.path().join("all.jsonl"), json!({"scope":"all"}));
    let content = meta.to_string();
    for fragment in ["截断的JSON", "这不是JSON", "缺公共字段"] {
        assert!(!content.contains(fragment) && !events.iter().any(|e| e.to_string().contains(fragment)), "损坏行不得进入导出：{events:?}");
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
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let edge = "2026-06-01T12:00:00.000Z";
    append_events(
        dir.path(),
        &[json!({"ts": edge, "level":"info", "event_type":"operation", "run_id": "018f0000-0000-7000-8000-00000000edge", "operation":"window.edge", "result":"ok"})],
    );
    let target = dir.path().join("edge.jsonl");
    // from = to = 事件时刻：端点含入
    let (output, _, events) =
        export_lines(&app, &target, json!({"from": edge, "to": edge}));
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
    assert!(
        app.execute(
            "provider.credential.set",
            json!({"id":"any-provider","api_key":api_key}),
        )
        .is_err()
    );
    use sha2::{Digest, Sha256};
    let token_hash: String = Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let (_, meta, events) = export_lines(&app, &dir.path().join("export.jsonl"), json!({}));
    let meta_text = meta.to_string();
    let events_text: Vec<String> = events.iter().map(|e| e.to_string()).collect();
    for secret in [token, &token_hash, api_key] {
        assert!(!meta_text.contains(secret), "meta 不得含 {secret}：{meta_text}");
        for (index, text) in events_text.iter().enumerate() {
            assert!(!text.contains(secret), "事件行 {index} 不得含凭据：{text}");
        }
    }
    // 产物只来自日志文件与摘要投影：不含数据库字节与图片内容。
    // 结构性证明——导出每行都是合法 JSON 事件，数据库魔数与二进制图像不可能出现。
    assert!(!meta_text.contains("SQLite format 3"), "{meta_text}");
    assert!(
        events.iter().all(|e| e["event_type"].is_string() && e["ts"].is_string() && e["level"].is_string()),
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
    assert!(scene["location"].as_str().unwrap().contains("diagnostics.rs"), "{scene}");
}
