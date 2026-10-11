use serde_json::{json, Value};
use wardrobe_core::{operations, Application};

/// 读回本进程日志中的任务事件（job.enqueue / job.state），按写入序。
fn job_events(app: &Application) -> Vec<Value> {
    let result = app
        .execute(
            "diagnostics.logs.query",
            json!({"operation":"job.","limit":100}),
        )
        .unwrap();
    result["items"].as_array().unwrap().clone()
}

/// 故障注入先例（encryption.rs 触发器注入）：jobs 每次行更新都经触发器插入一条
/// 引用不存在父行的延迟外键——语句本身成功、COMMIT 时外键检查失败，事务回滚。
fn inject_commit_failure(db: &std::path::Path) -> rusqlite::Connection {
    let trap = rusqlite::Connection::open(db).unwrap();
    trap.execute_batch(
        "CREATE TABLE commit_trap(job_id TEXT REFERENCES jobs(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER jobs_commit_trap AFTER UPDATE ON jobs BEGIN
           INSERT INTO commit_trap VALUES('commit-failure-parent');
         END;",
    )
    .unwrap();
    trap
}

/// save_job 的状态迁移事件必须在事务 commit 成功后才写入日志：
/// commit 失败（注入）不得残留未发生的事件；同一迁移随后成功提交时事件照常发出。
#[test]
fn commit_failure_leaves_no_job_transition_events() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap().to_string();
    let job = app
        .job_enqueue(&member, "local-test", "trap", json!({"input":"fixed"}))
        .unwrap();
    let id = job["id"].as_str().unwrap().to_string();
    // 入队已提交成功：恰好一条 enqueue 事件，无 state 迁移
    let before = job_events(&app);
    assert_eq!(before.len(), 1, "入队后只有 enqueue 事件：{before:?}");
    assert_eq!(before[0]["operation"], "job.enqueue");
    assert_eq!(before[0]["job_id"], json!(id));

    {
        let trap = inject_commit_failure(&db);
        let failed = app.job_claim(&member, &id, 30);
        assert_eq!(
            failed.unwrap_err().code,
            "storage_error",
            "注入的 commit 失败必须以存储错误浮出"
        );
        // commit 失败：任务仍为 queued，且不产生任何 job 事件（事件不得先于 commit）
        let events = job_events(&app);
        assert_eq!(events.len(), 1, "commit 失败不得残留 job 事件：{events:?}");
        assert_eq!(events[0]["operation"], "job.enqueue");
        drop(trap);
    }
    // 移除注入后同一迁移照常提交、事件照常发出（字段不变：job.state、state_from=queued）
    {
        let cleanup = rusqlite::Connection::open(&db).unwrap();
        cleanup
            .execute_batch("DROP TRIGGER jobs_commit_trap; DROP TABLE commit_trap;")
            .unwrap();
    }
    let claimed = app.job_claim(&member, &id, 30).unwrap();
    assert_eq!(claimed["state"], "running");
    let events = job_events(&app);
    assert_eq!(events.len(), 2, "成功提交后补上 state 迁移事件：{events:?}");
    assert_eq!(events[1]["operation"], "job.state");
    assert_eq!(events[1]["job_id"], json!(id));
    assert_eq!(events[1]["state_to"], "running");
    assert_eq!(events[1]["state_from"], "queued");
    assert_eq!(events[1]["trace_id"], job["trace_id"], "事件字段保持不变");
}

