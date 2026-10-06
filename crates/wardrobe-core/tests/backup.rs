use serde_json::json;
use wardrobe_core::Application;
trait MaintenanceTestApplication {
    fn execute_and_wait(
        &self,
        operation: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, wardrobe_core::ApiError>;
}
impl MaintenanceTestApplication for Application {
    fn execute_and_wait(
        &self,
        operation: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, wardrobe_core::ApiError> {
        let submitted = self.execute(operation, input)?;
        if let Some(id) = submitted["job_id"].as_str() {
            let job = self.execute("maintenance.worker.run", json!({"id":id}))?;
            if job["state"] == "succeeded" {
                Ok(job["result"].clone())
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
        } else {
            Ok(submitted)
        }
    }
}
#[test]
fn portable_backup_preview_restore_preserves_actual_history_after_restart() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"保留的衬衫","category":"top"}),
        )
        .unwrap();
    let wear = app.execute_and_wait("wear.record",json!({"member_id":"default","clothing_ids":[garment["id"]],"local_date":"2020-01-01","timezone":"UTC","operation_key":"wear"})).unwrap();
    app.execute_and_wait("care.record",json!({"member_id":"default","clothing_id":garment["id"],"action":"wash","local_date":"2020-01-02","timezone":"UTC","operation_key":"wash"})).unwrap();
    let intent = app
        .execute_and_wait(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"kept-intent","limit":1,"timezone":"UTC"}),
        )
        .unwrap();
    let package = archive.path().join("complete-backup");
    let backup = app
        .execute_and_wait(
            "backup.create",
            json!({"directory":package,"include_originals":false}),
        )
        .unwrap();
    assert_eq!(backup["includes_originals"], false);
    let preview = app
        .execute_and_wait("backup.preview", json!({"directory":package}))
        .unwrap();
    assert_eq!(preview["valid"], true);
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    restored
        .execute_and_wait(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .unwrap();
    drop(restored);
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        restored
            .execute_and_wait(
                "clothing.get",
                json!({"member_id":"default","id":garment["id"]})
            )
            .unwrap(),
        garment
    );
    assert_eq!(
        restored
            .execute_and_wait("wear.get", json!({"member_id":"default","id":wear["id"]}))
            .unwrap(),
        wear
    );
    assert_eq!(
        restored
            .execute_and_wait(
                "recommendation.get",
                json!({"member_id":"default","id":intent["id"]})
            )
            .unwrap(),
        intent
    );
    assert_eq!(
        restored
            .execute_and_wait(
                "care.status",
                json!({"member_id":"default","clothing_id":garment["id"]})
            )
            .unwrap()["wears_since_clean"],
        0
    );
}
#[test]
fn restored_display_is_self_contained_and_corrupt_archive_never_overwrites_current_facts() {
    let source = tempfile::tempdir().unwrap();
    let archives = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let picture = source.path().join("original.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([120, 80, 60]))
        .save(&picture)
        .unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let asset = app
        .execute_and_wait(
            "assets.import",
            json!({"member_id":"default","path":picture}),
        )
        .unwrap();
    let preview = app
        .execute_and_wait(
            "assets.preview",
            json!({"member_id":"default","id":asset["id"]}),
        )
        .unwrap();
    let directory = archives.path().join("backup");
    app.execute_and_wait(
        "backup.create",
        json!({"directory":directory,"include_originals":false}),
    )
    .unwrap();
    drop(app);
    source.close().unwrap();
    let dest = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    dest.execute_and_wait(
        "backup.restore",
        json!({"directory":directory,"confirm_overwrite":true}),
    )
    .unwrap();
    assert_eq!(
        dest.execute_and_wait(
            "assets.preview",
            json!({"member_id":"default","id":asset["id"]})
        )
        .unwrap(),
        preview
    );
    let retained = dest
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"覆盖失败必须保留","category":"top"}),
        )
        .unwrap();
    std::fs::write(directory.join("facts.sqlite"), b"truncated").unwrap();
    assert!(dest
        .execute_and_wait(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true})
        )
        .is_err());
    assert_eq!(
        dest.execute_and_wait(
            "clothing.get",
            json!({"member_id":"default","id":retained["id"]})
        )
        .unwrap(),
        retained
    );
}
#[test]
fn credentials_are_absent_from_every_backup_file_and_never_authorize_restored_installation() {
    use sha2::{Digest, Sha256};
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let canary = "PRIVATE_CLIENT_GRANT_CANARY_729418";
    let grant=app.execute_and_wait("client.authorize",json!({"name":canary,"mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap();
    let hash: String = Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    app.execute_and_wait("provider.save",json!({"name":"PRIVATE_PROVIDER_CANARY_248723","endpoint":"http://127.0.0.1:11434/v1","model":"fixture","capabilities":["text"],"authentication":"none"})).unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait(
        "backup.create",
        json!({"directory":directory,"include_originals":false}),
    )
    .unwrap();
    fn scan(path: &std::path::Path, needles: &[&str]) {
        for item in std::fs::read_dir(path).unwrap() {
            let p = item.unwrap().path();
            if p.is_dir() {
                scan(&p, needles);
            } else {
                let data = std::fs::read(p).unwrap();
                for needle in needles {
                    assert!(
                        !data.windows(needle.len()).any(|w| w == needle.as_bytes()),
                        "private canary in physical backup"
                    );
                }
            }
        }
    }
    scan(
        &directory,
        &[canary, token, &hash, "PRIVATE_PROVIDER_CANARY_248723"],
    );
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    restored
        .execute_and_wait(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true}),
        )
        .unwrap();
    assert_eq!(
        restored.execute_and_wait("client.list", json!({})).unwrap()["items"],
        json!([])
    );
    assert_eq!(
        restored
            .execute_as_client(token, "clothing.list", json!({"member_id":"default"}))
            .unwrap_err()
            .code,
        "unauthorized"
    );
}
#[test]
fn preview_explains_overwrite_and_real_prebackup_can_restore_previous_installation() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    app.execute_and_wait(
        "clothing.create",
        json!({"member_id":"default","name":"待恢复","category":"top"}),
    )
    .unwrap();
    let package = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":package}))
        .unwrap();
    let dest = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    let original = dest
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"当前旧库","category":"dress"}),
        )
        .unwrap();
    let preview = dest
        .execute_and_wait("backup.preview", json!({"directory":package}))
        .unwrap();
    assert_eq!(preview["incoming"]["clothing"], 1);
    assert_eq!(preview["current"]["clothing"], 1);
    assert_eq!(
        dest.execute_and_wait("backup.restore", json!({"directory":package}))
            .unwrap_err()
            .code,
        "confirmation_required"
    );
    let receipt = dest
        .execute_and_wait(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .unwrap();
    assert_eq!(
        dest.execute_and_wait(
            "clothing.get",
            json!({"member_id":"default","id":original["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    dest.execute_and_wait(
        "backup.restore",
        json!({"directory":receipt["rollback_backup"],"confirm_overwrite":true}),
    )
    .unwrap();
    drop(dest);
    let dest = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        dest.execute_and_wait(
            "clothing.get",
            json!({"member_id":"default","id":original["id"]})
        )
        .unwrap(),
        original
    );
}
#[test]
fn data_directory_switch_verifies_copy_and_existing_application_and_reopened_tools_follow_it() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let old = source.path().join("wardrobe.sqlite");
    let target = destination.path().join("new-data");
    let app = Application::open(&old).unwrap();
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"搬迁保留","category":"top"}),
        )
        .unwrap();
    let moved = app
        .execute_and_wait("data_directory.move", json!({"directory":target}))
        .unwrap();
    assert_eq!(moved["switched"], true);
    assert!(old.is_file());
    let second = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"迁移后写入","category":"shoes"}),
        )
        .unwrap();
    let reopened = Application::open(&old).unwrap();
    assert_eq!(
        reopened
            .execute_and_wait(
                "clothing.get",
                json!({"member_id":"default","id":second["id"]})
            )
            .unwrap(),
        second
    );
    let direct = Application::open(&target.join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        direct
            .execute_and_wait(
                "clothing.get",
                json!({"member_id":"default","id":garment["id"]})
            )
            .unwrap(),
        garment
    );
    assert_eq!(
        direct
            .execute_and_wait(
                "clothing.get",
                json!({"member_id":"default","id":second["id"]})
            )
            .unwrap(),
        second
    );
    assert_eq!(
        app.execute_and_wait("data_directory.move", json!({"directory":target}))
            .unwrap_err()
            .code,
        "already_exists"
    );
}
#[test]
fn directory_move_retains_local_grants_configuration_and_media_without_reading_old_assets() {
    let source = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let picture = source.path().join("fixture.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([180, 160, 90]))
        .save(&picture)
        .unwrap();
    let asset = app
        .execute_and_wait(
            "assets.import",
            json!({"member_id":"default","path":picture}),
        )
        .unwrap();
    let before = app
        .execute_and_wait(
            "assets.preview",
            json!({"member_id":"default","id":asset["id"]}),
        )
        .unwrap();
    let grant=app.execute_and_wait("client.authorize",json!({"name":"保留的本机授权","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    let provider=app.execute_and_wait("provider.save",json!({"name":"保留的模型配置","endpoint":"http://127.0.0.1:11434/v1","model":"fixture","capabilities":["text"],"authentication":"none"})).unwrap();
    let directory = root.path().join("moved");
    let submitted = app
        .execute_as_client(
            grant["token"].as_str().unwrap(),
            "data_directory.move",
            json!({"directory":directory}),
        )
        .unwrap();
    app.execute("maintenance.worker.run", json!({"id":submitted["job_id"]}))
        .unwrap();
    std::fs::remove_dir_all(source.path().join("assets")).unwrap();
    assert_eq!(
        app.execute_as_client(
            grant["token"].as_str().unwrap(),
            "assets.preview",
            json!({"member_id":"default","id":asset["id"]})
        )
        .unwrap(),
        before
    );
    assert_eq!(
        app.execute_and_wait("provider.list", json!({})).unwrap()["items"][0]["id"],
        provider["id"]
    );
    assert_eq!(
        app.execute_and_wait("client.list", json!({})).unwrap()["items"][0]["id"],
        grant["id"]
    );
}
#[test]
fn concurrent_old_connection_writes_are_present_after_directory_switch() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let old = source.path().join("wardrobe.sqlite");
    let app = Application::open(&old).unwrap();
    let (started, ready) = std::sync::mpsc::channel();
    let path = old.clone();
    let writer = std::thread::spawn(move || {
        let app = Application::open(&path).unwrap();
        let mut ids = Vec::new();
        for n in 0..100 {
            loop {
                match app.execute_and_wait(
                    "clothing.create",
                    json!({"member_id":"default","name":format!("并发 {n}"),"category":"top"}),
                ) {
                    Ok(item) => {
                        ids.push(item["id"].clone());
                        break;
                    }
                    Err(e)
                        if matches!(
                            e.code.as_str(),
                            "data_directory_moved" | "data_directory_moving"
                        ) =>
                    {
                        std::thread::yield_now();
                        continue;
                    }
                    Err(e) => panic!("{}", e.code),
                }
            }
            if n == 0 {
                started.send(()).unwrap();
            }
        }
        ids
    });
    ready.recv().unwrap();
    let target = destination.path().join("moved");
    app.execute_and_wait("data_directory.move", json!({"directory":target}))
        .unwrap();
    let ids = writer.join().unwrap();
    let read = Application::open(&target.join("wardrobe.sqlite")).unwrap();
    for id in ids {
        assert_eq!(
            read.execute_and_wait("clothing.get", json!({"member_id":"default","id":id}))
                .unwrap()["id"],
            id
        );
    }
}
#[test]
fn interrupted_pending_directory_publication_falls_back_to_old_source_before_any_new_writes() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let old = source.path().join("wardrobe.sqlite");
    let new = destination.path().join("wardrobe.sqlite");
    let app = Application::open(&old).unwrap();
    let kept = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"提交未完成的旧源","category":"top"}),
        )
        .unwrap();
    drop(app);
    let candidate = Application::open(&new).unwrap();
    candidate
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"尚未发布的新副本","category":"top"}),
        )
        .unwrap();
    drop(candidate);
    std::fs::write(source.path().join(".wardrobe-location.json"),json!({"version":1,"phase":"pending","source_file":"wardrobe.sqlite","target_database":new}).to_string()).unwrap();
    let recovered = Application::open(&old).unwrap();
    assert_eq!(
        recovered
            .execute_and_wait(
                "clothing.get",
                json!({"member_id":"default","id":kept["id"]})
            )
            .unwrap(),
        kept
    );
    assert!(!source.path().join(".wardrobe-location.json").exists());
}
#[test]
fn overwrite_removes_previous_installation_operational_jobs_and_authorization_without_losing_incoming_facts(
) {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let fact = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"恢复事实","category":"top"}),
        )
        .unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    let dest = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    let picture = target.path().join("draft.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([90, 120, 150]))
        .save(&picture)
        .unwrap();
    dest.execute_and_wait(
        "imports.create",
        json!({"member_id":"default","operation_key":"old-drafts","paths":[picture]}),
    )
    .unwrap();
    let grant=dest.execute_and_wait("client.authorize",json!({"name":"旧空间授权","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    let submitted = dest
        .execute_as_client(
            grant["token"].as_str().unwrap(),
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true}),
        )
        .unwrap();
    dest.execute("maintenance.worker.run", json!({"id":submitted["job_id"]}))
        .unwrap();
    assert_eq!(
        dest.execute_and_wait(
            "clothing.get",
            json!({"member_id":"default","id":fact["id"]})
        )
        .unwrap(),
        fact
    );
    assert_eq!(
        dest.execute_and_wait("jobs.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        dest.execute_as_client(
            grant["token"].as_str().unwrap(),
            "clothing.list",
            json!({"member_id":"default"})
        )
        .unwrap_err()
        .code,
        "unauthorized"
    );
}
#[test]
fn local_backup_preferences_persist_with_revision_but_are_not_portable_installation_settings() {
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("wardrobe.sqlite");
    let app = Application::open(&path).unwrap();
    let initial = app
        .execute_and_wait("backup.settings.get", json!({}))
        .unwrap();
    assert_eq!(initial["revision"], 0);
    let saved=app.execute_and_wait("backup.settings.set",json!({"directory":source.path().join("chosen-backups"),"include_originals":true,"expected_revision":0})).unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    assert_eq!(
        app.execute_and_wait("backup.settings.get", json!({}))
            .unwrap(),
        saved
    );
    assert_eq!(
        app.execute_and_wait(
            "backup.settings.set",
            json!({"directory":source.path(),"include_originals":false,"expected_revision":0})
        )
        .unwrap_err()
        .code,
        "conflict"
    );
}
#[test]
fn explicit_original_option_controls_real_bytes_and_restored_original_availability() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let picture = source.path().join("synthetic-original.png");
    image::RgbImage::from_pixel(64, 32, image::Rgb([87, 142, 203]))
        .save(&picture)
        .unwrap();
    let original = std::fs::read(&picture).unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let asset = app
        .execute_and_wait(
            "assets.import",
            json!({"member_id":"default","path":picture}),
        )
        .unwrap();
    for include in [false, true] {
        app.execute_and_wait("backup.create",json!({"directory":archive.path().join(if include{"with-original"}else{"display-only"}),"include_originals":include})).unwrap();
    }
    drop(app);
    source.close().unwrap();
    for include in [false, true] {
        let target = tempfile::tempdir().unwrap();
        let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
        let directory = archive.path().join(if include {
            "with-original"
        } else {
            "display-only"
        });
        let preview = restored
            .execute_and_wait("backup.preview", json!({"directory":directory}))
            .unwrap();
        assert_eq!(preview["manifest"]["includes_originals"], include);
        restored
            .execute_and_wait(
                "backup.restore",
                json!({"directory":directory,"confirm_overwrite":true}),
            )
            .unwrap();
        let read = restored
            .execute_and_wait(
                "assets.get",
                json!({"member_id":"default","id":asset["id"]}),
            )
            .unwrap();
        if include {
            assert_eq!(
                std::fs::read(read["original"]["local_path"].as_str().unwrap()).unwrap(),
                original
            );
        } else {
            assert!(read["original"]["local_path"].is_null());
            assert_eq!(read["original"]["backup_available"], false);
            for item in std::fs::read_dir(directory.join("media")).unwrap() {
                assert_ne!(std::fs::read(item.unwrap().path()).unwrap(), original);
            }
        }
        assert!(restored
            .execute_and_wait(
                "assets.preview",
                json!({"member_id":"default","id":asset["id"]})
            )
            .unwrap()["data_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/webp;base64,"));
    }
}
#[test]
fn immutable_versions_revised_wear_care_and_detached_historical_display_survive_relocated_backup() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let picture = source.path().join("synthetic.png");
    image::RgbImage::from_pixel(32, 48, image::Rgb([178, 88, 142]))
        .save(&picture)
        .unwrap();
    let asset = app
        .execute_and_wait(
            "assets.import",
            json!({"member_id":"default","path":picture}),
        )
        .unwrap();
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"当时的旧衬衫","category":"top"}),
        )
        .unwrap();
    app.execute_and_wait(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":garment["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    let outfit=app.execute_and_wait("outfit.create",json!({"member_id":"default","name":"原始搭配","items":[{"clothing_id":garment["id"],"role":"base","layer":0}]})).unwrap();
    app.execute_and_wait("outfit.update",json!({"member_id":"default","id":outfit["id"],"expected_version":1,"name":"新版搭配","items":[{"clothing_id":garment["id"],"role":"outer","layer":2}]})).unwrap();
    let wear=app.execute_and_wait("wear.record",json!({"member_id":"default","clothing_ids":[garment["id"]],"local_date":"2020-01-01","timezone":"UTC","operation_key":"wear"})).unwrap();
    app.execute_and_wait("wear.correct",json!({"member_id":"default","root_event_id":wear["id"],"expected_revision":1,"local_date":"2020-01-02","timezone":"UTC","operation_key":"correct-wear","notes":"实际补正"})).unwrap();
    let care=app.execute_and_wait("care.record",json!({"member_id":"default","clothing_id":garment["id"],"action":"wash","local_date":"2020-01-03","timezone":"UTC","operation_key":"care"})).unwrap();
    app.execute_and_wait("care.correct",json!({"member_id":"default","root_event_id":care["id"],"expected_revision":1,"action":"dry_clean","local_date":"2020-01-04","timezone":"UTC","operation_key":"correct-care"})).unwrap();
    app.execute_and_wait(
        "assets.detach",
        json!({"member_id":"default","clothing_id":garment["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    app.execute_and_wait(
        "clothing.delete",
        json!({"member_id":"default","id":garment["id"]}),
    )
    .unwrap();
    let wear_history = app
        .execute_and_wait(
            "wear.history",
            json!({"member_id":"default","root_event_id":wear["id"]}),
        )
        .unwrap();
    let care_history = app
        .execute_and_wait(
            "care.history",
            json!({"member_id":"default","root_event_id":care["id"]}),
        )
        .unwrap();
    let collage = app
        .execute_and_wait(
            "outfit.render_local",
            json!({"member_id":"default","id":outfit["id"]}),
        )
        .unwrap();
    assert_eq!(collage["missing_images"], 0);
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    drop(app);
    source.close().unwrap();
    let relocated = archive.path().join("relocated-package");
    std::fs::rename(&directory, &relocated).unwrap();
    let dest = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    dest.execute_and_wait(
        "backup.restore",
        json!({"directory":relocated,"confirm_overwrite":true}),
    )
    .unwrap();
    drop(dest);
    let dest = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        dest.execute_and_wait(
            "outfit.version",
            json!({"member_id":"default","id":outfit["id"],"version":1})
        )
        .unwrap(),
        outfit
    );
    assert_eq!(
        dest.execute_and_wait(
            "wear.history",
            json!({"member_id":"default","root_event_id":wear["id"]})
        )
        .unwrap(),
        wear_history
    );
    assert_eq!(
        dest.execute_and_wait(
            "care.history",
            json!({"member_id":"default","root_event_id":care["id"]})
        )
        .unwrap(),
        care_history
    );
    assert_eq!(
        dest.execute_and_wait(
            "outfit.render_local",
            json!({"member_id":"default","id":outfit["id"]})
        )
        .unwrap(),
        collage
    );
    assert_eq!(
        dest.execute_and_wait(
            "clothing.get",
            json!({"member_id":"default","id":garment["id"]})
        )
        .unwrap()["status"],
        "deleted"
    );
}
#[test]
fn portable_media_content_remains_decodable_with_extensionless_transport_object_names() {
    use sha2::{Digest, Sha256};
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let picture = source.path().join("synthetic.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([76, 154, 197]))
        .save(&picture)
        .unwrap();
    let asset = app
        .execute_and_wait(
            "assets.import",
            json!({"member_id":"default","path":picture}),
        )
        .unwrap();
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"传输对象不靠扩展名","category":"top"}),
        )
        .unwrap();
    app.execute_and_wait(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":garment["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    let outfit=app.execute_and_wait("outfit.create",json!({"member_id":"default","name":"真实图片搭配","items":[{"clothing_id":garment["id"],"role":"base","layer":0}]})).unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    drop(app);
    let db = rusqlite::Connection::open(directory.join("facts.sqlite")).unwrap();
    let raw: String = db
        .query_row("SELECT document FROM assets", [], |r| r.get(0))
        .unwrap();
    let mut document: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let previous = document["display"]["local_path"]
        .as_str()
        .unwrap()
        .to_owned();
    let object = "media/extensionless-display-object";
    std::fs::rename(directory.join(&previous), directory.join(object)).unwrap();
    document["display"]["local_path"] = json!(object);
    db.execute("UPDATE assets SET document=?1", [document.to_string()])
        .unwrap();
    drop(db);
    let manifest_path = directory.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    for entry in manifest["files"].as_array_mut().unwrap() {
        if entry["path"] == previous {
            entry["path"] = json!(object);
        }
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
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    restored
        .execute_and_wait(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true}),
        )
        .unwrap();
    assert_eq!(
        restored
            .execute_and_wait(
                "outfit.render_local",
                json!({"member_id":"default","id":outfit["id"]})
            )
            .unwrap()["missing_images"],
        0
    );
    restored
        .execute_and_wait(
            "assets.rebuild_thumbnail",
            json!({"member_id":"default","id":asset["id"]}),
        )
        .unwrap();
}
#[test]
fn preview_rejects_missing_json_asset_references_even_when_database_and_file_hashes_are_consistent()
{
    use sha2::{Digest, Sha256};
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let picture = source.path().join("synthetic.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([116, 82, 201]))
        .save(&picture)
        .unwrap();
    let asset = app
        .execute_and_wait(
            "assets.import",
            json!({"member_id":"default","path":picture}),
        )
        .unwrap();
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"引用必须存在","category":"top"}),
        )
        .unwrap();
    app.execute_and_wait(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":garment["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    let db = rusqlite::Connection::open(directory.join("facts.sqlite")).unwrap();
    db.execute("DELETE FROM assets", []).unwrap();
    drop(db);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let bytes = std::fs::read(directory.join("facts.sqlite")).unwrap();
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
    assert_eq!(
        app.execute_and_wait("backup.preview", json!({"directory":directory}))
            .unwrap_err()
            .code,
        "backup_invalid"
    );
    assert!(app
        .execute_and_wait(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true})
        )
        .is_err());
    assert_eq!(
        app.execute_and_wait(
            "clothing.get",
            json!({"member_id":"default","id":garment["id"]})
        )
        .unwrap()["display_asset_id"],
        asset["id"]
    );
}

