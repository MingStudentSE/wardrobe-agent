use serde_json::json;
use wardrobe_core::Application;

#[test]
fn member_plan_survives_restart_and_revision_protects_edit_pause() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.sqlite");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let plan = app.execute("schedule.create", json!({"member_id":"default", "name":"前夜准备", "kind":"recommendation", "local_time":"21:30", "timezone":"Asia/Shanghai", "weekdays":[1,2,3,4,5], "execution_device_id":device["id"], "occasion":"work", "target_day_offset":1, "enabled":true})).unwrap();
    assert_eq!(plan["revision"], 1);
    drop(app);
    let app = Application::open(&path).unwrap();
    let listed = app
        .execute("schedule.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(listed["plans"][0], plan);
    let edited = app.execute("schedule.update", json!({"member_id":"default","id":plan["id"], "expected_revision":1,"local_time":"20:15", "enabled":false})).unwrap();
    assert_eq!(edited["revision"], 2);
    assert_eq!(edited["enabled"], false);
    assert_eq!(edited["local_time"], "20:15");
    let stale = app
        .execute(
            "schedule.update",
            json!({"member_id":"default","id":plan["id"], "expected_revision":1,"enabled":true}),
        )
        .unwrap_err();
    assert_eq!(stale.code, "revision_conflict");
}