/// maintenance 链路（独立 maintenance_jobs 表）同样遵守「事件在 commit 成功后写入」：
/// 同 A3 的延迟外键触发器注入（AFTER UPDATE ON maintenance_jobs），失败路径零 job
/// 事件；移除注入后同一领取与完成迁移的事件照常发出，字段不变。
#[test]
fn maintenance_commit_failure_leaves_no_job_transition_events() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    let directory = dir.path().join("backup");
    let queued = app
        .execute("backup.create", json!({"directory":directory}))
        .unwrap();
    let id = queued["id"].as_str().unwrap().to_string();
    // 入队已提交成功：恰好一条 enqueue 事件；维护文档的 job_kind 取 operation 字段
    let before = job_events(&app);
    assert_eq!(before.len(), 1, "入队后只有 enqueue 事件：{before:?}");
    assert_eq!(before[0]["operation"], "job.enqueue");
    assert_eq!(before[0]["job_id"], json!(id));
    assert_eq!(before[0]["job_kind"], "backup.create");

    {
        let trap = rusqlite::Connection::open(&db).unwrap();
        trap.execute_batch(
            "CREATE TABLE commit_trap(job_id TEXT REFERENCES maintenance_jobs(id) DEFERRABLE INITIALLY DEFERRED);
             CREATE TRIGGER maintenance_commit_trap AFTER UPDATE ON maintenance_jobs BEGIN
               INSERT INTO commit_trap VALUES('commit-failure-parent');
             END;",
        )
        .unwrap();
        let failed = app.execute("maintenance.worker.run", json!({"id":id}));
        assert_eq!(
            failed.unwrap_err().code,
            "storage_error",
            "注入的 commit 失败必须以存储错误浮出"
        );
        // commit 失败：任务回滚为 queued，且不产生任何 job 事件（事件不得先于 commit）
        let events = job_events(&app);
        assert_eq!(events.len(), 1, "commit 失败不得残留 job 事件：{events:?}");
        assert_eq!(events[0]["operation"], "job.enqueue");
        drop(trap);
    }
    // 移除注入后同一任务照常执行：领取（queued→running）与完成（running→succeeded）
    // 的迁移事件在各自 commit 成功后照常发出，字段不变。
    {
        let cleanup = rusqlite::Connection::open(&db).unwrap();
        cleanup
            .execute_batch("DROP TRIGGER maintenance_commit_trap; DROP TABLE commit_trap;")
            .unwrap();
    }
    let finished = app
        .execute("maintenance.worker.run", json!({"id":id}))
        .unwrap();
    assert_eq!(finished["state"], "succeeded");
    let events = job_events(&app);
    assert_eq!(
        events.len(),
        3,
        "成功执行补上领取与完成迁移事件：{events:?}"
    );
    assert_eq!(events[1]["operation"], "job.state");
    assert_eq!(events[1]["job_id"], json!(id));
    assert_eq!(events[1]["state_to"], "running");
    assert_eq!(events[1]["state_from"], "queued");
    assert_eq!(events[2]["operation"], "job.state");
    assert_eq!(events[2]["state_to"], "succeeded");
    assert_eq!(events[2]["state_from"], "running");
    assert_eq!(
        events[1]["trace_id"], queued["trace_id"],
        "事件字段保持不变"
    );
}
/// 一次事务产生多个定格事件时（批量导入：每个入队一条，准备失败的条目再加一条
/// failed 迁移），事件按保存顺序、在 commit 成功后一次性发出。
#[test]
fn multi_event_batch_emits_every_frozen_event_in_order_after_commit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(4, 6, image::Rgb([245, 245, 245]))
        .save(&source)
        .unwrap();
    let bad = dir.path().join("missing.png");
    let app = Application::open(&dir.path().join("db")).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap().to_string();
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":member,"operation_key":"multi","paths":[source,source,bad]}),
        )
        .unwrap();
    let items = batch["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[2]["state"], "failed", "损坏源文件的条目入队即失败");
    let events = job_events(&app);
    assert_eq!(
        events.len(),
        4,
        "三个入队事件加一条 failed 迁移，一条不少：{events:?}"
    );
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event["operation"], if index == 3 { "job.state" } else { "job.enqueue" });
        let expected = items[if index == 3 { 2 } else { index }]["id"].clone();
        assert_eq!(event["job_id"], expected, "事件按条目顺序对位：{event:?}");
    }
    assert_eq!(events[3]["state_to"], "failed");
    assert_eq!(events[3]["state_from"], "queued");
    assert_eq!(events[3]["trace_id"], items[0]["trace_id"], "事件字段保持不变");
}

