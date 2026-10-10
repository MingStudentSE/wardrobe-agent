//! Ticket-27 receipt-portability public regression oracle.
//!
//! Two real paired installations (actual OS credential store, temporary
//! generated fixture passwords only) share one valid inventory/recommendation
//! feedback root, then independently answer the SAME logical
//! member/operation_key with two different corrections ("dislike" vs "skip").
//! Bounded public sync delivers both variants to both installations, where the
//! existing ambiguity guard (`sync_immutable` holding two accepted receipt
//! facts for one member:key) makes every public replay answer
//! `idempotency_conflict` BEFORE any archive exists.
//!
//! The physical event branch conflict is then resolved through public
//! `sync.resolve` (choosing one branch creates revision 3), so the final
//! receipt oracle is not hidden by the unrelated unresolved event conflict.
//!
//! The relay then publishes a real durable portable archive (public
//! `backup.create` maintenance job), publicly previews it, and restores it into
//! a fresh temporary Application. Reopened, the required durable-business
//! behavior is: the same logical key requests MUST STILL answer
//! `idempotency_conflict`, and both immutable feedback event sources must be
//! retained, while ordinary identical-tuple replay and genuinely new keys stay
//! valid.
//!
//! CURRENT ACTUAL BEHAVIOR (the red this file captures): the physical
//! `feedback_receipts` table has PRIMARY KEY(member_id, operation_key) and the
//! sync materializer uses INSERT OR IGNORE, so the relay keeps only its first
//! receipt; the alternative receipt variant lives only in `sync_immutable`,
//! which the portable archive excludes. `backup.preview` therefore accepts the
//! archive, and the restored installation wrongly returns the one cached
//! receipt (Ok) instead of the required `idempotency_conflict`.
//!
//! Only public Application operations are used. No active/private SQLite rows
//! are injected, no receipts/intents are faked, and the exported archive is
//! not modified. Run explicitly with `--ignored`.

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
            json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"temporary receipt-portability fixture peer password"}),
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
/// receipt-portability oracle under test.
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
            "SETUP(sync): {label} public sync worker failed: {:?}; this is a sync transport/setup failure, not the receipt-portability oracle",
            job["error"]
        );
        if job["state"] == "succeeded" {
            return;
        }
    }
    panic!("SETUP(sync): {label} failed to drain its bounded durable sync job");
}

/// Runs one durable maintenance job to its terminal state and asserts it
/// succeeded. Setup failures here (including backup.preview on the untouched
/// archive) are preconditions, never the receipt oracle under test.
fn maintenance_succeeded(app: &Application, job_id: &str, label: &str) -> serde_json::Value {
    let job = app
        .execute("maintenance.worker.run", json!({"id": job_id}))
        .unwrap_or_else(|e| {
            panic!(
                "SETUP(maintenance): {label} maintenance.worker.run failed: {} / {}",
                e.code, e.message
            )
        });
    assert_eq!(
        job["state"], "succeeded",
        "SETUP(maintenance): {label} job did not reach terminal success: {} / {}",
        job["error"]["code"], job["error"]["message"]
    );
    job
}

