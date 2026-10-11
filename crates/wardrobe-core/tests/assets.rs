use std::path::Path;
use serde_json::json;
use wardrobe_core::{operations, Application};

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
#[test]
fn detaching_an_image_preserves_bytes_and_other_clothing_references() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/generated-colors.png");
    let asset = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    let first = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"衬衫","category":"top"}),
        )
        .unwrap();
    let second = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"外套","category":"outerwear"}),
        )
        .unwrap();
    for item in [&first, &second] {
        app.execute(
            "assets.adopt",
            json!({"member_id":"default","clothing_id":item["id"],"asset_id":asset["id"]}),
        )
        .unwrap();
    }
    let detached = app
        .execute(
            "assets.detach",
            json!({"member_id":"default","clothing_id":first["id"],"asset_id":asset["id"]}),
        )
        .unwrap();
    assert_eq!(detached["asset_ids"], json!([]));
    assert!(detached["display_asset_id"].is_null());
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":"default","id":second["id"]})
        )
        .unwrap()["display_asset_id"],
        asset["id"]
    );
    assert!(app
        .execute(
            "assets.preview",
            json!({"member_id":"default","id":asset["id"]})
        )
        .is_ok());
    app.execute(
        "assets.detach",
        json!({"member_id":"default","clothing_id":second["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    let original = app
        .execute(
            "assets.get",
            json!({"member_id":"default","id":asset["id"]}),
        )
        .unwrap();
    assert_eq!(original["display"]["sync_scope"], "local_only");
    assert_eq!(
        std::fs::read(original["original"]["local_path"].as_str().unwrap()).unwrap(),
        std::fs::read(source).unwrap()
    );
}
#[test]
fn image_capabilities_describe_required_references_and_real_preview_output() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let capabilities = app.execute("capabilities", json!({})).unwrap();
    let operations = capabilities["operations"].as_array().unwrap();
    let reorder = operations
        .iter()
        .find(|operation| operation["name"] == "assets.reorder")
        .unwrap();
    assert!(reorder["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("asset_ids")));
    let preview = operations
        .iter()
        .find(|operation| operation["name"] == "assets.preview")
        .unwrap();
    assert!(preview["outputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("data_url")));
}

// ==== 自 registry_contracts.rs 迁入的运行时/内容契约（issue #70 增量5）====

#[test]
fn assets_effects_and_examples_match_actual_local_file_semantics() {
    let descriptors = operations().as_array().unwrap().clone();
    // sync_manifest 仅 SELECT 重算清单，不落库（root 独立审查）
    let manifest = descriptors
        .iter()
        .find(|d| d["name"] == json!("assets.sync_manifest"))
        .unwrap();
    assert_eq!(manifest["effect"]["kind"], json!("query"));
    // preview 条件性变更：缩略图缺失时同步重建（本地 blob 写 + 文档更新）
    let preview = descriptors
        .iter()
        .find(|d| d["name"] == json!("assets.preview"))
        .unwrap();
    let conditional = preview["effect"]["conditional_effects"]
        .as_array()
        .expect("preview 须声明条件性 effect");
    assert!(conditional.iter().any(|c| {
        c["when"]["variant"] == json!("thumbnail")
            && c["when"]["thumbnail_file_missing"] == json!(true)
            && c["effect"]["kind"] == json!("mutation")
            && c["effect"]["local_files"] == json!(true)
    }));
    let text = preview["examples"][0]["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<String>();
    assert!(
        text.contains("重建"),
        "preview 示例须说明缩略图缺失时同步重建: {text}"
    );
    // 语义前置：导入读本地文件写受管字节；采纳仅压缩展示图参与同步，原图保持本地
    let import = descriptors
        .iter()
        .find(|d| d["name"] == json!("assets.import"))
        .unwrap();
    let text = import["examples"][0]["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<String>();
    assert!(text.contains("本地") && text.contains("受管"), "{text}");
    let adopt = descriptors
        .iter()
        .find(|d| d["name"] == json!("assets.adopt"))
        .unwrap();
    let text = adopt["examples"][0]["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<String>();
    assert!(
        text.contains("原图保持本地") && text.contains("压缩"),
        "{text}"
    );
}

#[test]
fn preview_rebuilds_missing_thumbnail_for_temporary_generated_asset() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("w.sqlite");
    let app = Application::open(&db).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let source = dir.path().join("generated-colors.png");
    std::fs::write(
        &source,
        include_bytes!("../tests/fixtures/generated-colors.png"),
    )
    .unwrap();
    let imported = app
        .execute("assets.import", json!({"member_id":member,"path":source}))
        .unwrap();
    let id = imported["id"].as_str().unwrap().to_string();
    let doc = app
        .execute("assets.get", json!({"member_id":member,"id":id}))
        .unwrap();
    let original_thumbnail = doc["thumbnail"]["local_path"].as_str().unwrap().to_string();
    assert!(Path::new(&original_thumbnail).is_file(), "导入即生成缩略图");
    // 仅删除测试临时受管缩略图，制造缺失条件（不动用户文件）
    std::fs::remove_file(&original_thumbnail).unwrap();
    let preview = app
        .execute(
            "assets.preview",
            json!({"member_id":member,"id":id,"variant":"thumbnail"}),
        )
        .unwrap();
    assert!(preview["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/webp;base64,"));
    let doc = app
        .execute("assets.get", json!({"member_id":member,"id":id}))
        .unwrap();
    let rebuilt = doc["thumbnail"]["local_path"].as_str().unwrap();
    assert!(
        Path::new(rebuilt).is_file(),
        "preview 条件性重建缩略图并持久化"
    );
    // 默认 display 读取语义保持
    let display = app
        .execute(
            "assets.preview",
            json!({"member_id":member,"id":id,"variant":"display"}),
        )
        .unwrap();
    assert!(display["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/webp;base64,"));
}