/// 双事件路径（人物导入：入队 + 同事务内立即成功）按保存序先后发出，
/// 两条事件都属于同一任务且字段定格。
#[test]
fn double_event_path_emits_enqueue_then_state_for_the_same_job() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap().to_string();
    let person = app
        .execute(
            "tryon.person.import",
            json!({"member_id":member,"path":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fictional-model.png"),"person_kind":"model","rights_confirmed":true,"operation_key":"person"}),
        )
        .unwrap();
    assert_eq!(person["state"], "succeeded");
    let events = job_events(&app);
    assert_eq!(events.len(), 2, "入队与成功迁移各一条：{events:?}");
    assert_eq!(events[0]["operation"], "job.enqueue");
    assert_eq!(events[0]["job_id"], person["id"]);
    assert_eq!(events[1]["operation"], "job.state");
    assert_eq!(events[1]["job_id"], person["id"], "两条事件同一任务");
    assert_eq!(events[1]["state_to"], "succeeded");
    assert_eq!(events[1]["state_from"], "queued");
}

#[test]
fn concurrent_workers_have_one_lease_cancel_prevents_late_finish_and_expiry_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap().to_string();
    let job = app
        .job_enqueue(&member, "local-test", "one", json!({"input":"fixed"}))
        .unwrap();
    let id = job["id"].as_str().unwrap().to_string();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut threads = vec![];
    for _ in 0..2 {
        let (db, member, id, barrier) = (db.clone(), member.clone(), id.clone(), barrier.clone());
        threads.push(std::thread::spawn(move || {
            let app = Application::open(&db).unwrap();
            barrier.wait();
            app.job_claim(&member, &id, 1)
        }));
    }
    let claimed = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(claimed.iter().filter(|r| r.is_ok()).count(), 1);
    let lease = claimed.into_iter().find_map(Result::ok).unwrap();
    app.execute("jobs.cancel", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(
        app.job_finish(
            &member,
            &id,
            lease["lease_token"].as_str().unwrap(),
            "succeeded",
            json!({}),
            Value::Null
        )
        .unwrap_err()
        .code,
        "lease_lost"
    );
    app.execute("jobs.retry", json!({"member_id":member,"id":id}))
        .unwrap();
    let lease = app.job_claim(&member, &id, 1).unwrap();
    drop(app);
    std::thread::sleep(std::time::Duration::from_millis(1050));
    let app = Application::open(&db).unwrap();
    let reclaimed = app.job_claim(&member, &id, 30).unwrap();
    assert_ne!(lease["lease_token"], reclaimed["lease_token"]);
    assert_eq!(
        app.job_finish(
            &member,
            &id,
            lease["lease_token"].as_str().unwrap(),
            "succeeded",
            json!({}),
            Value::Null
        )
        .unwrap_err()
        .code,
        "lease_lost"
    );
    app.job_finish(
        &member,
        &id,
        reclaimed["lease_token"].as_str().unwrap(),
        "succeeded",
        json!({"done":true}),
        Value::Null,
    )
    .unwrap();
}
#[test]
fn transient_retries_are_bounded_delayed_and_permanent_errors_stay_failed() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap();
    let job = app
        .job_enqueue(member, "network-test", "retry", json!({}))
        .unwrap();
    let id = job["id"].as_str().unwrap();
    for attempt in 1..=3 {
        let lease = app.job_claim(member, id, 30).unwrap();
        let result = app
            .job_finish(
                member,
                id,
                lease["lease_token"].as_str().unwrap(),
                "failed",
                Value::Null,
                json!({"code":"http_429","retryable":true,"retry_after_ms":1000}),
            )
            .unwrap();
        assert_eq!(result["attempts"], attempt);
        if attempt < 3 {
            assert_eq!(result["state"], "queued");
            assert_eq!(
                app.job_claim(member, id, 30).unwrap_err().code,
                "job_unavailable"
            );
            std::thread::sleep(std::time::Duration::from_millis(1050));
        } else {
            assert_eq!(result["state"], "failed");
        }
    }
    let job = app
        .job_enqueue(member, "network-test", "invalid", json!({}))
        .unwrap();
    let id = job["id"].as_str().unwrap();
    let lease = app.job_claim(member, id, 30).unwrap();
    assert_eq!(
        app.job_finish(
            member,
            id,
            lease["lease_token"].as_str().unwrap(),
            "failed",
            Value::Null,
            json!({"code":"http_401","retryable":false})
        )
        .unwrap()["state"],
        "failed"
    );
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
fn jobs_weather_client_capabilities_family_contract() {
    // 真实运行时输出：任务生命周期、天气配置/缓存（本机HTTP桩真实联网）、客户端授权、能力发现
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("jw.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();

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

    let caps = app.execute("capabilities", json!({})).unwrap();
    assert_result_matches_schema(&schema_of("capabilities"), &caps, "capabilities");
    assert!(caps["operations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["name"] == json!("recommendation.generate")));

    let rec = app
        .execute(
            "learning.recompute",
            json!({"member_id":member,"operation_key":"jw-1"}),
        )
        .unwrap();
    let job_id = rec["job_id"].clone();
    let got = app
        .execute("jobs.get", json!({"member_id":member,"id":job_id}))
        .unwrap();
    assert_eq!(got["state"], json!("queued"));
    assert_result_matches_schema(&schema_of("jobs.get"), &got, "jobs.get");
    let listed = app
        .execute("jobs.list", json!({"member_id":member}))
        .unwrap();
    assert_result_matches_schema(&schema_of("jobs.list"), &listed, "jobs.list");
    let cancelled = app
        .execute("jobs.cancel", json!({"member_id":member,"id":job_id}))
        .unwrap();
    assert_eq!(cancelled["state"], json!("cancelled"));
    assert_result_matches_schema(&schema_of("jobs.cancel"), &cancelled, "jobs.cancel");
    let retried = app
        .execute("jobs.retry", json!({"member_id":member,"id":job_id}))
        .unwrap();
    assert_eq!(retried["state"], json!("queued"), "本地任务取消后可重试");
    assert_result_matches_schema(&schema_of("jobs.retry"), &retried, "jobs.retry");
    let done = app
        .execute(
            "learning.worker.run",
            json!({"member_id":member,"job_id":job_id}),
        )
        .unwrap();
    assert_eq!(done["state"], json!("succeeded"));
    let err = app
        .execute("jobs.cancel", json!({"member_id":member,"id":job_id}))
        .unwrap_err();
    assert_eq!(err.code, "invalid_state", "已完成任务不能取消");

    let off = app
        .execute(
            "weather.configure",
            json!({"member_id":member,"mode":"off"}),
        )
        .unwrap();
    assert_result_matches_schema(&schema_of("weather.configure"), &off, "weather.configure");
    let got_weather = app
        .execute("weather.get", json!({"member_id":member}))
        .unwrap();
    assert_result_matches_schema(&schema_of("weather.get"), &got_weather, "weather.get");
    let refreshed_off = app
        .execute("weather.refresh", json!({"member_id":member}))
        .unwrap();
    assert_eq!(refreshed_off["cache"], Value::Null, "mode=off刷新不联网");

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 2048];
        let _ = stream.read(&mut buf);
        let body = "{\"current\":{\"temperature_2m\":21.5,\"time\":1728000000},\"current_units\":{\"temperature_2m\":\"°C\",\"time\":\"unixtime\"}}";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    let configured = app
        .execute(
            "weather.configure",
            json!({"member_id":member,"mode":"automatic","location":"上海","latitude":31.2,"longitude":121.5,"usage":"noncommercial","endpoint":format!("http://127.0.0.1:{port}/v1")}),
        )
        .unwrap();
    assert_result_matches_schema(
        &schema_of("weather.configure"),
        &configured,
        "configure-automatic",
    );
    let refreshed = app
        .execute("weather.refresh", json!({"member_id":member}))
        .unwrap();
    server.join().unwrap();
    assert_eq!(
        refreshed["cache"]["temperature_c"],
        json!(21.5),
        "refresh真实调用配置端点"
    );
    assert_eq!(refreshed["last_error"], Value::Null);
    assert_result_matches_schema(&schema_of("weather.refresh"), &refreshed, "weather.refresh");

    let granted = app
        .execute(
            "client.authorize",
            json!({"name":"契约测试客户端","mode":"limited","operations":["recommendation.generate"],"member_ids":null,"expires_at":null}),
        )
        .unwrap();
    assert!(
        granted["token"].as_str().is_some(),
        "authorize一次性返回令牌"
    );
    assert_result_matches_schema(&schema_of("client.authorize"), &granted, "client.authorize");
    let clients = app.execute("client.list", json!({})).unwrap();
    assert_result_matches_schema(&schema_of("client.list"), &clients, "client.list");
    let revoked = app
        .execute("client.revoke", json!({"id":granted["id"]}))
        .unwrap();
    assert_eq!(revoked["revoked"], json!(true));
    assert_result_matches_schema(&schema_of("client.revoke"), &revoked, "client.revoke");
}
