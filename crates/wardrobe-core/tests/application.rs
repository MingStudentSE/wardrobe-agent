use serde_json::json;
use wardrobe_core::Application;
#[test]
fn no_key_first_entry_survives_restart_with_unknown_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&path).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let created = app.execute("clothing.create", json!({"member_id":space["member_id"],"name":"白色衬衫","category":"top","seasons":["spring","summer"],"purchase_price_minor":12900,"currency":"CNY"})).unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    let read = app
        .execute(
            "clothing.get",
            json!({"id":created["id"],"member_id":space["member_id"]}),
        )
        .unwrap();
    assert_eq!(read["name"], "白色衬衫");
    assert_eq!(read["purchase_price_minor"], 12900);
    assert_eq!(read["size_label"], serde_json::Value::Null);
    assert_eq!(read["seasons"], json!(["spring", "summer"]));
    assert_eq!(read["schema_version"], 1);
}
#[test]
fn invalid_page_and_member_are_typed_errors_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    assert_eq!(
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":" ","category":"top"})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
    assert_eq!(
        app.execute("clothing.list", json!({"member_id":"missing"}))
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        app.execute(
            "clothing.list",
            json!({"member_id":"default","limit":"many"})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
    assert_eq!(
        app.execute("clothing.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}
#[test]
fn pagination_preserves_every_created_item_without_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    for name in ["第一件", "第二件", "第三件"] {
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":"other"}),
        )
        .unwrap();
    }
    let first = app
        .execute("clothing.list", json!({"member_id":"default","limit":2}))
        .unwrap();
    let second = app
        .execute(
            "clothing.list",
            json!({"member_id":"default","limit":2,"cursor":first["next_cursor"]}),
        )
        .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["next_cursor"], serde_json::Value::Null);
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
}
#[test]
fn invalid_currency_and_fractional_minor_units_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    assert_eq!(app.execute("clothing.create",json!({"member_id":"default","name":"上衣","category":"top","purchase_price_minor":100,"currency":"ZZZ"})).unwrap_err().code,"invalid_input");
    assert_eq!(app.execute("clothing.create",json!({"member_id":"default","name":"上衣","category":"top","purchase_price_minor":1.5,"currency":"CNY"})).unwrap_err().code,"invalid_input");
}
#[test]
fn currency_exponent_keeps_yen_and_dinar_prices_unambiguous() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let yen=app.execute("clothing.create",json!({"member_id":"default","name":"日本购入衬衫","category":"top","purchase_price_minor":12900,"currency":"JPY"})).unwrap();
    assert_eq!(yen["price_exponent"], 0);
    let dinar=app.execute("clothing.create",json!({"member_id":"default","name":"纪念外套","category":"outerwear","purchase_price_minor":12900,"currency":"BHD"})).unwrap();
    assert_eq!(dinar["price_exponent"], 3);
}
#[test]
fn newer_database_version_is_refused_instead_of_downgraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("future.sqlite");
    // External database fixture, not an assertion against internal storage details.
    let fixture = rusqlite::Connection::open(&path).unwrap();
    fixture.execute_batch("PRAGMA user_version=99;").unwrap();
    drop(fixture);
    assert!(matches!(Application::open(&path),Err(error) if error.code=="unsupported_schema"));
}
#[test]
fn wrong_type_cursor_is_rejected_instead_of_restarting_page() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    assert_eq!(
        app.execute("clothing.list", json!({"member_id":"default","cursor":123}))
            .unwrap_err()
            .code,
        "invalid_input"
    );
}
#[test]
fn discovered_operations_describe_output_required_fields_for_consumers() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let capabilities = app.execute("capabilities", json!({})).unwrap();
    for operation in capabilities["operations"].as_array().unwrap() {
        assert_eq!(operation["outputSchema"]["type"], "object");
        assert!(operation["outputSchema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("schema_version")));
    }
    let schema = &capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "clothing.create")
        .unwrap()["outputSchema"];
    let doc = app.execute("space.default", json!({})).unwrap();
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":doc["member_id"],"name":"羊毛外套","category":"outerwear"}),
        )
        .unwrap();
    for field in schema["required"].as_array().unwrap() {
        assert!(item.get(field.as_str().unwrap()).is_some());
    }
    assert_eq!(
        schema["properties"]["purchase_price_minor"]["type"],
        json!(["integer", "null"])
    );
}
#[test]
fn supported_early_database_migrates_price_and_category_without_losing_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("early.sqlite");
    let fixture = rusqlite::Connection::open(&path).unwrap();
    fixture.execute_batch("CREATE TABLE members(id TEXT PRIMARY KEY,name TEXT NOT NULL); CREATE TABLE clothing(id TEXT PRIMARY KEY,member_id TEXT NOT NULL,document TEXT NOT NULL); INSERT INTO members VALUES('default','我的衣橱'); PRAGMA user_version=1;").unwrap();
    let original = json!({"schema_version":1,"id":"early-shirt","member_id":"default","name":"白色衬衫","category":"jumpsuit","seasons":["spring","autumn"],"price_minor":19990,"currency":"CNY","size_label":null,"status":"active","favorite":false,"created_at":"2026-10-07T00:00:00Z","updated_at":"2026-10-07T00:00:00Z"});
    fixture
        .execute(
            "INSERT INTO clothing VALUES('early-shirt','default',?1)",
            [original.to_string()],
        )
        .unwrap();
    drop(fixture);
    let app = Application::open(&path).unwrap();
    let item = app
        .execute(
            "clothing.get",
            json!({"id":"early-shirt","member_id":"default"}),
        )
        .unwrap();
    assert_eq!(item["purchase_price_minor"], 19990);
    assert_eq!(item["price_exponent"], 2);
    assert_eq!(item["category"], "one_piece");
    assert_eq!(item["id"], "early-shirt");
    assert_eq!(item["created_at"], "2026-10-07T00:00:00Z");
    drop(app);
    let reopened = Application::open(&path).unwrap();
    assert_eq!(
        reopened
            .execute(
                "clothing.get",
                json!({"id":"early-shirt","member_id":"default"})
            )
            .unwrap(),
        item
    );
}
