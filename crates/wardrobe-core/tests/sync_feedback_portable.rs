//! Ticket-27 slice 1 public regression oracle.
//!
//! A real two-installation encrypted sync delivers explicit recommendation
//! feedback to the receiver. The receiver legitimately lacks the sender's
//! private `recommendation_intents`. The synced feedback facts must still be
//! valid portable-backup evidence on the receiver:
//!
//! - `backup.create` only serializes stored rows, so it legitimately succeeds
//!   today and this test keeps asserting that success.
//! - `backup.preview` is the public usability oracle: it re-validates the
//!   serialized facts via `validate_references`, which runs
//!   `feedback::validate_stored_document`. Before #27 that validator
//!   unconditionally required the sender's private intent ("反馈来源意图不存在"),
//!   so preview failed with `backup_invalid`. This test asserts preview
//!   succeeds.
//!
//! Slice 2 (same fixture): the created receiver archive must restore into a
//! fresh temporary Application through a real durable maintenance job and, once
//! reopened, preserve the synced feedback snapshot/proposal verbatim while the
//! sender's private intent stays `not_found`. Intrinsically malformed portable
//! facts (controls, snapshot members, proposal) must be rejected by preview
//! even with honestly rehashed transport checksums, while the untouched
//! archive stays usable and live facts remain readable.
//!
//! Only public Application operations are used, against a real SQLite file and
//! the actual OS credential store (real Keychain keys, temporary generated
//! fixture passwords only). Run explicitly with `--ignored`.

use serde_json::json;
use wardrobe_core::Application;

struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(a) = Application::open(&self.0) {
            let _ = a.execute("encryption.unpair", json!({}));
        }
    }
}

fn pair(owner: &Application, peer: &Application, name: &str) {
    let offer = owner
        .execute("encryption.pair.offer", json!({"device_name": name}))
        .expect("SETUP(keychain/env): encryption.pair.offer failed");
    let receipt = peer
        .execute(
            "encryption.pair.accept",
            json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"temporary generated-fixture peer password"}),
        )
        .expect("SETUP(keychain/env): encryption.pair.accept failed");
    let confirmation = owner
        .execute(
            "encryption.pair.confirm",
            json!({"receipt": receipt["receipt"]}),
        )
        .expect("SETUP(keychain/env): encryption.pair.confirm failed");
    peer.execute(
        "encryption.device.apply",
        json!({"manifest": confirmation["manifest"]}),
    )
    .expect("SETUP(keychain/env): encryption.device.apply failed");
}

/// Bounded durable sync drain. A panic here is a sync/setup failure, not the
/// portable-backup oracle under test.
fn drain(app: &Application, label: &str) {
    let queued = app.execute("sync.run", json!({})).unwrap_or_else(|e| {
        panic!(
            "SETUP(sync): {label} sync.run failed: {} / {}",
            e.code, e.message
        )
    });
    let job_id = queued["job_id"]
        .as_str()
        .unwrap_or_else(|| panic!("SETUP(sync): {label} sync.run returned no job id"));
    for _ in 0..20 {
        let job = app
            .execute("sync.process", json!({"id": job_id}))
            .unwrap_or_else(|e| {
                panic!(
                    "SETUP(sync): {label} sync.process failed: {} / {}",
                    e.code, e.message
                )
            });
        assert_ne!(
            job["state"],
            "failed",
            "SETUP(sync): {label} public sync worker failed: {:?}; this is a sync transport/setup failure, not the portable-backup oracle",
            job["error"]
        );
        if job["state"] == "succeeded" {
            return;
        }
    }
    panic!("SETUP(sync): {label} failed to drain its bounded durable sync job");
}

/// Publishes one fresh, valid portable archive from the live receiver through
/// the real durable maintenance worker.
fn publish_receiver_archive(receiver: &Application, directory: &std::path::Path) {
    let submitted = receiver
        .execute(
            "backup.create",
            json!({"directory":directory,"include_originals":false}),
        )
        .expect("SETUP(backup): receiver backup.create submission failed");
    let job_id = submitted["job_id"]
        .as_str()
        .expect("SETUP(backup): backup.create must return a durable maintenance job id");
    let job = receiver
        .execute("maintenance.worker.run", json!({"id":job_id}))
        .expect("SETUP(backup): maintenance.worker.run failed to execute");
    assert_eq!(
        job["state"], "succeeded",
        "SETUP(backup): receiver archive publication failed: {} / {}",
        job["error"]["code"], job["error"]["message"]
    );
}

