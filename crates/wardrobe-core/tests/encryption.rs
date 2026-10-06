use serde_json::json;
use wardrobe_core::Application;
struct KeyCleanup(std::path::PathBuf);
impl Drop for KeyCleanup {
    fn drop(&mut self) {
        if let Ok(app) = Application::open(&self.0) {
            let _ = app.execute("encryption.unpair", json!({}));
        }
    }
}
#[test]
fn encrypted_envelopes_roundtrip_and_authenticate_workspace_kind_version_and_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let app = Application::open(&directory.path().join("wardrobe.sqlite")).unwrap();
    let recovery = app
        .execute(
            "encryption.recovery.create",
            json!({"password":"correct horse battery staple"}),
        )
        .unwrap();
    let input = json!({"bundle":recovery["bundle"],"password":"correct horse battery staple","kind":"changes","plaintext_base64":"cHJpdmF0ZSBjbG90aGluZyBmYWN0"});
    let encrypted = app.execute("encryption.seal", input.clone()).unwrap();
    let mut opening = json!({"bundle":recovery["bundle"],"password":"correct horse battery staple","kind":"changes","envelope":encrypted["envelope"]});
    let clear = app.execute("encryption.open", opening.clone()).unwrap();
    assert_eq!(clear["plaintext_base64"], "cHJpdmF0ZSBjbG90aGluZyBmYWN0");
    opening["password"] = json!("wrong password");
    assert_eq!(
        app.execute("encryption.open", opening.clone())
            .unwrap_err()
            .code,
        "authentication_failed"
    );
    opening["password"] = json!("correct horse battery staple");
    opening["kind"] = json!("blobs");
    assert_eq!(
        app.execute("encryption.open", opening.clone())
            .unwrap_err()
            .code,
        "authentication_failed"
    );
    opening["kind"] = json!("changes");
    opening["envelope"]["nonce"] = json!("AA==");
    assert_eq!(
        app.execute("encryption.open", opening).unwrap_err().code,
        "invalid_envelope"
    );
    let recovery_open = json!({"bundle":recovery["bundle"],"recovery_secret":recovery["recovery_secret"],"kind":"changes","envelope":encrypted["envelope"]});
    assert_eq!(
        app.execute("encryption.open", recovery_open.clone())
            .unwrap()["plaintext_base64"],
        "cHJpdmF0ZSBjbG90aGluZyBmYWN0"
    );
    let mut tampered = recovery_open.clone();
    tampered["envelope"]["ciphertext"] = json!("AA==");
    assert_eq!(
        app.execute("encryption.open", tampered).unwrap_err().code,
        "invalid_envelope"
    );
    let mut tampered = recovery_open.clone();
    tampered["envelope"]["ciphertext"] = json!("AAAAAAAAAAAAAAAAAAAAAA==");
    assert_eq!(
        app.execute("encryption.open", tampered).unwrap_err().code,
        "authentication_failed"
    );
    let mut wrong_space = recovery_open.clone();
    wrong_space["envelope"]["space_id"] = json!("019a98d9-381b-78c3-9f02-976a578b3a7a");
    assert_eq!(
        app.execute("encryption.open", wrong_space)
            .unwrap_err()
            .code,
        "authentication_failed"
    );
    let mut unsupported = recovery_open.clone();
    unsupported["envelope"]["protocol_version"] = json!(2);
    assert_eq!(
        app.execute("encryption.open", unsupported)
            .unwrap_err()
            .code,
        "invalid_envelope"
    );
    let second = app.execute("encryption.seal", input).unwrap();
    assert_ne!(encrypted["envelope"]["nonce"], second["envelope"]["nonce"]);
}
#[test]
#[ignore = "requires real OS credential store"]
fn encrypted_space_restarts_with_system_key_and_pairing_imports_same_master_on_second_device() {
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let a = Application::open(&a_dir.path().join("wardrobe.sqlite")).unwrap();
    let _ga = KeyCleanup(a_dir.path().join("wardrobe.sqlite"));
    let _gb = KeyCleanup(b_dir.path().join("wardrobe.sqlite"));
    let created = a
        .execute(
            "encryption.create",
            json!({"password":"strong temporary space password","device_name":"First test device"}),
        )
        .unwrap();
    assert_eq!(created["status"]["enabled"], true);
    let sealed = a
        .execute(
            "encryption.encrypt",
            json!({"kind":"changes","plaintext_base64":"aGVsbG8="}),
        )
        .unwrap();
    let offer = a
        .execute(
            "encryption.pair.offer",
            json!({"device_name":"Second test device"}),
        )
        .unwrap();
    let b = Application::open(&b_dir.path().join("wardrobe.sqlite")).unwrap();
    let accepted=b.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"second temporary space password"})).unwrap();
    let confirmed = a
        .execute(
            "encryption.pair.confirm",
            json!({"receipt":accepted["receipt"]}),
        )
        .unwrap();
    b.execute(
        "encryption.device.apply",
        json!({"manifest":confirmed["manifest"]}),
    )
    .unwrap();
    assert_eq!(
        b.execute(
            "encryption.decrypt",
            json!({"kind":"changes","envelope":sealed["envelope"]})
        )
        .unwrap()["plaintext_base64"],
        "aGVsbG8="
    );
    drop(a);
    let a = Application::open(&a_dir.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        a.execute(
            "encryption.decrypt",
            json!({"kind":"changes","envelope":sealed["envelope"]})
        )
        .unwrap()["plaintext_base64"],
        "aGVsbG8="
    );
    a.execute("encryption.unpair", json!({})).unwrap();
    b.execute("encryption.unpair", json!({})).unwrap();
}
#[test]
fn external_full_scope_cannot_manage_space_secrets_even_with_member_argument() {
    let d = tempfile::tempdir().unwrap();
    let app = Application::open(&d.path().join("wardrobe.sqlite")).unwrap();
    let client=app.execute("client.authorize",json!({"name":"full test client","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    for operation in [
        "encryption.recovery.create",
        "encryption.create",
        "encryption.pair.offer",
        "encryption.pair.accept",
        "encryption.unpair",
    ] {
        assert_eq!(
            app.execute_as_client(
                client["token"].as_str().unwrap(),
                operation,
                json!({"member_id":"default","password":"temporary test password"})
            )
            .unwrap_err()
            .code,
            "scope_denied"
        );
    }
}
#[test]
#[ignore = "requires real OS credential store"]
fn device_confirmation_is_single_use_and_revocation_rotates_future_keys_for_remaining_devices() {
    let da = tempfile::tempdir().unwrap();
    let db = tempfile::tempdir().unwrap();
    let dc = tempfile::tempdir().unwrap();
    let a = Application::open(&da.path().join("wardrobe.sqlite")).unwrap();
    let b = Application::open(&db.path().join("wardrobe.sqlite")).unwrap();
    let c = Application::open(&dc.path().join("wardrobe.sqlite")).unwrap();
    let _ga = KeyCleanup(da.path().join("wardrobe.sqlite"));
    let _gb = KeyCleanup(db.path().join("wardrobe.sqlite"));
    let _gc = KeyCleanup(dc.path().join("wardrobe.sqlite"));
    a.execute(
        "encryption.create",
        json!({"password":"temporary test space password","device_name":"owner"}),
    )
    .unwrap();
    let offer = a
        .execute("encryption.pair.offer", json!({"device_name":"to revoke"}))
        .unwrap();
    let pending=b.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"temporary target password"})).unwrap();
    assert_eq!(
        b.execute(
            "encryption.encrypt",
            json!({"kind":"changes","plaintext_base64":"aGVsbG8="})
        )
        .unwrap_err()
        .code,
        "pairing_pending"
    );
    let confirmation = a
        .execute(
            "encryption.pair.confirm",
            json!({"receipt":pending["receipt"]}),
        )
        .unwrap();
    let mut altered_manifest = confirmation["manifest"].clone();
    altered_manifest["signature"] = json!(
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
    );
    assert_eq!(
        b.execute(
            "encryption.device.apply",
            json!({"manifest":altered_manifest})
        )
        .unwrap_err()
        .code,
        "authentication_failed"
    );
    b.execute(
        "encryption.device.apply",
        json!({"manifest":confirmation["manifest"]}),
    )
    .unwrap();
    assert_eq!(
        a.execute(
            "encryption.pair.confirm",
            json!({"receipt":pending["receipt"]})
        )
        .unwrap_err()
        .code,
        "invitation_consumed"
    );
    let other = a
        .execute("encryption.pair.offer", json!({"device_name":"remaining"}))
        .unwrap();
    let otherpending=c.execute("encryption.pair.accept",json!({"invitation":other["invitation"],"pairing_secret":other["pairing_secret"],"password":"temporary third password"})).unwrap();
    let otherconfirmed = a
        .execute(
            "encryption.pair.confirm",
            json!({"receipt":otherpending["receipt"]}),
        )
        .unwrap();
    c.execute(
        "encryption.device.apply",
        json!({"manifest":otherconfirmed["manifest"]}),
    )
    .unwrap();
    let historical = a
        .execute(
            "encryption.encrypt",
            json!({"kind":"changes","plaintext_base64":"aGlzdG9yaWNhbA=="}),
        )
        .unwrap();
    let rotated=a.execute("encryption.device.revoke",json!({"device_id":pending["state"]["device_id"],"password":"temporary new space password"})).unwrap();
    c.execute(
        "encryption.device.apply",
        json!({"manifest":rotated["manifest"]}),
    )
    .unwrap();
    assert_eq!(
        c.execute(
            "encryption.decrypt",
            json!({"kind":"changes","envelope":historical["envelope"]})
        )
        .unwrap()["plaintext_base64"],
        "aGlzdG9yaWNhbA=="
    );
    assert_eq!(
        c.execute(
            "encryption.device.apply",
            json!({"manifest":otherconfirmed["manifest"]})
        )
        .unwrap_err()
        .code,
        "stale_device_state"
    );
    c.execute(
        "encryption.device.apply",
        json!({"manifest":rotated["manifest"]}),
    )
    .unwrap();
    assert_eq!(
        b.execute(
            "encryption.device.apply",
            json!({"manifest":rotated["manifest"]})
        )
        .unwrap_err()
        .code,
        "device_revoked"
    );
    let encrypted = a
        .execute(
            "encryption.encrypt",
            json!({"kind":"changes","plaintext_base64":"aGVsbG8="}),
        )
        .unwrap();
    assert_eq!(
        c.execute(
            "encryption.decrypt",
            json!({"kind":"changes","envelope":encrypted["envelope"]})
        )
        .unwrap()["plaintext_base64"],
        "aGVsbG8="
    );
    assert!(b
        .execute(
            "encryption.decrypt",
            json!({"kind":"changes","envelope":encrypted["envelope"]})
        )
        .is_err());
    for app in [&a, &b, &c] {
        app.execute("encryption.unpair", json!({})).unwrap();
    }
}
#[test]
#[ignore = "requires real OS credential store"]
fn recovery_restores_keys_and_password_change_preserves_existing_ciphertext() {
    let da = tempfile::tempdir().unwrap();
    let db = tempfile::tempdir().unwrap();
    let a = Application::open(&da.path().join("wardrobe.sqlite")).unwrap();
    let b = Application::open(&db.path().join("wardrobe.sqlite")).unwrap();
    let _ga = KeyCleanup(da.path().join("wardrobe.sqlite"));
    let _gb = KeyCleanup(db.path().join("wardrobe.sqlite"));
    let created = a
        .execute(
            "encryption.create",
            json!({"password":"old temporary password","device_name":"recover me"}),
        )
        .unwrap();
    let encrypted = a
        .execute(
            "encryption.encrypt",
            json!({"kind":"changes","plaintext_base64":"aGVsbG8="}),
        )
        .unwrap();
    let changed=a.execute("encryption.password.change",json!({"old_password":"old temporary password","new_password":"new temporary password"})).unwrap();
    a.execute("encryption.unpair", json!({})).unwrap();
    let wrong = b.execute(
        "encryption.restore",
        json!({"bundle":changed["recovery_bundle"],"password":"old temporary password"}),
    );
    assert_eq!(wrong.unwrap_err().code, "authentication_failed");
    b.execute(
        "encryption.restore",
        json!({"bundle":changed["recovery_bundle"],"recovery_secret":changed["recovery_secret"]}),
    )
    .unwrap();
    assert_eq!(
        b.execute(
            "encryption.decrypt",
            json!({"kind":"changes","envelope":encrypted["envelope"]})
        )
        .unwrap()["plaintext_base64"],
        "aGVsbG8="
    );
    assert!(
        created["recovery_secret"] != changed["recovery_secret"],
        "new recovery secret must differ"
    );
}
#[test]
#[ignore = "requires real OS credential store"]
fn two_connections_racing_confirmation_consume_invitation_once() {
    let da = tempfile::tempdir().unwrap();
    let db = tempfile::tempdir().unwrap();
    let ap = da.path().join("wardrobe.sqlite");
    let bp = db.path().join("wardrobe.sqlite");
    let a = Application::open(&ap).unwrap();
    let b = Application::open(&bp).unwrap();
    let _ga = KeyCleanup(ap.clone());
    let _gb = KeyCleanup(bp);
    a.execute(
        "encryption.create",
        json!({"password":"race temporary owner password","device_name":"race owner"}),
    )
    .unwrap();
    let offer = a
        .execute(
            "encryption.pair.offer",
            json!({"device_name":"race target"}),
        )
        .unwrap();
    let accepted=b.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"race temporary target password"})).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = ap.clone();
            let receipt = accepted["receipt"].clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let app = Application::open(&path).unwrap();
                barrier.wait();
                app.execute("encryption.pair.confirm", json!({"receipt":receipt}))
                    .map(|_| ())
                    .map_err(|e| e.code)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r,Err(code) if code=="invitation_consumed"))
            .count(),
        1
    );
}
#[test]
#[ignore = "requires real OS credential store"]
fn database_and_wal_do_not_persist_exported_recovery_or_unwrapped_private_keys() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use chacha20poly1305::{
        aead::{Aead, Payload},
        KeyInit, XChaCha20Poly1305, XNonce,
    };
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("wardrobe.sqlite");
    let app = Application::open(&path).unwrap();
    let _cleanup = KeyCleanup(path);
    let password = "storage privacy temporary password";
    let created = app
        .execute(
            "encryption.create",
            json!({"password":password,"device_name":"privacy fixture"}),
        )
        .unwrap();
    let bundle = &created["recovery_bundle"];
    let envelope = &bundle["recovery_wrap"];
    let recovery = STANDARD
        .decode(created["recovery_secret"].as_str().unwrap())
        .unwrap();
    let cipher = XChaCha20Poly1305::new_from_slice(&recovery).unwrap();
    let nonce = STANDARD
        .decode(envelope["nonce"].as_str().unwrap())
        .unwrap();
    let clear = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &STANDARD
                    .decode(envelope["ciphertext"].as_str().unwrap())
                    .unwrap(),
                aad: json!([
                    "wardrobe-e2ee",
                    1,
                    bundle["space_id"],
                    bundle["epoch"],
                    "recovery-wrap"
                ])
                .to_string()
                .as_bytes(),
            },
        )
        .unwrap();
    let wallet: serde_json::Value = serde_json::from_slice(&clear).unwrap();
    let private_strings = [
        wallet["device_secret"].as_str().unwrap(),
        wallet["owner_secret"].as_str().unwrap(),
        wallet["keys"][0]["key"].as_str().unwrap(),
        created["recovery_secret"].as_str().unwrap(),
        password,
    ];
    for file in std::fs::read_dir(d.path())
        .unwrap()
        .flatten()
        .filter(|f| f.file_type().unwrap().is_file())
    {
        let bytes = std::fs::read(file.path()).unwrap();
        for secret in private_strings {
            assert!(
                !bytes.windows(secret.len()).any(|b| b == secret.as_bytes()),
                "persisted artifact must not contain plaintext private material"
            );
        }
    }
}

