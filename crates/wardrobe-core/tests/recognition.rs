use serde_json::json;
use wardrobe_core::Application;
#[test]
fn explicit_recognition_records_selected_provider_and_images_without_changing_manual_facts() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let path = dir.path().join("generated.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([240, 240, 240]))
        .save(&path)
        .unwrap();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"我的手工名称","category":"top"}),
        )
        .unwrap();
    let asset = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":clothing["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    let provider=app.execute("provider.save",json!({"name":"本地视觉","endpoint":"http://127.0.0.1:18083/v1","model":"wardrobe-smolvlm","capabilities":["vision"],"authentication":"none"})).unwrap();
    let input = json!({"member_id":"default","operation_key":"explicit-analysis","provider_id":provider["id"],"clothing_id":clothing["id"],"asset_ids":[asset["id"]],"upload_role":"product_display","confirm_upload":true});
    let created = app.execute("recognition.create", input.clone()).unwrap();
    assert_eq!(created["state"], "queued");
    assert_eq!(created["payload"]["provider"]["model"], "wardrobe-smolvlm");
    assert_eq!(created["payload"]["asset_ids"], json!([asset["id"]]));
    assert_eq!(app.execute("recognition.create", input).unwrap(), created);
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":"default","id":clothing["id"]})
        )
        .unwrap()["name"],
        "我的手工名称"
    );
}

#[test]
fn actual_http_proposal_requires_review_and_reanalysis_preserves_manual_edits() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut s, _) = listener.accept().unwrap();
            let mut b = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                b.extend_from_slice(&chunk[..n]);
                if let Some(split) = b.windows(4).position(|w| w == b"\r\n\r\n") {
                    let h = String::from_utf8_lossy(&b[..split]);
                    let len: usize = h
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|v| v.parse().unwrap())
                        })
                        .unwrap();
                    if b.len() >= split + 4 + len {
                        let req: serde_json::Value =
                            serde_json::from_slice(&b[split + 4..split + 4 + len]).unwrap();
                        assert_eq!(req["model"], "http-fixture");
                        assert!(req["messages"][0]["content"][1]["image_url"]["url"]
                            .as_str()
                            .unwrap()
                            .starts_with("data:image/png;base64,"));
                        break;
                    }
                }
            }
            let body=json!({"model":"http-fixture","choices":[{"message":{"content":"White shirt; material and brand cannot be established."}}],"usage":{"prompt_tokens":25,"completion_tokens":12}}).to_string();
            write!(s,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let path = dir.path().join("test.png");
    image::RgbImage::new(16, 16).save(&path).unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"手工","category":"top"}),
        )
        .unwrap();
    let a = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":c["id"],"asset_id":a["id"]}),
    )
    .unwrap();
    let p=app.execute("provider.save",json!({"name":"HTTP fixture","endpoint":endpoint,"model":"http-fixture","capabilities":["vision"],"authentication":"none"})).unwrap();
    let j=app.execute("recognition.create",json!({"member_id":"default","clothing_id":c["id"],"asset_ids":[a["id"]],"provider_id":p["id"],"confirm_upload":true,"upload_role":"product_display","operation_key":"review"})).unwrap();
    let done = app
        .execute(
            "recognition.process",
            json!({"member_id":"default","id":j["id"]}),
        )
        .unwrap();

    assert_eq!(done["state"], "succeeded");
    let suggestion = app
        .execute(
            "recognition.get",
            json!({"member_id":"default","id":done["result"]["suggestion_id"]}),
        )
        .unwrap();
    assert_eq!(suggestion["state"], "pending_review");
    assert_eq!(suggestion["suggested_facts"]["category"], "top");
    assert_eq!(suggestion["suggested_facts"]["color"], json!(["white"]));
    assert_eq!(suggestion["provenance"]["model"], "http-fixture");
    assert_eq!(
        suggestion["raw_text"],
        "White shirt; material and brand cannot be established."
    );
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":"default","id":c["id"]}))
            .unwrap()["name"],
        "手工"
    );
    assert_eq!(app.execute("recognition.review",json!({"member_id":"default","id":suggestion["id"],"facts":{"category":"invented"},"confirm_review":true})).unwrap_err().code,"invalid_input");
    let approved=app.execute("recognition.review",json!({"member_id":"default","id":suggestion["id"],"facts":{"description":"白衬衫"},"confirm_review":true})).unwrap();
    assert_eq!(approved["state"], "reviewed");
    assert_eq!(app.execute("recognition.review",json!({"member_id":"default","id":suggestion["id"],"facts":{"description":"白衬衫"},"confirm_review":true})).unwrap(),approved);
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":"default","id":c["id"]}))
            .unwrap()["description"],
        "白衬衫"
    );
    let j2=app.execute("recognition.create",json!({"member_id":"default","clothing_id":c["id"],"asset_ids":[a["id"]],"provider_id":p["id"],"confirm_upload":true,"upload_role":"product_display","operation_key":"reanalysis"})).unwrap();
    let second = app
        .execute(
            "recognition.process",
            json!({"member_id":"default","id":j2["id"]}),
        )
        .unwrap();
    server.join().unwrap();
    let suggestion = app
        .execute(
            "recognition.get",
            json!({"member_id":"default","id":second["result"]["suggestion_id"]}),
        )
        .unwrap();
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":"default","id":c["id"]}))
            .unwrap()["description"],
        "白衬衫"
    );
    app.execute(
        "clothing.update",
        json!({"member_id":"default","id":c["id"],"name":"后改手工"}),
    )
    .unwrap();
    let err=app.execute("recognition.review",json!({"member_id":"default","id":suggestion["id"],"facts":{"description":"白衬衫"},"confirm_review":true})).unwrap_err();
    assert_eq!(err.code, "review_conflict");
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":"default","id":c["id"]}))
            .unwrap()["name"],
        "后改手工"
    );
}

