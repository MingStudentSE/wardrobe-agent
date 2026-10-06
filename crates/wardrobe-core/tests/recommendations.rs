use serde_json::{json, Value};
use wardrobe_core::Application;
fn item(a: &Application, name: &str, category: &str) -> Value {
    a.execute(
        "clothing.create",
        json!({"member_id":"default","name":name,"category":category}),
    )
    .unwrap()
}
#[test]
fn default_three_offline_distinct_real_sets_persist_without_creating_actual_wear() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    for (name, category) in [
        ("上衣A", "top"),
        ("上衣B", "top"),
        ("长裤A", "bottom"),
        ("裙子B", "bottom"),
        ("鞋", "shoes"),
    ] {
        item(&a, name, category);
    }
    let r = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"three","timezone":"Asia/Shanghai"}),
        )
        .unwrap();
    assert_eq!(r["proposals"].as_array().unwrap().len(), 3);
    let sets: std::collections::BTreeSet<Vec<String>> = r["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let mut s: Vec<String> = p["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["clothing_id"].as_str().unwrap().to_owned())
                .collect();
            s.sort();
            s
        })
        .collect();
    assert_eq!(sets.len(), 3);
    for p in r["proposals"].as_array().unwrap() {
        assert_eq!(p["complete"], true);
        assert_eq!(p["items"].as_array().unwrap().len(), 3);
    }
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    drop(a);
    let a = Application::open(&path).unwrap();
    assert_eq!(
        a.execute(
            "recommendation.get",
            json!({"member_id":"default","id":r["id"]})
        )
        .unwrap(),
        r
    );
}
#[test]
fn fixed_and_excluded_are_hard_and_dirty_fixed_is_an_explicit_conflict() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let fixed = item(&a, "固定衬衫", "top");
    let excluded = item(&a, "排除衬衫", "top");
    item(&a, "裤子", "bottom");
    item(&a, "鞋", "shoes");
    let request = json!({"member_id":"default","operation_key":"constraints","timezone":"UTC","fixed_ids":[fixed["id"]],"excluded_ids":[excluded["id"]],"limit":5});
    let r = a
        .execute("recommendation.generate", request.clone())
        .unwrap();
    assert_eq!(r["actual_count"], 1);
    for p in r["proposals"].as_array().unwrap() {
        let ids: Vec<_> = p["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["clothing_id"].clone())
            .collect();
        assert!(ids.contains(&fixed["id"]));
        assert!(!ids.contains(&excluded["id"]));
    }
    assert_eq!(
        a.execute("recommendation.generate", request.clone())
            .unwrap(),
        r
    );
    a.execute("care.mark_dirty",json!({"member_id":"default","clothing_id":fixed["id"],"dirty":true,"local_date":"2020-01-01","timezone":"UTC","operation_key":"dirty"})).unwrap();
    let mut later = request;
    later["operation_key"] = json!("later");
    assert_eq!(
        a.execute("recommendation.generate", later)
            .unwrap_err()
            .code,
        "constraint_conflict"
    );
}
#[test]
fn dress_and_fixed_layers_and_shortage_never_invent_inventory() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let dress = item(&a, "连衣裙", "dress");
    let coat = item(&a, "外套", "outerwear");
    let shoes = item(&a, "鞋", "shoes");
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"dress","timezone":"UTC","fixed_ids":[coat["id"]]})).unwrap();
    assert_eq!(r["actual_count"], 1);
    let p = &r["proposals"][0];
    assert_eq!(p["complete"], true);
    for c in [&dress, &coat, &shoes] {
        assert!(p["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["clothing_id"] == c["id"]));
    }
    assert!(p["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["clothing_id"] == coat["id"] && i["layer"] == 1));
    let b = Application::open(&d.path().join("empty")).unwrap();
    let top = item(&b, "唯一上衣", "top");
    let r = b
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"shortage","timezone":"UTC"}),
        )
        .unwrap();
    assert_eq!(r["actual_count"], 1);
    assert_eq!(r["proposals"][0]["complete"], false);
    assert_eq!(r["proposals"][0]["items"][0]["clothing_id"], top["id"]);
    assert_eq!(
        r["proposals"][0]["missing_roles"],
        json!(["bottom", "shoes"])
    );
    assert!(!r["shortage_reasons"].as_array().unwrap().is_empty());
}
#[test]
fn explicit_save_is_atomic_idempotent_template_only_and_rechecks_availability() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let top = item(&a, "衬衫", "top");
    item(&a, "裤子", "bottom");
    item(&a, "鞋", "shoes");
    let intent = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"generate","timezone":"UTC"}),
        )
        .unwrap();
    let save = json!({"member_id":"default","id":intent["id"],"proposal_index":0,"name":"下周搭配","operation_key":"save"});
    let saved = a.execute("recommendation.save", save.clone()).unwrap();
    assert_eq!(
        a.execute("recommendation.save", save.clone()).unwrap(),
        saved
    );
    assert_eq!(saved["items"].as_array().unwrap().len(), 3);
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        a.execute("outfit.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    a.execute("care.mark_dirty",json!({"member_id":"default","clothing_id":top["id"],"dirty":true,"local_date":"2020-01-01","timezone":"UTC","operation_key":"dirty"})).unwrap();
    let mut next = save.clone();
    next["operation_key"] = json!("save-again");
    assert_eq!(
        a.execute("recommendation.save", next).unwrap_err().code,
        "constraint_conflict"
    );
    drop(a);
    let a = Application::open(&path).unwrap();
    assert_eq!(a.execute("recommendation.save", save).unwrap(), saved);
}
#[test]
fn seven_local_day_actual_repeat_is_soft_explicit_and_priority_affects_first_result() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let first = item(&a, "常穿上衣", "top");
    let second = item(&a, "优先上衣", "top");
    let bottom = item(&a, "裤子", "bottom");
    let shoe = item(&a, "鞋", "shoes");
    let today = chrono::Utc::now().date_naive().to_string();
    a.execute("wear.record",json!({"member_id":"default","operation_key":"actual","local_date":today,"timezone":"UTC","clothing_ids":[first["id"],bottom["id"],shoe["id"]]})).unwrap();
    let r = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"recent","timezone":"UTC","limit":1}),
        )
        .unwrap();
    assert!(r["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["clothing_id"] == second["id"]));
    assert_eq!(r["proposals"][0]["repeat_relaxed"], false);
    let r = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"relax","timezone":"UTC","limit":3}),
        )
        .unwrap();
    assert_eq!(r["actual_count"], 2);
    assert!(r["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["repeat_relaxed"] == true && p["reasons"].to_string().contains("7天")));
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"priority","timezone":"UTC","limit":1,"avoid_recent":false,"inventory_priority_ids":[second["id"]]})).unwrap();
    assert!(r["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["clothing_id"] == second["id"]));
}
#[test]
fn replace_one_and_change_set_preserve_fixed_hard_controls_and_persist_child_intents() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let fixed = item(&a, "固定裤子", "bottom");
    for name in ["上衣A", "上衣B", "上衣C"] {
        item(&a, name, "top");
    }
    item(&a, "鞋", "shoes");
    let initial=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"initial","timezone":"UTC","limit":1,"fixed_ids":[fixed["id"]]})).unwrap();
    let p = &initial["proposals"][0];
    let old = p["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["role"] == "top")
        .unwrap();
    let replacement=a.execute("recommendation.replace",json!({"member_id":"default","id":initial["id"],"proposal_index":0,"clothing_id":old["clothing_id"],"operation_key":"replace"})).unwrap();
    let items = replacement["proposals"][0]["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["clothing_id"] == fixed["id"]));
    assert!(!items.iter().any(|i| i["clothing_id"] == old["clothing_id"]));
    assert_eq!(a.execute("recommendation.replace",json!({"member_id":"default","id":initial["id"],"proposal_index":0,"clothing_id":fixed["id"],"operation_key":"bad"})).unwrap_err().code,"constraint_conflict");
    let changed=a.execute("recommendation.change",json!({"member_id":"default","id":initial["id"],"proposal_index":0,"operation_key":"change"})).unwrap();
    assert_ne!(changed["proposals"][0]["items"], p["items"]);
    assert!(changed["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["clothing_id"] == fixed["id"]));
    assert_eq!(
        a.execute(
            "recommendation.get",
            json!({"member_id":"default","id":initial["id"]})
        )
        .unwrap(),
        initial
    );
}
#[test]
fn manual_weather_controls_are_explicit_offline_stale_and_strictly_validated() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    item(&a, "衬衫", "top");
    item(&a, "裤子", "bottom");
    item(&a, "鞋", "shoes");
    item(&a, "外套", "outerwear");
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"cold","timezone":"UTC","occasion":"work","weather":{"mode":"manual","temperature_c":5,"location":"上海","obtained_at":"2020-01-01T00:00:00Z"}})).unwrap();
    assert_eq!(r["weather_context"]["source"], "manual");
    assert_eq!(r["weather_context"]["stale"], true);
    assert!(r["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["role"] == "outerwear"));
    for weather in [
        json!({"mode":"manual","temperature_c":100}),
        json!({"mode":"automatic"}),
        json!({"mode":"off","temperature_c":10}),
    ] {
        assert_eq!(a.execute("recommendation.generate",json!({"member_id":"default","operation_key":weather.to_string(),"timezone":"UTC","weather":weather})).unwrap_err().code,"invalid_input");
    }
    assert_eq!(
        a.execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"unknown","timezone":"UTC","made_up":true})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
}