#[test]
#[ignore = "requires actual OS credential store"]
fn colliding_feedback_receipts_stay_ambiguous_after_portable_restore() {
    let author_dir = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let relay_dir = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let shared = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let archive = tempfile::tempdir().expect("SETUP(env): tempdir failed");
    let ap = author_dir.path().join("wardrobe.sqlite");
    let rp = relay_dir.path().join("wardrobe.sqlite");
    let author = Application::open(&ap).expect("SETUP(env): author Application::open failed");
    let relay = Application::open(&rp).expect("SETUP(env): relay Application::open failed");
    let _a = Cleanup(ap);
    let _r = Cleanup(rp);

    author
        .execute(
            "encryption.create",
            json!({"password":"temporary receipt-portability fixture author password","device_name":"receipt portability author"}),
        )
        .expect("SETUP(keychain/env): author encryption.create failed");
    pair(&author, &relay, "receipt portability relay");

    // Minimal real shared inventory for the offline rule planner: one member
    // with top/bottom/shoes, no provider and no paid model calls.
    let member = author
        .execute(
            "member.create",
            json!({"name":"receipt portability family"}),
        )
        .expect("SETUP: author member.create failed")["member_id"]
        .clone();
    for (name, category) in [
        ("receipt portability blue shirt", "top"),
        ("receipt portability trousers", "bottom"),
        ("receipt portability shoes", "shoes"),
    ] {
        author
            .execute(
                "clothing.create",
                json!({"member_id":member,"name":name,"category":category}),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "SETUP: author clothing.create {category} failed: {} / {}",
                    e.code, e.message
                )
            });
    }

    let intent = author
        .execute(
            "recommendation.generate",
            json!({"member_id":member,"operation_key":"ticket-27 receipt portability source","timezone":"UTC"}),
        )
        .expect("SETUP: author recommendation.generate failed");

    // Ordinary non-colliding feedback, later reused as the identical-replay
    // control on the restored installation.
    let feedback_input = json!({"member_id":member,"operation_key":"ticket-27 receipt portability receipt","intent_id":intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5,"comfort":null,"style":4}});
    let feedback = author
        .execute("feedback.record", feedback_input.clone())
        .expect("SETUP: author feedback.record failed");

    author
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":shared.path()}),
        )
        .expect("SETUP(sync): author sync.configure failed");
    relay
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":shared.path()}),
        )
        .expect("SETUP(sync): relay sync.configure failed");
    drain(&author, "author");
    drain(&relay, "relay");

    // The shared valid feedback root both installations will independently
    // revise under ONE logical member/operation_key.
    let branch_input = json!({"member_id":member,"operation_key":"ticket-27 receipt portability concurrent root","intent_id":intent["id"],"proposal_index":0,"reaction":"like"});
    let branch_root = author
        .execute("feedback.record", branch_input)
        .expect("SETUP: author concurrent feedback root failed");
    drain(&author, "author");
    drain(&relay, "relay");

    let author_correction_input = json!({"member_id":member,"operation_key":"ticket-27 receipt portability colliding correction","root_id":branch_root["root_id"],"expected_revision":1,"reaction":"dislike"});
    let author_branch = author
        .execute("feedback.correct", author_correction_input.clone())
        .expect("SETUP: author colliding correction failed");
    let relay_correction_input = json!({"member_id":member,"operation_key":"ticket-27 receipt portability colliding correction","root_id":branch_root["root_id"],"expected_revision":1,"reaction":"skip"});
    let relay_branch = relay
        .execute("feedback.correct", relay_correction_input.clone())
        .expect("SETUP: relay colliding correction failed");
    drain(&author, "author");
    drain(&relay, "relay");
    drain(&author, "author"); // Observe the other immutable receipt before replay checks.

    // Pre-archive proof: the existing logical-receipt ambiguity guard makes
    // EVERY public replay of this member/operation_key answer
    // idempotency_conflict on both installations, whichever variant is
    // replayed, before any archive exists.
    assert_eq!(
        relay
            .execute("feedback.correct", relay_correction_input.clone())
            .expect_err("the relay's own colliding receipt replay must be ambiguous before archive")
            .code,
        "idempotency_conflict"
    );
    assert_eq!(
        relay
            .execute("feedback.correct", author_correction_input.clone())
            .expect_err(
                "the relay must reject the author's colliding receipt variant before archive"
            )
            .code,
        "idempotency_conflict"
    );
    assert_eq!(
        author
            .execute("feedback.correct", author_correction_input.clone())
            .expect_err(
                "the author's own colliding receipt replay must be ambiguous before archive"
            )
            .code,
        "idempotency_conflict"
    );
    assert_eq!(
        author
            .execute("feedback.correct", relay_correction_input.clone())
            .expect_err(
                "the author must reject the relay's colliding receipt variant before archive"
            )
            .code,
        "idempotency_conflict"
    );

    // Distinguish the physical event conflict from the logical receipt: pick
    // one feedback branch through public sync.resolve so the final receipt
    // oracle is not hidden by the unrelated unresolved event conflict.
    let conflicts = relay
        .execute("sync.conflicts", json!({"member_id":member}))
        .expect("ORACLE(conflicts): relay sync.conflicts failed");
    let conflict = conflicts["items"]
        .as_array()
        .expect("sync.conflicts must list items")
        .iter()
        .find(|item| {
            item["entity_type"] == "feedback_events" && item["entity_id"] == branch_root["root_id"]
        })
        .expect("concurrent feedback revisions require explicit branch review");
    let selected = conflict["variants"]
        .as_array()
        .expect("the branch conflict must list its variants")
        .iter()
        .find(|variant| variant["document"] == author_branch)
        .expect("the author's dislike branch must be one of the conflicted variants");
    let choice = relay
        .execute(
            "sync.resolve",
            json!({"member_id":member,"operation_key":"ticket-27 receipt portability select branch","entity_type":"feedback_events","entity_id":branch_root["root_id"],"field":"branch","expected_token":conflict["token"],"branch_id":selected["branch_id"]}),
        )
        .expect("ORACLE(resolve): relay sync.resolve failed");
    assert_eq!(choice["document"]["revision"], 3);
    assert_eq!(choice["document"]["reaction"], "dislike");
    drain(&relay, "relay");
    drain(&author, "author");

    let expected_history = relay
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":branch_root["root_id"]}),
        )
        .expect("SETUP: relay feedback.history failed")["items"]
        .clone();
    assert_eq!(expected_history.as_array().expect("history items").len(), 4);
    assert!(expected_history
        .as_array()
        .unwrap()
        .contains(&author_branch));
    assert!(expected_history.as_array().unwrap().contains(&relay_branch));
    assert_eq!(
        author
            .execute(
                "feedback.history",
                json!({"member_id":member,"root_id":branch_root["root_id"]})
            )
            .expect("SETUP: author feedback.history failed")["items"],
        expected_history,
        "both installations must converge on the resolved feedback history before archiving"
    );

    // Real durable portable archive from the relay through the public
    // maintenance worker, then public preview. The relay physically kept only
    // its first (skip) receipt: PK(member_id, operation_key) plus INSERT OR
    // IGNORE dropped the author's variant, which survives only in
    // sync_immutable — excluded from the portable archive.
    let package = archive.path().join("relay-portable-backup");
    let submitted = relay
        .execute(
            "backup.create",
            json!({"directory":package,"include_originals":false}),
        )
        .expect("ORACLE(backup): relay backup.create submission failed");
    let job_id = submitted["job_id"]
        .as_str()
        .expect("ORACLE(backup): backup.create must return a durable maintenance job id");
    let job = maintenance_succeeded(&relay, job_id, "relay backup.create");
    assert!(
        job["result"].is_object(),
        "a succeeded relay backup must publish its result receipt"
    );

    let preview = relay
        .execute("backup.preview", json!({"directory":package}))
        .expect("ORACLE(preview): relay backup.preview submission failed");
    let preview_job_id = preview["job_id"]
        .as_str()
        .expect("ORACLE(preview): backup.preview must return a durable maintenance job id");
    let preview_job = maintenance_succeeded(&relay, preview_job_id, "relay backup.preview");
    assert_eq!(
        preview_job["result"]["valid"], true,
        "the untouched archive with the surviving cached receipt must stay publicly usable"
    );

    // Restore into a fresh temporary Application through a real durable
    // maintenance job, then reopen.
    let restore_dir = tempfile::tempdir().expect("SETUP(env): restore tempdir failed");
    let restored_path = restore_dir.path().join("wardrobe.sqlite");
    let restored = Application::open(&restored_path)
        .expect("SETUP(env): fresh restore Application::open failed");
    let restore_submitted = restored
        .execute(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .expect("ORACLE(restore): relay archive backup.restore submission failed");
    let restore_job_id = restore_submitted["job_id"]
        .as_str()
        .expect("ORACLE(restore): backup.restore must return a durable maintenance job id");
    maintenance_succeeded(&restored, restore_job_id, "restore into fresh Application");
    drop(restored);
    let reopened = Application::open(&restored_path)
        .expect("ORACLE(restore): reopened Application::open failed");

    // Both immutable feedback event sources must survive the archive.
    let reopened_history = reopened
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":branch_root["root_id"]}),
        )
        .expect("ORACLE(restore): reopened feedback.history failed")["items"]
        .clone();
    assert_eq!(
        reopened_history, expected_history,
        "the restored installation must retain the full resolved feedback history verbatim"
    );
    assert!(
        reopened_history
            .as_array()
            .expect("restored history items")
            .contains(&author_branch),
        "the author's immutable dislike correction source must survive the portable restore"
    );
    assert!(
        reopened_history
            .as_array()
            .expect("restored history items")
            .contains(&relay_branch),
        "the relay's immutable skip correction source must survive the portable restore"
    );

    // THE RED: the same logical member/operation_key requests must STILL
    // answer idempotency_conflict after restore. Current actual behavior: the
    // restored installation holds exactly one physical cached receipt (the
    // relay's skip variant) and no sync_immutable rows, so replaying that
    // variant's exact request wrongly returns the cached Ok result instead of
    // the required explicit conflict.
    assert_eq!(
        reopened
            .execute("feedback.correct", relay_correction_input.clone())
            .expect_err(
                "PORTABLE RECEIPT RED (ticket-27): distinct accepted feedback receipts for one member/operation_key are durable business state; a restored installation must not silently serve one cached winner. The physical feedback_receipts PK(member_id,operation_key) plus INSERT OR IGNORE materialization kept only the relay's first receipt, and the alternative variant lives only in sync_immutable, which the portable archive excludes, so restore returned the single cached receipt for the colliding request"
            )
            .code,
        "idempotency_conflict",
        "the restored logical receipt must stay explicitly ambiguous: differing accepted results exist for this member/operation_key"
    );
    assert_eq!(
        reopened
            .execute("feedback.correct", author_correction_input.clone())
            .expect_err(
                "PORTABLE RECEIPT RED (ticket-27): the author's colliding variant request must also stay idempotency_conflict on the restored installation, never a fresh accepted correction"
            )
            .code,
        "idempotency_conflict",
        "both colliding variants must keep answering idempotency_conflict after restore"
    );

    // Identical-replay control: an ordinary unchanged operation key keeps
    // replaying its original cached result after restore.
    assert_eq!(
        reopened
            .execute("feedback.record", feedback_input.clone())
            .expect("ORACLE(control): identical replay on the restored installation failed"),
        feedback,
        "identical-tuple replay must keep returning the original result after restore"
    );

    // Distinct-key control: a genuinely new operation key stays usable on the
    // restored installation; the ambiguity is scoped to the collided key.
    let distinct = reopened
        .execute(
            "feedback.correct",
            json!({"member_id":member,"operation_key":"ticket-27 receipt portability distinct correction","root_id":branch_root["root_id"],"expected_revision":3,"reaction":"like"}),
        )
        .expect("ORACLE(control): distinct new operation key on the restored installation failed");
    assert_eq!(
        distinct["revision"], 4,
        "a genuinely new operation key must keep working on the restored installation"
    );
}
