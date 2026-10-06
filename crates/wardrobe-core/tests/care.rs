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
