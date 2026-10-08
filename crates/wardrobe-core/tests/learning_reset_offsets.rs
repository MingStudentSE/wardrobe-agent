//! Public regression for the reset-cutoff timestamp root review: the recompute
//! cutoff compared event `created_at` as an RFC3339 string. A real instant at or
//! before the cutoff, re-expressed with a +14:00 fixed offset, sorts after a UTC
//! cutoff string and could be reabsorbed after `learning.reset`. The fixture
//! only rewrites the temporary exported portable archive database (event
//! documents plus the receipt copies the portable validator cross-checks) and
//! rehashes the manifest like the existing tests/backup.rs helpers; the active
//! application databases stay untouched.
#[path = "support/learning.rs"]
mod support;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use support::LearningTestSeam;
use wardrobe_core::Application;

/// Durable backup seam completing real maintenance jobs (tests/backup.rs contract).
trait DurableMaintenance {
    fn execute_and_finish(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, wardrobe_core::ApiError>;
}
impl DurableMaintenance for Application {
    fn execute_and_finish(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, wardrobe_core::ApiError> {
        let submitted = self.execute(operation, input)?;
        if let Some(id) = submitted["job_id"].as_str() {
            let job = self.execute("maintenance.worker.run", json!({"id": id}))?;
            if job["state"] == "succeeded" {
                Ok(job["result"].clone())
            } else {
                Err(wardrobe_core::ApiError::of(job["error"]["code"]
                        .as_str()
                        .unwrap_or("job_cancelled")
                        .to_owned(), job["error"]["message"]
                        .as_str()
                        .unwrap_or("任务未完成")
                        .to_owned()))
            }
        } else {
            Ok(submitted)
        }
    }
}

/// Re-express one stored event `created_at` as the exact same actual instant
/// with a +14:00 fixed offset (chrono east constructor). Returns
/// (rewritten document, original RFC3339, offset RFC3339).
fn reexpress_created_at_plus_fourteen(document: &str) -> (String, String, String) {
    let mut rewritten: Value = serde_json::from_str(document).expect("archive document decodes");
    let original = rewritten["created_at"]
        .as_str()
        .expect("event carries created_at")
        .to_owned();
    let instant = chrono::DateTime::parse_from_rfc3339(&original).expect("created_at parses");
    let offset = chrono::FixedOffset::east_opt(14 * 3600).expect("+14:00 is a valid offset");
    let reexpressed = instant.with_timezone(&offset).to_rfc3339();
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(&reexpressed).expect("re-expressed parses"),
        instant,
        "re-expression must keep the exact same actual instant"
    );
    assert_ne!(
        reexpressed, original,
        "fixture must really change the textual representation"
    );
    rewritten["created_at"] = json!(reexpressed.clone());
    (rewritten.to_string(), original, reexpressed)
}

/// Rehash the facts.sqlite manifest entry exactly like the existing
/// tests/backup.rs helpers so preview and restore genuinely validate.
fn rehash_facts_manifest(directory: &Path) {
    let manifest_path = directory.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    for entry in manifest["files"].as_array_mut().unwrap() {
        if entry["path"] == "facts.sqlite" {
            let bytes = std::fs::read(directory.join("facts.sqlite")).unwrap();
            entry["bytes"] = json!(bytes.len());
            entry["sha256"] = json!(Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>());
        }
    }
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
}

fn preview_and_restore_archive(archive: &Path, target: &Path) -> Application {
    let receiver = Application::open(&target.join("wardrobe.sqlite")).unwrap();
    // Public preview of the rewritten archive must genuinely succeed.
    let preview = receiver
        .execute_and_finish("backup.preview", json!({"directory":archive}))
        .unwrap();
    assert_eq!(preview["valid"], true);
    receiver
        .execute_and_finish(
            "backup.restore",
            json!({"directory":archive,"confirm_overwrite":true}),
        )
        .unwrap();
    drop(receiver);
    Application::open(&target.join("wardrobe.sqlite")).unwrap()
}

#[test]
fn wear_created_at_same_instant_plus_fourteen_offset_stays_excluded_after_learning_reset() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let garment = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"蓝衬衫","category":"top","color":["blue"]}),
        )
        .unwrap();
    let wear = app
        .execute(
            "wear.record",
            json!({"member_id":"default","operation_key":"wear","clothing_ids":[garment["id"]],"local_date":"2020-01-01","timezone":"UTC"}),
        )
        .unwrap();
    let wear_id = wear["id"].as_str().unwrap().to_owned();
    // Durable backup completed through the real maintenance worker.
    let package = archive.path().join("portable");
    let backup = app
        .execute_and_finish(
            "backup.create",
            json!({"directory":package,"include_originals":false}),
        )
        .unwrap();
    assert_eq!(backup["includes_originals"], false);
    drop(app);
    // Rewrite ONLY the temporary exported archive database: the wear event
    // document plus the receipt copy the portable validator cross-checks.
    let db = rusqlite::Connection::open(package.join("facts.sqlite")).unwrap();
    let document: String = db
        .query_row(
            "SELECT document FROM wear_events WHERE id=?1",
            [&wear_id],
            |r| r.get(0),
        )
        .unwrap();
    let (rewritten, original_created_at, offset_created_at) =
        reexpress_created_at_plus_fourteen(&document);
    db.execute(
        "UPDATE wear_events SET document=?1 WHERE id=?2",
        rusqlite::params![rewritten, wear_id],
    )
    .unwrap();
    let receipts = db
        .execute(
            "UPDATE wear_receipts SET result=?1 WHERE result=?2",
            rusqlite::params![rewritten, document],
        )
        .unwrap();
    drop(db);
    assert_eq!(receipts, 1, "wear receipt copy must exist in the archive");
    println!(
        "learning_reset_offsets: wear original_created_at={original_created_at} reexpressed_created_at={offset_created_at} same_actual_instant=true representation_offset=+14:00"
    );
    rehash_facts_manifest(&package);
    let restored = preview_and_restore_archive(&package, target.path());
    // Fixture sanity first: the restored receiver really still holds the wear
    // fact, carrying the exact same instant in its +14:00 representation.
    let restored_wear = restored
        .execute("wear.get", json!({"member_id":"default","id":wear_id}))
        .unwrap();
    assert_eq!(restored_wear["id"], json!(wear_id));
    assert_eq!(restored_wear["created_at"], json!(offset_created_at));
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(restored_wear["created_at"].as_str().unwrap())
            .unwrap(),
        chrono::DateTime::parse_from_rfc3339(&original_created_at).unwrap(),
        "restored wear keeps the original actual instant"
    );
    assert_eq!(
        restored
            .execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "actual wear fact must remain present before the learning assertions"
    );
    // Public reset, then ordinary recompute through the real receipt + worker.
    let reset = restored
        .execute_and_wait(
            "learning.reset",
            json!({"member_id":"default","operation_key":"reset-inferred","scope":"inferred"}),
        )
        .unwrap();
    let cutoff = reset["state"]["cutoff"].as_str().unwrap().to_owned();
    // Trap documentation: the +14:00 representation of the strictly earlier
    // instant sorts lexicographically after the UTC cutoff string while the
    // original UTC representation of the same instant does not.
    assert!(
        offset_created_at.as_str() > cutoff.as_str(),
        "fixture precondition: offset representation sorts after the cutoff string"
    );
    assert!(
        original_created_at.as_str() < cutoff.as_str(),
        "fixture precondition: UTC representation of the same instant sorts before the cutoff"
    );
    println!(
        "learning_reset_offsets: wear reset_cutoff={cutoff} offset_string_sorts_after_cutoff=true"
    );
    let profile = restored
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"after-reset"}),
        )
        .unwrap();
    assert_eq!(profile["reset_boundary"], json!(cutoff));
    // Product expectation under review (currently red): the wear instant is
    // strictly before the reset cutoff, so the recomputed inferred profile must
    // stay empty even when the stored representation uses +14:00.
    assert_eq!(
        profile["inferred"],
        json!([]),
        "pre-reset wear event (same actual instant, +14:00 representation) was reabsorbed after learning.reset"
    );
}