#[test]
fn highest_database_version_repairs_missing_encryption_tables_without_downgrade() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wardrobe.sqlite");
    drop(Application::open(&path).unwrap());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE encryption_state; DROP TABLE encryption_pairings;")
        .unwrap();
    connection
        .pragma_update(None, "user_version", wardrobe_core::DATABASE_SCHEMA_VERSION)
        .unwrap();
    drop(connection);
    let app = Application::open(&path).unwrap();
    assert_eq!(
        app.execute("encryption.status", json!({})).unwrap()["enabled"],
        false
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    let tables: u64 = connection.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('encryption_state', 'encryption_pairings')", [], |r|r.get(0)).unwrap();
    assert_eq!(tables, 2);
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        wardrobe_core::DATABASE_SCHEMA_VERSION
    );
}

#[test]
#[ignore = "requires real OS credential store"]
fn failed_rotation_is_atomic_and_recovery_keeps_all_historical_epochs() {
    let owner_dir = tempfile::tempdir().unwrap();
    let peer_dir = tempfile::tempdir().unwrap();
    let restored_dir = tempfile::tempdir().unwrap();
    let owner_path = owner_dir.path().join("wardrobe.sqlite");
    let peer_path = peer_dir.path().join("wardrobe.sqlite");
    let restored_path = restored_dir.path().join("wardrobe.sqlite");
    let owner = Application::open(&owner_path).unwrap();
    let peer = Application::open(&peer_path).unwrap();
    let restored = Application::open(&restored_path).unwrap();
    let _owner_cleanup = KeyCleanup(owner_path.clone());
    let _peer_cleanup = KeyCleanup(peer_path);
    let _restored_cleanup = KeyCleanup(restored_path);
    owner
        .execute(
            "encryption.create",
            json!({"password":"atomic owner temporary password","device_name":"atomic owner"}),
        )
        .unwrap();
    let old = owner
        .execute(
            "encryption.encrypt",
            json!({"kind":"changes","plaintext_base64":"b2xk"}),
        )
        .unwrap();
    let offer = owner
        .execute(
            "encryption.pair.offer",
            json!({"device_name":"atomic peer"}),
        )
        .unwrap();
    let accepted = peer.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"atomic peer temporary password"})).unwrap();
    let confirmed = owner
        .execute(
            "encryption.pair.confirm",
            json!({"receipt":accepted["receipt"]}),
        )
        .unwrap();
    peer.execute(
        "encryption.device.apply",
        json!({"manifest":confirmed["manifest"]}),
    )
    .unwrap();
    let before = owner.execute("encryption.status", json!({})).unwrap();
    let db = rusqlite::Connection::open(&owner_path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_rotation BEFORE UPDATE ON encryption_state BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;").unwrap();
    assert!(owner.execute("encryption.device.revoke",json!({"device_id":accepted["state"]["device_id"],"password":"atomic rotated temporary password"})).is_err());
    assert_eq!(
        owner.execute("encryption.status", json!({})).unwrap(),
        before
    );
    assert_eq!(
        owner
            .execute(
                "encryption.encrypt",
                json!({"kind":"changes","plaintext_base64":"bmV3"})
            )
            .unwrap()["envelope"]["epoch"],
        1
    );
    db.execute_batch("DROP TRIGGER reject_rotation;").unwrap();
    owner.execute("encryption.device.revoke",json!({"device_id":accepted["state"]["device_id"],"password":"atomic rotated temporary password"})).unwrap();
    let current = owner
        .execute(
            "encryption.encrypt",
            json!({"kind":"changes","plaintext_base64":"bmV3"}),
        )
        .unwrap();
    assert_eq!(current["envelope"]["epoch"], 2);
    let changed = owner.execute("encryption.password.change",json!({"old_password":"atomic rotated temporary password","new_password":"atomic changed temporary password"})).unwrap();
    restored.execute("encryption.restore",json!({"bundle":changed["recovery_bundle"],"recovery_secret":changed["recovery_secret"]})).unwrap();
    for (ciphertext, plain) in [(&old, "b2xk"), (&current, "bmV3")] {
        assert_eq!(
            restored
                .execute(
                    "encryption.decrypt",
                    json!({"kind":"changes","envelope":ciphertext["envelope"]})
                )
                .unwrap()["plaintext_base64"],
            plain
        );
    }
}
