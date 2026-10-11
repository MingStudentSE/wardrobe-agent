use serde_json::{json, Value};
use wardrobe_core::{operations, Application};
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

// ==== 自 registry_contracts.rs 迁入的运行时/内容契约（issue #70 增量5）====

fn assert_result_matches_schema(schema: &Value, result: &Value, context: &str) {
    assert_eq!(
        schema["type"].as_str(),
        Some("object"),
        "{context}: outputSchema 须为 object"
    );
    let props = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{context}: outputSchema 须声明 properties"));
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|r| r.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    let actual = result
        .as_object()
        .unwrap_or_else(|| panic!("{context} 结果须为对象"));
    for (key, value) in actual {
        let prop = props
            .get(key)
            .unwrap_or_else(|| panic!("{context}: 实际结果字段 {key} 未在 outputSchema 声明"));
        if let Some(const_value) = prop.get("const") {
            assert_eq!(value, const_value, "{context}: 字段 {key} 须等于 const");
        }
        if let Some(allowed) = prop["enum"].as_array() {
            assert!(allowed.contains(value), "{context}: 字段 {key} 须为枚举值");
        }
        let declared_types: Vec<String> = match prop.get("type") {
            Some(Value::String(t)) => vec![t.clone()],
            Some(Value::Array(ts)) => ts.iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            _ => vec![],
        };
        let actual_type = match value {
            Value::String(_) => "string",
            Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Bool(_) => "boolean",
            Value::Null => "null",
        };
        assert!(
            declared_types.is_empty()
                || declared_types.iter().any(|t| {
                    t == actual_type
                        || (t == "number" && value.is_number())
                        || (t == "integer" && value.as_f64().is_some_and(|n| n.fract() == 0.0))
                }),
            "{context}: 字段 {key} 声明类型不符"
        );
    }
    for key in required {
        assert!(
            actual.contains_key(key),
            "{context}: 缺少 required 字段 {key}"
        );
    }
}


#[test]
fn images_family_effects_examples_and_runtime_output_contract() {
    // 真实本地图片：rotate/main/restore与model.status运行时输出对照声明schema
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 24, image::Rgb([247, 246, 242]))
        .save(&source)
        .unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let asset = app
        .execute("assets.import", json!({"member_id":member,"path":source}))
        .unwrap();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"白衬衣","category":"top"}),
        )
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":member,"asset_id":asset["id"],"clothing_id":clothing["id"]}),
    )
    .unwrap();

    let descriptors = operations().as_array().unwrap().clone();
    let schema_of = |name: &str| -> Value {
        let d = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        Value::Object(
            d["outputSchema"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} 必须声明 outputSchema"))
                .clone(),
        )
    };

    let status = app
        .execute("images.model.status", json!({"member_id":member}))
        .unwrap();
    assert_result_matches_schema(&schema_of("images.model.status"), &status, "model.status");
    let rotated = app
        .execute(
            "images.rotate",
            json!({"member_id":member,"asset_id":asset["id"],"degrees":90}),
        )
        .unwrap();
    assert_eq!(rotated["lineage"][0]["action"], json!("rotate"));
    assert_result_matches_schema(&schema_of("images.rotate"), &rotated, "rotate");
    let main = app
        .execute(
            "images.main",
            json!({"member_id":member,"clothing_id":clothing["id"],"asset_id":asset["id"]}),
        )
        .unwrap();
    assert_result_matches_schema(&schema_of("images.main"), &main, "main");
    let restored = app
        .execute(
            "images.restore",
            json!({"member_id":member,"asset_id":asset["id"]}),
        )
        .unwrap();
    assert_result_matches_schema(&schema_of("images.restore"), &restored, "restore");
}
