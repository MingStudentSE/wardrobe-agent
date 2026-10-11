#[path = "support/learning.rs"]
mod support;
use serde_json::{json, Value};
use support::LearningTestSeam;
use wardrobe_core::{operations, Application};
#[test]
fn explicit_preference_is_durable_versioned_and_separate_from_inferred_profile() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let request = json!({"member_id":"default","operation_key":"blue","dimension":"color","value":"blue","weight":3,"note":"我明确喜欢蓝色"});
    let memory = a.execute_and_wait("learning.set", request.clone()).unwrap();
    assert_eq!(memory["source"]["kind"], "explicit_user");
    assert_eq!(memory["weight"], 3);
    assert_eq!(a.execute_and_wait("learning.set", request).unwrap(), memory);
    let profile = a
        .execute_and_wait("learning.get", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(profile["explicit"][0]["id"], memory["id"]);
    assert_eq!(profile["inferred"], json!([]));
    assert!(profile["algorithm_version"].is_string());
    assert_eq!(
        a.execute_and_wait("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    drop(a);
    let a = Application::open(&path).unwrap();
    assert_eq!(
        a.execute_and_wait("learning.get", json!({"member_id":"default"}))
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
            a.execute_and_wait(
                "clothing.create",
                json!({"member_id":"default","name":name,"category":category,"color":[color]}),
            )
            .unwrap()["id"]
                .clone(),
        );
    }
    let intent = a
        .execute_and_wait(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"r","timezone":"UTC"}),
        )
        .unwrap();
    let feedback=a.execute_and_wait("feedback.record",json!({"member_id":"default","operation_key":"f","intent_id":intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5}})).unwrap();
    let wear=a.execute_and_wait("wear.record",json!({"member_id":"default","operation_key":"w","clothing_ids":ids,"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    let before = a
        .execute_and_wait("wear.list", json!({"member_id":"default"}))
        .unwrap();
    let p = a
        .execute_and_wait(
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
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"again"}),
        )
        .unwrap();
    assert_eq!(p["inferred"], p2["inferred"]);
    assert_eq!(
        a.execute_and_wait("wear.list", json!({"member_id":"default"}))
            .unwrap(),
        before
    );
}
#[test]
fn recommendation_reads_learning_weights_without_overriding_excluded_or_fixed_ids() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let red = a
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"红","category":"top","color":["red"]}),
        )
        .unwrap();
    let blue = a
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
        )
        .unwrap();
    for category in ["bottom", "shoes"] {
        a.execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":category,"category":category}),
        )
        .unwrap();
    }
    a.execute_and_wait("learning.set",json!({"member_id":"default","operation_key":"prefer","dimension":"color","value":"blue","weight":5})).unwrap();
    let preferred = a
        .execute_and_wait(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"preferred","timezone":"UTC","limit":1}),
        )
        .unwrap();
    assert_eq!(
        preferred["proposals"][0]["items"][0]["clothing_id"],
        blue["id"]
    );
    assert!(preferred["learning_context"]["algorithm_version"].is_string());
    let fixed=a.execute_and_wait("recommendation.generate",json!({"member_id":"default","operation_key":"fixed","timezone":"UTC","limit":1,"fixed_ids":[red["id"]],"excluded_ids":[blue["id"]]})).unwrap();
    assert_eq!(fixed["proposals"][0]["items"][0]["clothing_id"], red["id"]);
}
#[test]
fn edits_and_deleted_inferences_reset_boundaries_survive_recompute_without_deleting_wear() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let clothing = a
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
        )
        .unwrap();
    a.execute_and_wait("wear.record",json!({"member_id":"default","operation_key":"wear","clothing_ids":[clothing["id"]],"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    let before = a
        .execute_and_wait("wear.list", json!({"member_id":"default"}))
        .unwrap();
    let explicit=a.execute_and_wait("learning.set",json!({"member_id":"default","operation_key":"explicit","dimension":"style","value":"casual","weight":4})).unwrap();
    let edited=a.execute_and_wait("learning.edit",json!({"member_id":"default","operation_key":"edit","root_id":explicit["id"],"expected_revision":1,"weight":-2,"note":"现在不偏好"})).unwrap();
    assert_eq!(edited["revision"], 2);
    assert_eq!(edited["weight"], -2);
    assert_eq!(a.execute_and_wait("learning.edit",json!({"member_id":"default","operation_key":"stale","root_id":explicit["id"],"expected_revision":1,"weight":1})).unwrap_err().code,"stale_revision");
    let learned = a
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"learn"}),
        )
        .unwrap();
    assert!(!learned["inferred"].as_array().unwrap().is_empty());
    a.execute_and_wait("learning.delete",json!({"member_id":"default","operation_key":"forget-blue","dimension":"color","value":"blue"})).unwrap();
    let again = a
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"again"}),
        )
        .unwrap();
    assert!(!again["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["dimension"] == "color" && v["value"] == "blue"));
    a.execute_and_wait(
        "learning.reset",
        json!({"member_id":"default","operation_key":"reset","scope":"inferred"}),
    )
    .unwrap();
    let reset = a
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"afterreset"}),
        )
        .unwrap();
    assert_eq!(reset["inferred"], json!([]));
    assert_eq!(
        a.execute_and_wait("learning.get", json!({"member_id":"default"}))
            .unwrap()["explicit"][0]["weight"],
        -2
    );
    let rebuilt = a
        .execute_and_wait(
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
        a.execute_and_wait("wear.list", json!({"member_id":"default"}))
            .unwrap(),
        before
    );
}
#[test]
fn source_snapshots_and_effective_corrections_are_used_and_management_history_is_paginated() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let garment = a
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
        )
        .unwrap();
    let wear=a.execute_and_wait("wear.record",json!({"member_id":"default","operation_key":"wear","clothing_ids":[garment["id"]],"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    a.execute_and_wait(
        "clothing.update",
        json!({"member_id":"default","id":garment["id"],"color":["red"]}),
    )
    .unwrap();
    let p = a
        .execute_and_wait(
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
    a.execute_and_wait("wear.revoke",json!({"member_id":"default","operation_key":"revoke","root_event_id":wear["root_event_id"],"expected_revision":1,"reason":"误记"})).unwrap();
    assert_eq!(
        a.execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"rr"})
        )
        .unwrap()["inferred"],
        json!([])
    );
    let first = a
        .execute_and_wait("learning.history", json!({"member_id":"default","limit":1}))
        .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert!(first["next_cursor"].is_string());
    let second = a
        .execute_and_wait(
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
    a.execute_and_wait("clothing.create",json!({"member_id":"default","name":"蓝棉衬衫","category":"top","color":["blue"],"material":["cotton"],"style":["casual"]})).unwrap();
    for category in ["bottom", "shoes"] {
        a.execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":category,"category":category}),
        )
        .unwrap();
    }
    let r = a
        .execute_and_wait(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"r","timezone":"UTC"}),
        )
        .unwrap();
    a.execute_and_wait("feedback.record",json!({"member_id":"default","operation_key":"skip","intent_id":r["id"],"proposal_index":0,"reaction":"skip"})).unwrap();
    assert_eq!(
        a.execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"skiponly"})
        )
        .unwrap()["inferred"],
        json!([])
    );
    a.execute_and_wait("feedback.record",json!({"member_id":"default","operation_key":"scores","intent_id":r["id"],"proposal_index":0,"reaction":"skip","scores":{"comfort":5,"style":1}})).unwrap();
    let p = a
        .execute_and_wait(
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
    let member = a.execute_and_wait("space.default", json!({})).unwrap()["member_id"].clone();
    let other = a
        .execute_and_wait("member.create", json!({"name":"other"}))
        .unwrap()["member_id"]
        .clone();
    let m=a.execute_and_wait("learning.set",json!({"member_id":member,"operation_key":"own","dimension":"style","value":"casual","weight":2})).unwrap();
    assert_eq!(
        a.execute_and_wait("learning.get", json!({"member_id":other}))
            .unwrap()["explicit"],
        json!([])
    );
    assert_eq!(a.execute_and_wait("learning.edit",json!({"member_id":other,"operation_key":"foreign","root_id":m["id"],"expected_revision":1,"weight":3})).unwrap_err().code,"not_found");
    let grant=a.execute_and_wait("client.authorize",json!({"name":"memory-reader","expires_at":null,"mode":"limited","operations":["learning.get","learning.history"],"member_ids":[member]})).unwrap();
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
        assert_eq!(a.execute_and_wait("learning.set",json!({"member_id":member,"operation_key":"bad","dimension":dimension,"value":"180","weight":2})).unwrap_err().code,"invalid_input");
    }
    for weight in [json!(6), json!(-6), json!(2.5), json!("2"), json!(true)] {
        assert_eq!(a.execute_and_wait("learning.set",json!({"member_id":member,"operation_key":"bad","dimension":"style","value":"casual","weight":weight})).unwrap_err().code,"invalid_input");
    }
    drop(a);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE learning_profiles;DROP TABLE learning_state;DROP TABLE learning_history;DROP TABLE learning_receipts;DROP TABLE learning_memories;PRAGMA user_version=20;").unwrap();
    drop(db);
    let a = Application::open(&path).unwrap();
    assert_eq!(
        a.execute_and_wait("learning.get", json!({"member_id":"default"}))
            .unwrap()["explicit"],
        json!([])
    );
    assert!(a.execute_and_wait("learning.set",json!({"member_id":"default","operation_key":"repaired","dimension":"color","value":"blue","weight":3})).is_ok());
}
#[test]
fn same_feature_requires_edit_instead_of_competing_explicit_values_and_cas_is_atomic() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let pref=a.execute_and_wait("learning.set",json!({"member_id":"default","operation_key":"one","dimension":"color","value":"blue","weight":3})).unwrap();
    assert_eq!(a.execute_and_wait("learning.set",json!({"member_id":"default","operation_key":"two","dimension":"color","value":"blue","weight":-3})).unwrap_err().code,"preference_exists");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = vec![];
    for n in [1, 2] {
        let path = path.clone();
        let root = pref["id"].clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move||{let a=Application::open(&path).unwrap();barrier.wait();a.execute_and_wait("learning.edit",json!({"member_id":"default","operation_key":format!("race{n}"),"root_id":root,"expected_revision":1,"weight":n}))}));
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

