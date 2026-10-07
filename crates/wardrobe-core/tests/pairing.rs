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
fn pairing_generate_anchors_every_real_proposal_persists_immutable_intent_without_wear() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let anchor = item(&a, "锚点上衣", "top");
    item(&a, "裤子", "bottom");
    item(&a, "鞋", "shoes");
    let r = a
        .execute(
            "pairing.generate",
            json!({"member_id":"default","operation_key":"anchor-top","anchor_id":anchor["id"],"timezone":"UTC","limit":2}),
        )
        .unwrap();
    let proposals = r["proposals"].as_array().unwrap();
    assert!(!proposals.is_empty());
    for p in proposals {
        let anchor_count = p["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["clothing_id"] == anchor["id"])
            .count();
        assert_eq!(anchor_count, 1, "{p}");
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