#[test]
fn batch_failure_leaves_independent_manual_drafts_saveable() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let path = dir.path().join("test.png");
    image::RgbImage::new(16, 16).save(&path).unwrap();
    let batch = app
        .execute(
            "imports.create",
            json!({"member_id":"default","operation_key":"batch","paths":[path]}),
        )
        .unwrap();
    let batch = app
        .execute(
            "imports.process",
            json!({"member_id":"default","id":batch["id"],"item_id":batch["items"][0]["id"]}),
        )
        .unwrap();
    let p=app.execute("provider.save",json!({"name":"offline","endpoint":"http://127.0.0.1:1/v1","model":"offline","capabilities":["vision"],"authentication":"none"})).unwrap();
    let input = json!({"member_id":"default","operation_key":"batch-analysis","provider_id":p["id"],"batch_id":batch["id"],"draft_id":batch["drafts"][0]["id"],"asset_ids":[batch["items"][0]["asset_id"]],"upload_role":"product_display","confirm_upload":true});
    let job = app.execute("recognition.create", input).unwrap();
    let failed = app
        .execute(
            "recognition.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    assert_eq!(failed["state"], "waiting_input");
    app.execute("jobs.cancel", json!({"member_id":"default","id":job["id"]}))
        .unwrap();
    assert_eq!(
        app.execute("jobs.retry", json!({"member_id":"default","id":job["id"]}))
            .unwrap_err()
            .code,
        "submission_already_sent"
    );
    let saved=app.execute("imports.commit",json!({"member_id":"default","id":batch["id"],"draft_id":batch["drafts"][0]["id"],"operation_key":"manual-batch","facts":{"name":"手工可保存","category":"top"}})).unwrap();
    assert_eq!(saved["name"], "手工可保存");
}

