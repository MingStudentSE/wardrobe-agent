use serde_json::json;
use wardrobe_core::Application;
#[test]
#[ignore = "actual 10k clothing / 100k wear projection scale evidence; run explicitly"]
fn nonempty_learning_ranks_ten_thousand_inventory_hundred_thousand_effective_wears() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let seed=a.execute("clothing.create",json!({"member_id":"default","name":"基准上衣","category":"top","color":["blue"],"style":["casual"]})).unwrap();
    let wear=a.execute("wear.record",json!({"member_id":"default","operation_key":"seed","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[seed["id"]]})).unwrap();
    drop(a);
    let mut db = rusqlite::Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    {
        let mut clothing = tx.prepare("INSERT INTO clothing VALUES(?1,?2,?3)").unwrap();
        for i in 0..9999 {
            let mut c = seed.clone();
            c["id"] = json!(format!("scale-clothing-{i:05}"));
            c["name"] = json!(format!("基准衣物{i}"));
            c["category"] = json!(["top", "bottom", "shoes"][i % 3]);
            c["style"] = json!([format!("learning-style-{}", i % 64)]);
            c["color"] = json!([if i % 2 == 0 { "blue" } else { "red" }]);
            clothing
                .execute(rusqlite::params![
                    c["id"].as_str(),
                    c["member_id"].as_str(),
                    c.to_string()
                ])
                .unwrap();
        }
        let mut events = tx
            .prepare("INSERT INTO wear_events VALUES(?1,?2,?1,1,?3)")
            .unwrap();
        for i in 0..99999 {
            let mut w = wear.clone();
            let id = format!("scale-wear-{i:06}");
            w["id"] = json!(id);
            w["root_event_id"] = w["id"].clone();
            events
                .execute(rusqlite::params![
                    id,
                    w["member_id"].as_str(),
                    w.to_string()
                ])
                .unwrap();
        }
    }
    tx.commit().unwrap();
    drop(db);
    let a = Application::open(&path).unwrap();
    for i in 0..64 {
        a.execute("learning.set",json!({"member_id":"default","operation_key":format!("pref{i}"),"dimension":"style","value":format!("learning-style-{i}"),"weight":i%11-5})).unwrap();
    }
    a.execute("learning.set",json!({"member_id":"default","operation_key":"blue","dimension":"color","value":"blue","weight":5})).unwrap();
    let learn_clock = std::time::Instant::now();
    let profile = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"full100k"}),
        )
        .unwrap();
    let learn_ms = learn_clock.elapsed().as_millis();
    assert_eq!(profile["inferred"].as_array().unwrap().len(), 3);
    assert!(profile["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["source"]["wear_ids"].as_array().unwrap().len() == 100000));
    let clock = std::time::Instant::now();
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"scale","timezone":"UTC","limit":5,"fixed_ids":[seed["id"]]})).unwrap();
    let elapsed = clock.elapsed();
    assert_eq!(r["learning_context"]["explicit_count"], 65);
    assert_eq!(r["learning_context"]["inferred_count"], 3);
    assert_eq!(r["actual_count"], 5);
    for proposal in r["proposals"].as_array().unwrap() {
        assert_eq!(proposal["complete"], true);
        assert!(proposal["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["clothing_id"] == seed["id"]));
    }
    println!("LEARNING_SCALE recompute_ms={learn_ms} explicit=65 inferred=3 inventory=10000 effective_wears=100000 requested=5 actual=5 fixed_preserved=true elapsed_ms={} db_bytes={}",elapsed.as_millis(),std::fs::metadata(&path).unwrap().len());
}
