use serde_json::json;
use wardrobe_core::Application;
#[test]
fn recompute_returns_durable_job_before_profile_changes_and_explicit_worker_publishes() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let c = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
        )
        .unwrap();
    a.execute("wear.record",json!({"member_id":"default","operation_key":"wear","clothing_ids":[c["id"]],"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    let input = json!({"member_id":"default","operation_key":"learn"});
    let r = a.execute("learning.recompute", input.clone()).unwrap();
    assert!(r["job_id"].is_string());
    assert_eq!(r["state"], "queued");
    assert_eq!(
        a.execute("learning.get", json!({"member_id":"default"}))
            .unwrap()["inferred"],
        json!([])
    );
    assert_eq!(
        a.execute("learning.recompute", input.clone()).unwrap()["job_id"],
        r["job_id"]
    );
    drop(a);
    let a = Application::open(&path).unwrap();
    assert_eq!(
        a.execute("jobs.get", json!({"member_id":"default","id":r["job_id"]}))
            .unwrap()["state"],
        "queued"
    );
    let j = a
        .execute(
            "learning.worker.run",
            json!({"member_id":"default","job_id":r["job_id"]}),
        )
        .unwrap();
    assert_eq!(j["state"], "succeeded");
    assert!(j["result"]["profile_id"].is_string());
    let p = a
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap();
    assert!(p["inferred"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["value"] == "blue"));
    assert_eq!(
        a.execute(
            "learning.worker.run",
            json!({"member_id":"default","job_id":r["job_id"]})
        )
        .unwrap(),
        j
    );
}

#[test]
fn queued_rebuild_cannot_undo_later_reset_and_cancel_survives_reopen() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let receipt = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"before-reset","include_prior":true}),
        )
        .unwrap();
    a.execute(
        "learning.reset",
        json!({"member_id":"default","operation_key":"reset","scope":"inferred"}),
    )
    .unwrap();
    let job = a
        .execute(
            "learning.worker.run",
            json!({"member_id":"default","job_id":receipt["job_id"]}),
        )
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(job["error"]["code"], "memory_changed");
    let reset = a
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap();
    assert!(reset["reset_boundary"].is_string());
    let next = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"cancel"}),
        )
        .unwrap();
    a.execute(
        "jobs.cancel",
        json!({"member_id":"default","id":next["job_id"]}),
    )
    .unwrap();
    drop(a);
    let a = Application::open(&path).unwrap();
    let cancelled = a
        .execute(
            "learning.worker.run",
            json!({"member_id":"default","job_id":next["job_id"]}),
        )
        .unwrap();
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(
        a.execute("learning.get", json!({"member_id":"default"}))
            .unwrap()["id"],
        reset["id"]
    );
}

