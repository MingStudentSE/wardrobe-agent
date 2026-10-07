//! Ticket-27 restore-vs-orphan-sync-journal public regression oracle.
//!
//! One owner installation and one peer installation pair into the owner's
//! ORIGINAL encrypted space (actual OS credential store, temporary generated
//! fixture passwords only) and share one explicit member with one baseline
//! garment through the real durable sync transport until settled. The peer
//! then publishes a real durable portable archive (public `backup.create`
//! maintenance job) BEFORE creating one additional offline garment. That
//! garment is never published: there are deliberately NO sync calls between
//! its creation and the restore below, so it must never reach the remote
//! transport.
//!
//! The peer publicly previews the existing archive (maintenance worker,
//! succeeds) and restores it over itself (`backup.restore` with explicit
//! `confirm_overwrite`, maintenance worker, succeeds). After reopening, the
//! restored local facts are checked: the baseline survives, the discarded
//! offline garment is absent locally. The restored installation then
//! explicitly re-pairs into the owner's original space via the real
//! offer/accept/confirm/apply flow, reconfigures the ORIGINAL shared folder,
//! and both installations drain bounded public sync.
//!
//! REQUIRED DURABLE BEHAVIOR: the discarded never-published garment stays
//! absent on the owner, and stays absent on a third fresh installation that
//! later pairs into the same space and drains the real transport; the
//! baseline converges everywhere. Already-published remote data is never
//! asserted deleted and no tombstones are fabricated: only public Application
//! operations are used, no SQLite/private rows, no faked receipts, and the
//! exported archive boundary is never modified.
//!
//! CURRENT ACTUAL BEHAVIOR (the red this file captures): public restore
//! removes encryption trust and business tables but retains the private sync
//! protocol state (`sync_bootstrap` done/cursor, `sync_journal`/inbox,
//! frontier/fields, publications and targets). The discarded offline
//! garment's journal entry therefore survives the restore as an orphan, and
//! re-pairing plus `sync.configure` republishes that never-published garment
//! onto the shared transport, where the owner (and any fresh peer) wrongly
//! materializes it. If this test is green after the fix, that is a genuine
//! green: do not weaken the oracle to manufacture the historical red.
//!
//! Run explicitly with `--ignored`.

use serde_json::json;
use wardrobe_core::Application;

/// Actual keychain fixtures require cleanup: unpair every installation so the
/// temporary generated credentials do not leak past the test process.
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(a) = Application::open(&self.0) {
            let _ = a.execute("encryption.unpair", json!({}));
        }
    }
}

/// Real pairing into the owner's original space: actual offer on the owner,
/// accept with a fresh temporary fixture password on the joining device,
/// confirm on the owner, apply on the joining device.
fn pair(owner: &Application, joining: &Application, device_name: &str, password: &str) {
    let offer = owner
        .execute("encryption.pair.offer", json!({"device_name":device_name}))
        .expect("SETUP(keychain/env): encryption.pair.offer failed");
    let receipt = joining
        .execute(
            "encryption.pair.accept",
            json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":password}),
        )
        .expect("SETUP(keychain/env): encryption.pair.accept failed");
    let confirmation = owner
        .execute(
            "encryption.pair.confirm",
            json!({"receipt": receipt["receipt"]}),
        )
        .expect("SETUP(keychain/env): encryption.pair.confirm failed");
    joining
        .execute(
            "encryption.device.apply",
            json!({"manifest": confirmation["manifest"]}),
        )
        .expect("SETUP(keychain/env): encryption.device.apply failed");
}

/// Bounded durable sync drain (at most 20 public worker batches). A panic
/// here is a sync/setup failure, not the restore-journal oracle under test.
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
            "SETUP(sync): {label} public sync worker failed: {:?}; this is a sync transport/setup failure, not the restore-journal oracle",
            job["error"]
        );
        if job["state"] == "succeeded" {
            return;
        }
    }
    panic!("SETUP(sync): {label} failed to drain its bounded durable sync job");
}

