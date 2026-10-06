use serde_json::json;
use wardrobe_core::Application;
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(a) = Application::open(&self.0) {
            let _ = a.execute("encryption.unpair", json!({}));
        }
    }
}
#[test]
#[ignore = "requires actual OS credential store"]
fn two_installations_exchange_member_and_clothing_incrementally_without_duplicate_facts() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let ap = a.path().join("wardrobe.sqlite");
    let bp = b.path().join("wardrobe.sqlite");
    let owner = Application::open(&ap).unwrap();
    let peer = Application::open(&bp).unwrap();
    let _a = Cleanup(ap.clone());
    let _b = Cleanup(bp.clone());
    let existing = owner
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"启用同步之前的外套","category":"outerwear"}),
        )
        .unwrap();
    let created = owner
        .execute(
            "encryption.create",
            json!({"password":"temporary sync owner password","device_name":"sync owner"}),
        )
        .unwrap();
    let offer = owner
        .execute("encryption.pair.offer", json!({"device_name":"sync peer"}))
        .unwrap();
    let receipt=peer.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"temporary sync peer password"})).unwrap();
    let confirmation = owner
        .execute(
            "encryption.pair.confirm",
            json!({"receipt":receipt["receipt"]}),
        )
        .unwrap();
    peer.execute(
        "encryption.device.apply",
        json!({"manifest":confirmation["manifest"]}),
    )
    .unwrap();
    let member = owner
        .execute("member.create", json!({"name":"同步家人"}))
        .unwrap();
    let garment = owner
        .execute(
            "clothing.create",
            json!({"member_id":member["member_id"],"name":"同步蓝衬衫","category":"top"}),
        )
        .unwrap();
    let source = a.path().join("private-local-original.png");
    let mut image = image::RgbImage::new(1200, 1200);
    let mut seed = 42u32;
    for pixel in image.pixels_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *pixel = image::Rgb([(seed >> 24) as u8, (seed >> 16) as u8, (seed >> 8) as u8]);
    }
    image.save(&source).unwrap();
    let imported = owner
        .execute(
            "assets.import",
            json!({"member_id":member["member_id"],"path":source}),
        )
        .unwrap();
    owner.execute("assets.adopt",json!({"member_id":member["member_id"],"clothing_id":garment["id"],"asset_id":imported["id"]})).unwrap();
    let look=owner.execute("outfit.create",json!({"member_id":member["member_id"],"name":"跨设备搭配历史","items":[{"clothing_id":garment["id"],"role":"top","layer":0}]})).unwrap();
    let wear=owner.execute("wear.record",json!({"member_id":member["member_id"],"source_outfit_id":look["id"],"source_version":1,"local_date":"2026-10-01","timezone":"Asia/Shanghai","operation_key":"sync wear"})).unwrap();
    let wear=owner.execute("wear.correct",json!({"member_id":member["member_id"],"root_event_id":wear["root_event_id"],"expected_revision":1,"clothing_ids":[garment["id"]],"local_date":"2026-10-02","timezone":"Asia/Shanghai","operation_key":"sync correction"})).unwrap();
    owner.execute("wear.revoke",json!({"member_id":member["member_id"],"root_event_id":wear["root_event_id"],"expected_revision":2,"reason":"实际没有穿","operation_key":"sync revocation"})).unwrap();
    owner.execute("care.configure",json!({"member_id":member["member_id"],"clothing_id":garment["id"],"wash_after_wears":3,"expected_revision":0,"operation_key":"sync care config"})).unwrap();
    let care=owner.execute("care.record",json!({"member_id":member["member_id"],"clothing_id":garment["id"],"action":"wash","local_date":"2026-10-01","timezone":"Asia/Shanghai","operation_key":"sync care"})).unwrap();
    owner.execute("care.revoke",json!({"member_id":member["member_id"],"root_event_id":care["root_event_id"],"expected_revision":1,"reason":"记错了","operation_key":"sync care revoke"})).unwrap();
    owner
        .execute(
            "insights.configure",
            json!({"member_id":member["member_id"],"stale_days":89,"expected_revision":0}),
        )
        .unwrap();
    for app in [&owner, &peer] {
        app.execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote.path()}),
        )
        .unwrap();
    }
    owner.execute("sync.run", json!({})).unwrap();
    fn chunk(path: &std::path::Path) -> Option<std::path::PathBuf> {
        for entry in std::fs::read_dir(path).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                if let Some(found) = chunk(&p) {
                    return Some(found);
                }
            } else if p.extension().is_some_and(|s| s == "object")
                && p.parent().unwrap().file_name().unwrap() == "blobs"
            {
                return Some(p);
            }
        }
        None
    }
    let chunk_path = chunk(remote.path()).unwrap();
    let backup = a.path().join("interrupted-download-block");
    std::fs::rename(&chunk_path, &backup).unwrap();
    let interrupted = peer.execute("sync.run", json!({})).unwrap();
    assert_eq!(interrupted["missing_assets"], 1);
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":garment["id"]})
        )
        .unwrap()["name"],
        "同步蓝衬衫"
    );
    let placeholder = peer
        .execute(
            "assets.get",
            json!({"member_id":member["member_id"],"id":imported["id"]}),
        )
        .unwrap();
    assert_eq!(placeholder["display"]["pending"], true);
    let missing_wear=peer.execute("wear.record",json!({"member_id":member["member_id"],"clothing_ids":[garment["id"]],"local_date":"2026-10-03","timezone":"Asia/Shanghai","operation_key":"wear without original or display"})).unwrap();
    assert!(missing_wear["id"].is_string());
    std::fs::rename(&backup, &chunk_path).unwrap();
    let received = peer.execute("sync.run", json!({})).unwrap();
    assert_eq!(
        received["missing_assets"],
        0,
        "{}",
        peer.execute("sync.status", json!({})).unwrap()
    );
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":existing["member_id"],"id":existing["id"]})
        )
        .unwrap()["name"],
        "启用同步之前的外套"
    );
    assert_eq!(
        peer.execute(
            "outfit.version",
            json!({"member_id":member["member_id"],"id":look["id"],"version":1})
        )
        .unwrap()["name"],
        "跨设备搭配历史"
    );
    assert_eq!(
        peer.execute(
            "wear.history",
            json!({"member_id":member["member_id"],"root_event_id":wear["root_event_id"]})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        peer.execute(
            "care.history",
            json!({"member_id":member["member_id"],"root_event_id":care["root_event_id"]})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(peer.execute("insights.summary",json!({"member_id":member["member_id"],"as_of_date":"2026-10-03","timezone":"Asia/Shanghai"})).unwrap()["preferences"]["stale_days"],89);
    assert_eq!(
        peer.execute(
            "care.status",
            json!({"member_id":member["member_id"],"clothing_id":garment["id"]})
        )
        .unwrap()["wash_after_wears"],
        3
    );
    let remote_asset = peer
        .execute(
            "assets.get",
            json!({"member_id":member["member_id"],"id":imported["id"]}),
        )
        .unwrap();
    assert!(remote_asset["original"].is_null());
    let preview = peer
        .execute(
            "assets.preview",
            json!({"member_id":member["member_id"],"id":imported["id"],"variant":"thumbnail"}),
        )
        .unwrap();
    assert!(preview["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/webp;base64,"));
    let source_original = std::fs::read(&source).unwrap();
    fn scan(path: &std::path::Path, original: &[u8]) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                scan(&path, original);
            } else {
                let bytes = std::fs::read(path).unwrap();
                assert!(!bytes
                    .windows(b"private-local-original".len())
                    .any(|v| v == b"private-local-original"));
                assert!(!bytes.windows(original.len()).any(|v| v == original));
            }
        }
    }
    scan(remote.path(), &source_original);
    let got = peer
        .execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":garment["id"]}),
        )
        .unwrap();
    assert_eq!(got["name"], "同步蓝衬衫");
    peer.execute("sync.run", json!({})).unwrap();
    assert_eq!(
        peer.execute("clothing.list", json!({"member_id":member["member_id"]}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    peer.execute(
        "clothing.update",
        json!({"member_id":member["member_id"],"id":garment["id"],"name":"第二设备可编辑"}),
    )
    .unwrap();
    peer.execute("sync.run", json!({})).unwrap();
    owner.execute("sync.run", json!({})).unwrap();
    assert_eq!(
        owner
            .execute(
                "clothing.get",
                json!({"member_id":member["member_id"],"id":garment["id"]})
            )
            .unwrap()["name"],
        "第二设备可编辑"
    );
    owner.execute("clothing.update",json!({"member_id":member["member_id"],"id":garment["id"],"name":"源端待审核名称","tags":["owner tag"],"color":["blue"]})).unwrap();
    peer.execute("clothing.update",json!({"member_id":member["member_id"],"id":garment["id"],"name":"第二设备待审核名称","tags":["peer tag"],"size_label":"M"})).unwrap();
    for app in [&owner, &peer, &owner, &peer] {
        app.execute("sync.run", json!({})).unwrap();
    }
    for app in [&owner, &peer] {
        let doc = app
            .execute(
                "clothing.get",
                json!({"member_id":member["member_id"],"id":garment["id"]}),
            )
            .unwrap();
        assert!(doc["tags"]
            .as_array()
            .unwrap()
            .contains(&json!("owner tag")));
        assert!(doc["tags"].as_array().unwrap().contains(&json!("peer tag")));
        assert_eq!(doc["color"], json!(["blue"]));
        assert_eq!(doc["size_label"], "M");
    }
    let conflicts = owner
        .execute("sync.conflicts", json!({"member_id":member["member_id"]}))
        .unwrap();
    let conflict = conflicts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["entity_id"] == garment["id"] && v["field"] == "name")
        .unwrap();
    assert_eq!(conflict["variants"].as_array().unwrap().len(), 2);
    let chosen = owner
        .execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":garment["id"]}),
        )
        .unwrap()["name"]
        .clone();
    owner.execute("sync.resolve",json!({"member_id":member["member_id"],"entity_type":"clothing","entity_id":garment["id"],"field":"name","value":chosen,"expected_token":conflict["token"],"operation_key":"choose shared name"})).unwrap();
    owner.execute("sync.run", json!({})).unwrap();
    peer.execute("sync.run", json!({})).unwrap();
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":garment["id"]})
        )
        .unwrap()["name"],
        chosen
    );
    assert!(peer
        .execute("sync.conflicts", json!({"member_id":member["member_id"]}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["entity_id"] != garment["id"] || v["field"] != "name"));
    let root=owner.execute("wear.record",json!({"member_id":member["member_id"],"clothing_ids":[garment["id"]],"local_date":"2026-10-03","timezone":"Asia/Shanghai","operation_key":"concurrent root"})).unwrap();
    owner.execute("sync.run", json!({})).unwrap();
    peer.execute("sync.run", json!({})).unwrap();
    let care_root=owner.execute("care.record",json!({"member_id":member["member_id"],"clothing_id":garment["id"],"action":"wash","local_date":"2026-10-03","timezone":"Asia/Shanghai","operation_key":"concurrent care root"})).unwrap();
    owner.execute("sync.run", json!({})).unwrap();
    peer.execute("sync.run", json!({})).unwrap();
    let mut care_branches = vec![];
    let mut branches = vec![];
    for (app, date, key) in [
        (&owner, "2026-10-04", "owner correction"),
        (&peer, "2026-10-05", "peer correction"),
    ] {
        branches.push(app.execute("wear.correct",json!({"member_id":member["member_id"],"root_event_id":root["root_event_id"],"expected_revision":1,"clothing_ids":[garment["id"]],"local_date":date,"timezone":"Asia/Shanghai","operation_key":key})).unwrap());
        care_branches.push(app.execute("care.correct",json!({"member_id":member["member_id"],"root_event_id":care_root["root_event_id"],"expected_revision":1,"action":"wash","local_date":date,"timezone":"Asia/Shanghai","operation_key":format!("care {key}")})).unwrap());
        app.execute("outfit.update",json!({"member_id":member["member_id"],"id":look["id"],"expected_version":1,"name":key,"items":[{"clothing_id":garment["id"],"role":"top","layer":0}]})).unwrap();
    }
    for app in [&owner, &peer, &owner, &peer] {
        app.execute("sync.run", json!({})).unwrap();
    }
    for app in [&owner, &peer] {
        let history = app
            .execute(
                "wear.history",
                json!({"member_id":member["member_id"],"root_event_id":root["root_event_id"]}),
            )
            .unwrap();
        assert_eq!(history["items"].as_array().unwrap().len(), 3);
        for branch in &branches {
            assert!(history["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["id"] == branch["id"] && v["local_date"] == branch["local_date"]));
        }
        let versions=app.execute("sync.history",json!({"member_id":member["member_id"],"entity_type":"outfit_versions","entity_id":format!("{}:2",look["id"].as_str().unwrap())})).unwrap();
        assert_eq!(versions["items"].as_array().unwrap().len(), 2);
    }
    let branch_conflicts = owner
        .execute("sync.conflicts", json!({"member_id":member["member_id"]}))
        .unwrap();
    for (table, entity) in [
        ("wear_events", root["root_event_id"].as_str().unwrap()),
        ("care_events", care_root["root_event_id"].as_str().unwrap()),
        ("outfit_versions", look["id"].as_str().unwrap()),
    ] {
        let conflict = branch_conflicts["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| {
                v["entity_type"] == table && v["entity_id"] == entity && v["field"] == "branch"
            })
            .expect("immutable branches require explicit review");
        let selected = &conflict["variants"][0];
        let request = json!({"member_id":member["member_id"],"entity_type":table,"entity_id":entity,"field":"branch","branch_id":selected["branch_id"],"expected_token":conflict["token"],"operation_key":format!("review {table}")});
        let result = owner.execute("sync.resolve", request.clone()).unwrap();
        assert_eq!(owner.execute("sync.resolve", request).unwrap(), result);
        assert_eq!(
            result["document"][if table == "outfit_versions" {
                "version"
            } else {
                "revision"
            }],
            3
        );
    }
    for app in [&owner, &peer, &owner, &peer] {
        app.execute("sync.run", json!({})).unwrap();
    }
    for app in [&owner, &peer] {
        let history = app
            .execute(
                "wear.history",
                json!({"member_id":member["member_id"],"root_event_id":root["root_event_id"]}),
            )
            .unwrap();
        assert_eq!(history["items"].as_array().unwrap().len(), 4);
        for branch in &branches {
            assert!(history["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == branch));
        }
        let care_history = app
            .execute(
                "care.history",
                json!({"member_id":member["member_id"],"root_event_id":care_root["root_event_id"]}),
            )
            .unwrap();
        assert_eq!(care_history["items"].as_array().unwrap().len(), 4);
        for branch in &care_branches {
            assert!(care_history["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == branch));
        }
        let old=app.execute("sync.history",json!({"member_id":member["member_id"],"entity_type":"outfit_versions","entity_id":format!("{}:2",look["id"].as_str().unwrap())})).unwrap();
        assert_eq!(old["items"].as_array().unwrap().len(), 2);
        let conflicts = app
            .execute("sync.conflicts", json!({"member_id":member["member_id"]}))
            .unwrap();
        assert!(conflicts["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["field"] != "branch"));
    }
    assert_eq!(
        owner
            .execute(
                "outfit.get",
                json!({"member_id":member["member_id"],"id":look["id"]})
            )
            .unwrap(),
        peer.execute(
            "outfit.get",
            json!({"member_id":member["member_id"],"id":look["id"]})
        )
        .unwrap()
    );
    let a = owner
        .execute("wear.summary", json!({"member_id":member["member_id"]}))
        .unwrap();
    let b = peer
        .execute("wear.summary", json!({"member_id":member["member_id"]}))
        .unwrap();
    assert_eq!(a, b);
    // Authenticated independent sender fixture uses only its generated recovery bundle and secret.
    use base64::Engine;
    use sha2::{Digest, Sha256};
    use wardrobe_core::sync::storage::{LocalFolder, SyncStorage, WriteCondition};
    let read =
        rusqlite::Connection::open_with_flags(&ap, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let operation: String = read
        .query_row(
            "SELECT op_id FROM sync_journal WHERE entity_type='wear_events' AND entity_id=?1",
            [root["id"].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    drop(read);
    let storage = LocalFolder::new(remote.path()).unwrap();
    let space = created["recovery_bundle"]["space_id"].as_str().unwrap();
    let original: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            remote
                .path()
                .join(format!("{space}/changes/{operation}.object")),
        )
        .unwrap(),
    )
    .unwrap();
    use chacha20poly1305::{
        aead::{Aead, KeyInit, Payload},
        XChaCha20Poly1305, XNonce,
    };
    let aad = |kind: &str| {
        json!(["wardrobe-e2ee", 1, space, 1, kind])
            .to_string()
            .into_bytes()
    };
    let recovery_key = base64::engine::general_purpose::STANDARD
        .decode(created["recovery_secret"].as_str().unwrap())
        .unwrap();
    let recovery_envelope = &created["recovery_bundle"]["recovery_wrap"];
    let open = |key: &[u8], envelope: &serde_json::Value, kind: &str| {
        let nonce = base64::engine::general_purpose::STANDARD
            .decode(envelope["nonce"].as_str().unwrap())
            .unwrap();
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(envelope["ciphertext"].as_str().unwrap())
            .unwrap();
        XChaCha20Poly1305::new_from_slice(key)
            .unwrap()
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &aad(kind),
                },
            )
            .unwrap()
    };
    let wallet: serde_json::Value =
        serde_json::from_slice(&open(&recovery_key, recovery_envelope, "recovery-wrap")).unwrap();
    let master = base64::engine::general_purpose::STANDARD
        .decode(wallet["keys"][0]["key"].as_str().unwrap())
        .unwrap();
    let mut changes_key = [0u8; 32];
    hkdf::Hkdf::<Sha256>::new(Some(space.as_bytes()), &master)
        .expand(&aad("changes"), &mut changes_key)
        .unwrap();
    let mut changed: serde_json::Value =
        serde_json::from_slice(&open(&changes_key, &original["envelope"], "changes")).unwrap();
    let malicious_id = uuid::Uuid::now_v7().to_string();
    changed["op_id"] = json!(malicious_id);
    changed["after"]["notes"] = json!("authenticated immutable identity rewrite");
    let wrap = |payload: &serde_json::Value| {
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).unwrap();
        let ciphertext = XChaCha20Poly1305::new_from_slice(&changes_key)
            .unwrap()
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: payload.to_string().as_bytes(),
                    aad: &aad("changes"),
                },
            )
            .unwrap();
        let envelope = json!({"protocol_version":1,"algorithm":"XChaCha20-Poly1305","space_id":space,"epoch":1,"kind":"changes","nonce":base64::engine::general_purpose::STANDARD.encode(nonce),"ciphertext":base64::engine::general_purpose::STANDARD.encode(ciphertext)});
        json!({"protocol_version":1,"identity":original["identity"],"envelope":envelope})
            .to_string()
            .into_bytes()
    };
    let bytes = wrap(&changed);
    let object = format!("{space}/changes/{malicious_id}.object");
    storage
        .put(&object, &mut bytes.as_slice(), WriteCondition::CreateOnly)
        .unwrap();
    let marker = wrap(
        &json!({"key":object,"bytes":bytes.len(),"sha256":Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>()}),
    );
    storage
        .put(
            &format!("{space}/changes/{malicious_id}.commit"),
            &mut marker.as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    let invalid_id = uuid::Uuid::now_v7().to_string();
    let mut invalid = changed.clone();
    invalid["op_id"] = json!(invalid_id);
    invalid["entity_type"] = json!("clothing");
    invalid["entity_id"] = garment["id"].clone();
    invalid["after"] = owner
        .execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":garment["id"]}),
        )
        .unwrap();
    invalid["after"]["category"] = json!("not-a-domain-category");
    invalid["before"] = serde_json::Value::Null;
    let bytes = wrap(&invalid);
    let object = format!("{space}/changes/{invalid_id}.object");
    storage
        .put(&object, &mut bytes.as_slice(), WriteCondition::CreateOnly)
        .unwrap();
    let marker = wrap(
        &json!({"key":object,"bytes":bytes.len(),"sha256":Sha256::digest(&bytes).iter().map(|b|format!("{b:02x}")).collect::<String>()}),
    );
    storage
        .put(
            &format!("{space}/changes/{invalid_id}.commit"),
            &mut marker.as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member["member_id"],"id":garment["id"],"size_label":"L"}),
        )
        .unwrap();
    let result = owner
        .execute("sync.run", json!({}))
        .expect("bad immutable payload is isolated without blocking valid facts");
    assert_eq!(result["quarantined"], 2);
    peer.execute("sync.run", json!({})).unwrap();
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":garment["id"]})
        )
        .unwrap()["size_label"],
        "L"
    );
    for app in [&owner, &peer] {
        let history = app
            .execute(
                "wear.history",
                json!({"member_id":member["member_id"],"root_event_id":root["root_event_id"]}),
            )
            .unwrap();
        assert_eq!(history["items"].as_array().unwrap().len(), 4);
        assert!(history["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == &root));
        let status = app.execute("sync.status", json!({})).unwrap();
        assert!(status["quarantine"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["op_id"] == malicious_id && v["code"] == "sync_identity_conflict"));
        assert!(status["quarantine"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["op_id"] == invalid_id && v["code"] == "sync_invalid"));
        assert_eq!(
            app.execute("sync.run", json!({})).unwrap()["quarantined"],
            0
        );
    }

    let disposable=owner.execute("clothing.create",json!({"member_id":member["member_id"],"name":"offline delete/edit","category":"top","tags":["baseline"]})).unwrap();
    owner.execute("sync.run", json!({})).unwrap();
    peer.execute("sync.run", json!({})).unwrap();
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member["member_id"],"id":disposable["id"],"tags":[]}),
        )
        .unwrap();
    peer.execute("clothing.update",json!({"member_id":member["member_id"],"id":disposable["id"],"tags":["baseline","new while offline"]})).unwrap();
    for app in [&owner, &peer, &owner, &peer] {
        app.execute("sync.run", json!({})).unwrap();
    }
    for app in [&owner, &peer] {
        assert_eq!(
            app.execute(
                "clothing.get",
                json!({"member_id":member["member_id"],"id":disposable["id"]})
            )
            .unwrap()["tags"],
            json!(["new while offline"])
        );
    }
    owner
        .execute(
            "clothing.delete",
            json!({"member_id":member["member_id"],"id":disposable["id"]}),
        )
        .unwrap();
    peer.execute("clothing.update",json!({"member_id":member["member_id"],"id":disposable["id"],"name":"edited while disconnected"})).unwrap();
    for app in [&owner, &peer, &owner, &peer] {
        app.execute("sync.run", json!({})).unwrap();
    }
    for app in [&owner, &peer] {
        assert_eq!(
            app.execute(
                "clothing.get",
                json!({"member_id":member["member_id"],"id":disposable["id"]})
            )
            .unwrap()["status"],
            "deleted"
        );
    }
    let conflicts = owner
        .execute("sync.conflicts", json!({"member_id":member["member_id"]}))
        .unwrap();
    let conflict = conflicts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["entity_id"] == disposable["id"] && v["field"] == "lifecycle")
        .expect("concurrent delete/edit requires review");
    owner.execute("sync.resolve",json!({"member_id":member["member_id"],"entity_type":"clothing","entity_id":disposable["id"],"field":"lifecycle","value":"active","expected_token":conflict["token"],"operation_key":"explicitly restore edited item"})).unwrap();
    for app in [&owner, &peer, &owner, &peer] {
        app.execute("sync.run", json!({})).unwrap();
    }
    for app in [&owner, &peer] {
        let doc = app
            .execute(
                "clothing.get",
                json!({"member_id":member["member_id"],"id":disposable["id"]}),
            )
            .unwrap();
        assert_eq!(doc["status"], "active");
        assert_eq!(doc["name"], "edited while disconnected");
        assert!(app
            .execute("sync.conflicts", json!({"member_id":member["member_id"]}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["entity_id"] != disposable["id"] || v["field"] != "lifecycle"));
    }
    let delayed_member = owner
        .execute("member.create", json!({"name":"out of order member"}))
        .unwrap();
    let delayed=owner.execute("clothing.create",json!({"member_id":delayed_member["member_id"],"name":"fact before its member","category":"top"})).unwrap();
    owner.execute("sync.run", json!({})).unwrap();
    let read =
        rusqlite::Connection::open_with_flags(&ap, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let operation: String = read
        .query_row(
            "SELECT op_id FROM sync_journal WHERE entity_type='members' AND entity_id=?1",
            [delayed_member["member_id"].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    drop(read);
    let marker = remote
        .path()
        .join(format!("{space}/changes/{operation}.commit"));
    let parked = ap.parent().unwrap().join("delayed-member-marker");
    std::fs::rename(&marker, &parked).unwrap();
    peer.execute("sync.run", json!({})).unwrap();
    let state = peer.execute("sync.status", json!({})).unwrap();
    assert!(
        state["pending"].as_u64().is_some_and(|n| n > 0),
        "out of order dependencies must be durably staged"
    );
    drop(peer);
    let peer = Application::open(&bp).unwrap();
    assert_eq!(
        peer.execute("sync.status", json!({})).unwrap()["pending"],
        state["pending"]
    );
    std::fs::rename(&parked, &marker).unwrap();
    peer.execute("sync.run", json!({})).unwrap();
    assert_eq!(
        peer.execute("sync.status", json!({})).unwrap()["pending"],
        0
    );
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":delayed_member["member_id"],"id":delayed["id"]})
        )
        .unwrap()["name"],
        "fact before its member"
    );
}

#[test]
fn legacy_schema_keeps_actual_history_asset_foreign_keys_and_repairs_sync_tables_without_downgrade()
{
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&path).unwrap();
    let garment = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"legacy retained coat","category":"top"}),
        )
        .unwrap();
    let source = dir.path().join("generated.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([12, 34, 56]))
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
    let outfit=app.execute("outfit.create",json!({"member_id":"default","name":"legacy look","items":[{"clothing_id":garment["id"],"role":"top","layer":0}]})).unwrap();
    let wear=app.execute("wear.record",json!({"member_id":"default","source_outfit_id":outfit["id"],"source_version":1,"local_date":"2026-10-01","timezone":"Asia/Shanghai","operation_key":"legacy wear"})).unwrap();
    let care=app.execute("care.record",json!({"member_id":"default","clothing_id":garment["id"],"action":"wash","local_date":"2026-10-02","timezone":"Asia/Shanghai","operation_key":"legacy care"})).unwrap();
    drop(app);
    // Fixture is the documented old schema; facts and referenced bytes came from public APIs.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF;PRAGMA legacy_alter_table=ON;")
        .unwrap();
    for table in ["wear_events", "care_events", "outfit_versions"] {
        for kind in ["INSERT", "UPDATE"] {
            db.execute_batch(&format!("DROP TRIGGER IF EXISTS sync_{table}_{kind}"))
                .unwrap();
        }
        db.execute_batch(&format!("ALTER TABLE {table} RENAME TO fixture_saved"))
            .unwrap();
        let ddl = if table == "outfit_versions" {
            "outfit_id TEXT NOT NULL REFERENCES outfits(id),version INTEGER NOT NULL,document TEXT NOT NULL,PRIMARY KEY(outfit_id,version)"
        } else {
            "id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),root_event_id TEXT NOT NULL,revision INTEGER NOT NULL,document TEXT NOT NULL,UNIQUE(root_event_id,revision)"
        };
        db.execute_batch(&format!("CREATE TABLE {table}({ddl});INSERT INTO {table} SELECT * FROM fixture_saved;DROP TABLE fixture_saved")).unwrap();
    }
    db.execute_batch(
        "DROP TABLE sync_fields;PRAGMA user_version=14;PRAGMA legacy_alter_table=OFF;",
    )
    .unwrap();
    drop(db);
    let app = Application::open(&path).unwrap();
    assert_eq!(
        app.execute(
            "wear.history",
            json!({"member_id":"default","root_event_id":wear["root_event_id"]})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        app.execute(
            "care.history",
            json!({"member_id":"default","root_event_id":care["root_event_id"]})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        app.execute(
            "outfit.version",
            json!({"member_id":"default","id":outfit["id"],"version":1})
        )
        .unwrap()["name"],
        "legacy look"
    );
    assert!(app
        .execute(
            "assets.preview",
            json!({"member_id":"default","id":asset["id"],"variant":"thumbnail"})
        )
        .is_ok());
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        wardrobe_core::DATABASE_SCHEMA_VERSION
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM wear_asset_refs", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM care_asset_refs", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, u32>(
            0
        ))
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM sync_fields", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
}