#[test]
fn feedback_created_at_same_instant_plus_fourteen_offset_stays_excluded_after_learning_reset() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    for (name, category, color) in [
        ("蓝衬衫", "top", "blue"),
        ("黑裤", "bottom", "black"),
        ("黑鞋", "shoes", "black"),
    ] {
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category,"color":[color]}),
        )
        .unwrap();
    }
    let intent = app
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"intent","timezone":"UTC"}),
        )
        .unwrap();
    let feedback = app
        .execute(
            "feedback.record",
            json!({"member_id":"default","operation_key":"like","intent_id":intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5}}),
        )
        .unwrap();
    let feedback_id = feedback["id"].as_str().unwrap().to_owned();
    let package = archive.path().join("portable");
    app.execute_and_finish(
        "backup.create",
        json!({"directory":package,"include_originals":false}),
    )
    .unwrap();
    drop(app);
    let db = rusqlite::Connection::open(package.join("facts.sqlite")).unwrap();
    let document: String = db
        .query_row(
            "SELECT document FROM feedback_events WHERE id=?1",
            [&feedback_id],
            |r| r.get(0),
        )
        .unwrap();
    let (rewritten, original_created_at, offset_created_at) =
        reexpress_created_at_plus_fourteen(&document);
    db.execute(
        "UPDATE feedback_events SET document=?1 WHERE id=?2",
        rusqlite::params![rewritten, feedback_id],
    )
    .unwrap();
    let receipts = db
        .execute(
            "UPDATE feedback_receipts SET result=?1 WHERE result=?2",
            rusqlite::params![rewritten, document],
        )
        .unwrap();
    drop(db);
    assert_eq!(
        receipts, 1,
        "feedback receipt copy must exist in the archive"
    );
    println!(
        "learning_reset_offsets: feedback original_created_at={original_created_at} reexpressed_created_at={offset_created_at} same_actual_instant=true representation_offset=+14:00"
    );
    rehash_facts_manifest(&package);
    let restored = preview_and_restore_archive(&package, target.path());
    // Fixture sanity: the restored receiver still holds the feedback fact with
    // the exact same instant re-expressed as +14:00, and the clothing facts.
    let items = restored
        .execute("feedback.list", json!({"member_id":"default"}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .clone();
    let restored_feedback = items
        .iter()
        .find(|v| v["id"] == json!(feedback_id))
        .unwrap();
    assert_eq!(restored_feedback["created_at"], json!(offset_created_at));
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(restored_feedback["created_at"].as_str().unwrap())
            .unwrap(),
        chrono::DateTime::parse_from_rfc3339(&original_created_at).unwrap(),
        "restored feedback keeps the original actual instant"
    );
    assert_eq!(
        restored
            .execute("clothing.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        3,
        "colored clothing facts must remain present before the learning assertions"
    );
    let reset = restored
        .execute_and_wait(
            "learning.reset",
            json!({"member_id":"default","operation_key":"reset-inferred","scope":"inferred"}),
        )
        .unwrap();
    let cutoff = reset["state"]["cutoff"].as_str().unwrap().to_owned();
    assert!(offset_created_at.as_str() > cutoff.as_str());
    assert!(original_created_at.as_str() < cutoff.as_str());
    println!(
        "learning_reset_offsets: feedback reset_cutoff={cutoff} offset_string_sorts_after_cutoff=true"
    );
    let profile = restored
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"after-reset"}),
        )
        .unwrap();
    assert_eq!(profile["reset_boundary"], json!(cutoff));
    // Product expectation under review (currently red): the feedback instant is
    // strictly before the reset cutoff, so the recomputed inferred profile must
    // stay empty even when the stored representation uses +14:00.
    assert_eq!(
        profile["inferred"],
        json!([]),
        "pre-reset feedback event (same actual instant, +14:00 representation) was reabsorbed after learning.reset"
    );
}
