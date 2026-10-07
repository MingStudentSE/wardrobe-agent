//! Legacy schema-20 feedback receipt archive public compatibility oracle.
//!
//! Archives exported by the former schema-20 build stored `feedback_receipts`
//! with the former PRIMARY KEY(member_id, operation_key) (same four column
//! names/types) and user_version 20. The public compatibility contract under
//! test is:
//!
//! 1. `backup.preview` ACCEPTS such an archive through the legacy-receipt
//!    column carve-out (only for `database_version <= 21` and exactly the
//!    former member/operation_key primary key shape) instead of rejecting it
//!    with "备份字段结构不支持".
//! 2. `backup.restore` replants the real receipt row into the current
//!    four-column primary key table of a fresh installation (restore copies
//!    rows, the destination keeps its own current table shapes).
//! 3. After reopen, the identical `feedback.record` input with the same
//!    operation key replays to EXACTLY the original feedback document (the
//!    (member_id, operation_key) receipt lookup survives the PK transition),
//!    and facts/history (clothing, feedback, history, empty wears) are intact.
//! 4. A deliberately wrong primary key shape (member_id only — never a real
//!    historical schema) must be REJECTED by `backup.preview` with
//!    `backup_invalid`, and the rejection must not mutate destination facts.
//!
//! Fixture rules followed: only public `Application` operations build the
//! baseline (synthetic 3-item inventory, one real recommendation, one real
//! like feedback with a stable operation key, real durable `backup.create`
//! maintenance job). The former primary key is written ONLY into the exported
//! `facts.sqlite` archive copy (never the active database), preserving the
//! four column names/types and the real row values, then the archive is
//! rehashed with the existing manifest pattern (bytes + sha256 + database
//! version, user_version 20 kept as the legitimate old schema version).
//!
//! Expected status: GREEN immediately on acbabf6 — the compatibility carve-out
//! and the receipt replay lookup are already implemented. This file is the
//! durable public regression oracle for that behavior, not a red repro.

use serde_json::json;
use wardrobe_core::Application;

fn maintenance(a: &Application, operation: &str, input: serde_json::Value) -> serde_json::Value {
    let receipt = a.execute(operation, input).unwrap();
    let job = a
        .execute("maintenance.worker.run", json!({"id":receipt["job_id"]}))
        .unwrap();
    assert_eq!(job["state"], "succeeded", "{job}");
    job["result"].clone()
}

/// Rebuild ONLY the exported archive's feedback_receipts primary key to the
/// requested shape, keeping the four column names/types and every real row,
/// and stamp the legitimate old schema version 20 (never the active database).
/// A real schema-20 archive predates the schema-21 `wardrobe_categories`
/// dictionary table, so that anachronistic table is dropped from the exported
/// copy to keep the fixture historically accurate (the active database keeps
/// its own current table shapes).
fn rebuild_receipts_primary_key(package: &std::path::Path, primary_key: &str) {
    let db = rusqlite::Connection::open(package.join("facts.sqlite")).unwrap();
    db.execute_batch(&format!(
        "ALTER TABLE feedback_receipts RENAME TO feedback_receipts_legacy;
         CREATE TABLE feedback_receipts(member_id TEXT NOT NULL,operation_key TEXT NOT NULL,request TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY({primary_key}));
         INSERT INTO feedback_receipts SELECT * FROM feedback_receipts_legacy;
         DROP TABLE feedback_receipts_legacy;
         DROP TABLE IF EXISTS wardrobe_categories;
         PRAGMA user_version=20;"
    ))
    .unwrap();
}

/// Rehash the changed facts.sqlite into the manifest using the existing
/// fixture pattern (bytes + sha256 per file entry, database_version 20).
fn rehash_manifest(package: &std::path::Path) {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(package.join("facts.sqlite")).unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(package.join("manifest.json")).unwrap()).unwrap();
    manifest["database_version"] = json!(20);
    for file in manifest["files"].as_array_mut().unwrap() {
        if file["path"] == "facts.sqlite" {
            file["bytes"] = json!(bytes.len());
            file["sha256"] = json!(Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>());
        }
    }
    std::fs::write(package.join("manifest.json"), manifest.to_string()).unwrap();
}

