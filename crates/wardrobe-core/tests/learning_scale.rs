#[path = "support/learning.rs"]
mod support;
use serde_json::json;
use support::LearningTestSeam;
use wardrobe_core::Application;
#[test]
#[ignore = "actual 10k clothing / 100k wear projection scale evidence; run explicitly"]
fn nonempty_learning_ranks_ten_thousand_inventory_hundred_thousand_effective_wears() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let seed=a.execute_and_wait("clothing.create",json!({"member_id":"default","name":"基准上衣","category":"top","color":["blue"],"style":["casual"]})).unwrap();
    let wear=a.execute_and_wait("wear.record",json!({"member_id":"default","operation_key":"seed","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[seed["id"]]})).unwrap();
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
            let id = format!("00000000-0000-4000-8000-{i:012}");
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
        a.execute_and_wait("learning.set",json!({"member_id":"default","operation_key":format!("pref{i}"),"dimension":"style","value":format!("learning-style-{i}"),"weight":i%11-5})).unwrap();
    }
    a.execute_and_wait("learning.set",json!({"member_id":"default","operation_key":"blue","dimension":"color","value":"blue","weight":5})).unwrap();
    let learn_clock = std::time::Instant::now();
    let receipt = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"full100k"}),
        )
        .unwrap();
    let enqueue_ms = learn_clock.elapsed().as_millis();
    let job = a
        .execute(
            "learning.worker.run",
            json!({"member_id":"default","job_id":receipt["job_id"]}),
        )
        .unwrap();
    assert_eq!(job["state"], "succeeded");
    let profile = a
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap();
    let learn_ms = learn_clock.elapsed().as_millis();
    assert_eq!(profile["inferred"].as_array().unwrap().len(), 3);
    assert!(profile["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["source"]["wear_ids"].as_array().unwrap().len() == 100000));
    let clock = std::time::Instant::now();
    let r=a.execute_and_wait("recommendation.generate",json!({"member_id":"default","operation_key":"scale","timezone":"UTC","limit":5,"fixed_ids":[seed["id"]]})).unwrap();
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
    let package = d.path().join("archive");
    fn maintenance(a: &Application, op: &str, input: serde_json::Value) -> serde_json::Value {
        let receipt = a.execute(op, input).unwrap();
        let job = a
            .execute("maintenance.worker.run", json!({"id":receipt["job_id"]}))
            .unwrap();
        assert_eq!(job["state"], "succeeded", "{job}");
        job["result"].clone()
    }
    let create_clock = std::time::Instant::now();
    let _backup = maintenance(
        &a,
        "backup.create",
        json!({"directory":package,"include_originals":false}),
    );
    let create_ms = create_clock.elapsed().as_millis();
    let preview_clock = std::time::Instant::now();
    assert_eq!(
        maintenance(&a, "backup.preview", json!({"directory":package}))["valid"],
        true
    );
    let preview_ms = preview_clock.elapsed().as_millis();
    let profile_bytes = profile.to_string().len();
    let db_bytes = std::fs::metadata(&path).unwrap().len();
    drop(a);
    std::fs::remove_file(&path).unwrap();
    let target = d.path().join("restored");
    let b = Application::open(&target).unwrap();
    let restore_clock = std::time::Instant::now();
    maintenance(
        &b,
        "backup.restore",
        json!({"directory":package,"confirm_overwrite":true}),
    );
    let restore_ms = restore_clock.elapsed().as_millis();
    drop(b);
    let b = Application::open(&target).unwrap();
    let imported = b
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(imported["explicit"].as_array().unwrap().len(), 65);
    assert_eq!(imported["source_stale"], true);
    assert!(imported["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["source"]["wear_ids"].as_array().unwrap().len() == 100000));
    let restored_db = rusqlite::Connection::open(&target).unwrap();
    assert_eq!(
        restored_db
            .query_row("SELECT COUNT(*) FROM clothing", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        10000
    );
    assert_eq!(
        restored_db
            .query_row("SELECT COUNT(*) FROM wear_events", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        100000
    );
    println!("LEARNING_PORTABLE_SCALE enqueue_ms={enqueue_ms} profile_bytes={profile_bytes} create_ms={create_ms} preview_ms={preview_ms} restore_ms={restore_ms} source_removed=true reopen=true explicit=65 inferred_stale=true facts_10k100k_preserved=true");
    println!("LEARNING_SCALE recompute_ms={learn_ms} explicit=65 inferred=3 inventory=10000 effective_wears=100000 requested=5 actual=5 fixed_preserved=true elapsed_ms={} db_bytes={}",elapsed.as_millis(),db_bytes);
}