#[test]
fn restore_uses_only_verified_inventory_and_keeps_unlisted_objects_outside_local_media() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let package = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":package}))
        .unwrap();
    std::fs::write(
        package.join("media/unlisted-private-object"),
        b"must never adopt this object",
    )
    .unwrap();
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    restored
        .execute_and_wait(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .unwrap();
    for directory in std::fs::read_dir(target.path().join("assets")).unwrap() {
        let directory = directory.unwrap().path();
        if directory.is_dir() {
            assert!(!directory.join("media/unlisted-private-object").exists());
        }
    }
}

#[test]
fn preview_rejects_undecodable_display_even_when_transport_hash_matches() {
    use sha2::{Digest, Sha256};
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let picture = source.path().join("synthetic.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([100, 90, 80]))
        .save(&picture)
        .unwrap();
    app.execute_and_wait(
        "assets.import",
        json!({"member_id":"default","path":picture}),
    )
    .unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    let db = rusqlite::Connection::open(directory.join("facts.sqlite")).unwrap();
    let raw: String = db
        .query_row("SELECT document FROM assets", [], |row| row.get(0))
        .unwrap();
    let asset: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let path = asset["display"]["local_path"].as_str().unwrap();
    let bytes = b"invalid image with a legitimate transport checksum";
    std::fs::write(directory.join(path), bytes).unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    for entry in manifest["files"].as_array_mut().unwrap() {
        if entry["path"] == path {
            entry["bytes"] = json!(bytes.len());
            entry["sha256"] = json!(Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>());
        }
    }
    std::fs::write(directory.join("manifest.json"), manifest.to_string()).unwrap();
    assert_eq!(
        app.execute_and_wait("backup.preview", json!({"directory":directory}))
            .unwrap_err()
            .code,
        "backup_invalid"
    );
}

#[test]
fn already_open_authorized_application_cannot_write_after_restore_removes_its_grant() {
    use std::{
        io::Write,
        sync::mpsc,
        time::{Duration, Instant},
    };
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let incoming = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let package = archive.path().join("backup");
    incoming
        .execute_and_wait("backup.create", json!({"directory":package}))
        .unwrap();
    let db = target.path().join("wardrobe.sqlite");
    let app = Application::open(&db).unwrap();
    for n in 0..4u8 {
        let picture = target.path().join(format!("original-{n}.png"));
        image::RgbImage::from_pixel(16, 16, image::Rgb([n, 90, 120]))
            .save(&picture)
            .unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&picture)
            .unwrap()
            .write_all(&vec![n; 35_000_000])
            .unwrap();
        app.execute_and_wait(
            "assets.import",
            json!({"member_id":"default","path":picture}),
        )
        .unwrap();
        std::fs::remove_file(picture).unwrap();
    }
    std::fs::remove_dir_all(&package).unwrap();
    app.execute_and_wait("backup.create", json!({"directory":package}))
        .unwrap();
    let grant = app.execute_and_wait("client.authorize",json!({"name":"temporary old application","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap().to_owned();
    let (ready_tx, ready_rx) = mpsc::channel();
    let target_path = target.path().to_owned();
    let writer = std::thread::spawn(move || {
        let old = Application::open(&db).unwrap();
        ready_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let copying = std::fs::read_dir(target_path.join("backup-rollback"))
                .ok()
                .is_some_and(|entries| {
                    entries.filter_map(Result::ok).any(|entry| {
                        entry.file_name().to_string_lossy().contains("staging-")
                            && std::fs::read_dir(entry.path().join("media"))
                                .ok()
                                .is_some_and(|mut files| files.next().is_some())
                    })
                });
            if copying {
                break;
            }
            assert!(Instant::now() < deadline, "prebackup was not observed");
            std::thread::yield_now();
        }
        let blocked = old.execute_as_client(
            &token,
            "clothing.create",
            json!({"member_id":"default","name":"old grant must not write","category":"top"}),
        );
        let next = old.execute_as_client(
            &token,
            "clothing.create",
            json!({"member_id":"default","name":"old grant must still not write","category":"top"}),
        );
        (blocked, next)
    });
    ready_rx.recv().unwrap();
    app.execute_and_wait(
        "backup.restore",
        json!({"directory":package,"confirm_overwrite":true}),
    )
    .unwrap();
    let (blocked, next) = writer.join().unwrap();
    let error = blocked.unwrap_err();
    assert_eq!(next.unwrap_err().code, "unauthorized");
    assert!(
        matches!(
            error.code.as_str(),
            "unauthorized" | "authorization_revoked" | "data_restored"
        ),
        "restore boundary must reject stale authorization explicitly, got {}",
        error.code
    );
    assert!(app
        .execute_and_wait("clothing.list", json!({"member_id":"default"}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn unsupported_versions_and_cross_platform_paths_never_change_current_installation() {
    let installation = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let app = Application::open(&installation.path().join("wardrobe.sqlite")).unwrap();
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"current must remain","category":"top"}),
        )
        .unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    for path in [
        "../outside",
        "C:/outside",
        "media/../../outside",
        "media\\outside",
    ] {
        let mut manifest = original.clone();
        manifest["files"][0]["path"] = json!(path);
        std::fs::write(directory.join("manifest.json"), manifest.to_string()).unwrap();
        assert_eq!(
            app.execute_and_wait("backup.preview", json!({"directory":directory}))
                .unwrap_err()
                .code,
            "backup_invalid"
        );
        assert!(app
            .execute_and_wait(
                "backup.restore",
                json!({"directory":directory,"confirm_overwrite":true})
            )
            .is_err());
    }
    for key in ["version", "database_version"] {
        let mut manifest = original.clone();
        manifest[key] = json!(999999);
        std::fs::write(directory.join("manifest.json"), manifest.to_string()).unwrap();
        assert_eq!(
            app.execute_and_wait("backup.preview", json!({"directory":directory}))
                .unwrap_err()
                .code,
            "backup_invalid"
        );
    }
    assert_eq!(
        app.execute_and_wait(
            "clothing.get",
            json!({"member_id":"default","id":garment["id"]})
        )
        .unwrap(),
        garment
    );
}

#[test]
fn preview_rejects_removed_foreign_key_declarations_before_any_overwrite() {
    use sha2::{Digest, Sha256};
    let installation = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let app = Application::open(&installation.path().join("wardrobe.sqlite")).unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    let db = rusqlite::Connection::open(directory.join("facts.sqlite")).unwrap();
    db.execute_batch("DROP TABLE wear_asset_refs;CREATE TABLE wear_asset_refs(event_id TEXT NOT NULL,asset_id TEXT NOT NULL,member_id TEXT NOT NULL,PRIMARY KEY(event_id,asset_id));INSERT INTO wear_asset_refs VALUES('missing-event','missing-asset','missing-member');").unwrap();
    drop(db);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let bytes = std::fs::read(directory.join("facts.sqlite")).unwrap();
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
    assert_eq!(
        app.execute_and_wait("backup.preview", json!({"directory":directory}))
            .unwrap_err()
            .code,
        "backup_invalid"
    );
    assert!(app
        .execute_and_wait(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true})
        )
        .is_err());
}

#[test]
fn preview_rejects_invalid_category_or_document_identity_with_legitimate_file_hashes() {
    use sha2::{Digest, Sha256};
    let installation = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let app = Application::open(&installation.path().join("wardrobe.sqlite")).unwrap();
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"unchanged fact","category":"top"}),
        )
        .unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    for (key, value) in [
        ("category", json!("illegal_category")),
        ("id", json!(uuid::Uuid::now_v7().to_string())),
        ("member_id", json!(uuid::Uuid::now_v7().to_string())),
    ] {
        let mut document = garment.clone();
        document[key] = value;
        let db = rusqlite::Connection::open(directory.join("facts.sqlite")).unwrap();
        db.execute("UPDATE clothing SET document=?1", [document.to_string()])
            .unwrap();
        drop(db);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap())
                .unwrap();
        let bytes = std::fs::read(directory.join("facts.sqlite")).unwrap();
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
        assert_eq!(
            app.execute_and_wait("backup.preview", json!({"directory":directory}))
                .unwrap_err()
                .code,
            "backup_invalid",
            "field {key} accepted"
        );
        assert!(app
            .execute_and_wait(
                "backup.restore",
                json!({"directory":directory,"confirm_overwrite":true})
            )
            .is_err());
        assert_eq!(
            app.execute_and_wait(
                "clothing.get",
                json!({"member_id":"default","id":garment["id"]})
            )
            .unwrap(),
            garment
        );
    }
}