fn copy_archive(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir(to).unwrap();
    std::fs::create_dir(to.join("media")).unwrap();
    std::fs::copy(from.join("facts.sqlite"), to.join("facts.sqlite")).unwrap();
    std::fs::copy(from.join("manifest.json"), to.join("manifest.json")).unwrap();
}

#[test]
fn schema20_member_operation_key_receipt_archive_previews_restores_and_replays_exactly() {
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source");
    let dest = d.path().join("dest");
    let legacy = d.path().join("legacy-schema20");
    let broken = d.path().join("wrong-primary-key");

    // Real baseline: synthetic inventory -> real recommendation -> real like
    // feedback under a stable operation key. Nothing about the receipt is faked.
    let a = Application::open(&source).unwrap();
    for (name, category) in [("衬衫", "top"), ("长裤", "bottom"), ("鞋", "shoes")] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
    let intent = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"ticket-27-legacy-recommendation","timezone":"UTC"}),
        )
        .unwrap();
    let input = json!({"member_id":"default","operation_key":"ticket-27-legacy-feedback","intent_id":intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5,"comfort":null,"style":4}});
    let f = a.execute("feedback.record", input.clone()).unwrap();
    maintenance(&a, "backup.create", json!({"directory":&legacy}));
    drop(a);

    // Former schema-20 archive: same four columns, former
    // (member_id, operation_key) primary key, real rows, user_version 20.
    rebuild_receipts_primary_key(&legacy, "member_id,operation_key");
    rehash_manifest(&legacy);

    // Separate copied archive with a never-existing wrong primary key shape.
    copy_archive(&legacy, &broken);
    rebuild_receipts_primary_key(&broken, "member_id");
    rehash_manifest(&broken);

    // Public preview accepts the legitimate legacy archive...
    let b = Application::open(&dest).unwrap();
    let preview = maintenance(&b, "backup.preview", json!({"directory":&legacy}));
    assert_eq!(preview["valid"], json!(true));
    // ...and public restore plants it into the fresh installation.
    maintenance(
        &b,
        "backup.restore",
        json!({"directory":&legacy,"confirm_overwrite":true}),
    );
    drop(b);

    // Reopen: identical input/key replays the EXACT original feedback document,
    // and facts/history are preserved.
    let b = Application::open(&dest).unwrap();
    assert_eq!(b.execute("feedback.record", input.clone()).unwrap(), f);
    assert_eq!(
        b.execute("feedback.get", json!({"member_id":"default","id":f["id"]}))
            .unwrap(),
        f
    );
    assert_eq!(
        b.execute(
            "feedback.history",
            json!({"member_id":"default","root_id":f["root_id"]})
        )
        .unwrap()["items"],
        json!([f.clone()])
    );
    assert_eq!(
        b.execute("clothing.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        b.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );

    // The wrong primary key shape is not any real historical schema: preview
    // must reject it as backup_invalid, leaving the restored facts untouched.
    let receipt = b
        .execute("backup.preview", json!({"directory":&broken}))
        .unwrap();
    let rejected = b
        .execute("maintenance.worker.run", json!({"id":receipt["job_id"]}))
        .unwrap();
    assert_eq!(rejected["state"], "failed", "{rejected}");
    assert_eq!(rejected["error"]["code"], "backup_invalid");
    assert_eq!(b.execute("feedback.record", input).unwrap(), f);
    assert_eq!(
        b.execute("feedback.get", json!({"member_id":"default","id":f["id"]}))
            .unwrap(),
        f
    );
    assert_eq!(
        b.execute("clothing.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