fn maintenance(a: &Application, operation: &str, input: serde_json::Value) -> serde_json::Value {
    let receipt = a.execute(operation, input).unwrap();
    let job = a
        .execute("maintenance.worker.run", json!({"id":receipt["job_id"]}))
        .unwrap();
    assert_eq!(job["state"], "succeeded", "{job}");
    job["result"].clone()
}
#[test]
fn portable_learning_keeps_preferences_and_marks_foreign_inference_stale() {
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source");
    let dest = d.path().join("dest");
    let package = d.path().join("package");
    let a = Application::open(&source).unwrap();
    let c = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
        )
        .unwrap();
    a.execute("wear.record",json!({"member_id":"default","operation_key":"wear","clothing_ids":[c["id"]],"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    a.execute("learning.set",json!({"member_id":"default","operation_key":"explicit","dimension":"color","value":"red","weight":4})).unwrap();
    let r = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"compute"}),
        )
        .unwrap();
    a.execute(
        "learning.worker.run",
        json!({"member_id":"default","job_id":r["job_id"]}),
    )
    .unwrap();
    let p = a
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(p["source_stale"], false);
    // Legacy embedded evidence is arbitrary immutable source material, never a local guard.
    let db = rusqlite::Connection::open(&source).unwrap();
    db.execute("UPDATE learning_profiles SET document=json_set(document,'$.inferred[0].source.sentinel',json(?1)) WHERE id=?2",rusqlite::params![json!({"local_source_version":987,"local_installation_generation":"nested-evidence"}).to_string(),p["id"].as_str().unwrap()]).unwrap();
    maintenance(
        &a,
        "backup.create",
        json!({"directory":package,"include_originals":false}),
    );
    let exported = rusqlite::Connection::open(package.join("facts.sqlite")).unwrap();
    let raw: String = exported
        .query_row(
            "SELECT document FROM learning_profiles WHERE id=?1",
            [p["id"].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    let portable: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(portable.get("local_source_version").is_none());
    assert!(portable.get("local_installation_generation").is_none());
    assert_eq!(
        portable["inferred"][0]["source"]["sentinel"]["local_source_version"],
        987
    );
    let b = Application::open(&dest).unwrap();
    maintenance(
        &b,
        "backup.restore",
        json!({"directory":package,"confirm_overwrite":true}),
    );
    let q = b
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(q["source_stale"], true);
    assert_eq!(q["explicit"].as_array().unwrap().len(), 1);
    assert_eq!(
        q["inferred"][0]["source"]["sentinel"]["local_installation_generation"],
        "nested-evidence"
    );
}

#[test]
fn running_worker_cannot_publish_after_public_reset_or_cancel() {
    for mode in 0..3 {
        let cancel = mode == 1;
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("db");
        let a = Application::open(&path).unwrap();
        let c = a
            .execute(
                "clothing.create",
                json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
            )
            .unwrap();
        let wear=a.execute("wear.record",json!({"member_id":"default","operation_key":"wear","clothing_ids":[c["id"]],"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<15000) INSERT INTO wear_events(id,member_id,root_event_id,revision,document) SELECT printf('known-%05d',x),member_id,printf('known-%05d',x),1,json_set(document,'$.id',printf('known-%05d',x),'$.root_id',printf('known-%05d',x)) FROM wear_events,n WHERE id=?1",[wear["id"].as_str().unwrap()]).unwrap();
        drop(db);
        let receipt = a
            .execute(
                "learning.recompute",
                json!({"member_id":"default","operation_key":"concurrent"}),
            )
            .unwrap();
        let id = receipt["job_id"].as_str().unwrap().to_owned();
        let worker_path = path.clone();
        let worker_id = id.clone();
        let thread = std::thread::spawn(move || {
            Application::open(&worker_path).unwrap().execute(
                "learning.worker.run",
                json!({"member_id":"default","job_id":worker_id}),
            )
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let job = a
                .execute("jobs.get", json!({"member_id":"default","id":id}))
                .unwrap();
            if job["state"] == "running" {
                break;
            }
            assert!(
                job["state"] != "succeeded",
                "fixture must intercept an actual running worker"
            );
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        if cancel {
            a.execute("jobs.cancel", json!({"member_id":"default","id":id}))
                .unwrap();
        } else if mode == 0 {
            a.execute(
                "learning.reset",
                json!({"member_id":"default","operation_key":"during-run","scope":"inferred"}),
            )
            .unwrap();
        } else {
            a.execute("wear.revoke",json!({"member_id":"default","operation_key":"late-revoke","root_event_id":wear["root_event_id"],"expected_revision":1,"reason":"误记"})).unwrap();
        }
        let result = thread.join().unwrap().unwrap();
        assert_eq!(result["state"], if cancel { "cancelled" } else { "failed" });
        if !cancel {
            assert_eq!(result["error"]["code"], "memory_changed");
        }
        assert!(a
            .execute("learning.get", json!({"member_id":"default"}))
            .unwrap()["inferred"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}

#[test]
fn worker_only_client_cannot_lift_reset_boundary() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let receipt = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"full-rebuild","include_prior":true}),
        )
        .unwrap();
    let grant=a.execute("client.authorize",json!({"name":"worker only","mode":"limited","operations":["learning.worker.run"],"member_ids":null,"expires_at":null})).unwrap();
    let error = a
        .execute_as_client(
            grant["token"].as_str().unwrap(),
            "learning.worker.run",
            json!({"member_id":"default","job_id":receipt["job_id"]}),
        )
        .unwrap_err();
    assert_eq!(error.code, "scope_denied");
    let allowed=a.execute("client.authorize",json!({"name":"worker and intent","mode":"limited","operations":["learning.worker.run","learning.recompute"],"member_ids":[receipt["member_id"]],"expires_at":null})).unwrap();
    let other = a
        .execute("member.create", json!({"name":"另一个成员"}))
        .unwrap();
    assert_eq!(
        a.execute_as_client(
            allowed["token"].as_str().unwrap(),
            "learning.worker.run",
            json!({"member_id":other["member_id"],"job_id":receipt["job_id"]})
        )
        .unwrap_err()
        .code,
        "member_denied"
    );

    assert_eq!(
        a.execute(
            "jobs.get",
            json!({"member_id":"default","id":receipt["job_id"]})
        )
        .unwrap()["state"],
        "queued"
    );
    let completed = a
        .execute_as_client(
            allowed["token"].as_str().unwrap(),
            "learning.worker.run",
            json!({"member_id":"default","job_id":receipt["job_id"]}),
        )
        .unwrap();
    assert_eq!(completed["state"], "succeeded");
}

#[test]
fn resigned_portable_learning_domain_corruption_is_rejected_without_replacing_local_facts() {
    use sha2::{Digest, Sha256};
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("source")).unwrap();
    let c = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"蓝","category":"top","color":["blue"]}),
        )
        .unwrap();
    a.execute("wear.record",json!({"member_id":"default","operation_key":"wear","clothing_ids":[c["id"]],"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    a.execute("learning.set",json!({"member_id":"default","operation_key":"explicit","dimension":"color","value":"red","weight":4})).unwrap();
    let r = a
        .execute(
            "learning.recompute",
            json!({"member_id":"default","operation_key":"compute"}),
        )
        .unwrap();
    a.execute(
        "learning.worker.run",
        json!({"member_id":"default","job_id":r["job_id"]}),
    )
    .unwrap();
    a.execute("learning.delete",json!({"member_id":"default","operation_key":"delete-unused","dimension":"style","value":"unused"})).unwrap();
    let foreign = a
        .execute("member.create", json!({"name":"另一个来源成员"}))
        .unwrap();
    let member = foreign["member_id"].clone();
    let foreign_cloth = a
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"另一件蓝衣","category":"top","color":["blue"]}),
        )
        .unwrap();
    let foreign_wear=a.execute("wear.record",json!({"member_id":member,"operation_key":"foreign-wear","clothing_ids":[foreign_cloth["id"]],"local_date":"2020-01-01","timezone":"UTC"})).unwrap();
    let foreign_receipt = a
        .execute(
            "learning.recompute",
            json!({"member_id":member,"operation_key":"foreign-compute"}),
        )
        .unwrap();
    let foreign_job = a
        .execute(
            "learning.worker.run",
            json!({"member_id":member,"job_id":foreign_receipt["job_id"]}),
        )
        .unwrap();
    let b = Application::open(&d.path().join("destination")).unwrap();
    let preserved = b
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"必须保留","category":"top"}),
        )
        .unwrap();
    let mut mutations=[
        "UPDATE learning_memories SET document=json_set(document,'$.weight',99)",
        "UPDATE learning_memories SET document=json_set(document,'$.dimension','body_size')",
        "UPDATE learning_profiles SET document=json_set(document,'$.inferred[0].source.wear_ids[0]','nonexistent-source') WHERE json_array_length(json_extract(document,'$.inferred'))>0",
        "UPDATE learning_profiles SET document=json_set(document,'$.inferred[0].weight',-4) WHERE json_array_length(json_extract(document,'$.inferred'))>0",
        "UPDATE learning_state SET document=json_set(document,'$.cutoff','invalid-time')",
        "UPDATE learning_state SET document=json_set(document,'$.suppressed',json('[123]'))",
        "UPDATE learning_receipts SET result=json_set(result,'$.profile_id','foreign-profile') WHERE operation_key='compute'",
        "UPDATE learning_memories SET document=json_set(document,'$.revision',3)",
    ].into_iter().map(str::to_owned).collect::<Vec<_>>();
    mutations.push(format!("UPDATE learning_profiles SET document=json_set(document,'$.inferred[0].source.wear_ids[0]','{}') WHERE member_id='{}' AND json_array_length(json_extract(document,'$.inferred'))>0",foreign_wear["id"].as_str().unwrap(),c["member_id"].as_str().unwrap()));
    mutations.push(format!("UPDATE learning_receipts SET result=json_set(result,'$.profile_id','{}') WHERE operation_key='compute'",foreign_job["result"]["profile_id"].as_str().unwrap()));
    for (index, sql) in mutations.iter().enumerate() {
        let package = d.path().join(format!("archive-{index}"));
        maintenance(&a, "backup.create", json!({"directory":package}));
        let db = rusqlite::Connection::open(package.join("facts.sqlite")).unwrap();
        db.execute_batch(sql).unwrap();
        drop(db);
        let bytes = std::fs::read(package.join("facts.sqlite")).unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(package.join("manifest.json")).unwrap()).unwrap();
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
        let receipt = b
            .execute(
                "backup.restore",
                json!({"directory":package,"confirm_overwrite":true}),
            )
            .unwrap();
        let failed = b
            .execute("maintenance.worker.run", json!({"id":receipt["job_id"]}))
            .unwrap();
        assert_eq!(failed["state"], "failed", "case {index}: {failed}");
        assert_eq!(
            failed["error"]["code"], "backup_invalid",
            "case {index}: {failed}"
        );
        assert_eq!(
            b.execute(
                "clothing.get",
                json!({"member_id":"default","id":preserved["id"]})
            )
            .unwrap()["name"],
            "必须保留"
        );
    }
}
#[test]
fn accepted_db19_archive_restores_empty_learning_without_relaxing_current_schema() {
    use sha2::{Digest, Sha256};
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("source")).unwrap();
    let c = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"旧版事实","category":"top"}),
        )
        .unwrap();
    let package = d.path().join("legacy");
    maintenance(&a, "backup.create", json!({"directory":package}));
    let db = rusqlite::Connection::open(package.join("facts.sqlite")).unwrap();
    db.execute_batch("DROP TABLE learning_memories;DROP TABLE learning_profiles;DROP TABLE learning_state;DROP TABLE learning_history;DROP TABLE learning_receipts;DROP TABLE IF EXISTS wardrobe_categories;DROP TABLE IF EXISTS schedule_executions;DROP TABLE IF EXISTS schedule_plans;").unwrap();
    // A genuine db19 archive predates the learning tables, the category
    // dictionary, and the schedule tables; its versioned families still carry
    // the strict legacy constraints (composite outfit_versions key and
    // per-root revision uniqueness) that schema22 relaxed. Rebuild them so the
    // fixture is an authentic historical shape instead of relaxing the
    // production validator (reference: bd4668e).
    for (table, refs, ddl, index) in [
        ("wear_events", "wear_asset_refs", "CREATE TABLE wear_events(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),root_event_id TEXT NOT NULL,revision INTEGER NOT NULL,document TEXT NOT NULL,UNIQUE(root_event_id,revision))", "CREATE INDEX wear_member_root ON wear_events(member_id,root_event_id,revision);"),
        ("care_events", "care_asset_refs", "CREATE TABLE care_events(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),root_event_id TEXT NOT NULL,revision INTEGER NOT NULL,document TEXT NOT NULL,UNIQUE(root_event_id,revision))", "CREATE INDEX care_member_root ON care_events(member_id,root_event_id,revision);"),
        ("feedback_events", "feedback_asset_refs", "CREATE TABLE feedback_events(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),root_id TEXT NOT NULL,revision INTEGER NOT NULL,document TEXT NOT NULL,UNIQUE(root_id,revision))", "CREATE INDEX feedback_member_root ON feedback_events(member_id,root_id,revision);"),
        ("outfit_versions", "", "CREATE TABLE outfit_versions(outfit_id TEXT NOT NULL REFERENCES outfits(id),version INTEGER NOT NULL,document TEXT NOT NULL,PRIMARY KEY(outfit_id,version))", ""),
    ] {
        let mut batch = format!("BEGIN;ALTER TABLE {table} RENAME TO {table}_legacy;{ddl};INSERT INTO {table} SELECT * FROM {table}_legacy;");
        if !refs.is_empty() {
            batch.push_str(&format!("ALTER TABLE {refs} RENAME TO {refs}_legacy;CREATE TABLE {refs}(event_id TEXT NOT NULL REFERENCES {table}(id),asset_id TEXT NOT NULL,member_id TEXT NOT NULL,PRIMARY KEY(event_id,asset_id),FOREIGN KEY(asset_id,member_id) REFERENCES assets(id,member_id));INSERT INTO {refs} SELECT * FROM {refs}_legacy;DROP TABLE {refs}_legacy;"));
        }
        batch.push_str(&format!("DROP TABLE {table}_legacy;{index}COMMIT;"));
        db.execute_batch(&batch).unwrap();
    }
    db.execute_batch("PRAGMA user_version=19;").unwrap();
    drop(db);
    let bytes = std::fs::read(package.join("facts.sqlite")).unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(package.join("manifest.json")).unwrap()).unwrap();
    manifest["database_version"] = json!(19);
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
    let b = Application::open(&d.path().join("dest")).unwrap();
    maintenance(&b, "backup.preview", json!({"directory":package}));
    maintenance(
        &b,
        "backup.restore",
        json!({"directory":package,"confirm_overwrite":true}),
    );
    assert_eq!(
        b.execute("clothing.get", json!({"member_id":"default","id":c["id"]}))
            .unwrap()["name"],
        "旧版事实"
    );
    assert!(b
        .execute("learning.get", json!({"member_id":"default"}))
        .unwrap()["explicit"]
        .as_array()
        .unwrap()
        .is_empty());
    let db = rusqlite::Connection::open(package.join("facts.sqlite")).unwrap();
    db.execute_batch("PRAGMA user_version=20").unwrap();
    drop(db);
    manifest["database_version"] = json!(20);
    let bytes = std::fs::read(package.join("facts.sqlite")).unwrap();
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
    let receipt = b
        .execute("backup.preview", json!({"directory":package}))
        .unwrap();
    let rejected = b
        .execute("maintenance.worker.run", json!({"id":receipt["job_id"]}))
        .unwrap();
    assert_eq!(rejected["state"], "failed");
    assert_eq!(rejected["error"]["code"], "backup_invalid");
}
