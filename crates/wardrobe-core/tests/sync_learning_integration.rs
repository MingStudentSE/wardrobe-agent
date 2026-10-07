use serde_json::json;
use wardrobe_core::Application;
#[path = "support/learning.rs"]
mod learning_support;
use learning_support::LearningTestSeam;

#[test]
fn fresh_install_feedback_and_wear_invalidate_the_recomputed_profile_without_reopening() {
    let temporary = tempfile::tempdir().unwrap();
    let app = Application::open(&temporary.path().join("db")).unwrap();
    let mut ids = Vec::new();
    for (name, category) in [("上衣", "top"), ("长裤", "bottom"), ("鞋", "shoes")] {
        let item = app
            .execute(
                "clothing.create",
                json!({"member_id":"default","name":name,"category":category}),
            )
            .unwrap();
        ids.push(item["id"].clone());
    }
    let intent = app
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"initial inventory","timezone":"UTC"}),
        )
        .unwrap();
    let feedback = app.execute("feedback.record", json!({"member_id":"default","operation_key":"initial feedback","intent_id":intent["id"],"proposal_index":0,"reaction":"like"})).unwrap();
    let first = app
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"first projection"}),
        )
        .unwrap();
    assert_eq!(first["source_stale"], false);
    app.execute("feedback.correct", json!({"member_id":"default","operation_key":"correct feedback","root_id":feedback["root_id"],"expected_revision":1,"reaction":"dislike"})).unwrap();
    assert_eq!(app.execute("learning.get", json!({"member_id":"default"})).unwrap()["source_stale"], true, "new feedback must invalidate a freshly recomputed profile on the first open, before any reopen repairs migrations");
    let second = app
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"second projection"}),
        )
        .unwrap();
    assert_eq!(second["source_stale"], false);
    app.execute("wear.record", json!({"member_id":"default","operation_key":"actual wear","clothing_ids":ids,"local_date":"2026-10-07","timezone":"UTC"})).unwrap();
    assert_eq!(
        app.execute("learning.get", json!({"member_id":"default"}))
            .unwrap()["source_stale"],
        true,
        "new actual wear must also invalidate the profile during the same first open"
    );
}