#[test]
fn failed_real_prebackup_preserves_current_authorization_and_cleans_staged_restore_files() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let incoming = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let package = archive.path().join("backup");
    incoming
        .execute_and_wait("backup.create", json!({"directory":package}))
        .unwrap();
    let app = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    let picture = target.path().join("owned.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([80, 70, 60]))
        .save(&picture)
        .unwrap();
    let asset = app
        .execute_and_wait(
            "assets.import",
            json!({"member_id":"default","path":picture}),
        )
        .unwrap();
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"must retain current facts","category":"top"}),
        )
        .unwrap();
    let grant=app.execute_and_wait("client.authorize",json!({"name":"synthetic rollback verifier","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    std::fs::remove_file(asset["original"]["local_path"].as_str().unwrap()).unwrap();
    assert_eq!(
        app.execute_and_wait(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true})
        )
        .unwrap_err()
        .code,
        "backup_io"
    );
    assert_eq!(
        app.execute_and_wait(
            "clothing.get",
            json!({"member_id":"default","id":garment["id"]})
        )
        .unwrap(),
        garment
    );
    app.execute_as_client(
        grant["token"].as_str().unwrap(),
        "clothing.list",
        json!({"member_id":"default"}),
    )
    .unwrap();
    for entry in std::fs::read_dir(target.path().join("assets")).unwrap() {
        assert!(!entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("restore-"));
    }
}

