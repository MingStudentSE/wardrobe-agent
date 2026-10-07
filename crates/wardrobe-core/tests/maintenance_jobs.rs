use serde_json::{json, Value};
use wardrobe_core::Application;
fn drive(app: &Application, operation: &str, input: Value) -> Value {
    let submitted = app.execute(operation, input).unwrap();
    if let Some(id) = submitted["job_id"].as_str() {
        app.execute("maintenance.worker.run", json!({"id":id}))
            .unwrap()["result"]
            .clone()
    } else {
        submitted
    }
}
#[test]
fn backup_create_returns_durable_job_before_creating_any_package() {
    let installation = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let db = installation.path().join("wardrobe.sqlite");
    let app = Application::open(&db).unwrap();
    let directory = archive.path().join("backup");
    let submitted = app
        .execute("backup.create", json!({"directory":directory}))
        .unwrap();
    let id = submitted["job_id"]
        .as_str()
        .expect("long operation must first return job_id");
    assert_eq!(submitted["state"], "queued");
    assert!(!directory.exists());
    drop(app);
    let app = Application::open(&db).unwrap();
    assert_eq!(
        app.execute("maintenance.get", json!({"id":id})).unwrap()["state"],
        "queued"
    );
}
#[test]
fn backup_preview_returns_job_before_reading_complete_package() {
    let installation = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let app = Application::open(&installation.path().join("wardrobe.sqlite")).unwrap();
    let directory = archive.path().join("backup");
    drive(&app, "backup.create", json!({"directory":directory}));
    let submitted = app
        .execute("backup.preview", json!({"directory":directory}))
        .unwrap();
    assert!(submitted["job_id"].is_string());
    assert_eq!(submitted["state"], "queued");
}
#[test]
fn backup_restore_returns_job_before_overwriting_any_current_fact() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let source = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let directory = archive.path().join("backup");
    drive(&source, "backup.create", json!({"directory":directory}));
    let app = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    let garment = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"still present before worker","category":"top"}),
        )
        .unwrap();
    let submitted = app
        .execute(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true}),
        )
        .unwrap();
    assert!(submitted["job_id"].is_string());
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":"default","id":garment["id"]})
        )
        .unwrap(),
        garment
    );
}
#[test]
fn directory_move_returns_job_before_publishing_location() {
    let installation = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let app = Application::open(&installation.path().join("wardrobe.sqlite")).unwrap();
    let directory = destination.path().join("moved");
    let submitted = app
        .execute("data_directory.move", json!({"directory":directory}))
        .unwrap();
    assert!(submitted["job_id"].is_string());
    assert!(!directory.exists());
}
#[test]
fn member_limited_clients_cannot_read_or_control_whole_space_jobs() {
    let root = tempfile::tempdir().unwrap();
    let app = Application::open(&root.path().join("wardrobe.sqlite")).unwrap();
    let job = app
        .execute(
            "backup.create",
            json!({"directory":root.path().join("backup")}),
        )
        .unwrap();
    let member = app.execute("member.list", json!({})).unwrap()["items"][0]["member_id"].clone();
    let grant = app.execute("client.authorize", json!({"name":"member scoped","mode":"full","operations":[],"member_ids":[member],"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap();
    for operation in [
        "maintenance.get",
        "maintenance.cancel",
        "maintenance.retry",
        "maintenance.worker.run",
    ] {
        assert_eq!(
            app.execute_as_client(token, operation, json!({"id":job["id"]}))
                .unwrap_err()
                .code,
            "member_denied",
            "{operation}"
        );
    }
    assert_eq!(
        app.execute_as_client(token, "maintenance.list", json!({}))
            .unwrap_err()
            .code,
        "member_denied"
    );
    assert!(app
        .execute_as_client(
            token,
            "jobs.get",
            json!({"member_id":member,"id":job["id"]})
        )
        .is_err());
    assert!(app
        .execute_as_client(token, "jobs.list", json!({"member_id":member}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn queued_cancel_is_durable_and_explicit_retry_preserves_intent() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("wardrobe.sqlite");
    let app = Application::open(&database).unwrap();
    let input = json!({"directory":root.path().join("backup"),"operation_key":"stable"});
    let job = app.execute("backup.create", input.clone()).unwrap();
    assert_eq!(
        app.execute("backup.create", input).unwrap()["id"],
        job["id"]
    );
    app.execute("maintenance.cancel", json!({"id":job["id"]}))
        .unwrap();
    assert!(app
        .execute("maintenance.worker.run", json!({"id":job["id"]}))
        .is_err());
    drop(app);
    let app = Application::open(&database).unwrap();
    assert_eq!(
        app.execute("maintenance.get", json!({"id":job["id"]}))
            .unwrap()["state"],
        "cancelled"
    );
    app.execute("maintenance.retry", json!({"id":job["id"]}))
        .unwrap();
    assert_eq!(
        app.execute("maintenance.worker.run", json!({"id":job["id"]}))
            .unwrap()["state"],
        "succeeded"
    );
    assert!(app
        .execute("maintenance.cancel", json!({"id":job["id"]}))
        .is_err());
}