#[test]
fn restart_after_submission_never_posts_again_and_member_scope_precedes_network() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let p=app.execute("provider.save",json!({"name":"no request","endpoint":format!("http://{}/v1",listener.local_addr().unwrap()),"model":"fixture","capabilities":["vision"],"authentication":"none"})).unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"人工","category":"top"}),
        )
        .unwrap();
    let path = dir.path().join("test.png");
    image::RgbImage::new(16, 16).save(&path).unwrap();
    let a = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":c["id"],"asset_id":a["id"]}),
    )
    .unwrap();
    let mut job=app.execute("recognition.create",json!({"member_id":"default","operation_key":"crash","clothing_id":c["id"],"provider_id":p["id"],"asset_ids":[a["id"]],"upload_role":"original","confirm_upload":true})).unwrap();
    let other = app
        .execute("member.create", json!({"name":"隔离成员"}))
        .unwrap();
    assert_eq!(
        app.execute(
            "recognition.process",
            json!({"member_id":other["member_id"],"id":job["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    // Durable crash fixture: accepted submission marker and expired lease survive restart.
    job["state"] = json!("running");
    job["payload"]["submission"] = json!("submitted");
    job["lease_expires"] = json!(0);
    drop(app);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE jobs SET document=?1 WHERE id=?2",
        rusqlite::params![job.to_string(), job["id"].as_str()],
    )
    .unwrap();
    drop(conn);
    let app = Application::open(&db).unwrap();
    let result = app
        .execute(
            "recognition.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    assert_eq!(result["state"], "waiting_input");
    assert_eq!(result["error"]["code"], "submission_unknown");
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
fn read_http(stream: &mut std::net::TcpStream) {
    use std::io::Read;
    let mut bytes = vec![];
    loop {
        let mut chunk = [0; 4096];
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(split) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let len: usize = String::from_utf8_lossy(&bytes[..split])
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length: ")
                        .map(|v| v.parse().unwrap())
                })
                .unwrap();
            if bytes.len() >= split + 4 + len {
                return;
            }
        }
    }
}
fn network_job(app: &Application, path: &std::path::Path, endpoint: &str) -> serde_json::Value {
    image::RgbImage::new(16, 16).save(path).unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"手工事实","category":"top"}),
        )
        .unwrap();
    let a = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":c["id"],"asset_id":a["id"]}),
    )
    .unwrap();
    let p=app.execute("provider.save",json!({"name":"HTTP boundary","endpoint":endpoint,"model":"fixture","capabilities":["vision"],"authentication":"none"})).unwrap();
    app.execute("recognition.create",json!({"member_id":"default","operation_key":"boundary","clothing_id":c["id"],"provider_id":p["id"],"asset_ids":[a["id"]],"upload_role":"product_display","confirm_upload":true})).unwrap()
}
#[test]
fn truncated_submission_is_durable_unknown_and_cannot_blind_retry() {
    use std::io::Write;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http(&mut s);
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 9999\r\nConnection: close\r\n\r\n{\"choices\":").unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    let j = network_job(&app, &dir.path().join("fixture.png"), &endpoint);
    let out = app
        .execute(
            "recognition.process",
            json!({"member_id":"default","id":j["id"]}),
        )
        .unwrap();
    server.join().unwrap();
    assert_eq!(out["state"], "waiting_input");
    assert_eq!(out["error"]["code"], "submission_unknown");
    drop(app);
    let app = Application::open(&db).unwrap();
    app.execute("jobs.cancel", json!({"member_id":"default","id":j["id"]}))
        .unwrap();
    assert_eq!(
        app.execute("jobs.retry", json!({"member_id":"default","id":j["id"]}))
            .unwrap_err()
            .code,
        "submission_already_sent"
    );
    assert!(app
        .execute("recognition.list", json!({"member_id":"default"}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn cancellation_while_http_runs_cannot_publish_proposal() {
    use std::io::Write;
    for status in ["200 OK", "429 Too Many Requests"] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let (sent, received) = std::sync::mpsc::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_http(&mut s);
            sent.send(()).unwrap();
            wait.recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            let body = r#"{"choices":[{"message":{"content":"a white shirt"}}]}"#;
            write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db");
        let app = Application::open(&db).unwrap();
        let j = network_job(&app, &dir.path().join("fixture.png"), &endpoint);
        let worker_db = db.clone();
        let id = j["id"].clone();
        let worker = std::thread::spawn(move || {
            Application::open(&worker_db).unwrap().execute(
                "recognition.process",
                json!({"member_id":"default","id":id}),
            )
        });
        received
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        app.execute("jobs.cancel", json!({"member_id":"default","id":j["id"]}))
            .unwrap();
        release.send(()).unwrap();
        assert_eq!(worker.join().unwrap().unwrap_err().code, "lease_lost");
        server.join().unwrap();
        assert_eq!(
            app.execute("jobs.get", json!({"member_id":"default","id":j["id"]}))
                .unwrap()["state"],
            "cancelled"
        );
        assert!(app
            .execute("recognition.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