fn assert_rejects_corrupted_domain_document(
    table: &str,
    column: &str,
    mutate: impl FnOnce(&mut serde_json::Value),
) {
    use sha2::{Digest, Sha256};
    let installation = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let app = Application::open(&installation.path().join("wardrobe.sqlite")).unwrap();
    let mut garments = Vec::new();
    for category in ["top", "bottom", "shoes"] {
        garments.push(
            app.execute_and_wait(
                "clothing.create",
                json!({"member_id":"default","name":category,"category":category}),
            )
            .unwrap(),
        );
    }
    app.execute_and_wait("wear.record",json!({"member_id":"default","clothing_ids":[garments[0]["id"]],"local_date":"2020-01-01","timezone":"UTC","operation_key":"wear-fixture"})).unwrap();
    app.execute_and_wait("care.record",json!({"member_id":"default","clothing_id":garments[0]["id"],"action":"wash","local_date":"2020-01-02","timezone":"UTC","operation_key":"care-fixture"})).unwrap();
    let intent=app.execute_and_wait("recommendation.generate",json!({"member_id":"default","operation_key":"intent-fixture","timezone":"UTC","avoid_recent":false})).unwrap();
    app.execute_and_wait("recommendation.save",json!({"member_id":"default","id":intent["id"],"proposal_index":0,"name":"saved fixture","operation_key":"save-fixture"})).unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    let db = rusqlite::Connection::open(directory.join("facts.sqlite")).unwrap();
    let raw: String = db
        .query_row(
            &format!("SELECT {column} FROM {table} LIMIT 1"),
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut document: serde_json::Value = serde_json::from_str(&raw).unwrap();
    mutate(&mut document);
    db.execute(
        &format!("UPDATE {table} SET {column}=?1"),
        [document.to_string()],
    )
    .unwrap();
    drop(db);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let bytes = std::fs::read(directory.join("facts.sqlite")).unwrap();
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
    assert_eq!(
        app.execute_and_wait("backup.preview", json!({"directory":directory}))
            .unwrap_err()
            .code,
        "backup_invalid"
    );
}
#[test]
fn preview_domain_wear_root_must_match_sql_row() {
    assert_rejects_corrupted_domain_document("wear_events", "document", |doc| {
        doc["root_event_id"] = json!(uuid::Uuid::now_v7().to_string())
    });
}
#[test]
fn preview_domain_care_revision_must_match_sql_row() {
    assert_rejects_corrupted_domain_document("care_events", "document", |doc| {
        doc["revision"] = json!(99)
    });
}
#[test]
fn preview_domain_outfit_version_must_match_sql_row() {
    assert_rejects_corrupted_domain_document("outfit_versions", "document", |doc| {
        doc["version"] = json!(99)
    });
}
#[test]
fn preview_domain_intent_controls_must_match_persisted_request() {
    assert_rejects_corrupted_domain_document("recommendation_intents", "document", |doc| {
        doc["controls"]["operation_key"] = json!("different-intent-key")
    });
}
#[test]
fn preview_domain_save_receipt_cannot_point_at_another_outfit() {
    assert_rejects_corrupted_domain_document("recommendation_saves", "result", |doc| {
        doc["id"] = json!(uuid::Uuid::now_v7().to_string())
    });
}
#[test]
fn preview_domain_receipt_member_must_match_sql_row() {
    assert_rejects_corrupted_domain_document("wear_receipts", "result", |doc| {
        doc["member_id"] = json!(uuid::Uuid::now_v7().to_string())
    });
}

#[test]
fn restored_old_configuration_and_saved_outfit_receipts_keep_original_revisions() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let mut clothes = Vec::new();
    for category in ["top", "bottom", "shoes"] {
        clothes.push(
            app.execute_and_wait(
                "clothing.create",
                json!({"member_id":"default","name":category,"category":category}),
            )
            .unwrap(),
        );
    }
    let configure = json!({"member_id":"default","clothing_id":clothes[0]["id"],"wash_after_wears":3,"expected_revision":0,"operation_key":"configuration-original"});
    let original = app
        .execute_and_wait("care.configure", configure.clone())
        .unwrap();
    app.execute_and_wait("care.configure",json!({"member_id":"default","clothing_id":clothes[0]["id"],"wash_after_wears":5,"expected_revision":1,"operation_key":"configuration-next"})).unwrap();
    let intent = app
        .execute_and_wait(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"source-intent","timezone":"UTC"}),
        )
        .unwrap();
    let save = json!({"member_id":"default","id":intent["id"],"proposal_index":0,"name":"original saved outfit","operation_key":"original-save"});
    let saved = app
        .execute_and_wait("recommendation.save", save.clone())
        .unwrap();
    let items:Vec<_>=saved["items"].as_array().unwrap().iter().map(|item|json!({"clothing_id":item["clothing_id"],"role":item["role"],"layer":item["layer"]})).collect();
    app.execute_and_wait("outfit.update",json!({"member_id":"default","id":saved["id"],"name":"newer mutable template","expected_version":1,"items":items})).unwrap();
    let directory = archive.path().join("backup");
    app.execute_and_wait("backup.create", json!({"directory":directory}))
        .unwrap();
    drop(app);
    source.close().unwrap();
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    restored
        .execute_and_wait(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true}),
        )
        .unwrap();
    drop(restored);
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        restored
            .execute_and_wait("care.configure", configure)
            .unwrap(),
        original
    );
    assert_eq!(
        restored
            .execute_and_wait("recommendation.save", save)
            .unwrap(),
        saved
    );
    assert_eq!(
        restored
            .execute_and_wait(
                "outfit.get",
                json!({"member_id":"default","id":saved["id"]})
            )
            .unwrap()["version"],
        2
    );
}