/// Rehashes the facts.sqlite manifest entry after an intentional portable
/// archive edit, so a later preview rejects on intrinsic fact validation, not
/// on transport checksums. Same helper pattern as tests/backup.rs.
fn rehash_facts_sqlite(package: &std::path::Path) {
    use sha2::{Digest, Sha256};
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(package.join("manifest.json")).unwrap())
            .expect("SETUP(backup): manifest must stay parseable");
    let bytes = std::fs::read(package.join("facts.sqlite")).unwrap();
    for entry in manifest["files"].as_array_mut().unwrap() {
        if entry["path"] == "facts.sqlite" {
            entry["bytes"] = json!(bytes.len());
            entry["sha256"] = json!(Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>());
        }
    }
    std::fs::write(package.join("manifest.json"), manifest.to_string()).unwrap();
}

/// Edits every stored feedback document inside the exported portable archive
/// only. The live installation database is never touched: the feedback_events
/// rows belong to the archive copy, exactly like tests/backup.rs tampering.
fn edit_exported_feedback_document(
    package: &std::path::Path,
    mutate: impl Fn(&mut serde_json::Value),
) {
    let db = rusqlite::Connection::open(package.join("facts.sqlite")).unwrap();
    let mut edits = vec![];
    {
        let mut statement = db
            .prepare("SELECT id,member_id,document FROM feedback_events")
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let id: String = row.get(0).unwrap();
            let member_id: String = row.get(1).unwrap();
            let document: serde_json::Value =
                serde_json::from_str(&row.get::<_, String>(2).unwrap())
                    .expect("SETUP(backup): exported feedback document must stay parseable");
            edits.push((id, member_id, document));
        }
    }
    assert!(
        !edits.is_empty(),
        "SETUP(backup): the exported archive must contain the synced feedback fact"
    );
    for (id, member_id, mut document) in edits {
        mutate(&mut document);
        db.execute(
            "UPDATE feedback_events SET document=?1 WHERE id=?2 AND member_id=?3",
            rusqlite::params![document.to_string(), id, member_id],
        )
        .unwrap();
    }
    drop(db);
    rehash_facts_sqlite(package);
}

/// Reverses a feedback document's `inventory_snapshots` in place. The
/// precondition (at least two snapshots) is asserted so the edit is always a
/// real permutation, never a no-op.
fn reverse_inventory_snapshots(document: &mut serde_json::Value, label: &str) {
    let snapshots = document["inventory_snapshots"]
        .as_array()
        .unwrap_or_else(|| panic!("SETUP(backup): {label} must carry inventory_snapshots"))
        .clone();
    assert!(
        snapshots.len() >= 2,
        "SETUP(backup): {label} needs at least two inventory snapshots for a real reorder"
    );
    document["inventory_snapshots"] = json!(snapshots.into_iter().rev().collect::<Vec<_>>());
}

