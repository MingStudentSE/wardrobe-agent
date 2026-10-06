use serde_json::json;
use wardrobe_core::Application;
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(app) = Application::open(&self.0) {
            let _ = app.execute("encryption.unpair", json!({}));
        }
    }
}
#[test]
#[ignore = "requires real OS credential store"]
fn paired_installation_reads_application_generated_ciphertext_package_without_original_or_local_secrets(
) {
    let owner_dir = tempfile::tempdir().unwrap();
    let peer_dir = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let owner_path = owner_dir.path().join("wardrobe.sqlite");
    let peer_path = peer_dir.path().join("wardrobe.sqlite");
    let owner = Application::open(&owner_path).unwrap();
    let peer = Application::open(&peer_path).unwrap();
    let _owner = Cleanup(owner_path.clone());
    let _peer = Cleanup(peer_path);
    owner
        .execute(
            "encryption.create",
            json!({"password":"temporary export owner password","device_name":"export owner"}),
        )
        .unwrap();
    let offer = owner
        .execute(
            "encryption.pair.offer",
            json!({"device_name":"export peer"}),
        )
        .unwrap();
    let accept=peer.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"temporary export peer password"})).unwrap();
    let confirmation = owner
        .execute(
            "encryption.pair.confirm",
            json!({"receipt":accept["receipt"]}),
        )
        .unwrap();
    peer.execute(
        "encryption.device.apply",
        json!({"manifest":confirmation["manifest"]}),
    )
    .unwrap();
    let clothing=owner.execute("clothing.create",json!({"member_id":"default","name":"Private export sweater fact","category":"top","notes":"Private export purchase note"})).unwrap();
    let source = owner_dir.path().join("private-camera-source.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([40, 50, 60]))
        .save(&source)
        .unwrap();
    let original = std::fs::read(&source).unwrap();
    let asset = owner
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    owner
        .execute(
            "assets.adopt",
            json!({"member_id":"default","clothing_id":clothing["id"],"asset_id":asset["id"]}),
        )
        .unwrap();
    // An imported but unadopted preview must stay local even when its bytes match.
    let private_preview = owner
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    let outfit=owner.execute("outfit.create",json!({"member_id":"default","name":"History look","items":[{"clothing_id":clothing["id"],"role":"top","layer":0}]})).unwrap();
    owner.execute("wear.record",json!({"member_id":"default","operation_key":"history","local_date":"2026-10-01","timezone":"Asia/Shanghai","source_outfit_id":outfit["id"],"source_version":1})).unwrap();
    owner.execute("care.configure",json!({"member_id":"default","clothing_id":clothing["id"],"wash_after_wears":3,"expected_revision":0,"operation_key":"export threshold"})).unwrap();
    let care=owner.execute("care.record",json!({"member_id":"default","clothing_id":clothing["id"],"action":"wash","local_date":"2026-10-01","timezone":"Asia/Shanghai","operation_key":"export wash"})).unwrap();
    let care=owner.execute("care.correct",json!({"member_id":"default","root_event_id":care["root_event_id"],"expected_revision":1,"action":"dry_clean","local_date":"2026-10-01","timezone":"Asia/Shanghai","operation_key":"export correction"})).unwrap();
    owner.execute("care.revoke",json!({"member_id":"default","root_event_id":care["root_event_id"],"expected_revision":2,"reason":"historical correction","operation_key":"export revoke"})).unwrap();
    owner
        .execute(
            "assets.detach",
            json!({"member_id":"default","clothing_id":clothing["id"],"asset_id":asset["id"]}),
        )
        .unwrap();
    let preferences = owner
        .execute(
            "insights.configure",
            json!({"member_id":"default","stale_days":89,"expected_revision":0}),
        )
        .unwrap();
    let exported = owner
        .execute(
            "encryption.package.export",
            json!({"directory":external.path()}),
        )
        .unwrap();
    let path = std::path::Path::new(exported["path"].as_str().unwrap());
    assert!(path.starts_with(external.path().canonicalize().unwrap()));
    let mut files = 0;
    for entry in std::fs::read_dir(path).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        files += 1;
        for private in [
            b"Private export sweater fact".as_slice(),
            b"Private export purchase note".as_slice(),
            owner_dir.path().to_str().unwrap().as_bytes(),
            original.as_slice(),
        ] {
            assert!(
                !bytes.windows(private.len()).any(|w| w == private),
                "external object leaked plaintext/local-only source"
            );
        }
    }
    assert!(files >= 3);
    let result = peer
        .execute("encryption.package.read", json!({"path":path}))
        .unwrap();
    assert_eq!(
        result["snapshot"]["insights_preferences"][0]["stale_days"],
        89
    );
    assert_eq!(
        result["snapshot"]["insights_preferences"][0]["id"],
        preferences["id"]
    );
    assert!(result["snapshot"].get("frequency").is_none());
    let peer_summary = peer
        .execute(
            "insights.summary",
            json!({"member_id":"default","as_of_date":"2026-10-01","timezone":"Asia/Shanghai"}),
        )
        .unwrap();
    assert_eq!(peer_summary["preferences"]["stale_days"], 90);
    assert_eq!(peer_summary["preferences"]["revision"], 0);
    assert_eq!(result["snapshot"]["care_config"][0]["wash_after_wears"], 3);
    assert_eq!(
        result["snapshot"]["care_events"].as_array().unwrap().len(),
        3
    );
    assert_eq!(
        result["snapshot"]["care_events"][0]["snapshot"]["name"],
        "Private export sweater fact"
    );
    assert_eq!(
        result["snapshot"]["care_events"][0]["snapshot"]["display_asset_id"],
        asset["id"]
    );
    assert_eq!(
        result["snapshot"]["care_asset_refs"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(result["snapshot"].get("dirty").is_none());
    assert!(result["snapshot"].get("care_receipts").is_none());
    assert_eq!(
        result["snapshot"]["clothing"][0]["name"],
        "Private export sweater fact"
    );
    assert_eq!(
        result["snapshot"]["outfit_versions"][0]["items"][0]["snapshot"]["display_asset_id"],
        asset["id"]
    );
    assert_eq!(
        result["snapshot"]["wear_events"][0]["source_outfit"]["items"][0]["snapshot"]
            ["display_asset_id"],
        asset["id"]
    );
    assert_eq!(result["assets"].as_array().unwrap().len(), 1);
    assert_eq!(result["assets"][0]["id"], asset["id"]);
    assert!(result["assets"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["id"] != private_preview["id"]));
    assert_eq!(result["assets"][0]["role"], "product_display");
    assert!(result.to_string().find("private-camera-source").is_none());
    assert!(result.to_string().find("local_path").is_none());
    // Authenticated file inventory rejects modification and incomplete packages.
    let object = std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap() != "package.json")
        .unwrap();
    let saved = std::fs::read(&object).unwrap();
    std::fs::write(&object, b"corrupted external file").unwrap();
    // A corrupt retained display fails export; it never falls back to the intact original.
    let display = std::path::Path::new(asset["display"]["local_path"].as_str().unwrap());
    let saved_display = std::fs::read(display).unwrap();
    std::fs::write(display, b"corrupt retained display").unwrap();
    let existing_packages = std::fs::read_dir(external.path()).unwrap().count();
    assert!(owner
        .execute(
            "encryption.package.export",
            json!({"directory":external.path()})
        )
        .is_err());
    assert_eq!(
        std::fs::read_dir(external.path()).unwrap().count(),
        existing_packages
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    std::fs::write(display, saved_display).unwrap();
    assert!(peer
        .execute("encryption.package.read", json!({"path":path}))
        .is_err());
    std::fs::OpenOptions::new()
        .write(true)
        .open(&object)
        .unwrap()
        .set_len(25 * 1024 * 1024)
        .unwrap();
    assert!(peer
        .execute("encryption.package.read", json!({"path":path}))
        .is_err());
    std::fs::write(&object, &saved).unwrap();
    #[cfg(unix)]
    {
        let alias = external.path().join("package-link");
        std::os::unix::fs::symlink(path, &alias).unwrap();
        assert!(peer
            .execute("encryption.package.read", json!({"path":alias}))
            .is_err());
        std::fs::remove_file(alias).unwrap();
    }
    let header_path = path.join("package.json");
    let saved_header = std::fs::read(&header_path).unwrap();
    let mut header: serde_json::Value = serde_json::from_slice(&saved_header).unwrap();
    header["manifest"]["signature"] = json!("AAAAAAAA");
    std::fs::write(&header_path, serde_json::to_vec(&header).unwrap()).unwrap();
    assert!(peer
        .execute("encryption.package.read", json!({"path":path}))
        .is_err());
    std::fs::write(&header_path, saved_header).unwrap();
    // Offline peer cannot learn a revocation it has not received. After applying
    // current owner-signed state it rejects all package reads on that installation.
    let status = peer.execute("encryption.status", json!({})).unwrap();
    let revoked=owner.execute("encryption.device.revoke",json!({"device_id":status["state"]["device_id"],"password":"new temporary export password"})).unwrap();
    assert!(peer
        .execute("encryption.package.read", json!({"path":path}))
        .is_ok());
    assert!(owner
        .execute("encryption.package.read", json!({"path":path}))
        .is_err());
    assert_eq!(
        peer.execute(
            "encryption.device.apply",
            json!({"manifest":revoked["manifest"]})
        )
        .unwrap_err()
        .code,
        "device_revoked"
    );
    assert_eq!(
        peer.execute("encryption.package.read", json!({"path":path}))
            .unwrap_err()
            .code,
        "device_revoked"
    );
    assert!(peer
        .execute("clothing.list", json!({"member_id":"default"}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn full_external_client_cannot_export_or_read_local_encryption_packages() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("wardrobe.sqlite")).unwrap();
    let client=app.execute("client.authorize",json!({"name":"package client","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    for operation in ["encryption.package.export", "encryption.package.read"] {
        assert_eq!(
            app.execute_as_client(
                client["token"].as_str().unwrap(),
                operation,
                json!({"member_id":"default","directory":dir.path(),"path":dir.path()})
            )
            .unwrap_err()
            .code,
            "scope_denied"
        );
    }
}

#[test]
#[ignore = "requires real OS credential store"]
fn oversized_real_fact_snapshot_fails_before_publishing_and_preserves_local_facts() {
    let dir = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&db).unwrap();
    let _cleanup = Cleanup(db);
    app.execute(
        "encryption.create",
        json!({"password":"temporary oversized package password","device_name":"oversized export"}),
    )
    .unwrap();
    let notes = "x".repeat(10000);
    for i in 0..1700 {
        app.execute("clothing.create",json!({"member_id":"default","name":format!("Preserved fact {i}"),"category":"top","notes":notes})).unwrap();
    }
    assert_eq!(
        app.execute(
            "encryption.package.export",
            json!({"directory":external.path()})
        )
        .unwrap_err()
        .code,
        "package_too_large"
    );
    assert_eq!(std::fs::read_dir(external.path()).unwrap().count(), 0);
    assert!(!app
        .execute("clothing.list", json!({"member_id":"default"}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
#[ignore = "requires real OS credential store"]
fn secondary_display_referenced_by_immutable_outfit_version_remains_in_encrypted_package() {
    let dir = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&db).unwrap();
    let _cleanup = Cleanup(db);
    app.execute("encryption.create",json!({"password":"temporary historical package password","device_name":"historical export"})).unwrap();
    let garment = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"Two retained displays","category":"top"}),
        )
        .unwrap();
    let mut assets = vec![];
    for color in [40u8, 80u8] {
        let source = dir.path().join(format!("camera-{color}.png"));
        image::RgbImage::from_pixel(32, 32, image::Rgb([color, 50, 60]))
            .save(&source)
            .unwrap();
        let asset = app
            .execute(
                "assets.import",
                json!({"member_id":"default","path":source}),
            )
            .unwrap();
        app.execute(
            "assets.adopt",
            json!({"member_id":"default","clothing_id":garment["id"],"asset_id":asset["id"]}),
        )
        .unwrap();
        assets.push(asset);
    }
    app.execute("outfit.create",json!({"member_id":"default","name":"Immutable all-image references","items":[{"clothing_id":garment["id"],"role":"top","layer":0}]})).unwrap();
    app.execute(
        "assets.detach",
        json!({"member_id":"default","clothing_id":garment["id"],"asset_id":assets[0]["id"]}),
    )
    .unwrap();
    assert_eq!(
        app.execute(
            "assets.get",
            json!({"member_id":"default","id":assets[0]["id"]})
        )
        .unwrap()["display"]["state"],
        "preview"
    );
    let exported = app
        .execute(
            "encryption.package.export",
            json!({"directory":external.path()}),
        )
        .unwrap();
    assert_eq!(exported["asset_count"], 2);
    let inspected = app
        .execute("encryption.package.read", json!({"path":exported["path"]}))
        .unwrap();
    assert_eq!(
        inspected["snapshot"]["outfit_versions"][0]["items"][0]["snapshot"]["asset_ids"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
