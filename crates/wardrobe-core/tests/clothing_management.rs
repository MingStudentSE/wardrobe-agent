use serde_json::json;
use wardrobe_core::Application;
#[test]
fn optional_clothing_facts_round_trip_and_patch_preserves_omitted_facts() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let item=app.execute("clothing.create",json!({"member_id":"default","name":"棉衬衫","category":"top","color":["white"],"material":["棉"],"brand":"品牌","purchase_date":"2026-10-01","purchase_price_minor":12900,"currency":"CNY"})).unwrap();
    assert_eq!(item["material"], json!(["棉"]));
    assert_eq!(item["purchase_date"], "2026-10-01");
    let edited = app
        .execute(
            "clothing.update",
            json!({"member_id":"default","id":item["id"],"name":"白衬衫","size_label":"M"}),
        )
        .unwrap();
    assert_eq!(edited["color"], json!(["white"]));
    assert_eq!(edited["size_label"], "M");
    assert_eq!(edited["purchase_price_minor"], 12900);
}
#[test]
fn soft_delete_hides_clothing_and_explicit_restore_keeps_identity_and_facts() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"外套","category":"outerwear","favorite":true}),
        )
        .unwrap();
    let scope = json!({"member_id":"default","id":item["id"]});
    app.execute("clothing.archive", scope.clone()).unwrap();
    assert_eq!(
        app.execute(
            "clothing.list",
            json!({"member_id":"default","status":"archived"})
        )
        .unwrap()["items"][0]["id"],
        item["id"]
    );
    app.execute("clothing.delete", scope.clone()).unwrap();
    assert_eq!(
        app.execute("clothing.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        app.execute(
            "clothing.update",
            json!({"member_id":"default","id":item["id"],"name":"changed"})
        )
        .unwrap_err()
        .code,
        "conflict"
    );
    let restored = app.execute("clothing.restore", scope).unwrap();
    assert_eq!(restored["id"], item["id"]);
    assert_eq!(restored["status"], "archived");
    assert_eq!(restored["favorite"], true);
    assert!(restored["deleted_at"].is_null());
}
#[test]
fn combined_filters_search_sort_and_pagination_find_only_matching_facts() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    for (name, price, category) in [
        ("春衬衫", 200, "top"),
        ("白衬衫", 100, "top"),
        ("棉裤", 50, "bottom"),
    ] {
        app.execute("clothing.create",json!({"member_id":"default","name":name,"category":category,"seasons":["spring"],"color":["white"],"tags":["通勤"],"favorite":true,"purchase_price_minor":price,"currency":"CNY"})).unwrap();
    }
    let query = json!({"member_id":"default","search":"衬衫","category":"top","season":"spring","color":"white","tag":"通勤","favorite":true,"sort":"price_asc","limit":1});
    let page = app.execute("clothing.list", query.clone()).unwrap();
    assert_eq!(page["items"][0]["name"], "白衬衫");
    let mut next = query;
    next["cursor"] = page["next_cursor"].clone();
    let page = app.execute("clothing.list", next).unwrap();
    assert_eq!(page["items"][0]["name"], "春衬衫");
    assert!(page["next_cursor"].is_null());
    assert_eq!(
        app.execute(
            "clothing.list",
            json!({"member_id":"default","sort":"bogus"})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
}
#[test]
fn audit_history_retains_prior_facts_after_edit_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"原名","category":"top"}),
        )
        .unwrap();
    let scope = json!({"member_id":"default","id":item["id"]});
    app.execute(
        "clothing.update",
        json!({"member_id":"default","id":item["id"],"name":"新名"}),
    )
    .unwrap();
    app.execute("clothing.delete", scope.clone()).unwrap();
    let timeline = app.execute("clothing.history", scope).unwrap();
    assert_eq!(timeline["items"][0]["snapshot"]["name"], "原名");
    assert_eq!(timeline["items"][1]["snapshot"]["name"], "新名");
    assert_eq!(timeline["items"][2]["operation"], "clothing.delete");
}
#[test]
fn invalid_facts_are_rejected_without_partial_changes() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"valid","category":"top"}),
        )
        .unwrap();
    for patch in [
        json!({"purchase_date":"2026-02-30"}),
        json!({"seasons":["all"]}),
        json!({"purchase_price_minor":9007199254740992u64,"currency":"CNY"}),
        json!({"purchase_price_minor":1.5,"currency":"CNY"}),
        json!({"color":[null]}),
    ] {
        let mut input = patch;
        input["id"] = item["id"].clone();
        input["member_id"] = json!("default");
        input["name"] = json!("invalid change");
        assert_eq!(
            app.execute("clothing.update", input).unwrap_err().code,
            "invalid_input"
        );
    }
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":"default","id":item["id"]})
        )
        .unwrap()["name"],
        "valid"
    );
}
#[test]
fn canonical_taxonomy_and_color_ids_are_versioned_and_validated() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let item=app.execute("clothing.create",json!({"member_id":"default","name":"衬衫","category":"top","sub_category":"shirt","color":["white"],"color_description":"奶白","material":["棉"],"style":["通勤"]})).unwrap();
    assert_eq!(item["sub_category"], "shirt");
    assert_eq!(item["color"], json!(["white"]));
    assert_eq!(item["material"], json!(["棉"]));
    assert_eq!(
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":"错误","category":"shoes","sub_category":"shirt"})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
}
#[test]
fn clothing_history_lifecycle_and_edit_are_scoped_to_the_owner_member() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let owner = app.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let other = app
        .execute("member.create", json!({"name":"另一成员"}))
        .unwrap()["member_id"]
        .clone();
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":owner,"name":"私人衬衫","category":"top"}),
        )
        .unwrap();
    for operation in [
        "clothing.get",
        "clothing.history",
        "clothing.update",
        "clothing.archive",
        "clothing.delete",
        "clothing.restore",
    ] {
        assert_eq!(
            app.execute(
                operation,
                json!({"member_id":other,"id":item["id"],"name":"篡改"})
            )
            .unwrap_err()
            .code,
            "not_found"
        );
    }
    assert_eq!(
        app.execute(
            "clothing.history",
            json!({"member_id":owner,"id":item["id"]})
        )
        .unwrap()["items"][0]["snapshot"]["name"],
        "私人衬衫"
    );
}
#[test]
fn unknown_patch_fields_cannot_silently_override_identity_assets_or_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"衬衫","category":"top"}),
        )
        .unwrap();
    assert_eq!(
        app.execute(
            "clothing.update",
            json!({"member_id":"default","id":item["id"],"status":"deleted"})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
}
