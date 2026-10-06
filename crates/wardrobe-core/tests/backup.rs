use serde_json::json;
use wardrobe_core::Application;
#[test]
fn portable_backup_preview_restore_preserves_actual_history_after_restart() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let garment = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"保留的衬衫","category":"top"}),
        )
        .unwrap();
    let wear = app.execute("wear.record",json!({"member_id":"default","clothing_ids":[garment["id"]],"local_date":"2020-01-01","timezone":"UTC","operation_key":"wear"})).unwrap();
    app.execute("care.record",json!({"member_id":"default","clothing_id":garment["id"],"action":"wash","local_date":"2020-01-02","timezone":"UTC","operation_key":"wash"})).unwrap();
    let package = archive.path().join("complete-backup");
    let backup = app
        .execute(
            "backup.create",
            json!({"directory":package,"include_originals":false}),
        )
        .unwrap();
    assert_eq!(backup["includes_originals"], false);
    let preview = app
        .execute("backup.preview", json!({"directory":package}))
        .unwrap();
    assert_eq!(preview["valid"], true);
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    restored
        .execute(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .unwrap();
    drop(restored);
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        restored
            .execute(
                "clothing.get",
                json!({"member_id":"default","id":garment["id"]})
            )
            .unwrap(),
        garment
    );
    assert_eq!(
        restored
            .execute("wear.get", json!({"member_id":"default","id":wear["id"]}))
            .unwrap(),
        wear
    );
    assert_eq!(
        restored
            .execute(
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
        .execute(
            "assets.import",
            json!({"member_id":"default","path":picture}),
        )
        .unwrap();
    let preview = app
        .execute(
            "assets.preview",
            json!({"member_id":"default","id":asset["id"]}),
        )
        .unwrap();
    let directory = archives.path().join("backup");
    app.execute(
        "backup.create",
        json!({"directory":directory,"include_originals":false}),
    )
    .unwrap();
    drop(app);
    source.close().unwrap();
    let dest = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    dest.execute(
        "backup.restore",
        json!({"directory":directory,"confirm_overwrite":true}),
    )
    .unwrap();
    assert_eq!(
        dest.execute(
            "assets.preview",
            json!({"member_id":"default","id":asset["id"]})
        )
        .unwrap(),
        preview
    );
    let retained = dest
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"覆盖失败必须保留","category":"top"}),
        )
        .unwrap();
    std::fs::write(directory.join("facts.sqlite"), b"truncated").unwrap();
    assert!(dest
        .execute(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true})
        )
        .is_err());
    assert_eq!(
        dest.execute(
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
    let grant=app.execute("client.authorize",json!({"name":canary,"mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap();
    let hash: String = Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    app.execute("provider.save",json!({"name":"PRIVATE_PROVIDER_CANARY_248723","endpoint":"http://127.0.0.1:11434/v1","model":"fixture","capabilities":["text"],"authentication":"none"})).unwrap();
    let directory = archive.path().join("backup");
    app.execute(
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
        .execute(
            "backup.restore",
            json!({"directory":directory,"confirm_overwrite":true}),
        )
        .unwrap();
    assert_eq!(
        restored.execute("client.list", json!({})).unwrap()["items"],
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
    app.execute(
        "clothing.create",
        json!({"member_id":"default","name":"待恢复","category":"top"}),
    )
    .unwrap();
    let package = archive.path().join("backup");
    app.execute("backup.create", json!({"directory":package}))
        .unwrap();
    let dest = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    let original = dest
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"当前旧库","category":"dress"}),
        )
        .unwrap();
    let preview = dest
        .execute("backup.preview", json!({"directory":package}))
        .unwrap();
    assert_eq!(preview["incoming"]["clothing"], 1);
    assert_eq!(preview["current"]["clothing"], 1);
    assert_eq!(
        dest.execute("backup.restore", json!({"directory":package}))
            .unwrap_err()
            .code,
        "confirmation_required"
    );
    let receipt = dest
        .execute(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .unwrap();
    assert_eq!(
        dest.execute(
            "clothing.get",
            json!({"member_id":"default","id":original["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    dest.execute(
        "backup.restore",
        json!({"directory":receipt["rollback_backup"],"confirm_overwrite":true}),
    )
    .unwrap();
    drop(dest);
    let dest = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        dest.execute(
            "clothing.get",
            json!({"member_id":"default","id":original["id"]})
        )
        .unwrap(),
        original
    );
}
