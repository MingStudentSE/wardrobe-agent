use serde_json::{json, Value};
use wardrobe_core::Application;
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