// ==== 自 registry_contracts.rs 迁入的运行时/内容契约（issue #70 增量5）====

fn assert_result_matches_schema(schema: &Value, result: &Value, context: &str) {
    assert_eq!(
        schema["type"].as_str(),
        Some("object"),
        "{context}: outputSchema 须为 object"
    );
    let props = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{context}: outputSchema 须声明 properties"));
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|r| r.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    let actual = result
        .as_object()
        .unwrap_or_else(|| panic!("{context} 结果须为对象"));
    for (key, value) in actual {
        let prop = props
            .get(key)
            .unwrap_or_else(|| panic!("{context}: 实际结果字段 {key} 未在 outputSchema 声明"));
        if let Some(const_value) = prop.get("const") {
            assert_eq!(value, const_value, "{context}: 字段 {key} 须等于 const");
        }
        if let Some(allowed) = prop["enum"].as_array() {
            assert!(allowed.contains(value), "{context}: 字段 {key} 须为枚举值");
        }
        let declared_types: Vec<String> = match prop.get("type") {
            Some(Value::String(t)) => vec![t.clone()],
            Some(Value::Array(ts)) => ts.iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            _ => vec![],
        };
        let actual_type = match value {
            Value::String(_) => "string",
            Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Bool(_) => "boolean",
            Value::Null => "null",
        };
        assert!(
            declared_types.is_empty()
                || declared_types.iter().any(|t| {
                    t == actual_type
                        || (t == "number" && value.is_number())
                        || (t == "integer" && value.as_f64().is_some_and(|n| n.fract() == 0.0))
                }),
            "{context}: 字段 {key} 声明类型不符"
        );
    }
    for key in required {
        assert!(
            actual.contains_key(key),
            "{context}: 缺少 required 字段 {key}"
        );
    }
}


