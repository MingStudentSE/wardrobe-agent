use serde_json::json;
use wardrobe_core::Application;
#[test]
fn explicit_actual_wear_counts_events_and_local_days_without_template_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let shirt = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"衬衫","category":"top"}),
        )
        .unwrap();
    let outfit = app.execute("outfit.create",json!({"member_id":"default","name":"出门","items":[{"clothing_id":shirt["id"],"role":"base","layer":0}]})).unwrap();
    let before = app
        .execute("wear.summary", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(before["items"], json!([]));
    let request = json!({"member_id":"default","operation_key":"first","local_date":"2020-01-02","timezone":"Asia/Shanghai","clothing_ids":[shirt["id"]],"notes":"上午"});
    let first = app.execute("wear.record", request.clone()).unwrap();
    assert_eq!(app.execute("wear.record", request).unwrap(), first);
    app.execute("wear.record",json!({"member_id":"default","operation_key":"second","local_date":"2020-01-02","timezone":"Asia/Shanghai","source_outfit_id":outfit["id"],"source_version":1})).unwrap();
    let summary = app
        .execute("wear.summary", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(summary["items"][0]["wearCount"], 2);
    assert_eq!(summary["items"][0]["wearDays"], 1);
    assert_eq!(first["items"][0]["snapshot"]["name"], "衬衫");
}
#[test]
fn corrections_and_revocation_append_history_recompute_and_preserve_exact_retry_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&path).unwrap();
    let a = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"原白衬衫","category":"top"}),
        )
        .unwrap();
    let b = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"换穿裤子","category":"bottom"}),
        )
        .unwrap();
    let request = json!({"member_id":"default","operation_key":"record-a","local_date":"2020-01-02","timezone":"Asia/Shanghai","clothing_ids":[a["id"]]});
    let first = app.execute("wear.record", request.clone()).unwrap();
    let correction = json!({"member_id":"default","root_event_id":first["id"],"expected_revision":1,"operation_key":"correct-a","local_date":"2020-01-03","timezone":"Pacific/Honolulu","clothing_ids":[b["id"]],"notes":"实际换穿"});
    let changed = app.execute("wear.correct", correction.clone()).unwrap();
    assert_eq!(changed["revision"], 2);
    assert_eq!(changed["supersedes_event_id"], first["id"]);
    assert_eq!(
        app.execute("wear.summary", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([{"clothing_id":b["id"],"wearCount":1,"wearDays":1}])
    );
    assert_eq!(app.execute("wear.record", request).unwrap(), first);
    let mut stale = correction.clone();
    stale["operation_key"] = json!("other-correction");
    assert_eq!(
        app.execute("wear.correct", stale).unwrap_err().code,
        "version_conflict"
    );
    let revoked=app.execute("wear.revoke",json!({"member_id":"default","root_event_id":first["id"],"expected_revision":2,"operation_key":"revoke-a","reason":"误记"})).unwrap();
    assert_eq!(revoked["revision"], 3);
    assert_eq!(app.execute("wear.correct", correction).unwrap(), changed);
    app.execute(
        "clothing.delete",
        json!({"member_id":"default","id":a["id"]}),
    )
    .unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    let history = app
        .execute(
            "wear.history",
            json!({"member_id":"default","root_event_id":first["id"]}),
        )
        .unwrap();
    assert_eq!(history["items"].as_array().unwrap().len(), 3);
    assert_eq!(
        history["items"][0]["items"][0]["snapshot"]["name"],
        "原白衬衫"
    );
    assert_eq!(
        app.execute("wear.summary", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}
#[test]
fn public_history_keeps_template_snapshot_and_refuses_cross_member_and_future_facts() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let a = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"历史白衬衫","category":"top"}),
        )
        .unwrap();
    let look=app.execute("outfit.create",json!({"member_id":"default","name":"旧模板","items":[{"clothing_id":a["id"],"role":"base","layer":0}]})).unwrap();
    let wear=app.execute("wear.record",json!({"member_id":"default","operation_key":"template","local_date":"2020-01-01","local_time":"00:30","timezone":"Asia/Shanghai","source_outfit_id":look["id"],"source_version":1})).unwrap();
    assert_eq!(wear["occurred_at"], "2019-12-31T16:30:00+00:00");
    app.execute(
        "clothing.update",
        json!({"member_id":"default","id":a["id"],"name":"新名字"}),
    )
    .unwrap();
    app.execute(
        "clothing.delete",
        json!({"member_id":"default","id":a["id"]}),
    )
    .unwrap();
    app.execute(
        "outfit.delete",
        json!({"member_id":"default","id":look["id"]}),
    )
    .unwrap();
    let changed=app.execute("wear.correct",json!({"member_id":"default","root_event_id":wear["id"],"expected_revision":1,"operation_key":"date-only","local_date":"2020-01-02","timezone":"America/New_York"})).unwrap();
    assert_eq!(changed["items"][0]["snapshot"]["name"], "历史白衬衫");
    assert_eq!(changed["source_outfit"]["name"], "旧模板");
    let other = app
        .execute("member.create", json!({"name":"别人"}))
        .unwrap();
    assert_eq!(
        app.execute(
            "wear.history",
            json!({"member_id":other["member_id"],"root_event_id":wear["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    for (date, zone, time, code) in [
        ("2999-01-01", "Asia/Shanghai", "12:00", "future_date"),
        ("2020-01-02", "Bogus/Timezone", "12:00", "invalid_input"),
        ("2020-03-08", "America/New_York", "02:30", "invalid_input"),
        ("2020-11-01", "America/New_York", "01:30", "invalid_input"),
    ] {
        assert_eq!(app.execute("wear.record",json!({"member_id":"default","operation_key":date.to_owned()+zone+time,"local_date":date,"local_time":time,"timezone":zone,"clothing_ids":[a["id"]]})).unwrap_err().code,code);
    }
    let list = app
        .execute("wear.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0], changed);
    assert_eq!(
        app.execute("wear.get", json!({"member_id":"default","id":wear["id"]}))
            .unwrap(),
        wear
    );
}
#[test]
fn revoked_wear_retains_adopted_display_after_current_garment_detaches_it() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let path = dir.path().join("public-generated.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([240, 230, 200]))
        .save(&path)
        .unwrap();
    let a = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"有图单品","category":"top"}),
        )
        .unwrap();
    let image = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","asset_id":image["id"],"clothing_id":a["id"]}),
    )
    .unwrap();
    let event=app.execute("wear.record",json!({"member_id":"default","operation_key":"pictured","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[a["id"]]})).unwrap();
    app.execute("wear.revoke",json!({"member_id":"default","root_event_id":event["id"],"operation_key":"rev-picture","expected_revision":1,"reason":"误记"})).unwrap();
    app.execute(
        "assets.detach",
        json!({"member_id":"default","asset_id":image["id"],"clothing_id":a["id"]}),
    )
    .unwrap();
    assert_eq!(
        app.execute(
            "assets.get",
            json!({"member_id":"default","id":image["id"]})
        )
        .unwrap()["state"],
        "adopted"
    );
    let manifest = app
        .execute("assets.sync_manifest", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(manifest["assets"].as_array().unwrap().len(), 1);
}
#[test]
fn clients_discover_revision_contract_and_cannot_write_with_read_only_scope() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let capabilities = app.execute("capabilities", json!({})).unwrap();
    let record = capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "wear.record")
        .unwrap();
    assert!(record["outputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("root_event_id")));
    let grant=app.execute("client.authorize",json!({"name":"history-reader","expires_at":null,"mode":"limited","operations":["wear.list","wear.summary"],"member_ids":[member]})).unwrap();
    let token = grant["token"].as_str().unwrap();
    assert_eq!(
        app.execute_as_client(token, "wear.summary", json!({"member_id":member}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        app.execute_as_client(token, "wear.record", json!({"member_id":member}))
            .unwrap_err()
            .code,
        "scope_denied"
    );
}
#[test]
fn two_clients_cannot_both_correct_one_revision_or_reuse_key_for_another_intent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"并发衬衫","category":"top"}),
        )
        .unwrap();
    let request = json!({"member_id":"default","operation_key":"one-intent","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[c["id"]]});
    let event = app.execute("wear.record", request.clone()).unwrap();
    let mut changed = request;
    changed["local_date"] = json!("2020-01-02");
    assert_eq!(
        app.execute("wear.record", changed).unwrap_err().code,
        "idempotency_conflict"
    );
    drop(app);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut workers = vec![];
    for n in 0..2 {
        let path = path.clone();
        let id = event["id"].clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move||{let app=Application::open(&path).unwrap();barrier.wait();app.execute("wear.correct",json!({"member_id":"default","root_event_id":id,"expected_revision":1,"operation_key":format!("client-{n}"),"local_date":"2020-01-02","timezone":"UTC","notes":format!("correction-{n}")}))}));
    }
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .next()
            .unwrap()
            .code,
        "version_conflict"
    );
    let app = Application::open(&path).unwrap();
    assert_eq!(
        app.execute(
            "wear.history",
            json!({"member_id":"default","root_event_id":event["id"]})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn local_days_remain_user_date_across_utc_midnight_and_db_nine_repairs_missing_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"跨区衣物","category":"top"}),
        )
        .unwrap();
    drop(app);
    let fixture = rusqlite::Connection::open(&path).unwrap();
    fixture.execute_batch("DROP TABLE wear_asset_refs; DROP TABLE wear_receipts; DROP TABLE wear_events; PRAGMA user_version=9;").unwrap();
    drop(fixture);
    let app = Application::open(&path).unwrap();
    for (key, date, zone, time, instant) in [
        (
            "shanghai",
            "2020-01-01",
            "Asia/Shanghai",
            "00:30",
            "2019-12-31T16:30:00+00:00",
        ),
        (
            "honolulu",
            "2019-12-31",
            "Pacific/Honolulu",
            "23:30",
            "2020-01-01T09:30:00+00:00",
        ),
    ] {
        let event=app.execute("wear.record",json!({"member_id":"default","operation_key":key,"local_date":date,"timezone":zone,"local_time":time,"clothing_ids":[c["id"]]})).unwrap();
        assert_eq!(event["occurred_at"], instant);
    }
    let summary = app
        .execute("wear.summary", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(summary["items"][0]["wearCount"], 2);
    assert_eq!(summary["items"][0]["wearDays"], 2);
}
#[test]
fn wear_snapshot_retains_all_associated_pictures_not_only_selected_display() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let path = dir.path().join("generated.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([240, 230, 200]))
        .save(&path)
        .unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"多图上衣","category":"top"}),
        )
        .unwrap();
    let first = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    let second = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    for image in [&first, &second] {
        app.execute(
            "assets.adopt",
            json!({"member_id":"default","clothing_id":c["id"],"asset_id":image["id"]}),
        )
        .unwrap();
    }
    app.execute("wear.record",json!({"member_id":"default","operation_key":"all-pictures","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[c["id"]]})).unwrap();
    for image in [&first, &second] {
        app.execute(
            "assets.detach",
            json!({"member_id":"default","clothing_id":c["id"],"asset_id":image["id"]}),
        )
        .unwrap();
        assert_eq!(
            app.execute(
                "assets.get",
                json!({"member_id":"default","id":image["id"]})
            )
            .unwrap()["state"],
            "adopted"
        );
    }
}
