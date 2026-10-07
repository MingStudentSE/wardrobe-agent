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
#[test]
fn pairing_list_keyset_pages_durable_anchor_intents_excluding_foreign_and_unmarked() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let member = |a: &Application, name: &str| -> String {
        a.execute("member.create", json!({"name":name})).unwrap()["member_id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let member_a = member(&a, "成员甲");
    let member_b = member(&a, "成员乙");
    let item = |a: &Application, member: &str, name: &str, category: &str| -> String {
        a.execute(
            "clothing.create",
            json!({"member_id":member,"name":name,"category":category}),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    // Two real complete inventories under two explicit members.
    let anchor_a = item(&a, &member_a, "锚点上衣", "top");
    let anchor_b = item(&a, &member_a, "另一件上衣", "top");
    item(&a, &member_a, "裤子", "bottom");
    item(&a, &member_a, "鞋", "shoes");
    let anchor_foreign = item(&a, &member_b, "乙锚点上衣", "top");
    item(&a, &member_b, "乙裤子", "bottom");
    item(&a, &member_b, "乙鞋", "shoes");
    let anchor = |a: &Application, member: &str, anchor: &str, key: &str| -> String {
        a.execute(
            "pairing.generate",
            json!({"member_id":member,"operation_key":key,"anchor_id":anchor,"timezone":"UTC","limit":1}),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    // Two independent intents for anchorA, one same-member intent on another
    // anchor, and one foreign member's pair.
    let first_a = anchor(&a, &member_a, &anchor_a, "pair-a1");
    let second_a = anchor(&a, &member_a, &anchor_a, "pair-a2");
    let _other_anchor = anchor(&a, &member_a, &anchor_b, "pair-a-other");
    let _foreign_pair = anchor(&a, &member_b, &anchor_foreign, "pair-b");
    // Ordinary unmarked intent from the shared recommendation path as control.
    let _unmarked = a
        .execute(
            "recommendation.generate",
            json!({"member_id":member_a,"operation_key":"ordinary","timezone":"UTC","limit":1}),
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(a);
    // Reopen before querying to prove the pairing marker is durable.
    let a = Application::open(&path).unwrap();
    let page1 = a
        .execute(
            "pairing.list",
            json!({"member_id":member_a,"anchor_id":anchor_a,"limit":1}),
        )
        .unwrap();
    assert_eq!(page1["schema_version"], 1, "{page1}");
    assert_eq!(page1["member_id"], json!(member_a), "{page1}");
    let page1_items = page1["items"].as_array().unwrap();
    assert_eq!(page1_items.len(), 1, "{page1}");
    let cursor = page1["next_cursor"].as_str().unwrap();
    assert!(!cursor.is_empty(), "{page1}");
    let page2 = a
        .execute(
            "pairing.list",
            json!({"member_id":member_a,"anchor_id":anchor_a,"limit":1,"cursor":cursor}),
        )
        .unwrap();
    let page2_items = page2["items"].as_array().unwrap();
    assert_eq!(page2_items.len(), 1, "{page2}");
    let terminal = &page2["next_cursor"];
    assert!(
        terminal.is_null() || terminal.as_str().is_some_and(|s| s.is_empty()),
        "{page2}"
    );
    // Bounded keyset pages never overlap.
    let page1_ids: Vec<&str> = page1_items
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    let page2_ids: Vec<&str> = page2_items
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(page1_ids.iter().all(|id| !page2_ids.contains(id)));
    // The union is exactly the two anchorA intent IDs from the public creation
    // responses: no duplicates, no other-anchor, foreign-member or unmarked intent.
    let mut listed: Vec<&str> = page1_ids.clone();
    listed.extend(page2_ids.clone());
    listed.sort();
    let mut expected = vec![first_a.as_str(), second_a.as_str()];
    expected.sort();
    assert_eq!(listed, expected);
}
