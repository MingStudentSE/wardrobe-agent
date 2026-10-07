use serde_json::{json, Value};
use wardrobe_core::Application;
#[test]
fn saved_lookbook_keeps_versions_independent_from_clothing_and_clone() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let garment = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"白衬衫","category":"top"}),
        )
        .unwrap();
    let outfit=app.execute("outfit.create",json!({"member_id":"default","name":"周末","items":[{"clothing_id":garment["id"],"role":"base","layer":0}]})).unwrap();
    assert_eq!(outfit["version"], 1);
    assert_eq!(outfit["items"][0]["snapshot"]["name"], "白衬衫");
    let old = outfit.clone();
    app.execute(
        "clothing.update",
        json!({"member_id":"default","id":garment["id"],"name":"已改名"}),
    )
    .unwrap();
    let updated=app.execute("outfit.update",json!({"member_id":"default","id":outfit["id"],"expected_version":1,"name":"周末新版","items":[{"clothing_id":garment["id"],"role":"outer","layer":2}]})).unwrap();
    assert_eq!(updated["version"], 2);
    let version = app
        .execute(
            "outfit.version",
            json!({"member_id":"default","id":outfit["id"],"version":1}),
        )
        .unwrap();
    assert_eq!(version, old);
    let cloned = app
        .execute(
            "outfit.clone",
            json!({"member_id":"default","id":outfit["id"],"name":"另一个周末"}),
        )
        .unwrap();
    assert_ne!(cloned["id"], outfit["id"]);
    let collage = app
        .execute(
            "outfit.render_local",
            json!({"member_id":"default","id":outfit["id"]}),
        )
        .unwrap();
    assert!(collage["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/webp;base64,"));
    assert_eq!(collage["missing_images"], 1);
    app.execute(
        "outfit.delete",
        json!({"member_id":"default","id":outfit["id"]}),
    )
    .unwrap();
    let restored = app
        .execute(
            "outfit.restore",
            json!({"member_id":"default","id":outfit["id"]}),
        )
        .unwrap();
    assert_eq!(restored["status"], "active");
    assert_eq!(restored["items"], updated["items"]);
    drop(app);
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        app.execute(
            "outfit.version",
            json!({"member_id":"default","id":outfit["id"],"version":1})
        )
        .unwrap(),
        old
    );
    let _: Value = cloned;
}
#[test]
fn manual_templates_accept_all_categories_but_reject_invalid_structure_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let mut items = vec![];
    let mut garments = vec![];
    for category in [
        "top",
        "bottom",
        "outerwear",
        "dress",
        "one_piece",
        "shoes",
        "bag",
        "accessory",
        "underwear",
        "sportswear",
        "other",
    ] {
        let c = app
            .execute(
                "clothing.create",
                json!({"member_id":"default","name":category,"category":category}),
            )
            .unwrap();
        items.push(json!({"clothing_id":c["id"],"role":category,"layer":items.len()}));
        garments.push(c);
    }
    let second = app
        .execute("member.create", json!({"name":"另一位"}))
        .unwrap();
    let foreign = app
        .execute(
            "clothing.create",
            json!({"member_id":second["member_id"],"name":"他人外套","category":"outerwear"}),
        )
        .unwrap();
    for bad in [
        json!([]),
        json!([items[0], items[0]]),
        json!([{"clothing_id":foreign["id"],"role":"外套","layer":0}]),
        json!([{"clothing_id":"missing","role":"外套","layer":0}]),
        json!([{"clothing_id":garments[0]["id"],"role":"","layer":0}]),
        json!([{"clothing_id":garments[0]["id"],"role":"base","layer":0.5}]),
    ] {
        assert!(app
            .execute(
                "outfit.create",
                json!({"member_id":"default","name":"无效","items":bad})
            )
            .is_err());
    }
    assert_eq!(
        app.execute("outfit.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    let outfit = app
        .execute(
            "outfit.create",
            json!({"member_id":"default","name":"全部类别","items":items}),
        )
        .unwrap();
    assert_eq!(outfit["items"].as_array().unwrap().len(), 11);
    app.execute(
        "outfit.favorite",
        json!({"member_id":"default","id":outfit["id"],"favorite":true}),
    )
    .unwrap();
    assert_eq!(
        app.execute(
            "outfit.list",
            json!({"member_id":"default","favorite":true})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for c in garments {
        assert_eq!(
            app.execute("clothing.get", json!({"member_id":"default","id":c["id"]}))
                .unwrap(),
            c
        );
        let history = app
            .execute(
                "clothing.history",
                json!({"member_id":"default","id":c["id"]}),
            )
            .unwrap();
        assert_eq!(history["items"].as_array().unwrap().len(), 1);
    }
    assert!(app
        .execute(
            "outfit.get",
            json!({"member_id":second["member_id"],"id":outfit["id"]})
        )
        .is_err());
    let grant=app.execute("client.authorize",json!({"name":"只读搭配","mode":"limited","operations":["outfit.get"],"expires_at":null,"member_ids":[second["member_id"]]})).unwrap();
    assert_eq!(
        app.execute_as_client(
            grant["token"].as_str().unwrap(),
            "outfit.get",
            json!({"member_id":"default","id":outfit["id"]})
        )
        .unwrap_err()
        .code,
        "member_denied"
    );
}
#[test]
fn collage_uses_immutable_adopted_display_and_retains_version_asset_after_detach() {
    use base64::Engine;
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let source = dir.path().join("red.png");
    image::RgbaImage::from_pixel(64, 32, image::Rgba([200, 20, 20, 255]))
        .save(&source)
        .unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"红衣","category":"dress"}),
        )
        .unwrap();
    let asset = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":c["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    let outfit=app.execute("outfit.create",json!({"member_id":"default","name":"红衣搭配","items":[{"clothing_id":c["id"],"role":"主衣","layer":0}]})).unwrap();
    app.execute(
        "assets.detach",
        json!({"member_id":"default","clothing_id":c["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    let retained = app
        .execute(
            "assets.get",
            json!({"member_id":"default","id":asset["id"]}),
        )
        .unwrap();
    assert_eq!(retained["display"]["sync_scope"], "workspace");
    let collage = app
        .execute(
            "outfit.render_local",
            json!({"member_id":"default","id":outfit["id"]}),
        )
        .unwrap();
    assert_eq!(collage["missing_images"], 0);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            collage["data_url"]
                .as_str()
                .unwrap()
                .split_once(',')
                .unwrap()
                .1,
        )
        .unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap().to_rgb8();
    let pixel = decoded.get_pixel(400, 240);
    assert!(pixel[0] > 180 && pixel[1] < 40 && pixel[2] < 40);
}
#[test]
fn editing_requires_the_version_seen_by_the_caller_and_preserves_conflicting_template() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"单品","category":"top"}),
        )
        .unwrap();
    let items = json!([{"clothing_id":c["id"],"role":"base","layer":0}]);
    let o = app
        .execute(
            "outfit.create",
            json!({"member_id":"default","name":"原版","items":items}),
        )
        .unwrap();
    assert_eq!(
        app.execute(
            "outfit.update",
            json!({"member_id":"default","id":o["id"],"name":"未说明版本","items":items})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
    let changed=app.execute("outfit.update",json!({"member_id":"default","id":o["id"],"expected_version":1,"name":"新版本","items":items})).unwrap();
    assert_eq!(app.execute("outfit.update",json!({"member_id":"default","id":o["id"],"expected_version":1,"name":"覆盖新版本","items":items})).unwrap_err().code,"version_conflict");
    assert_eq!(
        app.execute("outfit.get", json!({"member_id":"default","id":o["id"]}))
            .unwrap(),
        changed
    );
}
#[test]
fn existing_version_five_inventory_survives_lookbook_migration_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE members(id TEXT PRIMARY KEY,name TEXT NOT NULL); INSERT INTO members VALUES ('default','我'); CREATE TABLE clothing(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),document TEXT NOT NULL); PRAGMA user_version=5;").unwrap();
    db.execute("INSERT INTO clothing VALUES ('old-clothing','default',?1)",[json!({"schema_version":1,"id":"old-clothing","member_id":"default","name":"迁移前衣物","category":"outerwear","status":"active","favorite":false}).to_string()]).unwrap();
    drop(db);
    let app = Application::open(&path).unwrap();
    let old = app
        .execute(
            "clothing.get",
            json!({"member_id":"default","id":"old-clothing"}),
        )
        .unwrap();
    assert_eq!(old["name"], "迁移前衣物");
    let o=app.execute("outfit.create",json!({"member_id":"default","name":"迁移后搭配","items":[{"clothing_id":"old-clothing","role":"outer","layer":1}]})).unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    assert_eq!(
        app.execute("outfit.get", json!({"member_id":"default","id":o["id"]}))
            .unwrap(),
        o
    );
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":"default","id":"old-clothing"})
        )
        .unwrap(),
        old
    );
}
