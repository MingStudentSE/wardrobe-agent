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