#[test]
fn learning_family_effects_examples_and_runtime_output_contract() {
    // 真实运行时输出：显式偏好全生命周期；recompute异步入队由worker本地完成；事实不变
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("lr.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let top = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"上衣","category":"top"}),
        )
        .unwrap();

    let descriptors = operations().as_array().unwrap().clone();
    let schema_of = |name: &str| -> Value {
        let d = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        Value::Object(
            d["outputSchema"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} 必须声明 outputSchema"))
                .clone(),
        )
    };

    let set = app
        .execute(
            "learning.set",
            json!({"member_id":member,"operation_key":"lr-1","dimension":"color","value":"藏青","weight":3,"note":"喜欢"}),
        )
        .unwrap();
    assert_eq!(set["source"]["kind"], json!("explicit_user"));
    assert_result_matches_schema(&schema_of("learning.set"), &set, "set");
    let dup = app
        .execute(
            "learning.set",
            json!({"member_id":member,"operation_key":"lr-2","dimension":"color","value":"藏青","weight":1}),
        )
        .unwrap_err();
    assert_eq!(dup.code, "preference_exists", "同特征重复须拒绝");
    let profile = app
        .execute("learning.get", json!({"member_id":member}))
        .unwrap();
    assert_result_matches_schema(&schema_of("learning.get"), &profile, "get");
    assert_eq!(profile["explicit"].as_array().unwrap().len(), 1);
    let edited = app
        .execute(
            "learning.edit",
            json!({"member_id":member,"operation_key":"lr-3","root_id":set["root_id"],"expected_revision":1,"weight":-1}),
        )
        .unwrap();
    assert_eq!(edited["revision"], json!(2));
    assert_result_matches_schema(&schema_of("learning.edit"), &edited, "edit");
    let stale = app
        .execute(
            "learning.edit",
            json!({"member_id":member,"operation_key":"lr-4","root_id":set["root_id"],"expected_revision":1,"weight":2}),
        )
        .unwrap_err();
    assert_eq!(stale.code, "stale_revision", "过期修订必须拒绝");
    let history = app
        .execute("learning.history", json!({"member_id":member}))
        .unwrap();
    assert_result_matches_schema(&schema_of("learning.history"), &history, "history");

    let rec = app
        .execute(
            "learning.recompute",
            json!({"member_id":member,"operation_key":"lr-5"}),
        )
        .unwrap();
    assert_eq!(
        rec["state"],
        json!("queued"),
        "recompute返回任务收据而非画像"
    );
    assert_result_matches_schema(&schema_of("learning.recompute"), &rec, "recompute");
    let job_id = rec["job_id"].as_str().unwrap().to_string();
    let job = app
        .execute(
            "learning.worker.run",
            json!({"member_id":member,"job_id":job_id}),
        )
        .unwrap();
    assert_eq!(job["kind"], json!("learning_recompute"));
    assert_eq!(job["state"], json!("succeeded"));
    assert_result_matches_schema(&schema_of("learning.worker.run"), &job, "worker.run");
    let profile2 = app
        .execute("learning.get", json!({"member_id":member}))
        .unwrap();
    assert_eq!(profile2["source_stale"], json!(false), "重算后画像不再滞后");

    let deleted = app
        .execute(
            "learning.delete",
            json!({"member_id":member,"operation_key":"lr-6","dimension":"color","value":"藏青"}),
        )
        .unwrap();
    assert_eq!(deleted["operation"], json!("learning.delete"));
    assert_result_matches_schema(&schema_of("learning.delete"), &deleted, "delete");
    let profile3 = app
        .execute("learning.get", json!({"member_id":member}))
        .unwrap();
    assert_eq!(
        profile3["explicit"].as_array().unwrap().len(),
        0,
        "删除后显式偏好消失"
    );
    let reset = app
        .execute(
            "learning.reset",
            json!({"member_id":member,"operation_key":"lr-7","scope":"inferred"}),
        )
        .unwrap();
    assert_result_matches_schema(&schema_of("learning.reset"), &reset, "reset");
    let unchanged = app
        .execute("clothing.get", json!({"member_id":member,"id":top["id"]}))
        .unwrap();
    assert_eq!(unchanged["name"], json!("上衣"), "学习操作不改变衣物事实");

    let worker = descriptors
        .iter()
        .find(|d| d["name"] == json!("learning.worker.run"))
        .unwrap();
    assert_eq!(worker["effect"]["kind"], json!("mutation"));
    assert_eq!(worker["effect"]["network"], json!("none"), "画像重算纯本地");
    assert_eq!(
        worker["outputSchema"]["properties"]["kind"]["const"],
        json!("learning_recompute")
    );
}