/// Reorders ONLY each exported feedback document's `inventory_snapshots`
/// (reversed) inside the portable archive copy — the live installation
/// database is never touched. The same member, every snapshot ID, all
/// snapshot content and the proposal stay exactly as exported; the helper
/// proves the edit is a pure permutation before rehashing. Idempotent receipt
/// `result` copies of the same document are reordered identically so the
/// receipts keep matching the stored immutable facts.
fn reorder_exported_inventory_snapshots(package: &std::path::Path) {
    let db = rusqlite::Connection::open(package.join("facts.sqlite")).unwrap();
    let mut facts = vec![];
    {
        let mut statement = db
            .prepare("SELECT id,member_id,document FROM feedback_events")
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let id: String = row.get(0).unwrap();
            let member_id: String = row.get(1).unwrap();
            let document: serde_json::Value =
                serde_json::from_str(&row.get::<_, String>(2).unwrap())
                    .expect("SETUP(backup): exported feedback document must stay parseable");
            facts.push((id, member_id, document));
        }
    }
    assert!(
        !facts.is_empty(),
        "SETUP(backup): the exported archive must contain the synced feedback fact"
    );
    for (id, member_id, mut document) in facts {
        let before: Vec<String> = document["inventory_snapshots"]
            .as_array()
            .expect("SETUP(backup): exported snapshots must stay an array")
            .iter()
            .map(|snapshot| snapshot.to_string())
            .collect();
        reverse_inventory_snapshots(&mut document, "exported feedback event");
        let mut after: Vec<String> = document["inventory_snapshots"]
            .as_array()
            .expect("SETUP(backup): reordered snapshots must stay an array")
            .iter()
            .map(|snapshot| snapshot.to_string())
            .collect();
        after.sort();
        let mut sorted_before = before.clone();
        sorted_before.sort();
        assert_eq!(
            sorted_before, after,
            "the reorder must keep every snapshot ID and all snapshot content"
        );
        db.execute(
            "UPDATE feedback_events SET document=?1 WHERE id=?2 AND member_id=?3",
            rusqlite::params![document.to_string(), id, member_id],
        )
        .unwrap();
    }
    // Receipt result copies carry the same immutable document; they must stay
    // byte-consistent with the reordered stored facts or the rejection would
    // be a receipt mismatch, not the order question under test.
    let mut receipts = vec![];
    {
        let mut statement = db
            .prepare("SELECT member_id,operation_key,result FROM feedback_receipts")
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let member_id: String = row.get(0).unwrap();
            let operation_key: String = row.get(1).unwrap();
            let result: serde_json::Value = serde_json::from_str(&row.get::<_, String>(2).unwrap())
                .expect("SETUP(backup): exported receipt result must stay parseable");
            receipts.push((member_id, operation_key, result));
        }
    }
    let mut reordered_receipts = 0;
    for (member_id, operation_key, mut result) in receipts {
        if result["inventory_snapshots"]
            .as_array()
            .is_some_and(|snapshots| snapshots.len() >= 2)
        {
            reverse_inventory_snapshots(&mut result, "exported receipt result copy");
            db.execute(
                "UPDATE feedback_receipts SET result=?1 WHERE member_id=?2 AND operation_key=?3",
                rusqlite::params![result.to_string(), member_id, operation_key],
            )
            .unwrap();
            reordered_receipts += 1;
        }
    }
    eprintln!("reorder fixture: reordered receipt result copies: {reordered_receipts}");
    drop(db);
    rehash_facts_sqlite(package);
}

/// The rehashed malformed archive must be rejected by preview on intrinsic
/// fact validation (backup_invalid), not on transport checksums.
fn preview_must_reject(receiver: &Application, package: &std::path::Path, label: &str) {
    let submitted = receiver
        .execute("backup.preview", json!({"directory":package}))
        .expect("ORACLE(preview): malformed archive preview submission failed");
    let job_id = submitted["job_id"]
        .as_str()
        .expect("ORACLE(preview): backup.preview must return a durable maintenance job id");
    let job = receiver
        .execute("maintenance.worker.run", json!({"id":job_id}))
        .expect("ORACLE(preview): maintenance.worker.run failed to execute");
    eprintln!(
        "{label} preview job actual state: {} error: {} / {}",
        job["state"], job["error"]["code"], job["error"]["message"]
    );
    assert_eq!(
        job["state"], "failed",
        "{label}: a rehashed intrinsically malformed portable archive must be rejected by backup.preview"
    );
    assert_eq!(
        job["error"]["code"], "backup_invalid",
        "{label}: rejection must be the portable-facts validation failure, got: {} / {}",
        job["error"]["code"], job["error"]["message"]
    );
}

