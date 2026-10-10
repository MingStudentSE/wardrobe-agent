use serde_json::{json, Value};
use wardrobe_core::Application;

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
