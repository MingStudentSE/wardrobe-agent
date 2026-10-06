use serde_json::json;
use wardrobe_core::Application;

#[test]
fn imported_png_survives_restart_with_a_local_original_and_display() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("white.png");
    let bytes = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9,
        251, 3, 253, 251, 94, 107, 43, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    std::fs::write(&source, bytes).unwrap();
    let database = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&database).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let imported = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    assert_eq!(imported["display"]["width"], 1);
    assert_eq!(imported["original"]["sync_scope"], "local_only");
    assert_eq!(
        std::fs::read(imported["original"]["local_path"].as_str().unwrap()).unwrap(),
        bytes
    );
    drop(app);
    let app = Application::open(&database).unwrap();
    let read = app
        .execute(
            "assets.get",
            json!({"member_id":"default","id":imported["id"]}),
        )
        .unwrap();
    assert_eq!(read["display"], imported["display"]);
}

#[test]
fn large_jpeg_shrinks_and_thumbnail_can_be_rebuilt_without_original() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.jpg");
    let pixels = image::RgbImage::from_pixel(2400, 1200, image::Rgb([250, 249, 245]));
    pixels.save(&source).unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let imported = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    assert_eq!(imported["display"]["width"], 1600);
    assert_eq!(imported["display"]["height"], 800);
    std::fs::remove_file(imported["original"]["local_path"].as_str().unwrap()).unwrap();
    std::fs::remove_file(imported["thumbnail"]["local_path"].as_str().unwrap()).unwrap();
    let rebuilt = app
        .execute(
            "assets.rebuild_thumbnail",
            json!({"member_id":"default","id":imported["id"]}),
        )
        .unwrap();
    assert_eq!(rebuilt["thumbnail"]["width"], 320);
    let preview = app
        .execute(
            "assets.preview",
            json!({"member_id":"default","id":imported["id"],"variant":"thumbnail"}),
        )
        .unwrap();
    assert!(preview["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/webp;base64,"));
}

#[test]
fn malformed_input_and_overlarge_file_do_not_become_display_assets() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bad.jpg");
    std::fs::write(&source, b"not a photo").unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    assert_eq!(
        app.execute(
            "assets.import",
            json!({"member_id":"default","path":source})
        )
        .unwrap_err()
        .code,
        "image_decode_failed"
    );
    assert!(!dir.path().join("assets/display").exists());
    assert_eq!(std::fs::read(&source).unwrap(), b"not a photo");
    let file = std::fs::File::create(&source).unwrap();
    file.set_len(50 * 1024 * 1024 + 1).unwrap();
    assert_eq!(
        app.execute(
            "assets.import",
            json!({"member_id":"default","path":source})
        )
        .unwrap_err()
        .code,
        "image_too_large"
    );
}

#[test]
fn adopted_display_manifest_excludes_original_and_preview_assets() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(16, 8, image::Rgb([250, 250, 250]))
        .save(&source)
        .unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"白衣","category":"top"}),
        )
        .unwrap();
    let imported = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    assert_eq!(
        app.execute("assets.sync_manifest", json!({"member_id":"default"}))
            .unwrap()["assets"],
        json!([])
    );
    let adopted = app
        .execute(
            "assets.adopt",
            json!({"member_id":"default","asset_id":imported["id"],"clothing_id":clothing["id"]}),
        )
        .unwrap();
    assert_eq!(adopted["display_asset_id"], imported["id"]);
    let manifest = app
        .execute("assets.sync_manifest", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(manifest["assets"].as_array().unwrap().len(), 1);
    assert!(manifest.to_string().find("local_path").is_none());
    assert!(manifest
        .to_string()
        .find(imported["original"]["id"].as_str().unwrap())
        .is_none());
}

#[test]
fn heic_decodes_real_container_without_external_model_and_preserves_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("fixture.heic");
    let bytes = include_bytes!("fixtures/generated-colors.heic");
    std::fs::write(&source, bytes).unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let imported = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    assert_eq!(
        (
            imported["display"]["width"].as_u64(),
            imported["display"]["height"].as_u64()
        ),
        (Some(64), Some(32))
    );
    assert_eq!(
        std::fs::read(imported["original"]["local_path"].as_str().unwrap()).unwrap(),
        bytes
    );
}

#[test]
fn jpeg_exif_rotation_is_applied_once_before_display_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("rotated.jpg");
    image::RgbImage::from_pixel(80, 40, image::Rgb([230, 20, 40]))
        .save(&source)
        .unwrap();
    let encoded = std::fs::read(&source).unwrap();
    // A literal EXIF TIFF orientation=6 (90 degrees clockwise), not a filename hint.
    let exif = [
        0xff, 0xe1, 0, 34, b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12,
        1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut bytes = encoded[..2].to_vec();
    bytes.extend(exif);
    bytes.extend(&encoded[2..]);
    std::fs::write(&source, &bytes).unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let imported = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    assert_eq!(
        (
            imported["display"]["width"].as_u64(),
            imported["display"]["height"].as_u64()
        ),
        (Some(40), Some(80))
    );
    let output = std::fs::read(imported["display"]["local_path"].as_str().unwrap()).unwrap();
    assert!(!output.windows(4).any(|v| v == b"Exif"));
}

#[test]
fn header_over_sixty_megapixels_is_rejected_before_pixel_decode() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("too-large.png");
    std::fs::write(&source, include_bytes!("fixtures/over-limit-header.png")).unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    assert_eq!(
        app.execute(
            "assets.import",
            json!({"member_id":"default","path":source})
        )
        .unwrap_err()
        .code,
        "image_dimensions_exceeded"
    );
}
