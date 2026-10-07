use serde_json::json;
use wardrobe_core::Application;
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(app) = Application::open(&self.0) {
            let _ = app.execute("encryption.unpair", json!({}));
        }
    }
}

#[test]
#[ignore = "requires actual OS credential store"]
fn sync_run_returns_a_durable_job_before_publication_and_survives_restart_and_cancel() {
    let local = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let db = local.path().join("wardrobe.sqlite");
    let _cleanup = Cleanup(db.clone());
    let app = Application::open(&db).unwrap();
    app.execute(
        "encryption.create",
        json!({"password":"temporary sync job password","device_name":"sync job owner"}),
    )
    .unwrap();
    app.execute(
        "sync.configure",
        json!({"provider_kind":"local_folder","directory":remote.path()}),
    )
    .unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let submitted = app.execute("sync.run", json!({})).unwrap();
    let id = submitted["job_id"]
        .as_str()
        .expect("long sync must return a durable job_id before doing storage work")
        .to_owned();
    assert_eq!(
        std::fs::read_dir(remote.path()).unwrap().count(),
        0,
        "enqueue must not publish logs"
    );
    drop(app);
    let app = Application::open(&db).unwrap();
    let job = app
        .execute("jobs.get", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["kind"], "sync");
    assert_eq!(job["state"], "queued");
    let grant=app.execute("client.authorize",json!({"name":"limited member sync job","mode":"limited","operations":["sync.run","sync.process","jobs.get","jobs.list","jobs.cancel","jobs.retry"],"member_ids":[member],"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap();
    for op in ["jobs.get", "jobs.cancel", "jobs.retry"] {
        assert_eq!(
            app.execute_as_client(token, op, json!({"member_id":member,"id":id}))
                .unwrap_err()
                .code,
            "member_denied"
        );
    }
    assert!(app
        .execute_as_client(token, "jobs.list", json!({"member_id":member}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        app.execute_as_client(token, "sync.process", json!({"id":id,"member_id":member}))
            .unwrap_err()
            .code,
        "member_denied"
    );
    let member_job = app
        .job_enqueue(
            member.as_str().unwrap(),
            "member_fixture",
            "ordinary member job",
            json!({}),
        )
        .unwrap();
    assert_eq!(
        app.execute_as_client(
            token,
            "jobs.get",
            json!({"member_id":member,"id":member_job["id"]})
        )
        .unwrap()["kind"],
        "member_fixture"
    );
    let visible = app
        .execute_as_client(token, "jobs.list", json!({"member_id":member}))
        .unwrap();
    assert_eq!(visible["items"].as_array().unwrap().len(), 1);
    let all=app.execute("client.authorize",json!({"name":"all members read jobs only","mode":"limited","operations":["jobs.get"],"member_ids":null,"expires_at":null})).unwrap();
    let all_token = all["token"].as_str().unwrap();
    assert_eq!(
        app.execute_as_client(all_token, "jobs.get", json!({"member_id":member,"id":id}))
            .unwrap()["kind"],
        "sync"
    );
    assert_eq!(
        app.execute_as_client(all_token, "sync.run", json!({}))
            .unwrap_err()
            .code,
        "scope_denied"
    );
    let capabilities = app
        .execute_as_client(token, "capabilities", json!({}))
        .unwrap();
    assert!(capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|op| op["name"] != "sync.process"));
    app.execute("jobs.cancel", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(
        app.execute("sync.process", json!({"id":id}))
            .unwrap_err()
            .code,
        "job_unavailable"
    );
    assert_eq!(std::fs::read_dir(remote.path()).unwrap().count(), 0);
    app.execute("encryption.unpair", json!({})).unwrap();
}
