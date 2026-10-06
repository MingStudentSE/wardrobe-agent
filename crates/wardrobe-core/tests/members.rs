use serde_json::json;
use wardrobe_core::Application;
#[test]
fn member_profiles_and_selection_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    assert_eq!(space["name"], "我");
    assert_eq!(
        uuid::Uuid::parse_str(space["member_id"].as_str().unwrap())
            .unwrap()
            .get_version_num(),
        7
    );
    let second = app
        .execute("member.create", json!({"name":"小明"}))
        .unwrap();
    app.execute(
        "member.update",
        json!({"member_id":second["member_id"],"name":"小铭"}),
    )
    .unwrap();
    app.execute("member.select", json!({"member_id":second["member_id"]}))
        .unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    assert_eq!(
        app.execute("space.default", json!({})).unwrap()["selected_member_id"],
        second["member_id"]
    );
    assert_eq!(
        app.execute("member.list", json!({})).unwrap()["items"][1]["name"],
        "小铭"
    );
}
#[test]
fn member_inventory_and_wrong_member_requests_are_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let a = app.execute("space.default", json!({})).unwrap();
    let b = app
        .execute("member.create", json!({"name":"另一位"}))
        .unwrap();
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":a["member_id"],"name":"白衬衫","category":"top"}),
        )
        .unwrap();
    assert_eq!(
        app.execute("clothing.list", json!({"member_id":b["member_id"]}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":b["member_id"],"id":item["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    assert_eq!(
        app.execute("member.select", json!({"member_id":"missing"}))
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":"别名","category":"top"})
        )
        .unwrap()["member_id"],
        a["member_id"]
    );
}
#[test]
fn legacy_default_profile_keeps_custom_name_and_inventory_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE members(id TEXT PRIMARY KEY,name TEXT NOT NULL);CREATE TABLE clothing(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),document TEXT NOT NULL);CREATE TABLE audit(id INTEGER PRIMARY KEY,entity_id TEXT NOT NULL,operation TEXT NOT NULL,document TEXT NOT NULL);CREATE TABLE outbox(id INTEGER PRIMARY KEY,entity_id TEXT NOT NULL,document TEXT NOT NULL);INSERT INTO members VALUES('default','旧成员名字');PRAGMA user_version=2;").unwrap();
        let document = json!({"schema_version":1,"id":"018fb532-c427-7bb9-b84a-97ee42a5e112","member_id":"default","name":"保留衣物","category":"top","seasons":[],"purchase_price_minor":null,"price_exponent":null,"currency":null,"size_label":null,"status":"active","favorite":false,"created_at":"2025-01-01T00:00:00Z","updated_at":"2025-01-01T00:00:00Z"});
        db.execute(
            "INSERT INTO clothing VALUES(?1,'default',?2)",
            rusqlite::params![document["id"].as_str().unwrap(), document.to_string()],
        )
        .unwrap();
    }
    let app = Application::open(&path).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    assert_eq!(space["name"], "旧成员名字");
    let item = app
        .execute(
            "clothing.get",
            json!({"member_id":space["member_id"],"id":"018fb532-c427-7bb9-b84a-97ee42a5e112"}),
        )
        .unwrap();
    assert_eq!(item["created_at"], "2025-01-01T00:00:00Z");
    assert_eq!(item["member_id"], space["member_id"]);
    drop(app);
    assert_eq!(
        Application::open(&path)
            .unwrap()
            .execute("space.default", json!({}))
            .unwrap(),
        space
    );
}
#[test]
fn unsupported_profile_fields_are_rejected_instead_of_silently_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    assert_eq!(
        app.execute("member.create", json!({"name":"成员","role":"owner"}))
            .unwrap_err()
            .code,
        "invalid_input"
    );
}
