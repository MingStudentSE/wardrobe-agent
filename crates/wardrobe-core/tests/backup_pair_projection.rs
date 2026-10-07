use serde_json::{json, Value};
use wardrobe_core::Application;
// Awaiting helper for job-backed operations: learning recomputation runs on
// the learning worker, every other submitted job (including backup creation
// and preview) completes on the maintenance worker, exactly as the existing
// backup and learning test suites await their own jobs.
trait PairBackupTestApplication {
    fn execute_and_wait(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, wardrobe_core::ApiError>;
}
impl PairBackupTestApplication for Application {
    fn execute_and_wait(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, wardrobe_core::ApiError> {
        let submitted = self.execute(operation, input.clone())?;
        let Some(id) = submitted["job_id"].as_str() else {
            return Ok(submitted);
        };
        let worker = if operation == "learning.recompute" {
            "learning.worker.run"
        } else {
            "maintenance.worker.run"
        };
        let worker_input = if operation == "learning.recompute" {
            json!({"member_id":input["member_id"],"job_id":id})
        } else {
            json!({"id":id})
        };
        let job = self.execute(worker, worker_input)?;
        if job["state"] == "succeeded" {
            if operation == "learning.recompute" {
                self.execute("learning.get", json!({"member_id":input["member_id"]}))
            } else {
                Ok(job["result"].clone())
            }
        } else {
            Err(wardrobe_core::ApiError {
                code: job["error"]["code"]
                    .as_str()
                    .unwrap_or("job_cancelled")
                    .into(),
                message: job["error"]["message"]
                    .as_str()
                    .unwrap_or("任务未完成")
                    .into(),
            })
        }
    }
}
// Refresh the manifest bytes/sha256 for facts.sqlite after direct database
// tampering so preview reaches domain validation instead of failing on the
// digest, matching the established backup fixture contract.
fn rewrite_facts_digest(directory: &std::path::Path) {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(directory.join("facts.sqlite")).unwrap();
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    for entry in manifest["files"].as_array_mut().unwrap() {
        if entry["path"] == "facts.sqlite" {
            entry["bytes"] = json!(bytes.len());
            entry["sha256"] = json!(Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>());
        }
    }
    std::fs::write(directory.join("manifest.json"), manifest.to_string()).unwrap();
}
// Build a real space with anchored pair feedback, publish the learned profile
// and export a genuine backup archive. With foreign_member the space also
// carries a second member with its own anchored feedback and profile, so the
// archive holds two learning_profiles rows. Returns the archive directory, the
// first member's profile id and its stored pair projection.
fn export_pair_learning_backup(
    archive: &std::path::Path,
    seed: &str,
    foreign_member: bool,
) -> (std::path::PathBuf, String, Value) {
    let a = Application::open(&archive.join("wardrobe.sqlite")).unwrap();
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
    let member_b = if foreign_member {
        Some(
            a.execute("member.create", json!({"name":"备份成员乙"}))
                .unwrap()["member_id"]
                .as_str()
                .unwrap()
                .to_owned(),
        )
    } else {
        None
    };
    let anchor = item(&a, "default", &format!("{seed}锚点上衣"), "top");
    item(&a, "default", &format!("{seed}裤子"), "bottom");
    item(&a, "default", &format!("{seed}鞋"), "shoes");
    let intent = a
        .execute(
            "pairing.generate",
            json!({"member_id":"default","operation_key":format!("{seed}-anchor"),"anchor_id":anchor,"timezone":"UTC","limit":1}),
        )
        .unwrap();
    a.execute(
        "feedback.record",
        json!({"member_id":"default","operation_key":format!("{seed}-like"),"intent_id":intent["id"],"proposal_index":0,"reaction":"like"}),
    )
    .unwrap();
    if let Some(member_b) = &member_b {
        let anchor_b = item(&a, member_b, &format!("{seed}乙锚点上衣"), "top");
        item(&a, member_b, &format!("{seed}乙裤子"), "bottom");
        item(&a, member_b, &format!("{seed}乙鞋"), "shoes");
        let intent_b = a
            .execute(
                "pairing.generate",
                json!({"member_id":member_b,"operation_key":format!("{seed}-anchor-b"),"anchor_id":anchor_b,"timezone":"UTC","limit":1}),
            )
            .unwrap();
        a.execute(
            "feedback.record",
            json!({"member_id":member_b,"operation_key":format!("{seed}-like-b"),"intent_id":intent_b["id"],"proposal_index":0,"reaction":"like"}),
        )
        .unwrap();
        a.execute_and_wait(
            "learning.recompute",
            json!({"member_id":member_b,"operation_key":format!("{seed}-recompute-b")}),
        )
        .unwrap();
    }
    let profile = a
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":"default","operation_key":format!("{seed}-recompute")}),
        )
        .unwrap();
    let entries = profile["pair_projection"]["entries"].as_array().unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry["weight"].as_i64() == Some(2)),
        "{profile}"
    );
    let directory = archive.join("backup");
    a.execute_and_wait("backup.create", json!({"directory":directory.clone()}))
        .unwrap();
    // The exported archive must be physically present and complete before any
    // tampering, so rejections below are about content, not a missing file.
    assert!(directory.join("facts.sqlite").is_file(), "{directory:?}");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    assert!(
        manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == "facts.sqlite"),
        "{manifest}"
    );
    (
        directory,
        profile["id"].as_str().unwrap().to_owned(),
        profile["pair_projection"].clone(),
    )
}
// Rewrite one stored learning profile document through the public fact store
// and refresh the manifest digest afterwards.
fn update_profile_document(
    directory: &std::path::Path,
    profile_id: &str,
    mutate: impl FnOnce(&mut Value),
) {
    let db = rusqlite::Connection::open(directory.join("facts.sqlite")).unwrap();
    let raw: String = db
        .query_row(
            "SELECT document FROM learning_profiles WHERE id=?1",
            [profile_id],
            |r| r.get(0),
        )
        .unwrap();
    let mut document: Value = serde_json::from_str(&raw).unwrap();
    mutate(&mut document);
    db.execute(
        "UPDATE learning_profiles SET document=?1 WHERE id=?2",
        [document.to_string(), profile_id.to_owned()],
    )
    .unwrap();
    drop(db);
    rewrite_facts_digest(directory);
}
// An exported learning profile's pair projection is a derived fact: backup
// validation must reject a tampered pair weight exactly as it already rejects
// tampered inferred signals, through the public backup.preview seam.
#[test]
fn backup_preview_rejects_exported_pair_projection_with_tampered_weight() {
    let archive = tempfile::tempdir().unwrap();
    let (directory, profile_id, projection) =
        export_pair_learning_backup(archive.path(), "备份", false);
    // Move one pair weight inside the legal -5..=5 range so only projection
    // validation, not the clamp contract, can reject the archive.
    update_profile_document(&directory, &profile_id, |document| {
        let weight = document["pair_projection"]["entries"][0]["weight"]
            .as_i64()
            .unwrap();
        document["pair_projection"]["entries"][0]["weight"] =
            json!(if weight >= 5 { weight - 1 } else { weight + 1 });
    });
    let restored = Application::open(&archive.path().join("restored.sqlite")).unwrap();
    let error = restored
        .execute_and_wait("backup.preview", json!({"directory":directory}))
        .unwrap_err();
    assert_eq!(error.code, "backup_invalid", "{error:?}");
    assert!(projection["entries"].as_array().unwrap().len() >= 1);
}
// A genuine archive must restore and the learned pair projection must survive
// the roundtrip verbatim through the public learning seam.
#[test]
fn backup_restore_roundtrip_preserves_exported_pair_projection() {
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let (directory, _profile_id, projection) =
        export_pair_learning_backup(archive.path(), "往返", false);
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    restored
        .execute_and_wait(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true}),
        )
        .unwrap();
    let profile = restored
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(profile["pair_projection"], projection, "{profile}");
}
// Profiles exported before the pair projection existed carry no field: they
// must keep validating so old backups restore unchanged.
#[test]
fn backup_preview_accepts_legacy_profile_without_pair_projection_field() {
    let archive = tempfile::tempdir().unwrap();
    let (directory, profile_id, _) = export_pair_learning_backup(archive.path(), "旧版", false);
    update_profile_document(&directory, &profile_id, |document| {
        document.as_object_mut().unwrap().remove("pair_projection");
    });
    let restored = Application::open(&archive.path().join("restored.sqlite")).unwrap();
    let preview = restored
        .execute_and_wait("backup.preview", json!({"directory":directory}))
        .unwrap();
    assert_eq!(preview["valid"], json!(true), "{preview}");
}
// A projection naming a source feedback event that does not exist in the
// archive cannot re-derive and must be rejected.
#[test]
fn backup_preview_rejects_pair_projection_with_nonexistent_source_event() {
    let archive = tempfile::tempdir().unwrap();
    let (directory, profile_id, _) = export_pair_learning_backup(archive.path(), "缺失源", false);
    update_profile_document(&directory, &profile_id, |document| {
        document["pair_projection"]["source"]["feedback_ids"][0] = json!("missing-feedback-event");
    });
    let restored = Application::open(&archive.path().join("restored.sqlite")).unwrap();
    let error = restored
        .execute_and_wait("backup.preview", json!({"directory":directory}))
        .unwrap_err();
    assert_eq!(error.code, "backup_invalid", "{error:?}");
}
// A projection anchored on another member's feedback event is foreign: the
// event is not part of this member's fact scope and must be rejected.
#[test]
fn backup_preview_rejects_pair_projection_anchored_on_foreign_member_event() {
    let archive = tempfile::tempdir().unwrap();
    let (directory, profile_id, _) = export_pair_learning_backup(archive.path(), "越权源", true);
    let db = rusqlite::Connection::open(directory.join("facts.sqlite")).unwrap();
    let foreign: String = db
        .query_row(
            "SELECT id FROM feedback_events WHERE member_id!=(SELECT member_id FROM learning_profiles WHERE id=?1) LIMIT 1",
            [&profile_id],
            |r| r.get(0),
        )
        .unwrap();
    drop(db);
    update_profile_document(&directory, &profile_id, |document| {
        document["pair_projection"]["source"]["feedback_ids"][0] = json!(foreign);
    });
    let restored = Application::open(&archive.path().join("restored.sqlite")).unwrap();
    let error = restored
        .execute_and_wait("backup.preview", json!({"directory":directory}))
        .unwrap_err();
    assert_eq!(error.code, "backup_invalid", "{error:?}");
}
// Entries are a sorted unique edge set: a repeated edge or an unsorted order
// is not the canonical derived form and must be rejected.
#[test]
fn backup_preview_rejects_repeated_and_unsorted_pair_projection_entries() {
    for tamper in ["repeated", "unsorted"] {
        let archive = tempfile::tempdir().unwrap();
        let (directory, profile_id, projection) =
            export_pair_learning_backup(archive.path(), "乱序", false);
        assert!(
            projection["entries"].as_array().unwrap().len() >= 2,
            "{projection}"
        );
        update_profile_document(&directory, &profile_id, |document| {
            let entries = document["pair_projection"]["entries"]
                .as_array_mut()
                .unwrap();
            if tamper == "repeated" {
                let first = entries[0].clone();
                entries.push(first);
            } else {
                entries.swap(0, 1);
            }
        });
        let restored = Application::open(&archive.path().join("restored.sqlite")).unwrap();
        let error = restored
            .execute_and_wait("backup.preview", json!({"directory":directory}))
            .unwrap_err();
        assert_eq!(error.code, "backup_invalid", "{tamper}: {error:?}");
    }
}
