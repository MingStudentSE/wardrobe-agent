use serde_json::json;
use wardrobe_core::Application;
trait SyncBatch {
    #[track_caller]
    fn sync_batch(&self) -> Result<serde_json::Value, wardrobe_core::ApiError>;
}
impl SyncBatch for Application {
    fn sync_batch(&self) -> Result<serde_json::Value, wardrobe_core::ApiError> {
        let queued = self.execute("sync.run", json!({}))?;
        let processed = self.execute("sync.process", json!({"id":queued["job_id"]}))?;
        if processed["state"] == "failed" {
            panic!("sync worker failed: {}", processed["error"]);
        }
        Ok(processed["result"].clone())
    }
}
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
    owner.sync_batch().unwrap();
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
    let interrupted = peer.sync_batch().unwrap();
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
    let received = peer.sync_batch().unwrap();
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
    peer.sync_batch().unwrap();
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
    let peer_edit_step = peer.sync_batch().unwrap();
    let owner_edit_step = owner.sync_batch().unwrap();
    assert_eq!(peer_edit_step["sent"], 1);
    assert_eq!(owner_edit_step["pending"], 0);
    assert_eq!(
        owner_edit_step["has_more"], false,
        "both authenticated pages fit the declared small fixture budget"
    );
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
        app.sync_batch().unwrap();
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
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
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
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    let care_root=owner.execute("care.record",json!({"member_id":member["member_id"],"clothing_id":garment["id"],"action":"wash","local_date":"2026-10-03","timezone":"Asia/Shanghai","operation_key":"concurrent care root"})).unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
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
        app.sync_batch().unwrap();
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
    assert_eq!(
        owner
            .execute("wear.summary", json!({"member_id":member["member_id"]}))
            .expect_err("unresolved wear branches cannot become a definitive count")
            .code,
        "sync_conflict"
    );
    assert_eq!(
        owner
            .execute(
                "care.status",
                json!({"member_id":member["member_id"],"clothing_id":garment["id"]})
            )
            .expect_err("unresolved histories cannot produce definitive wash availability")
            .code,
        "sync_conflict"
    );
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
        if table == "wear_events" {
            assert_eq!(owner.execute("care.status", json!({"member_id":member["member_id"],"clothing_id":garment["id"]})).expect_err("wash projection must independently disclose an unresolved care branch after wear is selected").code, "sync_conflict");
        }
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
        app.sync_batch().unwrap();
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
    let reassigned_id = uuid::Uuid::now_v7().to_string();
    let mut reassigned = invalid.clone();
    reassigned["op_id"] = json!(reassigned_id);
    reassigned["after"] = owner
        .execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":garment["id"]}),
        )
        .unwrap();
    reassigned["after"]["member_id"] =
        owner.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let bytes = wrap(&reassigned);
    let object = format!("{space}/changes/{reassigned_id}.object");
    storage
        .put(&object, &mut bytes.as_slice(), WriteCondition::CreateOnly)
        .unwrap();
    let marker = wrap(
        &json!({"key":object,"bytes":bytes.len(),"sha256":Sha256::digest(&bytes).iter().map(|b|format!("{b:02x}")).collect::<String>()}),
    );
    storage
        .put(
            &format!("{space}/changes/{reassigned_id}.commit"),
            &mut marker.as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    let mut malformed_ids = Vec::new();
    for (field, value) in [
        ("protocol_version", json!(999)),
        ("clock", json!("not-a-vector")),
    ] {
        let malformed_id = uuid::Uuid::now_v7().to_string();
        let mut malformed = invalid.clone();
        malformed["op_id"] = json!(malformed_id);
        malformed["after"] = owner
            .execute(
                "clothing.get",
                json!({"member_id":member["member_id"],"id":garment["id"]}),
            )
            .unwrap();
        malformed[field] = value;
        let bytes = wrap(&malformed);
        let object = format!("{space}/changes/{malformed_id}.object");
        storage
            .put(&object, &mut bytes.as_slice(), WriteCondition::CreateOnly)
            .unwrap();
        let marker = wrap(
            &json!({"key":object,"bytes":bytes.len(),"sha256":Sha256::digest(&bytes).iter().map(|b|format!("{b:02x}")).collect::<String>()}),
        );
        storage
            .put(
                &format!("{space}/changes/{malformed_id}.commit"),
                &mut marker.as_slice(),
                WriteCondition::CreateOnly,
            )
            .unwrap();
        malformed_ids.push(malformed_id);
    }
    // This generated-key malicious publisher advertises actual immutable wire
    // objects through its authenticated index. Only its transport journal is
    // seeded; no receiver business rows or accepted clocks are fabricated.
    let publisher = original["identity"]["device_id"].as_str().unwrap();
    let namespace = format!("{space}/epochs/1/index/{publisher}");
    let head_key = format!("{namespace}/head.index");
    let head_meta = storage.stat(&head_key).unwrap();
    let old_head_envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(remote.path().join(&head_key)).unwrap()).unwrap();
    let old_head: serde_json::Value = serde_json::from_slice(&open(
        &changes_key,
        &old_head_envelope["envelope"],
        "changes",
    ))
    .unwrap();
    let old_number = old_head["pages"].as_u64().unwrap();
    let previous_envelope: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            remote
                .path()
                .join(format!("{namespace}/{old_number:020}.page")),
        )
        .unwrap(),
    )
    .unwrap();
    let previous_page: serde_json::Value = serde_json::from_slice(&open(
        &changes_key,
        &previous_envelope["envelope"],
        "changes",
    ))
    .unwrap();
    let hash = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let ids: Vec<_> = [
        malicious_id.clone(),
        invalid_id.clone(),
        reassigned_id.clone(),
    ]
    .into_iter()
    .chain(malformed_ids.iter().cloned())
    .collect();
    let entries: Vec<_> = ids
        .iter()
        .map(|id| {
            let key = format!("{space}/changes/{id}.commit");
            let bytes = std::fs::read(remote.path().join(&key)).unwrap();
            json!({"op_id":id,"key":key,"digest":hash(&bytes),"bytes":bytes.len()})
        })
        .collect();
    let number = old_number + 1;
    let advertised_page = json!({"protocol_version":1,"space_id":space,"epoch":1,"publisher":publisher,"page":number,"previous_digest":hash(previous_page.to_string().as_bytes()),"entries":entries});
    let bytes = wrap(&advertised_page);
    storage
        .put(
            &format!("{namespace}/{number:020}.page"),
            &mut bytes.as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    let head = json!({"protocol_version":1,"space_id":space,"epoch":1,"publisher":publisher,"pages":number,"last_page_digest":hash(&bytes)});
    storage
        .put(
            &head_key,
            &mut wrap(&head).as_slice(),
            WriteCondition::MatchVersion(head_meta.version),
        )
        .unwrap();
    let source_transport = rusqlite::Connection::open(&ap).unwrap();
    let target: String = source_transport
        .query_row(
            "SELECT json_extract(document,'$.target_key') FROM sync_targets WHERE id='primary'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    source_transport.execute("INSERT INTO sync_index_batches(target_id,epoch,page,document,published) VALUES(?1,1,?2,?3,1)", rusqlite::params![target,number,advertised_page.to_string()]).unwrap();
    for id in &ids {
        source_transport
            .execute(
                "INSERT INTO sync_index_entries(target_id,epoch,op_id,page) VALUES(?1,1,?2,?3)",
                rusqlite::params![target, id, number],
            )
            .unwrap();
    }
    drop(source_transport);
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member["member_id"],"id":garment["id"],"size_label":"L"}),
        )
        .unwrap();
    let result = owner
        .sync_batch()
        .expect("bad immutable payload is isolated without blocking valid facts");
    assert_eq!(result["quarantined"], 5);
    peer.sync_batch().unwrap();
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
        assert!(status["quarantine"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["op_id"] == reassigned_id && v["code"] == "sync_identity_conflict"));
        for id in &malformed_ids {
            assert!(status["quarantine"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["op_id"] == *id && v["code"] == "sync_invalid"));
        }
        assert_eq!(app.sync_batch().unwrap()["quarantined"], 0);
    }

    let disposable=owner.execute("clothing.create",json!({"member_id":member["member_id"],"name":"offline delete/edit","category":"top","tags":["baseline"]})).unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member["member_id"],"id":disposable["id"],"tags":[]}),
        )
        .unwrap();
    peer.execute("clothing.update",json!({"member_id":member["member_id"],"id":disposable["id"],"tags":["baseline","new while offline"]})).unwrap();
    for app in [&owner, &peer, &owner, &peer] {
        app.sync_batch().unwrap();
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
        app.sync_batch().unwrap();
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
        app.sync_batch().unwrap();
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
    owner.sync_batch().unwrap();
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
    peer.sync_batch().unwrap();
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
    peer.sync_batch().unwrap();
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
    let gap=owner.execute("clothing.create",json!({"member_id":member["member_id"],"name":"withheld causal predecessor","category":"top"})).unwrap();
    let follower=owner.execute("clothing.create",json!({"member_id":member["member_id"],"name":"later same-device fact","category":"top"})).unwrap();
    owner.sync_batch().unwrap();
    let read =
        rusqlite::Connection::open_with_flags(&ap, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let gap_operation:String=read.query_row("SELECT op_id FROM sync_journal WHERE entity_type='clothing' AND entity_id=?1 ORDER BY id DESC LIMIT 1",[gap["id"].as_str().unwrap()],|r|r.get(0)).unwrap();
    drop(read);
    let gap_marker = remote
        .path()
        .join(format!("{space}/changes/{gap_operation}.commit"));
    let parked_gap = ap.parent().unwrap().join("withheld-causal-marker");
    std::fs::rename(&gap_marker, &parked_gap).unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":follower["id"]})
        )
        .unwrap_err()
        .code,
        "not_found",
        "later facts cannot assert that an unseen causal predecessor was received"
    );
    let independent=peer.execute("clothing.create",json!({"member_id":member["member_id"],"name":"healthy independent device stream","category":"top"})).unwrap();
    peer.sync_batch().unwrap();
    owner.sync_batch().unwrap();
    assert_eq!(
        owner
            .execute(
                "clothing.get",
                json!({"member_id":member["member_id"],"id":independent["id"]})
            )
            .unwrap()["name"],
        "healthy independent device stream"
    );
    let waiting = peer.sync_batch().unwrap();
    assert_eq!(
        waiting["has_more"], false,
        "unavailable causal prerequisites do not cause an unbounded worker loop"
    );
    assert!(
        waiting["pending"].as_u64().unwrap() > 0,
        "waiting work remains durable and retryable"
    );
    drop(peer);
    let peer = Application::open(&bp).unwrap();
    assert!(
        peer.execute("sync.status", json!({})).unwrap()["pending"]
            .as_u64()
            .unwrap()
            > 0
    );
    std::fs::rename(&parked_gap, &gap_marker).unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":follower["id"]})
        )
        .unwrap()["name"],
        "later same-device fact"
    );
    assert_eq!(
        peer.execute("sync.status", json!({})).unwrap()["pending"],
        0
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
        "DROP TABLE sync_fields;DROP TABLE sync_index_errors;DROP TABLE sync_missing;PRAGMA user_version=14;PRAGMA legacy_alter_table=OFF;",
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
    for table in ["sync_index_errors", "sync_missing"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            0,
            "physical transport retry tables repair independently of global schema version"
        );
    }
}
#[test]
#[ignore = "requires actual OS credential store"]
fn incremental_publication_is_bounded_and_retains_the_rest_across_restart() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let ap = a.path().join("db");
    let bp = b.path().join("db");
    let owner = Application::open(&ap).unwrap();
    let peer = Application::open(&bp).unwrap();
    let _a = Cleanup(ap.clone());
    let _b = Cleanup(bp.clone());
    owner
        .execute(
            "encryption.create",
            json!({"password":"temporary bounded sync password","device_name":"bounded owner"}),
        )
        .unwrap();
    let offer = owner
        .execute(
            "encryption.pair.offer",
            json!({"device_name":"bounded peer"}),
        )
        .unwrap();
    let receipt=peer.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"temporary bounded peer password"})).unwrap();
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
    for i in 0..300 {
        owner.execute("clothing.create",json!({"member_id":"default","name":format!("bounded actual item {i}"),"category":"top"})).unwrap();
    }
    for app in [&owner, &peer] {
        app.execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote.path()}),
        )
        .unwrap();
    }
    let queued = owner
        .execute("sync.run", json!({"operation_key":"bounded-publication"}))
        .unwrap();
    let job_id = queued["job_id"].clone();
    assert_eq!(
        owner
            .execute("sync.run", json!({"operation_key":"bounded-publication"}))
            .unwrap()["job_id"],
        job_id
    );
    let processed = owner.execute("sync.process", json!({"id":job_id})).unwrap();
    assert_eq!(processed["state"], "queued");
    let first = processed["result"].clone();
    let mut index_heads = Vec::new();
    fn collect_heads(path: &std::path::Path, heads: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                collect_heads(&entry.path(), heads);
            } else if entry.file_name() == "head.index" {
                heads.push(entry.path());
            }
        }
    }
    collect_heads(remote.path(), &mut index_heads);
    assert_eq!(
        index_heads.len(),
        1,
        "bounded publication must durably advertise its authenticated publisher index head"
    );

    assert!(
        first["sent"].as_u64().unwrap() <= 256,
        "one run must not materialize an unbounded journal"
    );
    assert_eq!(first["has_more"], true);
    #[cfg(unix)]
    let first_publisher_head = std::fs::read(&index_heads[0]).unwrap();
    drop(owner);
    let owner = Application::open(&ap).unwrap();
    for _ in 0..10 {
        let processed = owner.execute("sync.process", json!({"id":job_id})).unwrap();
        let r = &processed["result"];
        assert!(r["sent"].as_u64().unwrap() <= 256);
        if processed["state"] == "succeeded" {
            break;
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let changes = index_heads[0].ancestors().nth(5).unwrap().join("changes");
        let prior = std::fs::metadata(&changes).unwrap().permissions();
        // Direct known-object reads remain possible; directory enumeration does not.
        // An authenticated publisher index must make old random-name scans unnecessary.
        std::fs::set_permissions(&changes, std::fs::Permissions::from_mode(0o111)).unwrap();
        let indexed_receive = peer.sync_batch();
        std::fs::set_permissions(&changes, prior).unwrap();
        let indexed_receive = indexed_receive
            .expect("a peer must receive indexed changes without enumerating their directory");
        assert!(
            indexed_receive["received"].as_u64().unwrap() > 0
                || indexed_receive["pending"].as_u64().unwrap() > 0
        );
        assert_eq!(indexed_receive["sent"], 0);
        let latest_head = std::fs::read(&index_heads[0]).unwrap();
        std::fs::write(&index_heads[0], &first_publisher_head).unwrap();
        let rollback_job = peer.execute("sync.run", json!({})).unwrap();
        let rollback_result = peer
            .execute("sync.process", json!({"id":rollback_job["job_id"]}))
            .unwrap();
        std::fs::write(&index_heads[0], latest_head).unwrap();
        assert_eq!(
            rollback_result["state"], "succeeded",
            "a bad publisher is isolated from healthy publishers"
        );
        assert_eq!(
            rollback_result["result"]["received"], 0,
            "a rolled-back authenticated head cannot provide facts"
        );
        let rollback_status = peer.execute("sync.status", json!({})).unwrap();
        assert!(rollback_status["transport_errors"].as_array().unwrap().iter().any(|e|e["code"]=="storage_corrupt"),"authenticated head rollback remains a durable corruption error after partial-page receipt");
    }
    for _ in 0..10 {
        let r = peer.sync_batch().unwrap();

        assert_eq!(
            r["sent"], 0,
            "namespace bootstrap must never echo a new local member fact"
        );
        assert!(
            r["received"].as_u64().unwrap() <= 256,
            "receiving must also bound each transaction batch"
        );
        if r["has_more"] == false {
            break;
        }
    }
    let member = owner.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let mut cursor = serde_json::Value::Null;
    let mut count = 0;
    loop {
        let mut input = json!({"member_id":member,"limit":100});
        if !cursor.is_null() {
            input["cursor"] = cursor.clone();
        }
        let page = peer.execute("clothing.list", input).unwrap();
        count += page["items"].as_array().unwrap().len();
        cursor = page["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(count, 300);
    assert_eq!(
        peer.sync_batch().unwrap()["sent"],
        0,
        "a remote bootstrap namespace must not create a local member fact or echo publication"
    );
    assert_eq!(owner.sync_batch().unwrap()["sent"], 0);

    let second = tempfile::tempdir().unwrap();
    owner
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":second.path()}),
        )
        .unwrap();
    let switched = owner.sync_batch().unwrap();
    assert_eq!(
        switched["sent"], 256,
        "a new target must receive the durable journal independently"
    );
    assert_eq!(switched["has_more"], true);
    owner.sync_batch().unwrap();
    owner
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote.path()}),
        )
        .unwrap();
    assert_eq!(
        owner.sync_batch().unwrap()["sent"],
        0,
        "returning to a known target must retain its publication receipts"
    );
    let legacy_fact = peer
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"actual legacy publisher arrival","category":"top"}),
        )
        .unwrap();
    peer.sync_batch().unwrap();
    let mut mixed_heads = Vec::new();
    collect_heads(remote.path(), &mut mixed_heads);
    let peer_head = mixed_heads
        .iter()
        .find(|path| **path != index_heads[0])
        .unwrap();
    let mut removed_metadata = Vec::new();
    for entry in std::fs::read_dir(peer_head.parent().unwrap()).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "index")
        {
            removed_metadata.push((path.clone(), std::fs::read(&path).unwrap()));
            std::fs::remove_file(path).unwrap();
        }
    }
    let mixed_receive = owner.sync_batch();
    for (path, bytes) in removed_metadata {
        std::fs::write(path, bytes).unwrap();
    }
    mixed_receive.unwrap();
    owner
        .execute(
            "clothing.get",
            json!({"member_id":member,"id":legacy_fact["id"]}),
        )
        .expect(
            "one indexed publisher must not hide another authenticated legacy publisher's facts",
        );
}