#[test]
fn resume_runs_only_latest_due_once_marks_old_missed_and_never_runs_other_device() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let plan=app.execute("schedule.create",json!({"member_id":"default","name":"护理","kind":"care","local_time":"21:30","timezone":"Asia/Shanghai","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true})).unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-07T22:00:00+08:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    set_eligible(&path, "2026-10-06T00:00:00Z");
    let result = app.schedule_tick_at(now).unwrap();
    assert_eq!(result["queued"].as_array().unwrap().len(), 1);
    assert_eq!(result["queued"][0]["plan_id"], plan["id"]);
    drop(app);
    let app = Application::open(&path).unwrap();
    assert_eq!(app.schedule_tick_at(now).unwrap()["queued"], json!([]));
    let old = now + chrono::Duration::days(1) + chrono::Duration::hours(3);
    assert_eq!(app.schedule_tick_at(old).unwrap()["queued"], json!([]));
    let history = app
        .execute(
            "schedule.history",
            json!({"member_id":"default","id":plan["id"]}),
        )
        .unwrap();
    assert!(history["executions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["status"] == "missed"));
    app.execute("schedule.update",json!({"member_id":"default","id":plan["id"],"expected_revision":1,"execution_device_id":uuid::Uuid::now_v7().to_string()})).unwrap();
    assert_eq!(
        app.schedule_tick_at(now + chrono::Duration::days(2))
            .unwrap()["queued"],
        json!([])
    );
}

#[test]
fn durable_care_worker_publishes_one_notification_and_cancelled_job_cannot_publish() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let now = chrono::Utc::now();
    let local = now.with_timezone(&chrono_tz::Asia::Shanghai);
    app.execute("schedule.create",json!({"member_id":"default","name":"护理","kind":"care","local_time":local.format("%H:%M").to_string(),"timezone":"Asia/Shanghai","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true})).unwrap();
    set_eligible(&path, &(now - chrono::Duration::minutes(1)).to_rfc3339());
    let queued = app.schedule_tick_at(now).unwrap();
    let job_id = &queued["queued"][0]["job_id"];
    let result = app
        .execute(
            "schedule.worker.run",
            json!({"member_id":"default","id":job_id}),
        )
        .unwrap();
    assert_eq!(result["state"], "succeeded");
    let pending = app
        .execute("schedule.notifications", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(pending["items"].as_array().unwrap().len(), 1);
    assert_eq!(pending["items"][0]["status"], "pending");
    assert!(app
        .execute(
            "schedule.worker.run",
            json!({"member_id":"default","id":job_id})
        )
        .is_err());
    assert_eq!(
        app.execute("schedule.notifications", json!({"member_id":"default"}))
            .unwrap(),
        pending
    );
    let tomorrow = app
        .schedule_tick_at(now + chrono::Duration::days(1))
        .unwrap();
    let id = &tomorrow["queued"][0]["job_id"];
    app.execute("jobs.cancel", json!({"member_id":"default","id":id}))
        .unwrap();
    assert!(app
        .execute(
            "schedule.worker.run",
            json!({"member_id":"default","id":id})
        )
        .is_err());
    assert_eq!(
        app.execute("schedule.notifications", json!({"member_id":"default"}))
            .unwrap(),
        pending
    );
}

fn set_eligible(path: &std::path::Path, at: &str) {
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute(
        "UPDATE schedule_plans SET document=json_set(document,'$.eligible_since',?1)",
        [at],
    )
    .unwrap();
}

#[test]
fn external_worker_scope_does_not_grant_care_business_or_another_member() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let now = chrono::Utc::now();
    app.execute("schedule.create",json!({"member_id":member,"name":"护理","kind":"care","local_time":now.format("%H:%M").to_string(),"timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true})).unwrap();
    set_eligible(&path, &(now - chrono::Duration::minutes(1)).to_rfc3339());
    let queued = app.schedule_tick_at(now).unwrap();
    let id = queued["queued"][0]["job_id"].clone();
    let grant=app.execute("client.authorize",json!({"name":"worker only","mode":"limited","operations":["schedule.worker.run"],"member_ids":[member],"expires_at":null})).unwrap();
    assert_eq!(
        app.execute_as_client(
            grant["token"].as_str().unwrap(),
            "schedule.worker.run",
            json!({"member_id":member,"id":id})
        )
        .unwrap_err()
        .code,
        "scope_denied"
    );
    let another = app
        .execute("member.create", json!({"name":"另一个"}))
        .unwrap();
    assert_eq!(
        app.execute_as_client(
            grant["token"].as_str().unwrap(),
            "schedule.worker.run",
            json!({"member_id":another["member_id"],"id":id})
        )
        .unwrap_err()
        .code,
        "member_denied"
    );
}

#[test]
fn creation_and_edits_are_not_retroactive_and_paused_queue_cannot_execute() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let now = chrono::Utc::now();
    let plan=app.execute("schedule.create",json!({"member_id":"default","name":"护理","kind":"care","local_time":"00:00","timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true})).unwrap();
    app.schedule_tick_at(now).unwrap();
    assert_eq!(
        app.execute(
            "schedule.history",
            json!({"member_id":"default","id":plan["id"]})
        )
        .unwrap()["executions"],
        json!([])
    );
    let tomorrow = (now.date_naive() + chrono::Duration::days(1))
        .and_hms_opt(0, 30, 0)
        .unwrap()
        .and_utc();
    let queued = app.schedule_tick_at(tomorrow).unwrap();
    let id = queued["queued"][0]["job_id"].clone();
    app.execute(
        "schedule.update",
        json!({"member_id":"default","id":plan["id"],"expected_revision":1,"enabled":false}),
    )
    .unwrap();
    assert!(app
        .execute(
            "schedule.worker.run",
            json!({"member_id":"default","id":id})
        )
        .is_err());
    assert_eq!(
        app.execute("schedule.notifications", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}

#[test]
fn long_sleep_records_the_missed_interval_and_only_latest_within_configured_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let plan=app.execute("schedule.create",json!({"member_id":"default","name":"护理","kind":"care","local_time":"09:00","timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"catchup_minutes":240,"enabled":true})).unwrap();
    set_eligible(&path, "2026-09-01T00:00:00Z");
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-07T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let queued = app.schedule_tick_at(now).unwrap();
    assert_eq!(queued["queued"].as_array().unwrap().len(), 1);
    let history = app
        .execute(
            "schedule.history",
            json!({"member_id":"default","id":plan["id"]}),
        )
        .unwrap();
    assert!(history["executions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["missed_interval"]["from"] == "2026-09-01T00:00:00+00:00"));
    assert_eq!(app.schedule_tick_at(now).unwrap()["queued"], json!([]));
}

#[test]
fn previous_night_recommendation_uses_next_local_day_and_never_records_wear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let now = chrono::Utc::now();
    for category in ["top", "bottom", "shoes"] {
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":category,"category":category}),
        )
        .unwrap();
    }
    app.execute("schedule.create",json!({"member_id":"default","name":"前夜推荐","kind":"recommendation","local_time":now.format("%H:%M").to_string(),"timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"work","target_day_offset":1,"enabled":true})).unwrap();
    set_eligible(&path, &(now - chrono::Duration::minutes(1)).to_rfc3339());
    let queued = app.schedule_tick_at(now).unwrap();
    let job = app
        .execute(
            "schedule.worker.run",
            json!({"member_id":"default","id":queued["queued"][0]["job_id"]}),
        )
        .unwrap();
    assert_eq!(job["state"], "succeeded");
    assert_eq!(
        job["result"]["controls"]["target_date"],
        queued["queued"][0]["target_date"]
    );
    assert_eq!(job["result"]["creates_wear"], false);
    assert_eq!(
        app.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}

#[test]
fn ai_schedule_keeps_notification_pending_until_real_http_worker_result_and_records_child() {
    use std::io::{Read, Write};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let now = chrono::Utc::now();
    let mut items = vec![];
    for (category, role, layer) in [
        ("top", "top", 0),
        ("bottom", "bottom", 0),
        ("shoes", "shoes", 0),
    ] {
        let c = app
            .execute(
                "clothing.create",
                json!({"member_id":"default","name":category,"category":category}),
            )
            .unwrap();
        items.push(json!({"clothing_id":c["id"],"role":role,"layer":layer}));
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let provider=app.execute("provider.save",json!({"name":"controlled TCP","endpoint":format!("http://{}/v1",listener.local_addr().unwrap()),"model":"fixture-text","capabilities":["text"],"authentication":"none"})).unwrap();
    app.execute("schedule.create",json!({"member_id":"default","name":"AI前夜","kind":"recommendation","local_time":now.format("%H:%M").to_string(),"timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":1,"enabled":true,"strategy":"ai","provider_id":provider["id"],"weather":{"mode":"off"}})).unwrap();
    set_eligible(&path, &(now - chrono::Duration::minutes(1)).to_rfc3339());
    let queued = app.schedule_tick_at(now).unwrap();
    let id = queued["queued"][0]["job_id"].clone();
    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut data = vec![];
        let mut b = [0; 4096];
        loop {
            let n = socket.read(&mut b).unwrap();
            assert!(n > 0);
            data.extend_from_slice(&b[..n]);
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&data[..end]);
                let len = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + len {
                    break;
                }
            }
        }
        seen_tx.send(()).unwrap();
        release_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let body=json!({"choices":[{"message":{"content":json!({"proposals":[{"items":items}]}).to_string()}}]}).to_string();
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let worker_path = path.clone();
    let worker_id = id.clone();
    let worker = std::thread::spawn(move || {
        Application::open(&worker_path).unwrap().execute(
            "schedule.worker.run",
            json!({"member_id":"default","id":worker_id}),
        )
    });
    seen_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        app.execute("schedule.notifications", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    release_tx.send(()).unwrap();
    let result = worker.join().unwrap().unwrap();
    server.join().unwrap();
    assert_eq!(result["state"], "succeeded");
    assert!(result["result"]["intent_id"].is_string());
    assert_eq!(
        app.execute("schedule.notifications", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn outbox_claim_survives_restart_and_does_not_blind_resubmit_unknown_native_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    app.execute(
        "schedule.runtime.configure",
        json!({"expected_revision":1,"notifications_enabled":true}),
    )
    .unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let now = chrono::Utc::now();
    app.execute("schedule.create",json!({"member_id":"default","name":"护理","kind":"care","local_time":now.format("%H:%M").to_string(),"timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true})).unwrap();
    set_eligible(&path, &(now - chrono::Duration::minutes(1)).to_rfc3339());
    let queued = app.schedule_tick_at(now).unwrap();
    app.execute(
        "schedule.worker.run",
        json!({"member_id":"default","id":queued["queued"][0]["job_id"]}),
    )
    .unwrap();
    let claimed = app.schedule_claim_notification("default").unwrap().unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    assert!(app
        .schedule_claim_notification("default")
        .unwrap()
        .is_none());
    app.schedule_finish_notification(
        "default",
        claimed["id"].as_str().unwrap(),
        claimed["claim_token"].as_str().unwrap(),
        "unknown",
        Some("受控进程中断，未证明OS是否接收"),
    )
    .unwrap();
    assert!(app
        .schedule_claim_notification("default")
        .unwrap()
        .is_none());
    assert_eq!(
        app.execute("schedule.notifications", json!({"member_id":"default"}))
            .unwrap()["items"][0]["status"],
        "unknown"
    );
}

#[test]
fn portable_plans_and_history_restore_without_device_permissions_or_repeating_same_space_execution()
{
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let now = chrono::Utc::now();
    let plan=app.execute("schedule.create",json!({"member_id":"default","name":"护理","kind":"care","local_time":now.format("%H:%M").to_string(),"timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true})).unwrap();
    set_eligible(&path, &(now - chrono::Duration::minutes(1)).to_rfc3339());
    let old = dir.path().join("old");
    maintenance(
        &app,
        "backup.create",
        json!({"directory":old,"include_originals":false}),
    );
    let queued = app.schedule_tick_at(now).unwrap();
    app.execute(
        "schedule.worker.run",
        json!({"member_id":"default","id":queued["queued"][0]["job_id"]}),
    )
    .unwrap();
    let full = dir.path().join("full");
    maintenance(
        &app,
        "backup.create",
        json!({"directory":full,"include_originals":false}),
    );
    let newdir = tempfile::tempdir().unwrap();
    let newpath = newdir.path().join("db");
    let target = Application::open(&newpath).unwrap();
    maintenance(
        &target,
        "backup.restore",
        json!({"directory":full,"confirm_overwrite":true}),
    );
    drop(target);
    let target = Application::open(&newpath).unwrap();
    assert_eq!(
        target
            .execute("schedule.list", json!({"member_id":"default"}))
            .unwrap()["plans"][0]["id"],
        plan["id"]
    );
    assert_eq!(
        target
            .execute(
                "schedule.history",
                json!({"member_id":"default","id":plan["id"]})
            )
            .unwrap()["executions"][0]["status"],
        "succeeded"
    );
    assert_ne!(
        target.execute("schedule.device", json!({})).unwrap(),
        device
    );
    assert_eq!(
        target.execute("schedule.runtime.get", json!({})).unwrap()["notifications_enabled"],
        false
    );
    assert_eq!(target.schedule_tick_at(now).unwrap()["queued"], json!([]));
    maintenance(
        &app,
        "backup.restore",
        json!({"directory":old,"confirm_overwrite":true}),
    );
    drop(app);
    let app = Application::open(&path).unwrap();
    assert_eq!(app.schedule_tick_at(now).unwrap()["queued"], json!([]));
    let db = rusqlite::Connection::open(full.join("facts.sqlite")).unwrap();
    let local_count:i64=db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'schedule_local_%' OR name='schedule_notifications'",[],|r|r.get(0)).unwrap();
    assert_eq!(local_count, 0);
}
fn maintenance(app: &Application, operation: &str, input: serde_json::Value) -> serde_json::Value {
    let receipt = app.execute(operation, input).unwrap();
    let job = app
        .execute("maintenance.worker.run", json!({"id":receipt["job_id"]}))
        .unwrap();
    assert_eq!(job["state"], "succeeded", "{job}");
    job["result"].clone()
}

#[test]
fn hidden_host_obeys_runtime_transition_and_reports_sanitized_failure_persistently() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    assert!(app.schedule_host_should_run(true).unwrap());
    assert!(!app.schedule_host_should_run(false).unwrap());
    let runtime = app.execute("schedule.runtime.get", json!({})).unwrap();
    let runtime = app
        .execute(
            "schedule.runtime.configure",
            json!({"expected_revision":runtime["revision"],"background_enabled":true}),
        )
        .unwrap();
    assert!(app.schedule_host_should_run(false).unwrap());
    app.execute(
        "schedule.runtime.configure",
        json!({"expected_revision":runtime["revision"],"background_enabled":false}),
    )
    .unwrap();
    assert!(!app.schedule_host_should_run(false).unwrap());
    app.schedule_report_runtime_error("storage_error").unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    let state = app.execute("schedule.runtime.get", json!({})).unwrap();
    assert_eq!(state["last_error"]["code"], "storage_error");
    assert!(!state["last_error"]
        .to_string()
        .contains(path.to_str().unwrap()));
}

#[test]
fn dst_overlap_uses_earlier_instant_once_and_gap_is_explained_as_missed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let plan=app.execute("schedule.create",json!({"member_id":"default","name":"DST护理","kind":"care","local_time":"01:30","timezone":"America/New_York","weekdays":[7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true})).unwrap();
    set_eligible(&path, "2026-11-01T00:00:00Z");
    let overlap = chrono::DateTime::parse_from_rfc3339("2026-11-01T06:45:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let queued = app.schedule_tick_at(overlap).unwrap();
    assert_eq!(queued["queued"].as_array().unwrap().len(), 1);
    assert_eq!(queued["queued"][0]["due_at"], "2026-11-01T05:30:00+00:00");
    assert_eq!(
        app.schedule_tick_at(overlap + chrono::Duration::minutes(15))
            .unwrap()["queued"],
        json!([])
    );
    app.execute(
        "schedule.update",
        json!({"member_id":"default","id":plan["id"],"expected_revision":1,"local_time":"02:30"}),
    )
    .unwrap();
    set_eligible(&path, "2026-03-08T00:00:00Z");
    let gap = chrono::DateTime::parse_from_rfc3339("2026-03-08T08:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(app.schedule_tick_at(gap).unwrap()["queued"], json!([]));
    let history = app
        .execute(
            "schedule.history",
            json!({"member_id":"default","id":plan["id"]}),
        )
        .unwrap();
    assert!(history["executions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["reason"] == "nonexistent_local_time"));
}

#[test]
fn stale_native_claim_becomes_visible_unknown_without_resubmitting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    app.execute(
        "schedule.runtime.configure",
        json!({"expected_revision":1,"notifications_enabled":true}),
    )
    .unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let now = chrono::Utc::now();
    app.execute("schedule.create",json!({"member_id":"default","name":"护理","kind":"care","local_time":now.format("%H:%M").to_string(),"timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true})).unwrap();
    set_eligible(&path, &(now - chrono::Duration::minutes(1)).to_rfc3339());
    let queued = app.schedule_tick_at(now).unwrap();
    app.execute(
        "schedule.worker.run",
        json!({"member_id":"default","id":queued["queued"][0]["job_id"]}),
    )
    .unwrap();
    let claimed = app.schedule_claim_notification("default").unwrap().unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    assert!(app
        .schedule_claim_notification("default")
        .unwrap()
        .is_none());
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE schedule_notifications SET document=json_set(document,'$.claimed_at',?1) WHERE id=?2",rusqlite::params![(chrono::Utc::now()-chrono::Duration::minutes(10)).to_rfc3339(),claimed["id"].as_str()]).unwrap();
    app.execute(
        "schedule.runtime.configure",
        json!({"expected_revision":2,"notifications_enabled":false}),
    )
    .unwrap();
    assert!(app
        .schedule_claim_notification("default")
        .unwrap()
        .is_none());
    assert_eq!(
        app.execute("schedule.notifications", json!({"member_id":"default"}))
            .unwrap()["items"][0]["status"],
        "unknown"
    );
}

#[test]
fn planning_day_exclusion_uses_the_event_timezone_at_target_local_noon() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let top = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"排除上衣","category":"top"}),
        )
        .unwrap();
    for (name, category) in [("备用上衣", "top"), ("裤", "bottom"), ("鞋", "shoes")] {
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
    let base=app.execute("recommendation.generate",json!({"member_id":"default","operation_key":"zone-base","timezone":"Pacific/Kiritimati","fixed_ids":[top["id"]]})).unwrap();
    let feedback=app.execute("feedback.record",json!({"member_id":"default","operation_key":"zone-feedback","intent_id":base["id"],"proposal_index":0,"reaction":"day_exclude","timezone":"Pacific/Kiritimati","clothing_ids":[top["id"]]})).unwrap();
    let day = &feedback["day_scope"]["local_date"];
    assert_eq!(app.execute("recommendation.generate",json!({"member_id":"default","operation_key":"zone-same","timezone":"Pacific/Kiritimati","target_date":day,"fixed_ids":[top["id"]]})).unwrap_err().code,"constraint_conflict");
    // Honolulu noon on the same date is Kiritimati noon on the next date.
    // Comparing date strings would incorrectly keep yesterday's exclusion active.
    let result=app.execute("recommendation.generate",json!({"member_id":"default","operation_key":"zone-next","timezone":"Pacific/Honolulu","target_date":day,"fixed_ids":[top["id"]]})).unwrap();
    assert!(!result["proposals"].as_array().unwrap().is_empty());
    assert_eq!(result["controls"]["target_date"], *day);
}

#[test]
fn generic_ai_worker_cannot_bypass_scheduled_original_business_scope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    for category in ["top", "bottom", "shoes"] {
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":category,"category":category}),
        )
        .unwrap();
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    drop(listener);
    let provider=app.execute("provider.save",json!({"name":"unbound synthetic endpoint","endpoint":endpoint,"model":"fixture","capabilities":["text"],"authentication":"none"})).unwrap();
    let now = chrono::Utc::now();
    app.execute("schedule.create",json!({"member_id":"default","name":"AI计划","kind":"recommendation","local_time":now.format("%H:%M").to_string(),"timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":1,"enabled":true,"strategy":"ai","provider_id":provider["id"]})).unwrap();
    set_eligible(&path, &(now - chrono::Duration::minutes(1)).to_rfc3339());
    let queued = app.schedule_tick_at(now).unwrap();
    let grant=app.execute("client.authorize",json!({"name":"generic worker only","mode":"limited","operations":["recommendation.worker.run"],"member_ids":["default"],"expires_at":null})).unwrap();
    let result = app.execute_as_client(
        grant["token"].as_str().unwrap(),
        "recommendation.worker.run",
        json!({"member_id":"default","id":queued["queued"][0]["job_id"]}),
    );
    assert!(
        matches!(result,Err(ref e) if e.code=="scope_denied"),
        "{result:?}"
    );
}

#[test]
fn disabling_notifications_between_claims_preserves_the_next_pending_notice() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let now = chrono::Utc::now();
    app.execute(
        "schedule.runtime.configure",
        json!({"expected_revision":1,"notifications_enabled":true}),
    )
    .unwrap();
    for name in ["护理一", "护理二"] {
        app.execute("schedule.create",json!({"member_id":"default","name":name,"kind":"care","local_time":now.format("%H:%M").to_string(),"timezone":"UTC","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true})).unwrap();
    }
    set_eligible(&path, &(now - chrono::Duration::minutes(1)).to_rfc3339());
    let queued = app.schedule_tick_at(now).unwrap();
    for execution in queued["queued"].as_array().unwrap() {
        app.execute(
            "schedule.worker.run",
            json!({"member_id":"default","id":execution["job_id"]}),
        )
        .unwrap();
    }
    assert!(app
        .schedule_claim_notification("default")
        .unwrap()
        .is_some());
    app.execute(
        "schedule.runtime.configure",
        json!({"expected_revision":2,"notifications_enabled":false}),
    )
    .unwrap();
    assert!(app
        .schedule_claim_notification("default")
        .unwrap()
        .is_none());
    let notices = app
        .execute("schedule.notifications", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(
        notices["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["status"] == "pending")
            .count(),
        1
    );
}