#[test]
#[ignore = "requires actual OS credential store"]
fn synced_feedback_backs_up_on_receiver_without_sender_private_intent() {
    let sender_dir = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let receiver_dir = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let shared = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let archive = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let sp = sender_dir.path().join("wardrobe.sqlite");
    let rp = receiver_dir.path().join("wardrobe.sqlite");
    let sender = Application::open(&sp).expect("SETUP(env): sender Application::open failed");
    let receiver = Application::open(&rp).expect("SETUP(env): receiver Application::open failed");
    let _s = Cleanup(sp);
    let _r = Cleanup(rp);

    sender
        .execute(
            "encryption.create",
            json!({"password":"temporary generated-fixture sender password","device_name":"feedback sender"}),
        )
        .expect("SETUP(keychain/env): sender encryption.create failed");
    pair(&sender, &receiver, "feedback receiver");

    // Minimal real sender inventory: one new member with top/bottom/shoes.
    let member = sender
        .execute("member.create", json!({"name":"feedback backup family"}))
        .expect("SETUP: sender member.create failed")["member_id"]
        .clone();
    let mut clothing_ids = vec![];
    for (name, category) in [
        ("feedback backup blue shirt", "top"),
        ("feedback backup trousers", "bottom"),
        ("feedback backup shoes", "shoes"),
    ] {
        let garment = sender
            .execute(
                "clothing.create",
                json!({"member_id":member,"name":name,"category":category}),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "SETUP: sender clothing.create {category} failed: {} / {}",
                    e.code, e.message
                )
            });
        clothing_ids.push(garment["id"].clone());
    }

    // Offline rule planner (no provider, no paid model calls) plus explicit
    // like feedback with scores on proposal 0.
    let intent = sender
        .execute(
            "recommendation.generate",
            json!({"member_id":member,"operation_key":"ticket-27 portable feedback source","timezone":"UTC"}),
        )
        .expect("SETUP: sender recommendation.generate failed");
    let feedback = sender
        .execute(
            "feedback.record",
            json!({"member_id":member,"operation_key":"ticket-27 portable feedback receipt","intent_id":intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5,"comfort":null,"style":4}}),
        )
        .expect("SETUP: sender feedback.record failed");

    // Shared local-folder sync target; both installations drain through it.
    sender
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":shared.path()}),
        )
        .expect("SETUP(sync): sender sync.configure failed");
    receiver
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":shared.path()}),
        )
        .expect("SETUP(sync): receiver sync.configure failed");
    drain(&sender, "sender");
    drain(&receiver, "receiver");

    // Oracle 1: the receiver holds the complete portable feedback fact —
    // full proposal, controls and observed inventory snapshots.
    let sender_history = sender
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":feedback["root_id"]}),
        )
        .expect("SETUP: sender feedback.history failed")["items"]
        .clone();
    let receiver_history = receiver
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":feedback["root_id"]}),
        )
        .expect("ORACLE(sync): receiver feedback.history failed");
    assert_eq!(
        receiver_history["items"], sender_history,
        "synced feedback revisions must arrive on the receiver unchanged"
    );
    let stored = receiver
        .execute(
            "feedback.get",
            json!({"member_id":member,"id":feedback["id"]}),
        )
        .expect("ORACLE(sync): receiver feedback.get failed");
    assert_eq!(stored["intent_id"], intent["id"]);
    assert_eq!(stored["controls"], intent["controls"]);
    let items = stored["proposal"]["items"]
        .as_array()
        .expect("receiver feedback must carry the full immutable proposal");
    assert!(!items.is_empty(), "proposal must contain the planned items");
    let snapshots = stored["inventory_snapshots"]
        .as_array()
        .expect("receiver feedback must carry the observed inventory snapshots");
    assert_eq!(
        snapshots.len(),
        items.len(),
        "every proposal item needs its observed inventory snapshot"
    );
    for (snapshot, item) in snapshots.iter().zip(items) {
        assert_eq!(snapshot["id"], item["clothing_id"]);
        assert_eq!(snapshot["member_id"], member);
    }

    // Oracle 2: the sender's private recommendation intent is not on the
    // receiver; the exact public response code is not_found.
    let remote_intent = receiver
        .execute(
            "recommendation.get",
            json!({"member_id":member,"id":intent["id"]}),
        )
        .expect_err("the sender's private recommendation intent must stay private to the sender");
    assert_eq!(
        remote_intent.code, "not_found",
        "receiver must answer not_found for the sender's private intent, got: {}",
        remote_intent.message
    );

    // Oracle 3: portable backup creation on the receiver must succeed from
    // synced feedback facts alone. backup.create only serializes stored rows
    // into the archive, so this oracle legitimately holds today; the feedback
    // validator is not consulted here.
    let package = archive.path().join("receiver-portable-backup");
    let submitted = receiver
        .execute(
            "backup.create",
            json!({"directory":package,"include_originals":false}),
        )
        .expect("ORACLE(backup): receiver backup.create submission failed");
    let job_id = submitted["job_id"]
        .as_str()
        .expect("ORACLE(backup): backup.create must return a durable maintenance job id");
    let job = receiver
        .execute("maintenance.worker.run", json!({"id":job_id}))
        .expect("ORACLE(backup): maintenance.worker.run failed to execute");
    eprintln!(
        "receiver backup.create job actual state: {} error: {} / {}",
        job["state"], job["error"]["code"], job["error"]["message"]
    );
    assert_eq!(
        job["state"], "succeeded",
        "receiver backup.create (pure serialization) must succeed; actual job failed with {} / {}",
        job["error"]["code"], job["error"]["message"]
    );
    assert!(
        job["result"].is_object(),
        "a succeeded receiver backup must publish its result receipt"
    );

    // Oracle 4 (the red): the created archive must be publicly usable —
    // backup.preview re-validates the serialized facts through
    // validate_references, so it must succeed on the same archive directory.
    // Current actual behavior: the preview maintenance job fails with
    // backup_invalid because feedback::validate_stored_document demands the
    // sender's private recommendation intent ("反馈来源意图不存在").
    let preview = receiver
        .execute("backup.preview", json!({"directory":package}))
        .expect("ORACLE(preview): receiver backup.preview submission failed");
    let preview_job_id = preview["job_id"]
        .as_str()
        .expect("ORACLE(preview): backup.preview must return a durable maintenance job id");
    let preview_job = receiver
        .execute("maintenance.worker.run", json!({"id":preview_job_id}))
        .expect("ORACLE(preview): maintenance.worker.run failed to execute");
    eprintln!(
        "receiver backup.preview job actual state: {} error: {} / {}",
        preview_job["state"], preview_job["error"]["code"], preview_job["error"]["message"]
    );
    assert_eq!(
        preview_job["state"],
        "succeeded",
        "PORTABLE BACKUP RED (ticket-27 slice 1): synced feedback without the sender's private recommendation intent must still pass backup.preview's validate_references; actual preview job failed with {} / {}",
        preview_job["error"]["code"],
        preview_job["error"]["message"]
    );
    assert_eq!(
        preview_job["result"]["valid"], true,
        "a succeeded receiver backup.preview must report the archive as valid"
    );

    // Oracle 5: the receiver's portable archive restores into a fresh
    // Application through a real durable maintenance job. Reopened, the synced
    // feedback snapshot and proposal survive verbatim while the sender's
    // private recommendation intent remains absent (not_found).
    let restore_dir = tempfile::tempdir().expect("SETUP(env): restore tempdir failed");
    let restored_path = restore_dir.path().join("wardrobe.sqlite");
    let restored = Application::open(&restored_path)
        .expect("SETUP(env): fresh restore Application::open failed");
    let restore_submitted = restored
        .execute(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .expect("ORACLE(restore): receiver archive backup.restore submission failed");
    let restore_job_id = restore_submitted["job_id"]
        .as_str()
        .expect("ORACLE(restore): backup.restore must return a durable maintenance job id");
    let restore_job = restored
        .execute("maintenance.worker.run", json!({"id":restore_job_id}))
        .expect("ORACLE(restore): maintenance.worker.run failed to execute");
    eprintln!(
        "restored installation backup.restore job actual state: {} error: {} / {}",
        restore_job["state"], restore_job["error"]["code"], restore_job["error"]["message"]
    );
    assert_eq!(
        restore_job["state"], "succeeded",
        "the receiver's portable archive must restore into a fresh Application; actual job failed with {} / {}",
        restore_job["error"]["code"], restore_job["error"]["message"]
    );
    drop(restored);
    let reopened = Application::open(&restored_path)
        .expect("ORACLE(restore): reopened Application::open failed");
    let reopened_feedback = reopened
        .execute(
            "feedback.get",
            json!({"member_id":member,"id":feedback["id"]}),
        )
        .expect("ORACLE(restore): reopened feedback.get failed");
    assert_eq!(
        reopened_feedback, stored,
        "the restored installation must preserve the synced feedback document verbatim"
    );
    let reopened_history = reopened
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":feedback["root_id"]}),
        )
        .expect("ORACLE(restore): reopened feedback.history failed");
    assert_eq!(
        reopened_history["items"], sender_history,
        "the restored installation must preserve the full synced feedback history"
    );
    let restored_intent = reopened
        .execute(
            "recommendation.get",
            json!({"member_id":member,"id":intent["id"]}),
        )
        .expect_err("the sender's private recommendation intent must stay absent after restore");
    assert_eq!(
        restored_intent.code, "not_found",
        "restored installation must answer not_found for the sender's private intent, got: {}",
        restored_intent.message
    );

    // Oracle 6: intrinsically malformed feedback facts inside a portable
    // archive must be rejected by preview even when every transport checksum
    // is honestly rehashed. Each case starts from a fresh, valid receiver
    // archive; only the exported facts.sqlite document is edited, never any
    // live installation database.
    let negatives = tempfile::tempdir().expect("SETUP(env): negative archives tempdir failed");
    let controls_archive = negatives.path().join("malformed-controls");
    publish_receiver_archive(&receiver, &controls_archive);
    edit_exported_feedback_document(&controls_archive, |document| {
        document["controls"]["operation_key"] = json!("m".repeat(250));
    });
    preview_must_reject(
        &receiver,
        &controls_archive,
        "malformed controls operation_key",
    );

    let snapshot_member_archive = negatives.path().join("foreign-snapshot-member");
    publish_receiver_archive(&receiver, &snapshot_member_archive);
    edit_exported_feedback_document(&snapshot_member_archive, |document| {
        document["inventory_snapshots"][0]["member_id"] = json!(uuid::Uuid::now_v7().to_string());
    });
    preview_must_reject(
        &receiver,
        &snapshot_member_archive,
        "foreign inventory snapshot member",
    );

    let proposal_archive = negatives.path().join("malformed-proposal");
    publish_receiver_archive(&receiver, &proposal_archive);
    edit_exported_feedback_document(&proposal_archive, |document| {
        document["proposal"]["items"][0]["category"] = json!("not-a-category");
    });
    preview_must_reject(&receiver, &proposal_archive, "malformed proposal category");

    // The untouched archive remains publicly usable and every live fact stays
    // readable: the rejections came from the malformed portable facts, never
    // from the running installation.
    let untouched = receiver
        .execute("backup.preview", json!({"directory":package}))
        .expect("ORACLE(preview): untouched archive preview submission failed");
    let untouched_job_id = untouched["job_id"]
        .as_str()
        .expect("ORACLE(preview): backup.preview must return a durable maintenance job id");
    let untouched_job = receiver
        .execute("maintenance.worker.run", json!({"id":untouched_job_id}))
        .expect("ORACLE(preview): maintenance.worker.run failed to execute");
    assert_eq!(
        untouched_job["state"], "succeeded",
        "the untouched receiver archive must keep previewing valid: {} / {}",
        untouched_job["error"]["code"], untouched_job["error"]["message"]
    );
    assert_eq!(untouched_job["result"]["valid"], true);
    assert_eq!(
        receiver
            .execute(
                "feedback.get",
                json!({"member_id":member,"id":feedback["id"]})
            )
            .expect("ORACLE(live): receiver feedback.get after negative previews failed"),
        stored,
        "negative archive previews must never disturb the live synced facts"
    );
}