#[test]
#[ignore = "requires actual OS credential store"]
fn new_paired_installation_receives_original_remote_histories_on_a_new_target() {
    fn pair(owner: &Application, peer: &Application, name: &str) {
        let offer = owner
            .execute("encryption.pair.offer", json!({"device_name":name}))
            .unwrap();
        let receipt = peer.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"temporary generated-fixture peer password"})).unwrap();
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
    }
    fn drain(app: &Application) {
        let queued = app.execute("sync.run", json!({})).unwrap();
        for _ in 0..20 {
            let job = app
                .execute("sync.process", json!({"id":queued["job_id"]}))
                .unwrap();
            assert_ne!(
                job["state"], "failed",
                "public worker failed: {:?}",
                job["error"]
            );
            if job["state"] == "succeeded" {
                return;
            }
        }
        panic!("small three-device fixture failed to drain its bounded durable job");
    }
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let c = tempfile::tempdir().unwrap();
    let old_target = tempfile::tempdir().unwrap();
    let new_target = tempfile::tempdir().unwrap();
    let ap = a.path().join("db");
    let bp = b.path().join("db");
    let cp = c.path().join("db");
    let owner = Application::open(&ap).unwrap();
    let origin = Application::open(&bp).unwrap();
    let newcomer = Application::open(&cp).unwrap();
    let _a = Cleanup(ap);
    let _b = Cleanup(bp);
    let _c = Cleanup(cp);
    owner.execute("encryption.create",json!({"password":"temporary original-history owner password","device_name":"history relay"})).unwrap();
    pair(&owner, &origin, "original history author");
    let member = origin
        .execute("member.create", json!({"name":"original remote family"}))
        .unwrap()["member_id"]
        .clone();
    let garment = origin
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"original remote blue shirt","category":"top"}),
        )
        .unwrap();
    let look = origin.execute("outfit.create",json!({"member_id":member,"name":"original template snapshot","items":[{"clothing_id":garment["id"],"role":"top","layer":0}]})).unwrap();
    let wear = origin.execute("wear.record",json!({"member_id":member,"source_outfit_id":look["id"],"source_version":1,"local_date":"2026-10-01","timezone":"Asia/Shanghai","operation_key":"third-device original wear"})).unwrap();
    origin.execute("wear.correct",json!({"member_id":member,"root_event_id":wear["root_event_id"],"expected_revision":1,"clothing_ids":[garment["id"]],"local_date":"2026-10-02","timezone":"Asia/Shanghai","operation_key":"third-device correction"})).unwrap();
    origin.execute("wear.revoke",json!({"member_id":member,"root_event_id":wear["root_event_id"],"expected_revision":2,"reason":"historical correction retained","operation_key":"third-device revocation"})).unwrap();
    let care = origin.execute("care.record",json!({"member_id":member,"clothing_id":garment["id"],"action":"wash","local_date":"2026-10-01","timezone":"Asia/Shanghai","operation_key":"third-device original care"})).unwrap();
    origin.execute("care.revoke",json!({"member_id":member,"root_event_id":care["root_event_id"],"expected_revision":1,"reason":"retain original care revision","operation_key":"third-device care revocation"})).unwrap();
    origin.execute("outfit.update",json!({"member_id":member,"id":look["id"],"expected_version":1,"name":"later template","items":[{"clothing_id":garment["id"],"role":"top","layer":0}]})).unwrap();
    origin
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":garment["id"],"name":"current edited remote shirt"}),
        )
        .unwrap();

    for (name, category) in [("remote trousers", "bottom"), ("remote shoes", "shoes")] {
        origin
            .execute(
                "clothing.create",
                json!({"member_id":member,"name":name,"category":category}),
            )
            .unwrap();
    }
    let feedback_intent = origin
        .execute(
            "recommendation.generate",
            json!({"member_id":member,"operation_key":"portable feedback source","timezone":"UTC"}),
        )
        .unwrap();
    let feedback_input = json!({"member_id":member,"operation_key":"portable feedback receipt","intent_id":feedback_intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5,"comfort":null,"style":4}});
    let feedback = origin
        .execute("feedback.record", feedback_input.clone())
        .unwrap();
    origin.execute("feedback.correct",json!({"member_id":member,"operation_key":"portable feedback correction","root_id":feedback["root_id"],"expected_revision":1,"reaction":"dislike"})).unwrap();
    origin.execute("feedback.revoke",json!({"member_id":member,"operation_key":"portable feedback revocation","root_id":feedback["root_id"],"expected_revision":2,"reason":"original feedback history remains"})).unwrap();
    let expected_feedback = origin
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":feedback["root_id"]}),
        )
        .unwrap()["items"]
        .clone();
    for app in [&origin, &owner] {
        app.execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":old_target.path()}),
        )
        .unwrap();
    }
    drain(&origin);
    drain(&owner);
    let expected_wear = origin
        .execute(
            "wear.history",
            json!({"member_id":member,"root_event_id":wear["root_event_id"]}),
        )
        .unwrap()["items"]
        .clone();
    let expected_care = origin
        .execute(
            "care.history",
            json!({"member_id":member,"root_event_id":care["root_event_id"]}),
        )
        .unwrap()["items"]
        .clone();
    assert_eq!(
        owner
            .execute(
                "wear.history",
                json!({"member_id":member,"root_event_id":wear["root_event_id"]})
            )
            .unwrap()["items"],
        expected_wear
    );
    assert_eq!(
        owner
            .execute(
                "care.history",
                json!({"member_id":member,"root_event_id":care["root_event_id"]})
            )
            .unwrap()["items"],
        expected_care
    );
    assert_eq!(
        owner
            .execute(
                "feedback.history",
                json!({"member_id":member,"root_id":feedback["root_id"]})
            )
            .unwrap()["items"],
        expected_feedback,
        "all accepted explicit feedback revisions must be portable"
    );
    let branch_input = json!({"member_id":member,"operation_key":"portable concurrent feedback root","intent_id":feedback_intent["id"],"proposal_index":0,"reaction":"like"});
    let branch_root = origin.execute("feedback.record", branch_input).unwrap();
    drain(&origin);
    drain(&owner);
    let origin_correction_input = json!({"member_id":member,"operation_key":"colliding feedback correction receipt","root_id":branch_root["root_id"],"expected_revision":1,"reaction":"dislike"});
    let origin_branch = origin
        .execute("feedback.correct", origin_correction_input.clone())
        .unwrap();
    let relay_correction_input = json!({"member_id":member,"operation_key":"colliding feedback correction receipt","root_id":branch_root["root_id"],"expected_revision":1,"reaction":"skip"});
    let relay_branch = owner
        .execute("feedback.correct", relay_correction_input.clone())
        .unwrap();
    drain(&origin);
    drain(&owner);
    drain(&origin); // Observe the other immutable receipt before checking replay.
    let conflicts = owner
        .execute("sync.conflicts", json!({"member_id":member}))
        .unwrap();
    let conflict = conflicts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| {
            item["entity_type"] == "feedback_events" && item["entity_id"] == branch_root["root_id"]
        })
        .expect("concurrent feedback revisions require explicit branch review");
    assert_eq!(owner.execute("feedback.correct", relay_correction_input.clone()).expect_err("an ambiguous portable receipt cannot return the installation's first cached result").code, "idempotency_conflict");
    assert_eq!(
        origin
            .execute("feedback.correct", origin_correction_input.clone())
            .expect_err("both devices must reject a collided logical receipt")
            .code,
        "idempotency_conflict"
    );
    let unresolved_plan = owner.execute("recommendation.generate", json!({"member_id":member,"operation_key":"ambiguous feedback cannot guide recommendation","timezone":"UTC"}));
    assert_eq!(
        unresolved_plan
            .expect_err("planner must disclose unresolved mutually exclusive feedback")
            .code,
        "sync_conflict"
    );
    let unsafe_revision = owner.execute("feedback.correct", json!({"member_id":member,"operation_key":"must review divergent feedback first","root_id":branch_root["root_id"],"expected_revision":2,"reaction":"like"}));
    assert_eq!(
        unsafe_revision
            .expect_err("domain correction cannot silently select one concurrent parent")
            .code,
        "version_conflict"
    );
    let selected = conflict["variants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variant| variant["document"] == origin_branch)
        .unwrap();
    let choice = owner.execute("sync.resolve", json!({"member_id":member,"operation_key":"select original feedback branch","entity_type":"feedback_events","entity_id":branch_root["root_id"],"field":"branch","expected_token":conflict["token"],"branch_id":selected["branch_id"]})).unwrap();
    assert_eq!(choice["document"]["revision"], 3);
    assert_eq!(choice["document"]["reaction"], "dislike");
    assert!(choice["document"].get("kind").is_none());
    drain(&owner);
    drain(&origin);
    let expected_branch_history = owner
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":branch_root["root_id"]}),
        )
        .unwrap()["items"]
        .clone();
    assert_eq!(expected_branch_history.as_array().unwrap().len(), 4);
    assert!(expected_branch_history
        .as_array()
        .unwrap()
        .contains(&origin_branch));
    assert!(expected_branch_history
        .as_array()
        .unwrap()
        .contains(&relay_branch));
    assert_eq!(
        origin
            .execute(
                "feedback.history",
                json!({"member_id":member,"root_id":branch_root["root_id"]})
            )
            .unwrap()["items"],
        expected_branch_history
    );
    let mut paged_history = vec![];
    let mut feedback_cursor = serde_json::Value::Null;
    for _ in 0..5 {
        let mut input = json!({"member_id":member,"root_id":branch_root["root_id"],"limit":1});
        if !feedback_cursor.is_null() {
            input["cursor"] = feedback_cursor;
        }
        let page = owner.execute("feedback.history", input).unwrap();
        paged_history.extend(page["items"].as_array().unwrap().iter().cloned());
        feedback_cursor = page["next_cursor"].clone();
        if feedback_cursor.is_null() {
            break;
        }
    }
    assert_eq!(
        serde_json::Value::Array(paged_history),
        expected_branch_history,
        "bounded history pagination must preserve every same-revision branch"
    );
    pair(
        &owner,
        &newcomer,
        "third installation without original author",
    );
    for app in [&owner, &newcomer] {
        app.execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":new_target.path()}),
        )
        .unwrap();
    }
    drain(&owner);
    drain(&newcomer);
    assert_eq!(newcomer.execute("clothing.get",json!({"member_id":member,"id":garment["id"]})).expect("a fresh paired installation needs the received original stream, not only a snapshot with an unseen remote causal base")["name"],"current edited remote shirt");
    assert_eq!(
        newcomer
            .execute(
                "feedback.history",
                json!({"member_id":member,"root_id":branch_root["root_id"]})
            )
            .unwrap()["items"],
        expected_branch_history,
        "third installation preserves both original branches and selected new revision"
    );
    assert_eq!(
        newcomer
            .execute(
                "wear.history",
                json!({"member_id":member,"root_event_id":wear["root_event_id"]})
            )
            .unwrap()["items"],
        expected_wear
    );
    assert_eq!(
        newcomer
            .execute(
                "care.history",
                json!({"member_id":member,"root_event_id":care["root_event_id"]})
            )
            .unwrap()["items"],
        expected_care
    );
    assert_eq!(
        newcomer
            .execute(
                "feedback.history",
                json!({"member_id":member,"root_id":feedback["root_id"]})
            )
            .unwrap()["items"],
        expected_feedback
    );
    assert_eq!(newcomer.execute("feedback.record",feedback_input).unwrap(),feedback,"the portable receipt must replay the original result without requiring private recommendation intents");
    assert_eq!(
        newcomer
            .execute(
                "wear.history",
                json!({"member_id":member,"root_event_id":wear["root_event_id"]})
            )
            .unwrap()["items"],
        expected_wear,
        "ordinary feedback must not create or alter wear"
    );
    for version in [1, 2] {
        assert_eq!(
            newcomer
                .execute(
                    "outfit.version",
                    json!({"member_id":member,"id":look["id"],"version":version})
                )
                .unwrap(),
            origin
                .execute(
                    "outfit.version",
                    json!({"member_id":member,"id":look["id"],"version":version})
                )
                .unwrap()
        );
    }
}

