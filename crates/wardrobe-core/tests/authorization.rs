use serde_json::json;
use wardrobe_core::Application;
#[test]
fn trusted_gui_can_authorize_a_limited_client_with_a_one_time_token() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let a = app.execute("space.default", json!({})).unwrap();
    let grant=app.execute("client.authorize",json!({"name":"只读助手","mode":"limited","operations":["clothing.list"],"member_ids":[a["member_id"]],"expires_at":null})).unwrap();
    assert_eq!(grant["name"], "只读助手");
    assert_eq!(grant["token"].as_str().unwrap().len(), 64);
    assert_eq!(grant["mode"], "limited");
}
#[test]
fn limited_client_can_read_one_member_but_cannot_write_or_read_another() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let a = app.execute("space.default", json!({})).unwrap();
    let b = app
        .execute("member.create", json!({"name":"另一个成员"}))
        .unwrap();
    let grant=app.execute("client.authorize",json!({"name":"只读助手","mode":"limited","operations":["clothing.list"],"member_ids":[a["member_id"]],"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap();
    assert_eq!(
        app.execute_as_client(token, "clothing.list", json!({"member_id":a["member_id"]}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        app.execute_as_client(
            token,
            "clothing.create",
            json!({"member_id":a["member_id"],"name":"越权","category":"top"})
        )
        .unwrap_err()
        .code,
        "scope_denied"
    );
    assert_eq!(
        app.execute_as_client(token, "clothing.list", json!({"member_id":b["member_id"]}))
            .unwrap_err()
            .code,
        "member_denied"
    );
}
#[test]
fn full_client_does_not_need_repeat_confirmation_and_revocation_survives_restart_without_token_leak(
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let grant=app.execute("client.authorize",json!({"name":"完整助手","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap();
    for name in ["第一件", "第二件"] {
        app.execute_as_client(
            token,
            "clothing.create",
            json!({"member_id":"default","name":name,"category":"top"}),
        )
        .unwrap();
    }
    assert_eq!(
        app.execute_as_client(
            token,
            "provider.credential.status",
            json!({"id":"anything"})
        )
        .unwrap_err()
        .code,
        "scope_denied"
    );
    assert_eq!(
        app.execute_as_client(token, "client.authorize", json!({}))
            .unwrap_err()
            .code,
        "scope_denied"
    );
    let listed = app.execute("client.list", json!({})).unwrap();
    assert!(!listed.to_string().contains(token));
    assert!(listed["items"][0].get("token_hash").is_none());
    app.execute("client.revoke", json!({"id":grant["id"]}))
        .unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    assert_eq!(
        app.execute_as_client(token, "clothing.list", json!({"member_id":"default"}))
            .unwrap_err()
            .code,
        "authorization_revoked"
    );
    drop(app);
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(path).unwrap();
            assert!(!bytes.windows(token.len()).any(|w| w == token.as_bytes()));
        }
    }
}
#[test]
fn authorization_requires_an_explicit_member_scope_instead_of_silently_granting_all() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let result = app.execute(
        "client.authorize",
        json!({"name":"遗漏范围","mode":"full","operations":[],"expires_at":null}),
    );
    assert!(matches!(result,Err(e) if e.code=="invalid_input"));
}
#[test]
fn expired_token_is_refused_and_discovery_and_member_list_hide_ungranted_data() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let a = app.execute("space.default", json!({})).unwrap();
    let b = app
        .execute("member.create", json!({"name":"不可见成员"}))
        .unwrap();
    app.execute("member.select", json!({"member_id":b["member_id"]}))
        .unwrap();
    let grant=app.execute("client.authorize",json!({"name":"临时只读","mode":"limited","operations":["member.list","clothing.list"],"member_ids":[a["member_id"]],"expires_at":(chrono::Utc::now()+chrono::Duration::seconds(2)).to_rfc3339()})).unwrap();
    let token = grant["token"].as_str().unwrap();
    let members = app
        .execute_as_client(token, "member.list", json!({}))
        .unwrap();
    assert_eq!(members["items"].as_array().unwrap().len(), 1);
    assert_eq!(members["items"][0]["member_id"], a["member_id"]);
    assert_eq!(members["selected_member_id"], serde_json::Value::Null);
    let capabilities = app
        .execute_as_client(token, "capabilities", json!({}))
        .unwrap();
    assert!(!capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["name"] == "clothing.create"));
    std::thread::sleep(std::time::Duration::from_millis(2100));
    assert_eq!(
        app.execute_as_client(token, "clothing.list", json!({"member_id":a["member_id"]}))
            .unwrap_err()
            .code,
        "authorization_expired"
    );
}
