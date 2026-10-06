use serde_json::json;
use wardrobe_core::Application;
#[test]
fn durable_batch_keeps_source_and_idempotent_identity_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(4, 6, image::Rgb([245, 245, 245]))
        .save(&source)
        .unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let input = json!({"member_id":"default","operation_key":"batch-one","paths":[source]});
    let batch = app.execute("imports.create", input.clone()).unwrap();
    assert_eq!(batch["items"].as_array().unwrap().len(), 1);
    assert_eq!(batch["items"][0]["state"], "queued");
    assert_eq!(
        app.execute("imports.create", input).unwrap()["id"],
        batch["id"]
    );
    std::fs::remove_file(&source).unwrap();
    drop(app);
    let app = Application::open(&db).unwrap();
    let done = app
        .execute(
            "imports.process",
            json!({"member_id":"default","id":batch["id"]}),
        )
        .unwrap();
    assert_eq!(done["items"][0]["state"], "waiting_input");
    assert!(done["items"][0]["asset_id"].is_string());
}
#[test]
fn mixed_batch_can_cancel_retry_group_and_commit_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(4, 6, image::Rgb([245, 245, 245]))
        .save(&source)
        .unwrap();
    let bad = dir.path().join("bad.png");
    std::fs::write(&bad, b"corrupt").unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":"default","operation_key":"mixed","paths":[source,source,bad]}),
        )
        .unwrap();
    let id = &batch["id"];
    let item = &batch["items"][0]["id"];
    app.execute("jobs.cancel", json!({"member_id":"default","id":item}))
        .unwrap();
    app.execute("jobs.retry", json!({"member_id":"default","id":item}))
        .unwrap();
    for _ in 0..3 {
        app.execute("imports.process", json!({"member_id":"default","id":id}))
            .unwrap();
    }
    let batch = app
        .execute("imports.get", json!({"member_id":"default","id":id}))
        .unwrap();
    assert_eq!(batch["items"][2]["error"]["code"], "image_decode_failed");
    let group=app.execute("imports.group",json!({"member_id":"default","id":id,"item_ids":[batch["items"][0]["id"],batch["items"][1]["id"]]})).unwrap();
    let request = json!({"member_id":"default","id":id,"draft_id":group["drafts"][0]["id"],"operation_key":"save-once","facts":{"name":" 白色上衣 ","category":"top","brand":"test"}});
    let saved = app.execute("imports.commit", request.clone()).unwrap();
    assert_eq!(saved["name"], "白色上衣");
    assert_eq!(saved["asset_ids"].as_array().unwrap().len(), 2);
    drop(app);
    let app = Application::open(&db).unwrap();
    assert_eq!(
        app.execute("imports.commit", request).unwrap()["id"],
        saved["id"]
    );
    assert_eq!(
        app.execute("clothing.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        app.execute("assets.sync_manifest", json!({"member_id":"default"}))
            .unwrap()["assets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn missing_source_can_be_restored_then_retried_without_losing_other_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("later.png");
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":"default","operation_key":"missing","paths":[source]}),
        )
        .unwrap();
    assert_eq!(batch["items"][0]["error"]["code"], "source_unavailable");
    image::RgbImage::from_pixel(4, 6, image::Rgb([0, 0, 0]))
        .save(&source)
        .unwrap();
    app.execute(
        "jobs.retry",
        json!({"member_id":"default","id":batch["items"][0]["id"]}),
    )
    .unwrap();
    let done = app
        .execute(
            "imports.process",
            json!({"member_id":"default","id":batch["id"]}),
        )
        .unwrap();
    assert_eq!(done["items"][0]["state"], "waiting_input");
}
#[test]
fn draft_edits_survive_restart_and_default_unknowns_remain_manual() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(3, 3, image::Rgb([255, 255, 255]))
        .save(&source)
        .unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":"default","operation_key":"edit","paths":[source]}),
        )
        .unwrap();
    let draft = &batch["drafts"][0]["id"];
    app.execute("imports.update",json!({"member_id":"default","id":batch["id"],"draft_id":draft,"facts":{"name":"冬日毛衣","category":"top","material":["wool"]}})).unwrap();
    app.execute("imports.update",json!({"member_id":"default","id":batch["id"],"draft_id":draft,"facts":{"name":"冬日毛衣"}})).unwrap();
    drop(app);
    let app = Application::open(&db).unwrap();
    let read = app
        .execute(
            "imports.get",
            json!({"member_id":"default","id":batch["id"]}),
        )
        .unwrap();
    assert_eq!(read["drafts"][0]["facts"]["material"], json!(["wool"]));
    app.execute(
        "imports.process",
        json!({"member_id":"default","id":batch["id"]}),
    )
    .unwrap();
    let saved = app
        .execute(
            "imports.commit",
            json!({"member_id":"default","id":batch["id"],"draft_id":draft,"operation_key":"save"}),
        )
        .unwrap();
    assert_eq!(saved["name"], "冬日毛衣");
    assert_eq!(saved["material"], json!(["wool"]));
}
#[test]
fn batch_limits_grouping_scope_and_auth_are_enforced_without_partial_clothing() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(3, 3, image::Rgb([255, 255, 255]))
        .save(&source)
        .unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    assert_eq!(app.execute("imports.create",json!({"member_id":"default","operation_key":"too-many","paths":vec![source.clone();101]})).unwrap_err().code,"invalid_input");
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":"default","operation_key":"hundred","paths":vec![source;100]}),
        )
        .unwrap();
    assert_eq!(batch["drafts"].as_array().unwrap().len(), 100);
    let ids = batch["items"]
        .as_array()
        .unwrap()
        .iter()
        .take(21)
        .map(|i| i["id"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        app.execute(
            "imports.group",
            json!({"member_id":"default","id":batch["id"],"item_ids":ids})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
    let other = app
        .execute("member.create", json!({"name":"其他人"}))
        .unwrap();
    assert_eq!(
        app.execute(
            "imports.get",
            json!({"member_id":other["member_id"],"id":batch["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    let grant=app.execute("client.authorize",json!({"name":"query imports","mode":"limited","expires_at":null,"operations":["imports.get"],"member_ids":[batch["member_id"]]})).unwrap();
    let token = grant["token"].as_str().unwrap();
    assert_eq!(
        app.execute_as_client(
            token,
            "imports.get",
            json!({"member_id":"default","id":batch["id"]})
        )
        .unwrap()["id"],
        batch["id"]
    );
    assert_eq!(
        app.execute_as_client(
            token,
            "imports.process",
            json!({"member_id":"default","id":batch["id"]})
        )
        .unwrap_err()
        .code,
        "scope_denied"
    );
    assert_eq!(
        app.execute_as_client(
            token,
            "imports.get",
            json!({"member_id":other["member_id"],"id":batch["id"]})
        )
        .unwrap_err()
        .code,
        "member_denied"
    );
    assert_eq!(
        app.execute("clothing.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
#[test]
fn concurrent_commit_responses_reference_one_clothing_and_one_history_entry() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(3, 3, image::Rgb([255, 255, 255]))
        .save(&source)
        .unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":"default","operation_key":"concurrent","paths":[source]}),
        )
        .unwrap();
    app.execute(
        "imports.process",
        json!({"member_id":"default","id":batch["id"]}),
    )
    .unwrap();
    let request = json!({"member_id":"default","id":batch["id"],"draft_id":batch["drafts"][0]["id"],"operation_key":"save"});
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut workers = vec![];
    for _ in 0..2 {
        let (db, request, barrier) = (db.clone(), request.clone(), barrier.clone());
        workers.push(std::thread::spawn(move || {
            let app = Application::open(&db).unwrap();
            barrier.wait();
            app.execute("imports.commit", request).unwrap()
        }));
    }
    let saved = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(saved[0]["id"], saved[1]["id"]);
    assert_eq!(
        app.execute(
            "clothing.history",
            json!({"member_id":"default","id":saved[0]["id"]})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn version_eight_lookbook_gains_batch_tables_without_losing_versions_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"既有外套","category":"outerwear"}),
        )
        .unwrap();
    let outfit = app.execute("outfit.create", json!({"member_id":"default","name":"既有搭配","items":[{"clothing_id":clothing["id"],"role":"outer","layer":2}]})).unwrap();
    drop(app);
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.execute_batch("DROP TABLE import_drafts; DROP TABLE import_batches; DROP TABLE jobs; PRAGMA user_version=8;").unwrap();
    drop(connection);
    let source = dir.path().join("new.png");
    image::RgbImage::from_pixel(4, 6, image::Rgb([50, 80, 120]))
        .save(&source)
        .unwrap();
    let app = Application::open(&db).unwrap();
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":"default","operation_key":"after-v8","paths":[source]}),
        )
        .unwrap();
    app.execute(
        "imports.process",
        json!({"member_id":"default","id":batch["id"]}),
    )
    .unwrap();
    let saved = app.execute("imports.commit", json!({"member_id":"default","id":batch["id"],"draft_id":batch["drafts"][0]["id"],"operation_key":"v8-commit"})).unwrap();
    let new_outfit = app.execute("outfit.create", json!({"member_id":"default","name":"批量录入搭配","items":[{"clothing_id":saved["id"],"role":"base","layer":0}]})).unwrap();
    drop(app);
    let app = Application::open(&db).unwrap();
    assert_eq!(
        app.execute(
            "outfit.version",
            json!({"member_id":"default","id":outfit["id"],"version":1})
        )
        .unwrap(),
        outfit
    );
    assert_eq!(
        app.execute(
            "outfit.version",
            json!({"member_id":"default","id":new_outfit["id"],"version":1})
        )
        .unwrap(),
        new_outfit
    );
    let collage = app
        .execute(
            "outfit.render_local",
            json!({"member_id":"default","id":new_outfit["id"]}),
        )
        .unwrap();
    assert_eq!(collage["missing_images"], 0);
    assert_eq!(
        rusqlite::Connection::open(&db)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        8
    );
}
