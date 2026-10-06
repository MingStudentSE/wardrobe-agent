use serde_json::json;
use wardrobe_core::Application;
#[test]
fn model_configuration_survives_restart_without_a_key_or_blocking_manual_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let saved=app.execute("provider.save",json!({"name":"本机模型","endpoint":"http://127.0.0.1:11434/v1","model":"local-model","capabilities":["text","vision"],"authentication":"none"})).unwrap();
    drop(app);
    let app = Application::open(&path).unwrap();
    let list = app.execute("provider.list", json!({})).unwrap();
    assert_eq!(list["items"][0]["id"], saved["id"]);
    assert_eq!(list["items"][0]["model"], "local-model");
    assert_eq!(list["items"][0]["capabilities"], json!(["text", "vision"]));
    assert!(list["items"][0].get("api_key").is_none());
    app.execute("space.default", json!({})).unwrap();
    app.execute(
        "clothing.create",
        json!({"member_id":"default","name":"无key衣服","category":"top"}),
    )
    .unwrap();
}
#[test]
#[ignore = "OS credential-store smoke test requires an unlocked desktop keychain/Secret Service"]
fn credentials_can_be_set_replaced_and_removed_without_echo_or_database_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = Application::open(&path).unwrap();
    let p=app.execute("provider.save",json!({"name":"测试","endpoint":"https://example.com/v1","model":"test","capabilities":["text"],"authentication":"bearer"})).unwrap();
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Ok(entry) = keyring::Entry::new("wardrobe-agent.provider", &self.0) {
                let _ = entry.delete_credential();
            }
        }
    }
    use sha2::{Digest, Sha256};
    let _cleanup = Cleanup(format!(
        "{}:{}",
        p["id"].as_str().unwrap(),
        Sha256::digest(p["endpoint"].as_str().unwrap().as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ));
    let temporary_secret = format!("disposable-{}", uuid::Uuid::now_v7());
    let stored = app
        .execute(
            "provider.credential.set",
            json!({"id":p["id"],"api_key":temporary_secret}),
        )
        .unwrap();
    assert_eq!(stored["configured"], true);
    assert!(!stored.to_string().contains(&temporary_secret));
    app.execute("provider.save",json!({"id":p["id"],"name":"更新端点","endpoint":"https://other.example.com/v1","model":"test","capabilities":["text"],"authentication":"bearer"})).unwrap();
    assert_eq!(
        app.execute("provider.credential.status", json!({"id":p["id"]}))
            .unwrap()["configured"],
        false
    );
    let removed = app
        .execute("provider.credential.remove", json!({"id":p["id"]}))
        .unwrap();
    assert_eq!(removed["configured"], false);
    assert_eq!(
        app.execute("provider.credential.status", json!({"id":p["id"]}))
            .unwrap()["configured"],
        false
    );
    drop(app);
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(!bytes
            .windows(temporary_secret.len())
            .any(|w| w == temporary_secret.as_bytes()));
    }
}
#[test]
fn connection_validation_uses_configured_model_and_reports_only_verified_capability() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 8192];
        let n = stream.read(&mut bytes).unwrap();
        let request = String::from_utf8_lossy(&bytes[..n]);
        assert!(request.contains("POST /v1/chat/completions"));
        assert!(request.contains("fixture-model"));
        let body = r#"{"choices":[{"message":{"content":"ok"}}],"usage":{"total_tokens":2}}"#;
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let p=app.execute("provider.save",json!({"name":"fixture","endpoint":endpoint,"model":"fixture-model","capabilities":["text","vision"],"authentication":"none"})).unwrap();
    let verified = app
        .execute("provider.validate", json!({"id":p["id"]}))
        .unwrap();
    assert_eq!(verified["verified_capabilities"], json!(["text"]));
    assert_eq!(verified["usage"]["total_tokens"], 2);
    server.join().unwrap();
}
#[test]
fn provider_configuration_rejects_insecure_urls_unknown_image_capability_and_embedded_keys() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    for (endpoint, cap, extra, code) in [
        ("http://example.com/v1", "text", false, "invalid_input"),
        (
            "https://user:secret@example.com/v1",
            "text",
            false,
            "invalid_input",
        ),
        (
            "https://example.com/v1?key=x",
            "text",
            false,
            "invalid_input",
        ),
        (
            "https://example.com/v1",
            "try_on",
            false,
            "unsupported_capability",
        ),
        ("https://example.com/v1", "text", true, "invalid_input"),
    ] {
        let mut input = json!({"name":"p","endpoint":endpoint,"model":"m","capabilities":[cap],"authentication":"none"});
        if extra {
            input["api_key"] = json!("not-allowed");
        }
        assert_eq!(app.execute("provider.save", input).unwrap_err().code, code);
    }
}
