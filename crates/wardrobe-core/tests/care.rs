use serde_json::{json, Value};
use wardrobe_core::Application;
fn setup() -> (tempfile::TempDir, Application, Value) {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("wardrobe.sqlite")).unwrap();
    let c = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"衬衫","category":"top"}),
        )
        .unwrap();
    (d, a, c)
}
#[test]
fn positive_optional_threshold_and_manual_dirty_are_separate_from_lifecycle() {
    let (_d, a, c) = setup();
    let q = json!({"member_id":"default","clothing_id":c["id"]});
    let status = a.execute("care.status", q.clone()).unwrap();
    assert_eq!(status["wash_after_wears"], Value::Null);
    assert_eq!(status["available"], true);
    for threshold in [json!(0), json!(-1), json!(1.5)] {
        assert_eq!(a.execute("care.configure",json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":threshold,"expected_revision":0,"operation_key":format!("bad{threshold}")})).unwrap_err().code,"invalid_input");
    }
    a.execute("care.configure",json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":2,"expected_revision":0,"operation_key":"threshold"})).unwrap();
    a.execute("care.mark_dirty",json!({"member_id":"default","clothing_id":c["id"],"dirty":true,"local_date":"2020-01-01","timezone":"Asia/Shanghai","operation_key":"dirty"})).unwrap();
    let s = a.execute("care.status", q).unwrap();
    assert_eq!(s["manual_dirty"], true);
    assert_eq!(s["available"], false);
    assert_eq!(
        a.execute("clothing.get", json!({"member_id":"default","id":c["id"]}))
            .unwrap()["status"],
        "active"
    );
    assert_eq!(
        a.execute("care.dirty_list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn chronological_backdating_correction_revocation_and_maintenance_rebuild_real_counts() {
    let (_d, a, c) = setup();
    for (key, date) in [
        ("one", "2020-01-01"),
        ("two", "2020-01-02"),
        ("four", "2020-01-04"),
    ] {
        a.execute("wear.record",json!({"member_id":"default","operation_key":key,"local_date":date,"timezone":"Asia/Shanghai","clothing_ids":[c["id"]]})).unwrap();
    }
    let req = json!({"member_id":"default","clothing_id":c["id"],"action":"wash","operation_key":"wash","local_date":"2020-01-03","timezone":"Asia/Shanghai"});
    let wash = a.execute("care.record", req.clone()).unwrap();
    let q = json!({"member_id":"default","clothing_id":c["id"]});
    assert_eq!(
        a.execute("care.status", q.clone()).unwrap()["wears_since_clean"],
        1
    );
    a.execute("wear.record",json!({"member_id":"default","operation_key":"backdate","local_date":"2020-01-02","timezone":"Asia/Shanghai","clothing_ids":[c["id"]]})).unwrap();
    assert_eq!(
        a.execute("care.status", q.clone()).unwrap()["wears_since_clean"],
        1
    );
    let change = json!({"member_id":"default","root_event_id":wash["id"],"expected_revision":1,"operation_key":"correct","action":"wash","local_date":"2020-01-01","timezone":"Asia/Shanghai"});
    let corrected = a.execute("care.correct", change.clone()).unwrap();
    assert_eq!(
        a.execute("care.status", q.clone()).unwrap()["wears_since_clean"],
        3
    );
    assert_eq!(a.execute("care.record", req).unwrap(), wash);
    assert_eq!(a.execute("care.correct", change).unwrap(), corrected);
    a.execute("care.record",json!({"member_id":"default","clothing_id":c["id"],"action":"maintenance","operation_key":"care","local_date":"2020-01-05","timezone":"Asia/Shanghai"})).unwrap();
    assert_eq!(
        a.execute("care.status", q.clone()).unwrap()["wears_since_clean"],
        3
    );
    a.execute("care.revoke",json!({"member_id":"default","root_event_id":wash["id"],"expected_revision":2,"operation_key":"revoke","reason":"误记"})).unwrap();
    assert_eq!(a.execute("care.status", q).unwrap()["wears_since_clean"], 4);
    assert_eq!(
        a.execute(
            "care.history",
            json!({"member_id":"default","root_event_id":wash["id"]})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn caller_cannot_bypass_manual_dirty_intent_or_omit_config_revision() {
    let (_d, a, c) = setup();
    assert_eq!(a.execute("care.record",json!({"member_id":"default","clothing_id":c["id"],"action":"wash","dirty":true,"local_date":"2020-01-01","timezone":"UTC","operation_key":"bad"})).unwrap_err().code,"invalid_input");
    assert_eq!(a.execute("care.configure",json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":2,"operation_key":"threshold"})).unwrap_err().code,"invalid_input");
}
#[test]
fn manual_dirty_clear_cleaning_and_maintenance_follow_actual_chronology_and_wear_revocations() {
    let (_d, a, c) = setup();
    let base = json!({"member_id":"default","clothing_id":c["id"]});
    let mut d = base.clone();
    d["dirty"] = json!(true);
    d["operation_key"] = json!("flag");
    d["local_date"] = json!("2020-01-04");
    d["timezone"] = json!("UTC");
    a.execute("care.mark_dirty", d).unwrap();
    for (action, date, key) in [
        ("maintenance", "2020-01-05", "maintenance"),
        ("wash", "2020-01-03", "oldwash"),
    ] {
        let mut q = base.clone();
        q["action"] = json!(action);
        q["local_date"] = json!(date);
        q["timezone"] = json!("UTC");
        q["operation_key"] = json!(key);
        a.execute("care.record", q).unwrap();
    }
    assert_eq!(
        a.execute("care.status", base.clone()).unwrap()["manual_dirty"],
        true
    );
    let clean=a.execute("care.record",json!({"member_id":"default","clothing_id":c["id"],"action":"dry_clean","local_date":"2020-01-06","timezone":"UTC","operation_key":"dry"})).unwrap();
    assert_eq!(
        a.execute("care.status", base.clone()).unwrap()["manual_dirty"],
        false
    );
    a.execute("care.revoke",json!({"member_id":"default","root_event_id":clean["id"],"expected_revision":1,"operation_key":"undodry","reason":"误记"})).unwrap();
    assert_eq!(
        a.execute("care.status", base.clone()).unwrap()["manual_dirty"],
        true
    );
    a.execute("care.mark_dirty",json!({"member_id":"default","clothing_id":c["id"],"dirty":false,"local_date":"2020-01-07","timezone":"UTC","operation_key":"clear"})).unwrap();
    assert_eq!(
        a.execute("care.status", base.clone()).unwrap()["available"],
        true
    );
    let wear=a.execute("wear.record",json!({"member_id":"default","clothing_ids":[c["id"]],"local_date":"2020-01-08","timezone":"UTC","operation_key":"wear"})).unwrap();
    assert_eq!(
        a.execute("care.status", base.clone()).unwrap()["wears_since_clean"],
        1
    );
    a.execute("wear.revoke",json!({"member_id":"default","root_event_id":wear["id"],"expected_revision":1,"operation_key":"undowear","reason":"误记"})).unwrap();
    assert_eq!(
        a.execute("care.status", base).unwrap()["wears_since_clean"],
        0
    );
}
#[test]
fn timezone_exact_retries_cas_member_isolation_and_deleted_snapshot_history_survive_restart() {
    let (d, a, c) = setup();
    let m = a
        .execute("member.create", json!({"name":"另一个成员"}))
        .unwrap();
    assert_eq!(
        a.execute(
            "care.status",
            json!({"member_id":m["member_id"],"clothing_id":c["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    for (date, time, zone) in [
        ("2020-03-08", "02:30", "America/New_York"),
        ("2020-11-01", "01:30", "America/New_York"),
        ("2999-01-01", "12:00", "Pacific/Honolulu"),
    ] {
        assert!(a.execute("care.record",json!({"member_id":"default","clothing_id":c["id"],"action":"wash","local_date":date,"local_time":time,"timezone":zone,"operation_key":format!("bad{date}")})).is_err());
    }
    let request = json!({"member_id":"default","clothing_id":c["id"],"action":"wash","local_date":"2020-01-01","local_time":"23:30","timezone":"Pacific/Honolulu","operation_key":"wash"});
    let event = a.execute("care.record", request.clone()).unwrap();
    assert_eq!(event["occurred_at"], "2020-01-02T09:30:00+00:00");
    assert_eq!(event["time_precision"], "minute");
    let mut other = request.clone();
    other["action"] = json!("dry_clean");
    assert_eq!(
        a.execute("care.record", other).unwrap_err().code,
        "idempotency_conflict"
    );
    a.execute(
        "clothing.delete",
        json!({"member_id":"default","id":c["id"]}),
    )
    .unwrap();
    let corrected=a.execute("care.correct",json!({"member_id":"default","root_event_id":event["id"],"expected_revision":1,"operation_key":"correction","action":"dry_clean","local_date":"2020-01-03","timezone":"UTC"})).unwrap();
    assert_eq!(corrected["snapshot"]["name"], "衬衫");
    assert_eq!(a.execute("care.revoke",json!({"member_id":"default","root_event_id":event["id"],"expected_revision":1,"operation_key":"stale","reason":"误记"})).unwrap_err().code,"version_conflict");
    drop(a);
    let a = Application::open(&d.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(a.execute("care.record", request).unwrap(), event);
    assert_eq!(
        a.execute(
            "care.status",
            json!({"member_id":"default","clothing_id":c["id"]})
        )
        .unwrap()["exclusion_reasons"],
        json!(["deleted"])
    );
    assert_eq!(
        a.execute("care.availability", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn max_schema_reopens_and_repairs_actual_missing_care_tables() {
    let (d, a, c) = setup();
    drop(a);
    let p = d.path().join("wardrobe.sqlite");
    let db = rusqlite::Connection::open(&p).unwrap();
    db.execute_batch("DROP TABLE care_config; DROP TABLE care_asset_refs; DROP TABLE care_receipts; DROP TABLE care_events;").unwrap();
    drop(db);
    let a = Application::open(&p).unwrap();
    assert_eq!(
        a.execute(
            "care.status",
            json!({"member_id":"default","clothing_id":c["id"]})
        )
        .unwrap()["wash_after_wears"],
        Value::Null
    );
    assert_eq!(
        a.execute("recognition.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}
#[test]
fn shared_garment_across_templates_reaches_threshold_without_blocking_truthful_wear() {
    let (_d, a, c) = setup();
    for name in ["搭配一", "搭配二"] {
        let outfit=a.execute("outfit.create",json!({"member_id":"default","name":name,"items":[{"clothing_id":c["id"],"role":"base","layer":0}]})).unwrap();
        a.execute("wear.record",json!({"member_id":"default","source_outfit_id":outfit["id"],"source_version":1,"local_date":"2020-01-01","timezone":"UTC","operation_key":name})).unwrap();
    }
    a.execute("care.configure",json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":2,"expected_revision":0,"operation_key":"threshold"})).unwrap();
    let q = json!({"member_id":"default","clothing_id":c["id"]});
    let s = a.execute("care.status", q.clone()).unwrap();
    assert_eq!(s["wears_since_clean"], 2);
    assert_eq!(s["exclusion_reasons"], json!(["threshold_reached"]));
    a.execute("wear.record",json!({"member_id":"default","clothing_ids":[c["id"]],"local_date":"2020-01-02","timezone":"UTC","operation_key":"truthfuldirtywear"})).unwrap();
    assert_eq!(
        a.execute("care.status", q.clone()).unwrap()["wears_since_clean"],
        3
    );
    a.execute("care.configure",json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":null,"expected_revision":1,"operation_key":"unset"})).unwrap();
    assert_eq!(a.execute("care.status", q).unwrap()["available"], true);
}
#[test]
fn configuration_keeps_identity_creation_timestamp_and_uses_revision_cas() {
    let (_d, a, c) = setup();
    let first=a.execute("care.configure",json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":2,"expected_revision":0,"operation_key":"one"})).unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(first["id"].as_str().unwrap())
            .unwrap()
            .get_version_num(),
        7
    );
    let second=a.execute("care.configure",json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":3,"expected_revision":1,"operation_key":"two"})).unwrap();
    assert_eq!(second["id"], first["id"]);
    assert_eq!(second["created_at"], first["created_at"]);
    assert_eq!(a.execute("care.configure",json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":null,"expected_revision":1,"operation_key":"stale"})).unwrap_err().code,"version_conflict");
}