/// Runs one durable maintenance job to terminal success. Setup failures here
/// (including preview of the untouched archive) are preconditions, never the
/// restore-journal oracle under test.
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
fn restored_peer_never_republishes_discarded_offline_garment_from_orphan_journal() {
    let owner_dir = tempfile::tempdir().expect("SETUP(env): owner tempdir failed");
    let peer_dir = tempfile::tempdir().expect("SETUP(env): peer tempdir failed");
    let third_dir = tempfile::tempdir().expect("SETUP(env): third tempdir failed");
    let shared = tempfile::tempdir().expect("SETUP(env): shared tempdir failed");
    let archive = tempfile::tempdir().expect("SETUP(env): archive tempdir failed");
    let op = owner_dir.path().join("wardrobe.sqlite");
    let pp = peer_dir.path().join("wardrobe.sqlite");
    let tp = third_dir.path().join("wardrobe.sqlite");
    let owner = Application::open(&op).expect("SETUP(env): owner Application::open failed");
    let peer = Application::open(&pp).expect("SETUP(env): peer Application::open failed");
    let _o = Cleanup(op.clone());
    let _p = Cleanup(pp.clone());

    owner
        .execute(
            "encryption.create",
            json!({"password":"temporary restore-journal fixture owner password","device_name":"restore journal owner"}),
        )
        .expect("SETUP(keychain/env): owner encryption.create failed");
    pair(
        &owner,
        &peer,
        "restore journal peer",
        "temporary restore-journal fixture peer password",
    );

    // Minimal real shared facts: one explicit member, one baseline garment.
    // No provider, no paid model calls, no real photos.
    let member = owner
        .execute("member.create", json!({"name":"restore journal family"}))
        .expect("SETUP: owner member.create failed")["member_id"]
        .clone();
    let baseline = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"restore journal baseline coat","category":"outerwear"}),
        )
        .expect("SETUP: owner baseline clothing.create failed");
    let baseline_id = baseline["id"].as_str().expect("baseline clothing id");

    owner
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":shared.path()}),
        )
        .expect("SETUP(sync): owner sync.configure failed");
    peer.execute(
        "sync.configure",
        json!({"provider_kind":"local_folder","directory":shared.path()}),
    )
    .expect("SETUP(sync): peer sync.configure failed");
    drain(&owner, "owner");
    drain(&peer, "peer");

    // Precondition: the shared space is settled on both installations.
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":baseline_id}))
            .expect("SETUP(sync): peer never received the baseline garment")["name"],
        "restore journal baseline coat",
        "the peer must hold the settled baseline before any archive exists"
    );

    // Durable portable archive from the peer, BEFORE the offline garment
    // exists. The archive boundary is created and read only through public
    // maintenance workers and is never modified afterwards.
    let package = archive.path().join("peer-portable-backup");
    let submitted = peer
        .execute(
            "backup.create",
            json!({"directory":package,"include_originals":false}),
        )
        .expect("ORACLE(backup): peer backup.create submission failed");
    let job_id = submitted["job_id"]
        .as_str()
        .expect("ORACLE(backup): backup.create must return a durable maintenance job id");
    let job = maintenance_succeeded(&peer, job_id, "peer backup.create");
    assert!(
        job["result"].is_object(),
        "a succeeded peer backup must publish its result receipt"
    );

    // The discarded garment is created offline on the peer AFTER the archive
    // and is NEVER published: no sync calls happen between this creation and
    // the restore below.
    let discarded = peer
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"offline never published scarf","category":"top"}),
        )
        .expect("SETUP: peer offline clothing.create failed");
    assert_ne!(
        discarded["id"], baseline["id"],
        "the discarded and baseline fixtures must be distinct garments"
    );
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member,"id":discarded["id"]})
        )
        .expect("SETUP: peer lost its own offline garment before restore")["name"],
        "offline never published scarf"
    );

    // Public preview of the existing archive still succeeds before restore.
    let preview = peer
        .execute("backup.preview", json!({"directory":package}))
        .expect("ORACLE(preview): peer backup.preview submission failed");
    let preview_job_id = preview["job_id"]
        .as_str()
        .expect("ORACLE(preview): backup.preview must return a durable maintenance job id");
    let preview_job = maintenance_succeeded(&peer, preview_job_id, "peer backup.preview");
    assert_eq!(
        preview_job["result"]["valid"], true,
        "the untouched archive taken before the offline garment must stay publicly usable"
    );

    // Restore the archive over the paired peer through a real durable
    // maintenance job with explicit confirmation, then reopen.
    let restore_submitted = peer
        .execute(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .expect("ORACLE(restore): peer archive backup.restore submission failed");
    let restore_job_id = restore_submitted["job_id"]
        .as_str()
        .expect("ORACLE(restore): backup.restore must return a durable maintenance job id");
    let restore_job = maintenance_succeeded(&peer, restore_job_id, "restore over paired peer");
    assert_eq!(
        restore_job["result"]["restored"], true,
        "the restore must complete and reset local trust"
    );
    drop(peer);
    let restored_peer =
        Application::open(&pp).expect("ORACLE(restore): reopened peer Application::open failed");

    // Local facts after restore: baseline survives, discarded garment absent.
    assert_eq!(
        restored_peer
            .execute(
                "clothing.get",
                json!({"member_id":member,"id":baseline["id"]})
            )
            .expect("ORACLE(restore): the restored peer must keep the archived baseline")["name"],
        "restore journal baseline coat"
    );
    assert_eq!(
        restored_peer
            .execute("clothing.get", json!({"member_id":member,"id":discarded["id"]}))
            .expect_err(
                "RESTORE PRECONDITION: the discarded offline garment was recorded after the archive and must be gone locally after restore"
            )
            .code,
        "not_found",
        "the discarded garment must be locally discarded by the restore"
    );

    // Explicitly re-pair the restored installation into the owner's ORIGINAL
    // encrypted space with the real four-step flow, then reconfigure the
    // ORIGINAL shared folder.
    pair(
        &owner,
        &restored_peer,
        "restore journal re-paired peer",
        "temporary restore-journal fixture re-pair password",
    );
    restored_peer
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":shared.path()}),
        )
        .expect("SETUP(sync): restored peer sync.configure failed");
    drain(&restored_peer, "restored peer");
    drain(&owner, "owner");

    // THE RED: the discarded never-published garment must stay absent on the
    // owner. Current actual behavior: restore retained the private
    // sync_bootstrap/journal/inbox state, so the orphan journal entry for the
    // discarded garment is republished upon re-pair + configure, and the
    // owner wrongly materializes a garment that was never published before
    // the restore.
    assert_eq!(
        owner
            .execute("clothing.get", json!({"member_id":member,"id":discarded["id"]}))
            .expect_err(
                "RESTORE-JOURNAL RED (ticket-27): a garment that was never published before the restore must never reach the owner. Current actual behavior: public backup.restore removes encryption trust and business tables but retains sync_bootstrap done/cursor, sync_journal/inbox, frontier/fields, publications and targets, so the discarded offline garment's orphan journal entry survives the restore and is republished to the original shared folder when the restored peer re-pairs and configures sync, and the owner materializes it"
            )
            .code,
        "not_found",
        "the owner must never learn about the discarded offline garment"
    );
    assert_eq!(
        owner
            .execute(
                "clothing.get",
                json!({"member_id":member,"id":baseline["id"]})
            )
            .expect("ORACLE: the owner lost its own baseline garment")["name"],
        "restore journal baseline coat"
    );

    // A third fresh installation pairs into the same original space and
    // drains the real transport: it must converge on the restored facts only.
    let third = Application::open(&tp).expect("SETUP(env): third Application::open failed");
    let _t = Cleanup(tp.clone());
    pair(
        &owner,
        &third,
        "restore journal third device",
        "temporary restore-journal fixture third password",
    );
    third
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":shared.path()}),
        )
        .expect("SETUP(sync): third sync.configure failed");
    drain(&third, "third");
    drain(&owner, "owner after third");
    assert_eq!(
        third
            .execute(
                "clothing.get",
                json!({"member_id":member,"id":baseline["id"]})
            )
            .expect("ORACLE(third): the third installation must receive the baseline")["name"],
        "restore journal baseline coat",
        "the third installation must converge on the restored baseline"
    );
    assert_eq!(
        third
            .execute("clothing.get", json!({"member_id":member,"id":discarded["id"]}))
            .expect_err(
                "RESTORE-JOURNAL RED (ticket-27): the discarded garment reached the shared transport via the restored peer's orphan journal, so the fresh third installation received it too. A garment that was never published before the restore must be absent on every installation"
            )
            .code,
        "not_found",
        "the third installation must never learn about the discarded offline garment"
    );
}
