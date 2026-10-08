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
// Pair-learning oracle support: the public learning recompute + worker seam.
#[path = "support/learning.rs"]
mod support;
// Pair-learning oracle: positively anchored feedback must teach an
// anchor-specific top-bottom relation. Candidates share every attribute
// within their category (names/IDs differ only), so categorical learning
// weights alone can never satisfy it. Like (never dislike) is used because
// dislike already blocks the exact set without any learning. After public
// recompute, anchorA's taught pair must rank first, an identical second
// anchor must keep its baseline order, and ordinary outfit recommendations
// must carry the same learned pair through shared scoring.
#[test]
fn anchored_like_feedback_learns_anchor_specific_pair_ranking_shared_by_ordinary_recommendations() {
    use support::LearningTestSeam;
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let member = a
        .execute("member.create", json!({"name":"配对学习成员"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let item = |a: &Application, name: &str, category: &str| -> String {
        a.execute(
            "clothing.create",
            json!({"member_id":member.as_str(),"name":name,"category":category}),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let anchor_a = item(&a, "锚点上衣甲", "top");
    let anchor_b = item(&a, "锚点上衣乙", "top");
    let bottom_a = item(&a, "裤子甲", "bottom");
    let bottom_b = item(&a, "裤子乙", "bottom");
    let shoe = item(&a, "共享鞋", "shoes");
    let pair = |a: &Application, anchor: &str, key: &str| -> Value {
        a.execute(
            "pairing.generate",
            json!({"member_id":member.as_str(),"operation_key":key,"anchor_id":anchor,"timezone":"UTC","limit":5}),
        )
        .unwrap()
    };
    // The member of each anchored outfit that is neither anchor nor shoe.
    let bottoms = |intent: &Value, anchor: &str, shoe: &str| -> Vec<String> {
        intent["proposals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                p["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| i["clothing_id"].as_str().unwrap())
                    .find(|id| *id != anchor && *id != shoe)
                    .unwrap()
                    .to_owned()
            })
            .collect()
    };
    // Baseline: anchorA yields two distinct bottom proposals; the current
    // top-bottom order is captured from literal public proposal IDs instead
    // of assuming UUID tie order.
    let baseline_a = pair(&a, &anchor_a, "learn-a-baseline");
    let order_a = bottoms(&baseline_a, &anchor_a, &shoe);
    assert_eq!(order_a.len(), 2, "{baseline_a}");
    let mut seen_a = order_a.clone();
    seen_a.sort();
    let mut both_bottoms = vec![bottom_a.clone(), bottom_b.clone()];
    both_bottoms.sort();
    assert_eq!(seen_a, both_bottoms, "{baseline_a}");
    // Positive teaching target: the lower-ranked anchorA proposal's bottom.
    let teaching = order_a[1].clone();
    let baseline_b = pair(&a, &anchor_b, "learn-b-baseline");
    let order_b = bottoms(&baseline_b, &anchor_b, &shoe);
    assert_eq!(order_b.len(), 2, "{baseline_b}");
    let mut seen_b = order_b.clone();
    seen_b.sort();
    assert_eq!(seen_b, both_bottoms, "{baseline_b}");
    // Like the lower-ranked anchorA outfit through the ordinary feedback
    // contract; the pairing intent's controls keep the anchor context.
    let teaching_index = baseline_a["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .position(|p| {
            p["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["clothing_id"].as_str().unwrap() == teaching)
        })
        .unwrap();
    a.execute(
        "feedback.record",
        json!({"member_id":member.as_str(),"operation_key":"learn-like","intent_id":baseline_a["id"],"proposal_index":teaching_index,"reaction":"like"}),
    )
    .unwrap();
    // Feedback alone must not reorder or exclude: a fresh anchorA generation
    // keeps the baseline order before any learning recompute.
    let before = pair(&a, &anchor_a, "learn-a-prerecompute");
    assert_eq!(bottoms(&before, &anchor_a, &shoe), order_a, "{before}");
    a.execute_and_wait(
        "learning.recompute",
        json!({"member_id":member.as_str(),"operation_key":"learn-recompute"}),
    )
    .unwrap();
    // anchorA now ranks the positively taught pair first...
    let learned_a = pair(&a, &anchor_a, "learn-a-learned");
    assert_eq!(
        bottoms(&learned_a, &anchor_a, &shoe)[0],
        teaching,
        "{learned_a}"
    );
    // ...while the identical anchorB keeps its original bottom order: the
    // learned relation is anchor-specific, not a categorical preference.
    let learned_b = pair(&a, &anchor_b, "learn-b-learned");
    assert_eq!(
        bottoms(&learned_b, &anchor_b, &shoe),
        order_b,
        "{learned_b}"
    );
    // The learned pair must live in shared outfit scoring: the first ordinary
    // (non-pairing) recommendation contains anchorA with the taught bottom.
    let ordinary = a
        .execute(
            "recommendation.generate",
            json!({"member_id":member.as_str(),"operation_key":"learn-ordinary","timezone":"UTC","limit":5}),
        )
        .unwrap();
    let first: Vec<&str> = ordinary["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["clothing_id"].as_str().unwrap())
        .collect();
    assert!(
        first.contains(&anchor_a.as_str()) && first.contains(&teaching.as_str()),
        "{ordinary}"
    );
    // Positive feedback is not wear: no wear facts may exist anywhere.
    assert_eq!(
        a.execute("wear.list", json!({"member_id":member.as_str()}))
            .unwrap()["items"],
        json!([])
    );
}
// Pair projection lifecycle: only the effective latest revision of each
// feedback root may teach pairs. A like must publish positive pair rewards, a
// dislike correction on the same root must flip them, and a revocation must
// forget the root entirely on the next public recompute.
#[test]
fn pair_projection_tracks_correction_then_revocation_through_recompute() {
    use support::LearningTestSeam;
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let item = |a: &Application, name: &str, category: &str| -> String {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let anchor = item(&a, "修订锚点上衣", "top");
    item(&a, "修订裤子", "bottom");
    item(&a, "修订鞋", "shoes");
    let intent = a
        .execute(
            "pairing.generate",
            json!({"member_id":"default","operation_key":"revise-anchor","anchor_id":anchor,"timezone":"UTC","limit":1}),
        )
        .unwrap();
    let reaction = |a: &Application, key: &str, op: &str, extra: Value| -> Value {
        let mut input = json!({"member_id":"default","operation_key":key});
        input["root_id"] = extra["root_id"].clone();
        input["expected_revision"] = extra["expected_revision"].clone();
        if let Some(reason) = extra.get("reason") {
            input["reason"] = reason.clone();
        }
        if op == "feedback.correct" {
            input["reaction"] = json!("dislike");
        }
        a.execute(op, input).unwrap()
    };
    let recorded = a
        .execute(
            "feedback.record",
            json!({"member_id":"default","operation_key":"revise-like","intent_id":intent["id"],"proposal_index":0,"reaction":"like"}),
        )
        .unwrap();
    let projection = |a: &Application, key: &str| -> Value {
        a.execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":key}),
        )
        .unwrap()["pair_projection"]
            .clone()
    };
    let liked = projection(&a, "revise-recompute-1");
    let entries = liked["entries"].as_array().unwrap();
    assert!(!entries.is_empty(), "{liked}");
    assert!(
        entries.iter().all(|e| e["weight"].as_i64() == Some(2)),
        "{liked}"
    );
    assert_eq!(
        liked["source"]["feedback_ids"].as_array().unwrap().len(),
        1,
        "{liked}"
    );
    let corrected_doc = reaction(
        &a,
        "revise-correct",
        "feedback.correct",
        json!({"root_id":recorded["root_id"],"expected_revision":recorded["revision"]}),
    );
    let corrected = projection(&a, "revise-recompute-2");
    let entries = corrected["entries"].as_array().unwrap();
    assert!(!entries.is_empty(), "{corrected}");
    assert!(
        entries.iter().all(|e| e["weight"].as_i64() == Some(-2)),
        "{corrected}"
    );
    reaction(
        &a,
        "revise-revoke",
        "feedback.revoke",
        json!({"root_id":corrected_doc["root_id"],"expected_revision":corrected_doc["revision"],"reason":"修订撤销"}),
    );
    let revoked = projection(&a, "revise-recompute-3");
    assert_eq!(
        revoked,
        json!({"schema_version":1,"algorithm_version":"effective-pairs-v1","entries":[],"source":{"feedback_ids":[],"wear_ids":[]}}),
        "{revoked}"
    );
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}
// Reset must forget learned pairs immediately (no stale rewards after the
// managed publication), stay forgotten after a recompute that only sees
// pre-boundary events, and return only through a post-boundary event.
#[test]
fn pairing_reset_forgets_learned_pairs_until_post_boundary_recompute() {
    use support::LearningTestSeam;
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let member = a
        .execute("member.create", json!({"name":"重置配对成员"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let item = |a: &Application, name: &str, category: &str| -> String {
        a.execute(
            "clothing.create",
            json!({"member_id":member.as_str(),"name":name,"category":category}),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let anchor = item(&a, "重置锚点上衣", "top");
    item(&a, "重置裤子甲", "bottom");
    item(&a, "重置裤子乙", "bottom");
    let shoe = item(&a, "重置鞋", "shoes");
    let pair = |a: &Application, key: &str| -> Value {
        a.execute(
            "pairing.generate",
            json!({"member_id":member.as_str(),"operation_key":key,"anchor_id":anchor,"timezone":"UTC","limit":5}),
        )
        .unwrap()
    };
    let bottoms = |intent: &Value| -> Vec<String> {
        intent["proposals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                p["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| i["clothing_id"].as_str().unwrap())
                    .find(|id| *id != anchor && *id != shoe)
                    .unwrap()
                    .to_owned()
            })
            .collect()
    };
    let baseline = pair(&a, "reset-baseline");
    let order = bottoms(&baseline);
    assert_eq!(order.len(), 2, "{baseline}");
    let teaching = order[1].clone();
    let teaching_index = baseline["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .position(|p| {
            p["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["clothing_id"].as_str().unwrap() == teaching)
        })
        .unwrap();
    a.execute(
        "feedback.record",
        json!({"member_id":member.as_str(),"operation_key":"reset-like","intent_id":baseline["id"],"proposal_index":teaching_index,"reaction":"like"}),
    )
    .unwrap();
    a.execute_and_wait(
        "learning.recompute",
        json!({"member_id":member.as_str(),"operation_key":"reset-recompute-1"}),
    )
    .unwrap();
    assert_eq!(bottoms(&pair(&a, "reset-learned"))[0], teaching);
    a.execute(
        "learning.reset",
        json!({"member_id":member.as_str(),"operation_key":"reset-managed","scope":"inferred"}),
    )
    .unwrap();
    let forgotten = a
        .execute("learning.get", json!({"member_id":member.as_str()}))
        .unwrap();
    assert!(forgotten.get("pair_projection").is_none(), "{forgotten}");
    assert_eq!(bottoms(&pair(&a, "reset-after-managed")), order);
    // Only pre-boundary events exist, so an explicit recompute must not
    // resurrect the forgotten pairs.
    let after = a
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":member.as_str(),"operation_key":"reset-recompute-2"}),
        )
        .unwrap();
    assert_eq!(after["pair_projection"]["entries"], json!([]), "{after}");
    assert_eq!(bottoms(&pair(&a, "reset-still-baseline")), order);
    // A post-boundary like is the only way pairs return.
    let fresh = pair(&a, "reset-fresh-intent");
    let fresh_index = fresh["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .position(|p| {
            p["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["clothing_id"].as_str().unwrap() == teaching)
        })
        .unwrap();
    a.execute(
        "feedback.record",
        json!({"member_id":member.as_str(),"operation_key":"reset-like-2","intent_id":fresh["id"],"proposal_index":fresh_index,"reaction":"like"}),
    )
    .unwrap();
    a.execute_and_wait(
        "learning.recompute",
        json!({"member_id":member.as_str(),"operation_key":"reset-recompute-3"}),
    )
    .unwrap();
    assert_eq!(bottoms(&pair(&a, "reset-learned-again"))[0], teaching);
}
// Hard exclusion beats the pairing anchor: an anchor excluded for the day by
// explicit feedback must fail generation with a constraint conflict, and one
// member's anchored feedback must never teach pairs into another member's
// projection.
#[test]
fn pairing_rejects_day_excluded_anchor_and_foreign_feedback_never_crosses_members() {
    use support::LearningTestSeam;
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let member_b = a
        .execute("member.create", json!({"name":"隔离成员乙"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
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
    let anchor_a = item(&a, "default", "隔离锚点上衣", "top");
    item(&a, "default", "隔离裤子", "bottom");
    item(&a, "default", "隔离鞋", "shoes");
    let anchor_b = item(&a, &member_b, "乙锚点上衣", "top");
    item(&a, &member_b, "乙裤子", "bottom");
    item(&a, &member_b, "乙鞋", "shoes");
    let intent = |a: &Application, member: &str, anchor: &str, key: &str| -> Value {
        a.execute(
            "pairing.generate",
            json!({"member_id":member,"operation_key":key,"anchor_id":anchor,"timezone":"UTC","limit":1}),
        )
        .unwrap()
    };
    let intent_a = intent(&a, "default", &anchor_a, "isolate-a");
    let intent_b = intent(&a, &member_b, &anchor_b, "isolate-b");
    let projection = |a: &Application, member: &str, key: &str| -> Value {
        a.execute_and_wait(
            "learning.recompute",
            json!({"member_id":member,"operation_key":key}),
        )
        .unwrap()["pair_projection"]
            .clone()
    };
    let before = projection(&a, "default", "isolate-recompute-a1");
    assert_eq!(before["entries"], json!([]), "{before}");
    a.execute(
        "feedback.record",
        json!({"member_id":&member_b,"operation_key":"isolate-like-b","intent_id":intent_b["id"],"proposal_index":0,"reaction":"like"}),
    )
    .unwrap();
    projection(&a, &member_b, "isolate-recompute-b");
    // The foreign member's anchored feedback must not appear in member A's
    // profile, and a fresh recompute for A must not import it either.
    let kept = a
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(kept["pair_projection"], before, "{kept}");
    let after = projection(&a, "default", "isolate-recompute-a2");
    assert_eq!(after, before, "{after}");
    // The anchor itself is hard-excluded for the day by explicit feedback:
    // generation anchored on it must fail with a constraint conflict instead
    // of silently dropping the anchor.
    a.execute(
        "feedback.record",
        json!({"member_id":"default","operation_key":"isolate-exclude","intent_id":intent_a["id"],"proposal_index":0,"reaction":"day_exclude","timezone":"UTC","clothing_ids":[anchor_a]}),
    )
    .unwrap();
    let error = a
        .execute(
            "pairing.generate",
            json!({"member_id":"default","operation_key":"isolate-a-excluded","anchor_id":anchor_a,"timezone":"UTC","limit":1}),
        )
        .unwrap_err();
    assert_eq!(error.code, "constraint_conflict", "{error:?}");
}
// Real wear facts teach pair rewards too, and only effective wear revisions
// count: a revoked wear must stop teaching on the next public recompute.
#[test]
fn wear_then_revoke_teaches_and_forgets_pair_rewards_through_recompute() {
    use support::LearningTestSeam;
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let item = |a: &Application, name: &str, category: &str| -> String {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let anchor = item(&a, "穿着锚点上衣", "top");
    let bottom = item(&a, "穿着裤子", "bottom");
    let shoe = item(&a, "穿着鞋", "shoes");
    let projection = |a: &Application, key: &str| -> Value {
        a.execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":key}),
        )
        .unwrap()["pair_projection"]
            .clone()
    };
    let worn = a
        .execute(
            "wear.record",
            json!({"member_id":"default","operation_key":"wear-pair","local_date":"2026-01-01","timezone":"UTC","clothing_ids":[anchor,bottom,shoe]}),
        )
        .unwrap();
    let worn_projection = projection(&a, "wear-recompute-1");
    let entries = worn_projection["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "{worn_projection}");
    assert!(
        entries.iter().all(|e| e["weight"].as_i64() == Some(1)),
        "{worn_projection}"
    );
    assert_eq!(
        worn_projection["source"]["wear_ids"],
        json!([worn["id"]]),
        "{worn_projection}"
    );
    a.execute(
        "wear.revoke",
        json!({"member_id":"default","operation_key":"wear-pair-revoke","root_event_id":worn["root_event_id"],"expected_revision":worn["revision"],"reason":"穿着撤销"}),
    )
    .unwrap();
    let revoked = projection(&a, "wear-recompute-2");
    assert_eq!(
        revoked,
        json!({"schema_version":1,"algorithm_version":"effective-pairs-v1","entries":[],"source":{"feedback_ids":[],"wear_ids":[]}}),
        "{revoked}"
    );
}
// An explicit recompute with include_prior re-adopts facts that predate the
// reset boundary: forgotten pairs return exactly through that public switch.
#[test]
fn recompute_include_prior_explicitly_readopts_pre_boundary_pair_facts() {
    use support::LearningTestSeam;
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let item = |a: &Application, name: &str, category: &str| -> String {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let anchor = item(&a, "追溯锚点上衣", "top");
    item(&a, "追溯裤子", "bottom");
    item(&a, "追溯鞋", "shoes");
    let intent = a
        .execute(
            "pairing.generate",
            json!({"member_id":"default","operation_key":"prior-anchor","anchor_id":anchor,"timezone":"UTC","limit":1}),
        )
        .unwrap();
    a.execute(
        "feedback.record",
        json!({"member_id":"default","operation_key":"prior-like","intent_id":intent["id"],"proposal_index":0,"reaction":"like"}),
    )
    .unwrap();
    a.execute(
        "learning.reset",
        json!({"member_id":"default","operation_key":"prior-reset","scope":"inferred"}),
    )
    .unwrap();
    let projection = |a: &Application, key: &str, extra: Value| -> Value {
        let mut input = json!({"member_id":"default","operation_key":key});
        for (key, value) in extra.as_object().unwrap() {
            input[key.clone()] = value.clone();
        }
        a.execute_and_wait("learning.recompute", input).unwrap()["pair_projection"].clone()
    };
    let excluded = projection(&a, "prior-recompute-default", json!({}));
    assert_eq!(excluded["entries"], json!([]), "{excluded}");
    let readopted = projection(&a, "prior-recompute-include", json!({"include_prior":true}));
    let entries = readopted["entries"].as_array().unwrap();
    assert!(!entries.is_empty(), "{readopted}");
    assert!(
        entries.iter().all(|e| e["weight"].as_i64() == Some(2)),
        "{readopted}"
    );
}
// An anchor made unavailable by care state must fail generation with a
// constraint conflict, while its durable pairing history stays readable.
#[test]
fn unavailable_anchor_conflicts_but_pairing_history_stays_readable() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let item = |a: &Application, name: &str, category: &str| -> String {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let anchor = item(&a, "保养锚点上衣", "top");
    item(&a, "保养裤子", "bottom");
    item(&a, "保养鞋", "shoes");
    let anchor_intent = a
        .execute(
            "pairing.generate",
            json!({"member_id":"default","operation_key":"care-anchor","anchor_id":anchor,"timezone":"UTC","limit":1}),
        )
        .unwrap();
    a.execute(
        "care.mark_dirty",
        json!({"member_id":"default","clothing_id":anchor,"dirty":true,"local_date":"2026-01-02","timezone":"UTC","operation_key":"care-dirty"}),
    )
    .unwrap();
    let error = a
        .execute(
            "pairing.generate",
            json!({"member_id":"default","operation_key":"care-anchor-blocked","anchor_id":anchor,"timezone":"UTC","limit":1}),
        )
        .unwrap_err();
    assert_eq!(error.code, "constraint_conflict", "{error:?}");
    let history = a
        .execute(
            "pairing.list",
            json!({"member_id":"default","anchor_id":anchor,"limit":10}),
        )
        .unwrap();
    let ids: Vec<&str> = history["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![anchor_intent["id"].as_str().unwrap()],
        "{history}"
    );
}
// Exhaustive public oracle: with four indistinguishable candidates the whole
// outcome space is enumerable through the API alone. Liking the individually
// last outfit with a top score and disliking the first must exactly produce:
// the disliked set excluded, the liked set first, and the two remaining sets
// (each carrying one negative learned edge) still proposed.
#[test]
fn non_monotone_rewards_follow_small_exhaustive_public_oracle() {
    use support::LearningTestSeam;
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let item = |a: &Application, name: &str, category: &str| -> String {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let top_a = item(&a, "穷举上衣甲", "top");
    let top_b = item(&a, "穷举上衣乙", "top");
    let bottom_a = item(&a, "穷举裤子甲", "bottom");
    let bottom_b = item(&a, "穷举裤子乙", "bottom");
    let shoe = item(&a, "穷举鞋", "shoes");
    let outfit_ids = |proposal: &Value| -> Vec<String> {
        let mut ids: Vec<String> = proposal["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["clothing_id"].as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        ids
    };
    let generate = |a: &Application, key: &str| -> Value {
        a.execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":key,"timezone":"UTC","limit":5}),
        )
        .unwrap()
    };
    let baseline = generate(&a, "oracle-baseline");
    let outfits: Vec<Vec<String>> = baseline["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(outfit_ids)
        .collect();
    // 2 tops x 2 bottoms x 1 shoe: the public result space is exhaustive.
    assert_eq!(outfits.len(), 4, "{baseline}");
    let mut expected_space = vec![
        vec![top_a.clone(), bottom_a.clone(), shoe.clone()],
        vec![top_a.clone(), bottom_b.clone(), shoe.clone()],
        vec![top_b.clone(), bottom_a.clone(), shoe.clone()],
        vec![top_b.clone(), bottom_b.clone(), shoe.clone()],
    ];
    for outfit in &mut expected_space {
        outfit.sort();
    }
    expected_space.sort();
    let mut seen = outfits.clone();
    seen.sort();
    assert_eq!(seen, expected_space, "{baseline}");
    let best = &outfits[0];
    let worst = &outfits[3];
    let index_of = |intent: &Value, outfit: &[String]| -> usize {
        intent["proposals"]
            .as_array()
            .unwrap()
            .iter()
            .position(|p| outfit_ids(p) == *outfit)
            .unwrap()
    };
    a.execute(
        "feedback.record",
        json!({"member_id":"default","operation_key":"oracle-like-last","intent_id":baseline["id"],"proposal_index":index_of(&baseline,worst),"reaction":"like","scores":{"overall":5}}),
    )
    .unwrap();
    a.execute(
        "feedback.record",
        json!({"member_id":"default","operation_key":"oracle-dislike-first","intent_id":baseline["id"],"proposal_index":index_of(&baseline,best),"reaction":"dislike"}),
    )
    .unwrap();
    a.execute_and_wait(
        "learning.recompute",
        json!({"member_id":"default","operation_key":"oracle-recompute"}),
    )
    .unwrap();
    let learned = generate(&a, "oracle-learned");
    let learned_outfits: Vec<Vec<String>> = learned["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(outfit_ids)
        .collect();
    assert_eq!(learned_outfits[0], *worst, "{learned}");
    assert!(!learned_outfits.contains(best), "{learned}");
    assert_eq!(learned_outfits.len(), 3, "{learned}");
    let mut remaining = learned_outfits[1..].to_vec();
    remaining.sort();
    let mut expected_remaining = outfits
        .iter()
        .filter(|o| *o != best && *o != worst)
        .cloned()
        .collect::<Vec<_>>();
    expected_remaining.sort();
    assert_eq!(remaining, expected_remaining, "{learned}");
}
// Large enabled-pair scale: 10002 real clothing items (3333 red per role, one
// extra red shoe, and the two colorless taught members) whose candidate space
// spans 3334x3334x3334 combinations. The single anchored like teaches only the
// sparse anchor-to-bottom and anchor-to-shoe relations. Every ordinary item
// carries an explicitly preferred color, so the colorless taught pair ranks
// individually last; after the anchored like it must still win outright,
// within bounded wall time and with no candidate truncation.
#[test]
fn large_enabled_pair_inventory_ranks_taught_pair_first_within_bounds() {
    use std::time::Instant;
    use support::LearningTestSeam;
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let item = |a: &Application, name: &str, category: &str, color: Value| -> String {
        let mut input = json!({"member_id":"default","name":name,"category":category});
        if !color.is_null() {
            input["color"] = color;
        }
        a.execute("clothing.create", input).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    for index in 0..3333 {
        item(&a, &format!("批量上衣{index:04}"), "top", json!(["red"]));
        item(&a, &format!("批量裤子{index:04}"), "bottom", json!(["red"]));
        item(&a, &format!("批量鞋{index:04}"), "shoes", json!(["red"]));
    }
    item(&a, "批量鞋额外", "shoes", json!(["red"]));
    let taught_top = item(&a, "无色锚点上衣", "top", json!(null));
    let taught_bottom = item(&a, "无色锚点裤子", "bottom", json!(null));
    a.execute(
        "learning.set",
        json!({"member_id":"default","operation_key":"scale-prefer-red","dimension":"color","value":"red","weight":1}),
    )
    .unwrap();
    let generate = |a: &Application, key: &str| -> Value {
        a.execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":key,"timezone":"UTC","limit":5}),
        )
        .unwrap()
    };
    let outfit_ids = |proposal: &Value| -> Vec<String> {
        let mut ids: Vec<String> = proposal["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["clothing_id"].as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        ids
    };
    // Pre-learning the winner can only be an all-red trio: the taught pair's
    // members carry zero individual preference while every rival carries +1.
    let baseline = generate(&a, "scale-baseline");
    assert_eq!(baseline["proposals"].as_array().unwrap().len(), 5);
    let baseline_winner = outfit_ids(&baseline["proposals"][0]);
    assert!(!baseline_winner.contains(&taught_top), "{baseline}");
    assert!(!baseline_winner.contains(&taught_bottom), "{baseline}");
    // Sparse anchored teaching through the public pairing anchor control plus
    // the fixed-id controls: exactly one proposal, the taught pair plus shoe.
    let intent = a
        .execute(
            "pairing.generate",
            json!({"member_id":"default","operation_key":"scale-anchor","anchor_id":taught_top,"timezone":"UTC","limit":1,"fixed_ids":[taught_bottom]}),
        )
        .unwrap();
    assert_eq!(intent["proposals"].as_array().unwrap().len(), 1, "{intent}");
    let taught_shoe = intent["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["clothing_id"].as_str().unwrap().to_owned())
        .find(|id| *id != taught_top && *id != taught_bottom)
        .unwrap();
    a.execute(
        "feedback.record",
        json!({"member_id":"default","operation_key":"scale-like","intent_id":intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5}}),
    )
    .unwrap();
    a.execute_and_wait(
        "learning.recompute",
        json!({"member_id":"default","operation_key":"scale-recompute"}),
    )
    .unwrap();
    println!("scale fixture ready: 10002 items taught; running learned generation");
    let started = Instant::now();
    let learned = generate(&a, "scale-learned");
    let elapsed = started.elapsed();
    println!(
        "large enabled-pair recommendation.generate wall time: {elapsed:?} over 10002-item inventory (3334x3334x3334 combinations)"
    );
    assert!(elapsed.as_secs() < 30, "{elapsed:?}");
    let mut expected = vec![taught_top.clone(), taught_bottom.clone(), taught_shoe];
    expected.sort();
    assert_eq!(outfit_ids(&learned["proposals"][0]), expected, "{learned}");
}
