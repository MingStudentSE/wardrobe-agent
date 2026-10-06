use serde_json::json;
use wardrobe_core::Application;
#[test]
fn rotation_preserves_original_and_facts_and_restores_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 24, image::Rgb([247, 246, 242]))
        .save(&source)
        .unwrap();
    let database = dir.path().join("db");
    let app = Application::open(&database).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let asset = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"白衬衣","category":"top"}),
        )
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","asset_id":asset["id"],"clothing_id":clothing["id"]}),
    )
    .unwrap();
    let edited = app
        .execute(
            "images.rotate",
            json!({"member_id":"default","asset_id":asset["id"],"degrees":90}),
        )
        .unwrap();
    assert_ne!(edited["id"], asset["id"]);
    assert_eq!(edited["display"]["width"], 24);
    assert_eq!(edited["display"]["height"], 48);
    assert_eq!(edited["original"], asset["original"]);
    assert_eq!(edited["lineage"][0]["action"], "rotate");
    assert_eq!(
        edited["lineage"][0]["source_display_id"],
        asset["display"]["id"]
    );
    assert!(std::path::Path::new(asset["display"]["local_path"].as_str().unwrap()).is_file());
    drop(app);
    let app = Application::open(&database).unwrap();
    let restored = app
        .execute(
            "images.restore",
            json!({"member_id":"default","asset_id":edited["id"]}),
        )
        .unwrap();
    assert_eq!(restored["display"]["width"], 48);
    assert_eq!(restored["display"]["height"], 24);
    let facts = app
        .execute(
            "clothing.get",
            json!({"member_id":"default","id":clothing["id"]}),
        )
        .unwrap();
    assert_eq!(facts["name"], "白衬衣");
    let manifest = app
        .execute("assets.sync_manifest", json!({"member_id":"default"}))
        .unwrap();
    assert!(!manifest.to_string().contains("local_path"));
}
#[test]
fn failed_restore_keeps_source_and_derived_display_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 24, image::Rgb([200, 220, 240]))
        .save(&path)
        .unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let asset = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    let edited = app
        .execute(
            "images.rotate",
            json!({"member_id":"default","asset_id":asset["id"],"degrees":90}),
        )
        .unwrap();
    std::fs::remove_file(asset["original"]["local_path"].as_str().unwrap()).unwrap();
    let err = app
        .execute(
            "images.restore",
            json!({"member_id":"default","asset_id":edited["id"]}),
        )
        .unwrap_err();
    assert_eq!(err.code, "original_unavailable");
    assert_eq!(
        app.execute(
            "assets.get",
            json!({"member_id":"default","id":edited["id"]})
        )
        .unwrap(),
        edited
    );
    assert_eq!(
        app.execute(
            "assets.get",
            json!({"member_id":"default","id":asset["id"]})
        )
        .unwrap(),
        asset
    );
}
#[test]
fn local_cutout_model_missing_status_is_actionable_and_no_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let status = app
        .execute("images.model.status", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(status["state"], "missing");
    assert_eq!(status["model_id"], "u2netp-rembg-v0");
    assert_eq!(status["bytes"], 4574861);
}
#[test]
fn image_merge_and_replace_are_atomic_and_preserve_source_clothing() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 24, image::Rgb([200, 220, 240]))
        .save(&source)
        .unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let a = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    let b = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    let c1 = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"一","category":"top"}),
        )
        .unwrap();
    let c2 = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"二","category":"top"}),
        )
        .unwrap();
    let c1 = app
        .execute(
            "assets.adopt",
            json!({"member_id":"default","asset_id":a["id"],"clothing_id":c1["id"]}),
        )
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","asset_id":b["id"],"clothing_id":c2["id"]}),
    )
    .unwrap();
    let merged=app.execute("images.merge",json!({"member_id":"default","source_clothing_id":c1["id"],"clothing_id":c2["id"],"asset_ids":[a["id"]]})).unwrap();
    assert_eq!(merged["asset_ids"], json!([b["id"], a["id"]]));
    assert_eq!(merged["display_asset_id"], b["id"]);
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":"default","id":c1["id"]}))
            .unwrap(),
        c1
    );
    let edited = app
        .execute(
            "images.rotate",
            json!({"member_id":"default","asset_id":a["id"],"degrees":90}),
        )
        .unwrap();
    let replaced=app.execute("images.replace",json!({"member_id":"default","clothing_id":c2["id"],"asset_id":a["id"],"replacement_id":edited["id"]})).unwrap();
    assert_eq!(replaced["asset_ids"], json!([b["id"], edited["id"]]));
    assert_eq!(
        app.execute("assets.get", json!({"member_id":"default","id":a["id"]}))
            .unwrap()["display"],
        a["display"]
            .clone()
            .as_object()
            .map(|v| {
                let mut v = v.clone();
                v.insert("state".into(), json!("adopted"));
                v.insert("sync_scope".into(), json!("workspace"));
                serde_json::Value::Object(v)
            })
            .unwrap()
    );
}
#[test]
fn cutout_queue_survives_restart_and_missing_model_fails_without_changing_asset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 24, image::Rgb([247, 247, 247]))
        .save(&path)
        .unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let asset = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    let job = app
        .execute(
            "images.cutout",
            json!({"member_id":"default","asset_id":asset["id"],"operation_key":"cutout1"}),
        )
        .unwrap();
    assert_eq!(job["state"], "queued");
    drop(app);
    let app = Application::open(&db).unwrap();
    let failure = app
        .execute(
            "images.worker.run",
            json!({"member_id":"default","job_id":job["id"]}),
        )
        .unwrap();
    assert_eq!(failure["state"], "failed");
    assert_eq!(failure["error"]["code"], "model_missing");
    assert_eq!(
        app.execute(
            "assets.get",
            json!({"member_id":"default","id":asset["id"]})
        )
        .unwrap(),
        asset
    );
}
#[test]
#[ignore = "explicit official model download and public licensed photo inference; run for packaged runtime acceptance"]
fn real_model_download_white_shirt_cutout_and_cached_offline_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let download = app
        .execute(
            "images.model.download",
            json!({"member_id":"default","operation_key":"model-download"}),
        )
        .unwrap();
    let download = app
        .execute(
            "images.worker.run",
            json!({"member_id":"default","job_id":download["id"]}),
        )
        .unwrap();
    assert_eq!(download["state"], "succeeded", "{}", download);
    let source = std::env::var("WARDROBE_CUTOUT_PUBLIC_SAMPLE")
        .expect("set public licensed white shirt sample path");
    let original = std::fs::read(&source).unwrap();
    let asset = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    let job=app.execute("images.cutout",json!({"member_id":"default","asset_id":asset["id"],"operation_key":"white-shirt-cutout"})).unwrap();
    let done = app
        .execute(
            "images.worker.run",
            json!({"member_id":"default","job_id":job["id"]}),
        )
        .unwrap();
    assert_eq!(done["state"], "succeeded", "{}", done);
    let cutout = &done["result"];
    assert_ne!(cutout["id"], asset["id"]);
    assert_eq!(cutout["original"], asset["original"]);
    assert_eq!(
        app.execute(
            "assets.get",
            json!({"member_id":"default","id":asset["id"]})
        )
        .unwrap(),
        asset
    );
    assert_eq!(
        std::fs::read(asset["original"]["local_path"].as_str().unwrap()).unwrap(),
        original
    );
    let pixels = image::open(cutout["display"]["local_path"].as_str().unwrap())
        .unwrap()
        .to_rgba8();
    assert_eq!((pixels.width(), pixels.height()), (1200, 1600));
    assert!(
        pixels.get_pixel(600, 50)[3] < 20,
        "floor above shirt must be transparent"
    );
    assert!(
        pixels.get_pixel(600, 900)[3] > 240,
        "white shirt middle must stay opaque"
    );
    let transparent = pixels.pixels().filter(|p| p[3] < 20).count();
    let opaque = pixels.pixels().filter(|p| p[3] > 240).count();
    assert!(transparent > 500_000 && opaque > 500_000);
    if let Ok(output) = std::env::var("WARDROBE_CUTOUT_EVIDENCE_DIR") {
        std::fs::create_dir_all(&output).unwrap();
        std::fs::copy(
            cutout["display"]["local_path"].as_str().unwrap(),
            std::path::Path::new(&output).join("white-shirt-cutout.webp"),
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&output).join("actual-result.json"),
            serde_json::to_vec_pretty(
                &json!({"result":done,"transparent_pixels":transparent,"opaque_pixels":opaque}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    drop(app);
    let app = Application::open(&db).unwrap();
    assert_eq!(
        app.execute("images.model.status", json!({"member_id":"default"}))
            .unwrap()["cached_offline"],
        true
    );
    assert_eq!(
        app.execute(
            "images.worker.run",
            json!({"member_id":"default","job_id":job["id"]})
        )
        .unwrap(),
        done
    );
    let offline = app
        .execute(
            "images.cutout",
            json!({"member_id":"default","asset_id":asset["id"],"operation_key":"offline-second"}),
        )
        .unwrap();
    let offline = app
        .execute(
            "images.worker.run",
            json!({"member_id":"default","job_id":offline["id"]}),
        )
        .unwrap();
    assert_eq!(offline["state"], "succeeded");
    assert_eq!(offline["result"]["display"]["id"], cutout["display"]["id"]);
}
#[test]
fn twenty_image_limit_and_member_scope_reject_merge_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 24, image::Rgb([120, 190, 210]))
        .save(&path)
        .unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let source = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"来源","category":"top"}),
        )
        .unwrap();
    let target = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"目标","category":"top"}),
        )
        .unwrap();
    let mut source_ids = vec![];
    for n in 0..21 {
        let asset = app
            .execute("assets.import", json!({"member_id":"default","path":path}))
            .unwrap();
        let clothing = if n < 19 { &target } else { &source };
        app.execute(
            "assets.adopt",
            json!({"member_id":"default","asset_id":asset["id"],"clothing_id":clothing["id"]}),
        )
        .unwrap();
        if n >= 19 {
            source_ids.push(asset["id"].clone());
        }
    }
    let before = app
        .execute(
            "clothing.get",
            json!({"member_id":"default","id":target["id"]}),
        )
        .unwrap();
    let error=app.execute("images.merge",json!({"member_id":"default","clothing_id":target["id"],"source_clothing_id":source["id"],"asset_ids":source_ids})).unwrap_err();
    assert_eq!(error.code, "asset_limit");
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":"default","id":target["id"]})
        )
        .unwrap(),
        before
    );
    let other = app
        .execute("member.create", json!({"name":"其他成员"}))
        .unwrap();
    let foreign = app
        .execute(
            "clothing.create",
            json!({"member_id":other["member_id"],"name":"其他衣物","category":"top"}),
        )
        .unwrap();
    let error=app.execute("images.merge",json!({"member_id":"default","clothing_id":target["id"],"source_clothing_id":foreign["id"],"asset_ids":source_ids})).unwrap_err();
    assert_eq!(error.code, "not_found");
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":"default","id":target["id"]})
        )
        .unwrap(),
        before
    );
    let error = app
        .execute(
            "images.main",
            json!({"member_id":"default","clothing_id":target["id"],"asset_id":source_ids[0]}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");
}