#[test]
#[ignore = "requires actual OS credential store"]
fn corrupt_indexed_marker_is_durable_and_does_not_block_a_healthy_publisher() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let c = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let ap = a.path().join("db");
    let bp = b.path().join("db");
    let cp = c.path().join("db");
    let owner = Application::open(&ap).unwrap();
    let peer = Application::open(&bp).unwrap();
    let sink = Application::open(&cp).unwrap();
    let _cleanup = [
        Cleanup(ap.clone()),
        Cleanup(bp.clone()),
        Cleanup(cp.clone()),
    ];
    let space = owner.execute("encryption.create", json!({"password":"generated temporary transport fixture","device_name":"healthy source"})).unwrap()["recovery_bundle"]["space_id"].as_str().unwrap().to_owned();
    let mut manifest = serde_json::Value::Null;
    for (app, name) in [(&peer, "marker publisher"), (&sink, "healthy receiver")] {
        let offer = owner
            .execute("encryption.pair.offer", json!({"device_name":name}))
            .unwrap();
        let receipt=app.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"generated temporary peer"})).unwrap();
        manifest = owner
            .execute(
                "encryption.pair.confirm",
                json!({"receipt":receipt["receipt"]}),
            )
            .unwrap()["manifest"]
            .clone();
        app.execute("encryption.device.apply", json!({"manifest":manifest}))
            .unwrap();
    }
    peer.execute("encryption.device.apply", json!({"manifest":manifest}))
        .unwrap();
    let member_a = owner.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let member_b = peer.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let healthy = owner
        .execute(
            "clothing.create",
            json!({"member_id":member_a,"name":"healthy publisher fact","category":"top"}),
        )
        .unwrap();
    let delayed = peer
        .execute(
            "clothing.create",
            json!({"member_id":member_b,"name":"corrupt transport repaired","category":"bottom"}),
        )
        .unwrap();
    for app in [&owner, &peer, &sink] {
        app.execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote.path()}),
        )
        .unwrap();
    }
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    let read =
        rusqlite::Connection::open_with_flags(&bp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let op: String = read
        .query_row(
            "SELECT op_id FROM sync_journal WHERE entity_type='clothing' AND entity_id=?1",
            [delayed["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    drop(read);
    let marker = remote.path().join(format!("{space}/changes/{op}.commit"));
    let original = std::fs::read(&marker).unwrap();
    let mut corrupted = original.clone();
    corrupted[0] ^= 1;
    std::fs::write(&marker, &corrupted).unwrap();
    sink.sync_batch()
        .expect("one corrupt authenticated indexed marker must not stop a healthy publisher");
    assert_eq!(
        sink.execute(
            "clothing.get",
            json!({"member_id":member_a,"id":healthy["id"]})
        )
        .unwrap()["name"],
        "healthy publisher fact"
    );
    assert!(
        sink.execute("sync.status", json!({})).unwrap()["pending"]
            .as_u64()
            .unwrap()
            > 0
    );
    drop(sink);
    let sink = Application::open(&cp).unwrap();
    assert!(
        sink.execute("sync.status", json!({})).unwrap()["pending"]
            .as_u64()
            .unwrap()
            > 0,
        "missing transport retry must survive restart"
    );
    std::fs::write(&marker, &original).unwrap();
    sink.sync_batch().unwrap();
    assert_eq!(
        sink.execute(
            "clothing.get",
            json!({"member_id":member_b,"id":delayed["id"]})
        )
        .unwrap()["name"],
        "corrupt transport repaired"
    );
    assert_eq!(
        sink.execute("sync.status", json!({})).unwrap()["pending"],
        0
    );
    let publisher = peer.execute("encryption.status", json!({})).unwrap()["state"]["device_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let head_path = remote
        .path()
        .join(format!("{space}/epochs/1/index/{publisher}/head.index"));
    assert!(
        head_path.is_file(),
        "confirmed peer publisher has an advertised head"
    );
    let head_original = std::fs::read(&head_path).unwrap();
    let mut corrupt_head = head_original.clone();
    corrupt_head[0] ^= 1;
    std::fs::write(&head_path, &corrupt_head).unwrap();
    let another = owner.execute("clothing.create", json!({"member_id":member_a,"name":"healthy publisher past corrupt head","category":"top"})).unwrap();
    owner
        .sync_batch()
        .expect("a corrupt other-publisher head must be isolated from local publication");
    sink.sync_batch()
        .expect("a corrupt other-publisher head must not block healthy remote facts");
    assert_eq!(
        sink.execute(
            "clothing.get",
            json!({"member_id":member_a,"id":another["id"]})
        )
        .unwrap()["name"],
        "healthy publisher past corrupt head"
    );
    assert!(
        sink.execute("sync.status", json!({})).unwrap()["pending"]
            .as_u64()
            .unwrap()
            > 0
    );
    drop(sink);
    let sink = Application::open(&cp).unwrap();
    assert!(
        sink.execute("sync.status", json!({})).unwrap()["pending"]
            .as_u64()
            .unwrap()
            > 0,
        "publisher errors survive restart"
    );
    std::fs::write(&head_path, &head_original).unwrap();
    sink.sync_batch().unwrap();
    assert_eq!(
        sink.execute("sync.status", json!({})).unwrap()["pending"],
        0
    );
    std::fs::remove_file(&head_path).unwrap();
    let last = owner.execute("clothing.create", json!({"member_id":member_a,"name":"healthy past missing observed head","category":"top"})).unwrap();
    owner.sync_batch().unwrap();
    sink.sync_batch().unwrap();
    assert_eq!(
        sink.execute(
            "clothing.get",
            json!({"member_id":member_a,"id":last["id"]})
        )
        .unwrap()["name"],
        "healthy past missing observed head"
    );
    assert!(
        sink.execute("sync.status", json!({})).unwrap()["pending"]
            .as_u64()
            .unwrap()
            > 0,
        "missing previously authenticated head is durable pending, not an empty publisher"
    );
    std::fs::write(&head_path, &head_original).unwrap();
    sink.sync_batch().unwrap();
    assert_eq!(
        sink.execute("sync.status", json!({})).unwrap()["pending"],
        0
    );
    owner.execute("weather.configure", json!({"member_id":member_a,"mode":"automatic","location":"公开测试城市","latitude":31.2,"longitude":121.5,"usage":"noncommercial","endpoint":"http://127.0.0.1:1/private-local-weather"})).unwrap();
    owner.sync_batch().unwrap();
    sink.sync_batch().unwrap();
    let remote_weather = sink
        .execute("weather.get", json!({"member_id":member_a}))
        .unwrap();
    assert_eq!(
        remote_weather["config"]["location"], "公开测试城市",
        "explicit weather configuration must sync"
    );
    assert_eq!(
        remote_weather["config"]["endpoint"], "https://api.open-meteo.com/v1/forecast",
        "local endpoint must not travel"
    );
    assert!(remote_weather["cache"].is_null() && remote_weather["last_error"].is_null());
    let failed_weather = owner
        .execute("weather.refresh", json!({"member_id":member_a}))
        .unwrap();
    assert!(
        !failed_weather["last_error"].is_null(),
        "actual local connection failure is recorded only locally"
    );
    assert_eq!(
        owner.sync_batch().unwrap()["sent"],
        0,
        "derived weather errors must not publish operations"
    );
    sink.execute("weather.configure",json!({"member_id":member_a,"mode":"automatic","location":"公开测试城市","latitude":31.2,"longitude":121.5,"usage":"noncommercial","endpoint":"http://127.0.0.1:2/receiver-private"})).unwrap();
    assert_eq!(
        sink.sync_batch().unwrap()["sent"],
        0,
        "endpoint-only changes must remain local"
    );
    owner.execute("weather.configure",json!({"member_id":member_a,"mode":"automatic","location":"另一个公开城市","latitude":32.0,"longitude":120.0,"usage":"noncommercial","endpoint":"http://127.0.0.1:1/private-local-weather"})).unwrap();
    owner.sync_batch().unwrap();
    sink.sync_batch().unwrap();
    let updated = sink
        .execute("weather.get", json!({"member_id":member_a}))
        .unwrap();
    assert_eq!(updated["config"]["location"], "另一个公开城市");
    assert_eq!(
        updated["config"]["endpoint"], "http://127.0.0.1:2/receiver-private",
        "receiving config preserves local transport endpoint"
    );
    assert!(updated["cache"].is_null() && updated["last_error"].is_null());
}

#[test]
#[ignore = "requires actual OS credential store"]
fn pairing_later_device_preserves_authenticated_same_epoch_history() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let c = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let ap = a.path().join("db");
    let bp = b.path().join("db");
    let cp = c.path().join("db");
    let owner = Application::open(&ap).unwrap();
    let peer = Application::open(&bp).unwrap();
    let newcomer = Application::open(&cp).unwrap();
    let _cleanup = [Cleanup(ap), Cleanup(bp), Cleanup(cp)];
    owner
        .execute(
            "encryption.create",
            json!({"password":"generated old manifest fixture","device_name":"historical owner"}),
        )
        .unwrap();
    let pair = |app: &Application, name: &str| {
        let offer = owner
            .execute("encryption.pair.offer", json!({"device_name":name}))
            .unwrap();
        let receipt=app.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"generated peer fixture"})).unwrap();
        let confirmed = owner
            .execute(
                "encryption.pair.confirm",
                json!({"receipt":receipt["receipt"]}),
            )
            .unwrap();
        app.execute(
            "encryption.device.apply",
            json!({"manifest":confirmed["manifest"]}),
        )
        .unwrap();
        confirmed["manifest"].clone()
    };
    pair(&peer, "original peer");
    let member = owner.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let garment = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"published before later pairing","category":"top"}),
        )
        .unwrap();
    let peer_member = peer.execute("space.default", json!({})).unwrap()["member_id"].clone();
    let peer_fact=peer.execute("clothing.create",json!({"member_id":peer_member,"name":"accepted history from later revoked device","category":"bottom"})).unwrap();
    for app in [&owner, &peer] {
        app.execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote.path()}),
        )
        .unwrap();
    }
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    owner.sync_batch().unwrap();

    assert_eq!(
        owner
            .execute(
                "clothing.get",
                json!({"member_id":peer_member,"id":peer_fact["id"]})
            )
            .unwrap()["name"],
        "accepted history from later revoked device",
        "fixture must prove history accepted before revocation"
    );
    let latest = pair(&newcomer, "later peer");
    peer.execute("encryption.device.apply", json!({"manifest":latest}))
        .unwrap();
    newcomer
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote.path()}),
        )
        .unwrap();
    newcomer.sync_batch().unwrap();
    assert_eq!(
        newcomer
            .execute(
                "clothing.get",
                json!({"member_id":member,"id":garment["id"]})
            )
            .unwrap()["name"],
        "published before later pairing",
        "old owner-signed same-epoch history remains readable to a newly paired device"
    );
    owner.sync_batch().unwrap();
    let peer_device =
        peer.execute("encryption.status", json!({})).unwrap()["state"]["device_id"].clone();
    let rotated = owner
        .execute(
            "encryption.device.revoke",
            json!({"device_id":peer_device,"password":"generated epoch fixture"}),
        )
        .unwrap();
    newcomer
        .execute(
            "encryption.device.apply",
            json!({"manifest":rotated["manifest"]}),
        )
        .unwrap();
    let forbidden=peer.execute("clothing.create",json!({"member_id":peer_member,"name":"fresh write from revoked epoch","category":"bottom"})).unwrap();
    peer.sync_batch().unwrap();
    owner.sync_batch().unwrap();
    newcomer.sync_batch().unwrap();
    assert_eq!(
        newcomer
            .execute(
                "clothing.get",
                json!({"member_id":peer_member,"id":peer_fact["id"]})
            )
            .unwrap()["name"],
        "accepted history from later revoked device"
    );
    assert_eq!(
        newcomer
            .execute(
                "clothing.get",
                json!({"member_id":peer_member,"id":forbidden["id"]})
            )
            .unwrap_err()
            .code,
        "not_found",
        "fresh old-epoch writes must not cross revocation"
    );
    let d = tempfile::tempdir().unwrap();
    let dp = d.path().join("db");
    let after_rotation = Application::open(&dp).unwrap();
    let _d_cleanup = Cleanup(dp);
    let current = pair(&after_rotation, "fresh current-epoch peer");
    newcomer
        .execute("encryption.device.apply", json!({"manifest":current}))
        .unwrap();
    after_rotation
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote.path()}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    after_rotation.sync_batch().unwrap();
    assert_eq!(after_rotation.execute("clothing.get",json!({"member_id":peer_member,"id":peer_fact["id"]})).unwrap()["name"],"accepted history from later revoked device","a current authorized publisher must relay accepted historical facts to a newly paired post-rotation device");
    assert_eq!(
        after_rotation
            .execute(
                "clothing.get",
                json!({"member_id":peer_member,"id":forbidden["id"]})
            )
            .unwrap_err()
            .code,
        "not_found"
    );
}
