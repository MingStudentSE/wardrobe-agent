use serde_json::json;
use wardrobe_core::Application;
#[test]
fn explicit_preference_is_durable_versioned_and_separate_from_inferred_profile() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let request = json!({"member_id":"default","operation_key":"blue","dimension":"color","value":"blue","weight":3,"note":"我明确喜欢蓝色"});
    let memory = a.execute("learning.set", request.clone()).unwrap();
    assert_eq!(memory["source"]["kind"], "explicit_user");
    assert_eq!(memory["weight"], 3);
    assert_eq!(a.execute("learning.set", request).unwrap(), memory);
    let profile = a
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(profile["explicit"][0]["id"], memory["id"]);
    assert_eq!(profile["inferred"], json!([]));
    assert!(profile["algorithm_version"].is_string());
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    drop(a);
    let a = Application::open(&path).unwrap();
    assert_eq!(
        a.execute("learning.get", json!({"member_id":"default"}))
            .unwrap(),
        profile
    );
}
#[test]
fn known_feedback_and_actual_wear_recompute_deterministically_without_changing_facts() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let mut ids = vec![];
    for (name, category, color) in [
        ("蓝衬衫", "top", "blue"),
        ("裤", "bottom", "black"),
        ("鞋", "shoes", "black"),
    ] {
        ids.push(
            a.execute(
                "clothing.create",
                json!({"member_id":"default","name":name,"category":category,"color":[color]}),
            )
            .unwrap()["id"]
                .clone(),
        );
    }
    let intent = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"r","timezone":"UTC"}),
        )
        .unwrap();
    let feedback=a.execute("feedback.record",json!({"member_id":"default","operation_key":"f","intent_id":intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5}})).unwrap();
    let wear=a.execute("wear.record",json!({"member_id":"default","operation_key":"w","clothing_ids":ids,"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    let before = a
        .execute("wear.list", json!({"member_id":"default"}))
        .unwrap();
    let p = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"learn"}),
        )
        .unwrap();
    let blue = p["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["dimension"] == "color" && v["value"] == "blue")
        .unwrap();
    // Algorithm v1: like +2, overall5 +2, actual wear +1 per distinct feature/event.
    assert_eq!(blue["weight"], 5);
    assert_eq!(blue["source"]["kind"], "inferred_events");
    assert!(blue["source"]["feedback_ids"]
        .as_array()
        .unwrap()
        .contains(&feedback["id"]));
    assert!(blue["source"]["wear_ids"]
        .as_array()
        .unwrap()
        .contains(&wear["id"]));
    let p2 = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"again"}),
        )
        .unwrap();
    assert_eq!(p["inferred"], p2["inferred"]);
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap(),
        before
    );
}
#[test]
fn recommendation_reads_learning_weights_without_overriding_excluded_or_fixed_ids() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let red = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"红","category":"top","color":["red"]}),
        )
        .unwrap();
    let blue = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
        )
        .unwrap();
    for category in ["bottom", "shoes"] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":category,"category":category}),
        )
        .unwrap();
    }
    a.execute("learning.set",json!({"member_id":"default","operation_key":"prefer","dimension":"color","value":"blue","weight":5})).unwrap();
    let preferred = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"preferred","timezone":"UTC","limit":1}),
        )
        .unwrap();
    assert_eq!(
        preferred["proposals"][0]["items"][0]["clothing_id"],
        blue["id"]
    );
    assert!(preferred["learning_context"]["algorithm_version"].is_string());
    let fixed=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"fixed","timezone":"UTC","limit":1,"fixed_ids":[red["id"]],"excluded_ids":[blue["id"]]})).unwrap();
    assert_eq!(fixed["proposals"][0]["items"][0]["clothing_id"], red["id"]);
}
#[test]
fn edits_and_deleted_inferences_reset_boundaries_survive_recompute_without_deleting_wear() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let clothing = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
        )
        .unwrap();
    a.execute("wear.record",json!({"member_id":"default","operation_key":"wear","clothing_ids":[clothing["id"]],"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    let before = a
        .execute("wear.list", json!({"member_id":"default"}))
        .unwrap();
    let explicit=a.execute("learning.set",json!({"member_id":"default","operation_key":"explicit","dimension":"style","value":"casual","weight":4})).unwrap();
    let edited=a.execute("learning.edit",json!({"member_id":"default","operation_key":"edit","root_id":explicit["id"],"expected_revision":1,"weight":-2,"note":"现在不偏好"})).unwrap();
    assert_eq!(edited["revision"], 2);
    assert_eq!(edited["weight"], -2);
    assert_eq!(a.execute("learning.edit",json!({"member_id":"default","operation_key":"stale","root_id":explicit["id"],"expected_revision":1,"weight":1})).unwrap_err().code,"stale_revision");
    let learned = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"learn"}),
        )
        .unwrap();
    assert!(!learned["inferred"].as_array().unwrap().is_empty());
    a.execute("learning.delete",json!({"member_id":"default","operation_key":"forget-blue","dimension":"color","value":"blue"})).unwrap();
    let again = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"again"}),
        )
        .unwrap();
    assert!(!again["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["dimension"] == "color" && v["value"] == "blue"));
    a.execute(
        "learning.reset",
        json!({"member_id":"default","operation_key":"reset","scope":"inferred"}),
    )
    .unwrap();
    let reset = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"afterreset"}),
        )
        .unwrap();
    assert_eq!(reset["inferred"], json!([]));
    assert_eq!(
        a.execute("learning.get", json!({"member_id":"default"}))
            .unwrap()["explicit"][0]["weight"],
        -2
    );
    let rebuilt = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"fullrebuild","include_prior":true}),
        )
        .unwrap();
    assert!(rebuilt["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["value"] == "blue"));
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap(),
        before
    );
}
#[test]
fn source_snapshots_and_effective_corrections_are_used_and_management_history_is_paginated() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let garment = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
        )
        .unwrap();
    let wear=a.execute("wear.record",json!({"member_id":"default","operation_key":"wear","clothing_ids":[garment["id"]],"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    a.execute(
        "clothing.update",
        json!({"member_id":"default","id":garment["id"],"color":["red"]}),
    )
    .unwrap();
    let p = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"r"}),
        )
        .unwrap();
    assert!(p["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["value"] == "blue"));
    assert!(!p["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["value"] == "red"));
    a.execute("wear.revoke",json!({"member_id":"default","operation_key":"revoke","root_event_id":wear["root_event_id"],"expected_revision":1,"reason":"误记"})).unwrap();
    assert_eq!(
        a.execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"rr"})
        )
        .unwrap()["inferred"],
        json!([])
    );
    let first = a
        .execute("learning.history", json!({"member_id":"default","limit":1}))
        .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert!(first["next_cursor"].is_string());
    let second = a
        .execute(
            "learning.history",
            json!({"member_id":"default","limit":1,"cursor":first["next_cursor"]}),
        )
        .unwrap();
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
}
#[test]
fn optional_dimensions_have_explained_effect_and_skip_alone_is_not_dislike() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    a.execute("clothing.create",json!({"member_id":"default","name":"蓝棉衬衫","category":"top","color":["blue"],"material":["cotton"],"style":["casual"]})).unwrap();
    for category in ["bottom", "shoes"] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":category,"category":category}),
        )
        .unwrap();
    }
    let r = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"r","timezone":"UTC"}),
        )
        .unwrap();
    a.execute("feedback.record",json!({"member_id":"default","operation_key":"skip","intent_id":r["id"],"proposal_index":0,"reaction":"skip"})).unwrap();
    assert_eq!(
        a.execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"skiponly"})
        )
        .unwrap()["inferred"],
        json!([])
    );
    a.execute("feedback.record",json!({"member_id":"default","operation_key":"scores","intent_id":r["id"],"proposal_index":0,"reaction":"skip","scores":{"comfort":5,"style":1}})).unwrap();
    let p = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"scoresonly"}),
        )
        .unwrap();
    let signals = p["inferred"].as_array().unwrap();
    assert_eq!(
        signals
            .iter()
            .find(|v| v["dimension"] == "material" && v["value"] == "cotton")
            .unwrap()["weight"],
        2
    );
    assert_eq!(
        signals
            .iter()
            .find(|v| v["dimension"] == "color" && v["value"] == "blue")
            .unwrap()["weight"],
        -2
    );
    assert!(!signals.iter().any(|v| v["dimension"] == "category"));
}
#[test]
fn member_scope_validation_and_actual_missing_tables_repair_keep_learning_public() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let member = a.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let other = a.execute("member.create", json!({"name":"other"})).unwrap()["member_id"].clone();
    let m=a.execute("learning.set",json!({"member_id":member,"operation_key":"own","dimension":"style","value":"casual","weight":2})).unwrap();
    assert_eq!(
        a.execute("learning.get", json!({"member_id":other}))
            .unwrap()["explicit"],
        json!([])
    );
    assert_eq!(a.execute("learning.edit",json!({"member_id":other,"operation_key":"foreign","root_id":m["id"],"expected_revision":1,"weight":3})).unwrap_err().code,"not_found");
    let grant=a.execute("client.authorize",json!({"name":"memory-reader","expires_at":null,"mode":"limited","operations":["learning.get","learning.history"],"member_ids":[member]})).unwrap();
    let token = grant["token"].as_str().unwrap();
    assert!(a
        .execute_as_client(token, "learning.get", json!({"member_id":member}))
        .is_ok());
    assert_eq!(
        a.execute_as_client(
            token,
            "learning.reset",
            json!({"member_id":member,"operation_key":"deny","scope":"all"})
        )
        .unwrap_err()
        .code,
        "scope_denied"
    );
    assert_eq!(
        a.execute_as_client(token, "learning.get", json!({"member_id":other}))
            .unwrap_err()
            .code,
        "member_denied"
    );
    for dimension in ["height", "body_size", "unknown"] {
        assert_eq!(a.execute("learning.set",json!({"member_id":member,"operation_key":"bad","dimension":dimension,"value":"180","weight":2})).unwrap_err().code,"invalid_input");
    }
    for weight in [json!(6), json!(-6), json!(2.5), json!("2"), json!(true)] {
        assert_eq!(a.execute("learning.set",json!({"member_id":member,"operation_key":"bad","dimension":"style","value":"casual","weight":weight})).unwrap_err().code,"invalid_input");
    }
    drop(a);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE learning_profiles;DROP TABLE learning_state;DROP TABLE learning_history;DROP TABLE learning_receipts;DROP TABLE learning_memories;PRAGMA user_version=20;").unwrap();
    drop(db);
    let a = Application::open(&path).unwrap();
    assert_eq!(
        a.execute("learning.get", json!({"member_id":"default"}))
            .unwrap()["explicit"],
        json!([])
    );
    assert!(a.execute("learning.set",json!({"member_id":"default","operation_key":"repaired","dimension":"color","value":"blue","weight":3})).is_ok());
}
#[test]
fn same_feature_requires_edit_instead_of_competing_explicit_values_and_cas_is_atomic() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let pref=a.execute("learning.set",json!({"member_id":"default","operation_key":"one","dimension":"color","value":"blue","weight":3})).unwrap();
    assert_eq!(a.execute("learning.set",json!({"member_id":"default","operation_key":"two","dimension":"color","value":"blue","weight":-3})).unwrap_err().code,"preference_exists");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = vec![];
    for n in [1, 2] {
        let path = path.clone();
        let root = pref["id"].clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move||{let a=Application::open(&path).unwrap();barrier.wait();a.execute("learning.edit",json!({"member_id":"default","operation_key":format!("race{n}"),"root_id":root,"expected_revision":1,"weight":n}))}));
    }
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| r.as_ref().err().is_some_and(|e| e.code == "stale_revision"))
            .count(),
        1
    );
}