/// Ticket-27 root remaining item: a transport-valid reordering of
/// `inventory_snapshots` must stay publicly backup-valid. The archive variant
/// below reverses ONLY the snapshot order (same member, every ID, all
/// snapshot content, proposal untouched, receipt result copies consistent)
/// and must be ACCEPTED by public backup.preview and public backup.restore,
/// because the shared intrinsic snapshot validation is exact ID-set equality
/// with no array-order requirement. Current actual behavior:
/// feedback::validate_stored_document still zip-compares snapshots against
/// proposal order, so preview rejects the intrinsically identical snapshot ID
/// set with backup_invalid — the true red this regression captures with the
/// expected-success assertion kept intact.
#[test]
#[ignore = "requires actual OS credential store"]
fn reordered_inventory_snapshots_stay_preview_and_restore_valid() {
    // The same real two-installation fixture as the paired oracle above, with
    // distinct names so both ignored tests never share keychain identities or
    // idempotent operation keys when they run together.
    let sender_dir = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let receiver_dir = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let shared = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let archive = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let sp = sender_dir.path().join("wardrobe.sqlite");
    let rp = receiver_dir.path().join("wardrobe.sqlite");
    let sender = Application::open(&sp).expect("SETUP(env): sender Application::open failed");
    let receiver = Application::open(&rp).expect("SETUP(env): receiver Application::open failed");
    let _s = Cleanup(sp);
    let _r = Cleanup(rp);

    sender
        .execute(
            "encryption.create",
            json!({"password":"temporary generated-fixture reorder sender password","device_name":"feedback reorder sender"}),
        )
        .expect("SETUP(keychain/env): sender encryption.create failed");
    pair(&sender, &receiver, "feedback reorder receiver");

    let member = sender
        .execute("member.create", json!({"name":"feedback reorder family"}))
        .expect("SETUP: sender member.create failed")["member_id"]
        .clone();
    for (name, category) in [
        ("feedback reorder blue shirt", "top"),
        ("feedback reorder trousers", "bottom"),
        ("feedback reorder shoes", "shoes"),
    ] {
        sender
            .execute(
                "clothing.create",
                json!({"member_id":member,"name":name,"category":category}),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "SETUP: sender clothing.create {category} failed: {} / {}",
                    e.code, e.message
                )
            });
    }

    let intent = sender
        .execute(
            "recommendation.generate",
            json!({"member_id":member,"operation_key":"ticket-27 reorder feedback source","timezone":"UTC"}),
        )
        .expect("SETUP: sender recommendation.generate failed");
    let feedback = sender
        .execute(
            "feedback.record",
            json!({"member_id":member,"operation_key":"ticket-27 reorder feedback receipt","intent_id":intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5,"comfort":null,"style":4}}),
        )
        .expect("SETUP: sender feedback.record failed");

    sender
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":shared.path()}),
        )
        .expect("SETUP(sync): sender sync.configure failed");
    receiver
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":shared.path()}),
        )
        .expect("SETUP(sync): receiver sync.configure failed");
    drain(&sender, "sender");
    drain(&receiver, "receiver");

    let stored = receiver
        .execute(
            "feedback.get",
            json!({"member_id":member,"id":feedback["id"]}),
        )
        .expect("ORACLE(sync): receiver feedback.get failed");
    let sender_history = sender
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":feedback["root_id"]}),
        )
        .expect("SETUP: sender feedback.history failed")["items"]
        .clone();

    // A fresh, valid receiver archive via pure serialization (proven green by
    // the paired oracle), then the order-only mutant inside the exported
    // archive copy: ONLY inventory_snapshots reversed, everything else —
    // member, IDs, snapshot content, proposal, receipts consistency — kept,
    // and transport checksums honestly rehashed.
    let package = archive.path().join("receiver-reordered-backup");
    publish_receiver_archive(&receiver, &package);
    reorder_exported_inventory_snapshots(&package);

    // The expected accepted world: the identical archive facts with only the
    // snapshot order changed, verbatim after restore.
    let mut expected = stored.clone();
    reverse_inventory_snapshots(&mut expected, "expected restored feedback fact");
    let mut expected_history = sender_history.clone();
    for item in expected_history
        .as_array_mut()
        .expect("SETUP: sender history items must stay an array")
    {
        reverse_inventory_snapshots(item, "expected history item");
    }

    // Oracle A (the red): public backup.preview must ACCEPT the reordered
    // archive — the shared intrinsic snapshot validation is order-insensitive
    // exact ID-set equality. Current actual behavior: the preview maintenance
    // job fails with backup_invalid because feedback::validate_stored_document
    // zip-compares snapshots against proposal order.
    let preview = receiver
        .execute("backup.preview", json!({"directory":package}))
        .expect("ORACLE(preview): reordered archive preview submission failed");
    let preview_job_id = preview["job_id"]
        .as_str()
        .expect("ORACLE(preview): backup.preview must return a durable maintenance job id");
    let preview_job = receiver
        .execute("maintenance.worker.run", json!({"id":preview_job_id}))
        .expect("ORACLE(preview): maintenance.worker.run failed to execute");
    eprintln!(
        "reordered archive backup.preview job actual state: {} error: {} / {}",
        preview_job["state"], preview_job["error"]["code"], preview_job["error"]["message"]
    );
    assert_eq!(
        preview_job["state"],
        "succeeded",
        "PORTABLE BACKUP RED (ticket-27 order): an inventory_snapshots-only reorder of an otherwise identical archive (same member, every ID, all snapshot content, unchanged proposal, consistent receipt copies) is transport-valid and intrinsically order-insensitive, so public backup.preview must accept the identical snapshot ID set; actual preview job failed with {} / {}",
        preview_job["error"]["code"],
        preview_job["error"]["message"]
    );
    assert_eq!(
        preview_job["result"]["valid"], true,
        "a succeeded preview must report the reordered archive as valid"
    );

    // Oracle B: public backup.restore must accept the same reordered archive
    // into a fresh Application and preserve the reordered document verbatim.
    let restore_dir = tempfile::tempdir().expect("SETUP(env): restore tempdir failed");
    let restored_path = restore_dir.path().join("wardrobe.sqlite");
    let restored = Application::open(&restored_path)
        .expect("SETUP(env): fresh restore Application::open failed");
    let restore_submitted = restored
        .execute(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .expect("ORACLE(restore): reordered archive backup.restore submission failed");
    let restore_job_id = restore_submitted["job_id"]
        .as_str()
        .expect("ORACLE(restore): backup.restore must return a durable maintenance job id");
    let restore_job = restored
        .execute("maintenance.worker.run", json!({"id":restore_job_id}))
        .expect("ORACLE(restore): maintenance.worker.run failed to execute");
    eprintln!(
        "reordered archive backup.restore job actual state: {} error: {} / {}",
        restore_job["state"], restore_job["error"]["code"], restore_job["error"]["message"]
    );
    assert_eq!(
        restore_job["state"], "succeeded",
        "public backup.restore must accept the intrinsically identical reordered snapshot set; actual job failed with {} / {}",
        restore_job["error"]["code"], restore_job["error"]["message"]
    );
    drop(restored);
    let reopened = Application::open(&restored_path)
        .expect("ORACLE(restore): reopened Application::open failed");
    let reopened_feedback = reopened
        .execute(
            "feedback.get",
            json!({"member_id":member,"id":feedback["id"]}),
        )
        .expect("ORACLE(restore): reopened feedback.get failed");
    assert_ne!(
        reopened_feedback["inventory_snapshots"], stored["inventory_snapshots"],
        "the restored fact must be the genuinely reordered variant, not a silently normalized copy"
    );
    assert_eq!(
        reopened_feedback, expected,
        "restore must preserve the reordered feedback document verbatim: every snapshot ID and all content, only the order changed"
    );
    let reopened_history = reopened
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":feedback["root_id"]}),
        )
        .expect("ORACLE(restore): reopened feedback.history failed");
    assert_eq!(
        reopened_history["items"], expected_history,
        "the restored installation must preserve the reordered feedback history verbatim"
    );
    let restored_intent = reopened
        .execute(
            "recommendation.get",
            json!({"member_id":member,"id":intent["id"]}),
        )
        .expect_err("the sender's private recommendation intent must stay absent after restore");
    assert_eq!(
        restored_intent.code, "not_found",
        "restored installation must answer not_found for the sender's private intent, got: {}",
        restored_intent.message
    );
}
