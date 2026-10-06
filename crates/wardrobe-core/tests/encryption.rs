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
    let rotated=a.execute("encryption.device.revoke",json!({"device_id":pending["state"]["device_id"],"password":"temporary new space password"})).unwrap();
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
