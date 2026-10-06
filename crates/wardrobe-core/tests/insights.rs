use serde_json::json;
use wardrobe_core::Application;
#[test]
fn highest_database_version_repairs_missing_preferences_and_retains_wear_and_inventory() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("db");
    let a = Application::open(&p).unwrap();
    let item = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"迁移衣物","category":"top"}),
        )
        .unwrap();
    a.execute("wear.record",json!({"member_id":"default","operation_key":"migration","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[item["id"]]})).unwrap();
    drop(a);
    let fixture = rusqlite::Connection::open(&p).unwrap();
    fixture
        .execute_batch("DROP TABLE insights_preferences; PRAGMA user_version=14;")
        .unwrap();
    drop(fixture);
    let a = Application::open(&p).unwrap();
    let query = json!({"member_id":"default","as_of_date":"2020-04-01","timezone":"UTC"});
    let result = a.execute("insights.summary", query).unwrap();
    assert_eq!(result["inventory"]["total"], 1);
    assert_eq!(result["frequency"][0]["wearCount"], 1);
    assert_eq!(result["stale_days"], 90);
    assert_eq!(
        a.execute(
            "insights.configure",
            json!({"member_id":"default","stale_days":30,"expected_revision":0})
        )
        .unwrap()["revision"],
        1
    );
    let db = rusqlite::Connection::open(&p).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        wardrobe_core::DATABASE_SCHEMA_VERSION
    );
}
#[test]
fn member_insight_preferences_have_stable_identity_and_compare_and_swap_revision() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("db");
    let a = Application::open(&p).unwrap();
    let b = Application::open(&p).unwrap();
    let first = a
        .execute(
            "insights.configure",
            json!({"member_id":"default","stale_days":91,"expected_revision":0}),
        )
        .unwrap();
    assert_eq!(first["revision"], 1);
    assert_eq!(first["schema_version"], 1);
    assert!(uuid::Uuid::parse_str(first["id"].as_str().unwrap()).is_ok());
    assert_eq!(first["created_at"], first["updated_at"]);
    assert_eq!(
        b.execute(
            "insights.configure",
            json!({"member_id":"default","stale_days":89,"expected_revision":0})
        )
        .unwrap_err()
        .code,
        "version_conflict"
    );
    let second = b
        .execute(
            "insights.configure",
            json!({"member_id":"default","stale_days":null,"expected_revision":1}),
        )
        .unwrap();
    assert_eq!(second["revision"], 2);
    assert_eq!(second["id"], first["id"]);
    assert_eq!(second["created_at"], first["created_at"]);
    assert_eq!(second["stale_days"], 90);
    drop(a);
    drop(b);
    let c = Application::open(&p).unwrap();
    let actual = c
        .execute(
            "insights.summary",
            json!({"member_id":"default","as_of_date":"2020-01-01","timezone":"UTC"}),
        )
        .unwrap();
    assert_eq!(actual["preferences"], second);
}
#[test]
fn inventory_multicolor_and_actual_frequency_match_hand_calculation() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let a = app.execute("clothing.create",json!({"member_id":"default","name":"红蓝上衣","category":"top","color":["red","blue"]})).unwrap();
    let b = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"无色裤子","category":"bottom"}),
        )
        .unwrap();
    app.execute("outfit.create",json!({"member_id":"default","name":"保存但未穿","items":[{"clothing_id":b["id"],"role":"bottom","layer":0}]})).unwrap();
    for key in ["morning", "evening"] {
        app.execute("wear.record",json!({"member_id":"default","operation_key":key,"local_date":"2020-01-01","timezone":"Asia/Shanghai","clothing_ids":[a["id"]]})).unwrap();
    }
    let result = app
        .execute(
            "insights.summary",
            json!({"member_id":"default","as_of_date":"2020-04-01","timezone":"Asia/Shanghai"}),
        )
        .unwrap();
    assert_eq!(result["inventory"]["total"], 2);
    assert_eq!(result["inventory"]["color_denominator"], 2);
    assert_eq!(
        result["inventory"]["colors"],
        json!([{"value":"blue","count":1,"percent":50.0},{"value":"red","count":1,"percent":50.0},{"value":null,"count":1,"percent":50.0}])
    );
    let frequency = result["frequency"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["clothing_id"] == a["id"])
        .unwrap();
    assert_eq!(frequency["wearCount"], 2);
    assert_eq!(frequency["wearDays"], 1);
    assert_eq!(result["never_worn"][0]["clothing_id"], b["id"]);
}
#[test]
fn stale_boundaries_preferences_and_combinations_recompute_after_correction_and_revoke() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let mut ids = vec![];
    for (name, category, colors) in [
        ("A", "top", json!(["red", "blue"])),
        ("B", "top", json!(["red"])),
        ("C", "bottom", json!([])),
        ("D", "outerwear", json!(["green"])),
        ("E", "other", json!([])),
        ("F", "shoes", json!([])),
    ] {
        ids.push(
            app.execute(
                "clothing.create",
                json!({"member_id":"default","name":name,"category":category,"color":colors}),
            )
            .unwrap()["id"]
                .clone(),
        );
    }
    let mut events = vec![];
    for (key, date, selection) in [
        ("ab1", "2020-01-01", vec![ids[0].clone(), ids[1].clone()]),
        ("ab2", "2020-01-01", vec![ids[1].clone(), ids[0].clone()]),
        ("af", "2020-01-02", vec![ids[0].clone(), ids[5].clone()]),
        ("d", "2020-01-03", vec![ids[3].clone()]),
    ] {
        events.push(app.execute("wear.record",json!({"member_id":"default","operation_key":key,"local_date":date,"timezone":"Asia/Shanghai","clothing_ids":selection})).unwrap());
    }
    app.execute(
        "clothing.archive",
        json!({"member_id":"default","id":ids[3]}),
    )
    .unwrap();
    app.execute(
        "clothing.delete",
        json!({"member_id":"default","id":ids[5]}),
    )
    .unwrap();
    let query = json!({"member_id":"default","as_of_date":"2020-04-01","timezone":"Asia/Shanghai"});
    let before = app.execute("insights.summary", query.clone()).unwrap();
    assert_eq!(before["stale_days"], 90);
    assert_eq!(before["inventory"]["total"], 5);
    assert_eq!(
        before["inventory"]["by_status"],
        json!({"active":4,"archived":1,"deleted":1})
    );
    assert_eq!(
        before["inventory"]["categories"],
        json!([{"value":"bottom","count":1,"percent":20.0},{"value":"other","count":1,"percent":20.0},{"value":"outerwear","count":1,"percent":20.0},{"value":"top","count":2,"percent":40.0}])
    );
    assert_eq!(
        before["inventory"]["colors"],
        json!([{"value":"blue","count":1,"percent":20.0},{"value":"green","count":1,"percent":20.0},{"value":"red","count":2,"percent":40.0},{"value":null,"count":2,"percent":40.0}])
    );
    let stale = before["stale"].as_array().unwrap();
    assert_eq!(stale.len(), 2);
    assert!(stale
        .iter()
        .any(|v| v["clothing_id"] == ids[0] && v["days_since_worn"] == 90));
    assert!(stale
        .iter()
        .any(|v| v["clothing_id"] == ids[1] && v["days_since_worn"] == 91));
    assert_eq!(before["never_worn"].as_array().unwrap().len(), 2);
    let ab = before["combinations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| {
            v["clothing_ids"].as_array().unwrap().len() == 2
                && v["clothing_ids"].as_array().unwrap().contains(&ids[1])
        })
        .unwrap();
    assert_eq!(ab["wearCount"], 2);
    assert_eq!(ab["wearDays"], 1);
    assert_eq!(before["combinations"].as_array().unwrap().len(), 3);
    app.execute(
        "insights.configure",
        json!({"member_id":"default","stale_days":89,"expected_revision":0}),
    )
    .unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    let configured = app.execute("insights.summary", query.clone()).unwrap();
    assert_eq!(configured["stale_days"], 89);
    assert_eq!(configured["stale"].as_array().unwrap().len(), 3);
    app.execute("wear.correct",json!({"member_id":"default","root_event_id":events[2]["id"],"expected_revision":1,"operation_key":"correct-af","local_date":"2020-01-03","timezone":"Asia/Shanghai"})).unwrap();
    app.execute("wear.revoke",json!({"member_id":"default","root_event_id":events[1]["id"],"expected_revision":1,"operation_key":"revoke-ab2","reason":"误记"})).unwrap();
    let after = app.execute("insights.summary", query).unwrap();
    assert_eq!(after["effective_event_count"], 3);
    let a = after["frequency"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["clothing_id"] == ids[0])
        .unwrap();
    assert_eq!(a["wearCount"], 2);
    assert_eq!(a["wearDays"], 2);
    assert_eq!(a["days_since_worn"], 89);
    assert_eq!(
        app.execute(
            "insights.configure",
            json!({"member_id":"default","stale_days":null,"expected_revision":1})
        )
        .unwrap()["stale_days"],
        90
    );
}
#[test]
fn empty_denominators_member_preferences_authorization_and_timezone_cutoff_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let query = json!({"member_id":"default","as_of_date":"2020-01-02","timezone":"UTC"});
    let empty = app.execute("insights.summary", query.clone()).unwrap();
    assert_eq!(empty["inventory"]["total"], 0);
    assert_eq!(empty["inventory"]["colors"], json!([]));
    assert_eq!(empty["frequency"], json!([]));
    let member = app.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let grant=app.execute("client.authorize",json!({"name":"Insights reader","mode":"limited","operations":["insights.summary"],"member_ids":[member],"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap();
    assert_eq!(
        app.execute_as_client(token, "insights.summary", query.clone())
            .unwrap(),
        empty
    );
    assert_eq!(
        app.execute_as_client(
            token,
            "insights.configure",
            json!({"member_id":"default","stale_days":1})
        )
        .unwrap_err()
        .code,
        "scope_denied"
    );
    let other = app
        .execute("member.create", json!({"name":"Other"}))
        .unwrap();
    let mut foreign = query.clone();
    foreign["member_id"] = other["member_id"].clone();
    assert_eq!(
        app.execute_as_client(token, "insights.summary", foreign.clone())
            .unwrap_err()
            .code,
        "member_denied"
    );
    for n in [json!(0), json!(-1), json!(36501), json!(1.5), json!("90")] {
        assert_eq!(
            app.execute(
                "insights.configure",
                json!({"member_id":"default","stale_days":n})
            )
            .unwrap_err()
            .code,
            "invalid_input"
        );
    }
    app.execute(
        "insights.configure",
        json!({"member_id":"default","stale_days":2,"expected_revision":0}),
    )
    .unwrap();
    assert_eq!(
        app.execute("insights.summary", foreign).unwrap()["stale_days"],
        90
    );
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"跨日上衣","category":"top"}),
        )
        .unwrap();
    app.execute("wear.record",json!({"member_id":"default","operation_key":"boundary","local_date":"2020-01-01","local_time":"00:30","timezone":"Asia/Shanghai","clothing_ids":[c["id"]]})).unwrap();
    let utc = app.execute("insights.summary", query.clone()).unwrap();
    assert_eq!(utc["frequency"][0]["last_worn_date"], "2019-12-31");
    assert_eq!(utc["frequency"][0]["days_since_worn"], 2);
    assert_eq!(utc["frequency"][0]["wearDays"], 1);
    assert_eq!(utc["stale"].as_array().unwrap().len(), 1);
    let early = app
        .execute(
            "insights.summary",
            json!({"member_id":"default","as_of_date":"2019-12-30","timezone":"UTC"}),
        )
        .unwrap();
    assert_eq!(early["effective_event_count"], 0);
    assert_eq!(early["never_worn"].as_array().unwrap().len(), 1);
    for (date, zone) in [
        ("2020-02-30", "UTC"),
        ("2020-01-02", "Invalid/Timezone"),
        ("2999-01-01", "UTC"),
    ] {
        assert_eq!(
            app.execute(
                "insights.summary",
                json!({"member_id":"default","as_of_date":date,"timezone":zone})
            )
            .unwrap_err()
            .code,
            "invalid_input"
        );
    }
}
#[test]
fn highest_version_repairs_both_care_and_insight_preferences_without_erasing_actual_history() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("db");
    let a = Application::open(&p).unwrap();
    let c = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"共同迁移衣物","category":"top"}),
        )
        .unwrap();
    a.execute("wear.record",json!({"member_id":"default","clothing_ids":[c["id"]],"local_date":"2020-01-01","timezone":"UTC","operation_key":"wear"})).unwrap();
    let clean=a.execute("care.record",json!({"member_id":"default","clothing_id":c["id"],"action":"wash","local_date":"2020-01-02","timezone":"UTC","operation_key":"wash"})).unwrap();
    drop(a);
    let fixture = rusqlite::Connection::open(&p).unwrap();
    fixture
        .execute_batch(
            "DROP TABLE care_config; DROP TABLE insights_preferences; PRAGMA user_version=14;",
        )
        .unwrap();
    drop(fixture);
    let a = Application::open(&p).unwrap();
    let care = a
        .execute(
            "care.status",
            json!({"member_id":"default","clothing_id":c["id"]}),
        )
        .unwrap();
    assert_eq!(care["wash_after_wears"], serde_json::Value::Null);
    assert_eq!(care["wears_since_clean"], 0);
    assert_eq!(care["last_clean"]["id"], clean["id"]);
    let summary = a
        .execute(
            "insights.summary",
            json!({"member_id":"default","as_of_date":"2020-04-01","timezone":"UTC"}),
        )
        .unwrap();
    assert_eq!(summary["frequency"][0]["wearCount"], 1);
    assert_eq!(summary["stale_days"], 90);
    a.execute("care.configure",json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":2,"expected_revision":0,"operation_key":"careconfig"})).unwrap();
    a.execute(
        "insights.configure",
        json!({"member_id":"default","stale_days":89,"expected_revision":0}),
    )
    .unwrap();
    assert_eq!(
        a.execute("recognition.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    let db = rusqlite::Connection::open(&p).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        14
    );
}
