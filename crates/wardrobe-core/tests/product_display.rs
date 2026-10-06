use serde_json::json;
use wardrobe_core::Application;
#[test]
fn dedicated_image_provider_preserves_adapter_and_rejects_chat_validation() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    let provider=app.execute("provider.save",json!({"name":"Seedream","endpoint":"http://127.0.0.1:45678/api/v3","model":"doubao-seedream-5-0-flash-260915","adapter":"ark_seedream","capabilities":["image_generation"],"authentication":"none"})).unwrap();
    assert_eq!(provider["adapter"], "ark_seedream");
    assert_eq!(
        app.execute(
            "provider.validate",
            json!({"id":provider["id"],"capability":"image_generation"})
        )
        .unwrap_err()
        .code,
        "unsupported_capability"
    );
}
#[test]
fn product_job_requires_explicit_upload_and_keeps_old_display() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 32, image::Rgb([200, 210, 220]))
        .save(&source)
        .unwrap();
    let asset = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"shirt","category":"top"}),
        )
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","asset_id":asset["id"],"clothing_id":clothing["id"]}),
    )
    .unwrap();
    let p=app.execute("provider.save",json!({"name":"Seedream","endpoint":"http://127.0.0.1:45678/api/v3","model":"doubao-seedream-5-0-flash-260915","adapter":"ark_seedream","capabilities":["image_generation"],"authentication":"none"})).unwrap();
    let input = json!({"member_id":"default","clothing_id":clothing["id"],"asset_id":asset["id"],"provider_id":p["id"],"operation_key":"product-one","upload_original":true});
    let job = app.execute("product.generate", input.clone()).unwrap();
    assert_eq!(job["state"], "queued");
    assert_eq!(
        job["id"],
        app.execute("product.generate", input).unwrap()["id"]
    );
    let unchanged = app
        .execute(
            "clothing.get",
            json!({"member_id":"default","id":clothing["id"]}),
        )
        .unwrap();
    assert_eq!(unchanged["display_asset_id"], asset["id"]);
}
#[test]
fn real_http_generation_is_preview_until_adopted_and_never_blindly_resubmits() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/api/v3", server.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        for n in 0..2 {
            let (mut stream, _) = server.accept().unwrap();
            let mut bytes = vec![];
            let mut buf = [0; 8192];
            let boundary;
            loop {
                let count = stream.read(&mut buf).unwrap();
                bytes.extend_from_slice(&buf[..count]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    boundary = i + 4;
                    break;
                }
            }
            let headers = String::from_utf8_lossy(&bytes[..boundary]).into_owned();
            let len = headers
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < boundary + len {
                let count = stream.read(&mut buf).unwrap();
                bytes.extend_from_slice(&buf[..count]);
            }
            let response = if n == 0 {
                assert!(headers.starts_with("GET /api/v3/models "));
                json!({"data":[{"id":"doubao-seedream-5-0-flash-260915"}]})
            } else {
                assert!(headers.starts_with("POST /api/v3/images/generations "));
                let body: serde_json::Value = serde_json::from_slice(&bytes[boundary..]).unwrap();
                assert!(body["image"]
                    .as_str()
                    .unwrap()
                    .starts_with("data:image/png;base64,"));
                assert!(body.get("sequential_image_generation").is_none());
                assert_eq!(body["size"], "2K");
                let mut png = std::io::Cursor::new(vec![]);
                image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                    80,
                    60,
                    image::Rgb([180, 190, 200]),
                ))
                .write_to(&mut png, image::ImageFormat::Png)
                .unwrap();
                use base64::Engine;
                json!({"data":[{"b64_json":base64::engine::general_purpose::STANDARD.encode(png.into_inner())}],"model":"doubao-seedream-5-0-flash-260915"})
            };
            let text = response.to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",text.len(),text).unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let app = Application::open(&db).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let path = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 32, image::Rgb([180, 190, 200]))
        .save(&path)
        .unwrap();
    let asset = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"Blue shirt","category":"top"}),
        )
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","asset_id":asset["id"],"clothing_id":clothing["id"]}),
    )
    .unwrap();
    let p=app.execute("provider.save",json!({"name":"Seedream","endpoint":endpoint,"model":"doubao-seedream-5-0-flash-260915","adapter":"ark_seedream","capabilities":["image_generation"],"authentication":"none"})).unwrap();
    let wear=app.execute("wear.record",json!({"member_id":"default","operation_key":"before-generation","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[clothing["id"]]})).unwrap();
    let job=app.execute("product.generate",json!({"member_id":"default","clothing_id":clothing["id"],"asset_id":asset["id"],"provider_id":p["id"],"operation_key":"one","upload_original":true})).unwrap();
    let done = app
        .execute(
            "product.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    assert_eq!(done["state"], "waiting_input");
    thread.join().unwrap();
    let preview = app
        .execute(
            "assets.get",
            json!({"member_id":"default","id":done["result"]["asset_id"]}),
        )
        .unwrap();
    assert_eq!(preview["state"], "preview");
    assert_eq!(preview["display"]["width"], 80);
    assert_eq!(
        app.execute("jobs.retry", json!({"member_id":"default","id":job["id"]}))
            .unwrap_err()
            .code,
        "submission_already_sent"
    );
    let concurrent = app
        .execute(
            "images.rotate",
            json!({"member_id":"default","asset_id":asset["id"],"degrees":90}),
        )
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","asset_id":concurrent["id"],"clothing_id":clothing["id"]}),
    )
    .unwrap();
    assert_eq!(
        app.execute(
            "product.adopt",
            json!({"member_id":"default","id":job["id"]})
        )
        .unwrap_err()
        .code,
        "display_changed"
    );
    app.execute(
        "images.main",
        json!({"member_id":"default","asset_id":asset["id"],"clothing_id":clothing["id"]}),
    )
    .unwrap();
    let adopted = app
        .execute(
            "product.adopt",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    assert_eq!(adopted["state"], "succeeded");
    drop(app);
    let app = Application::open(&db).unwrap();
    let c = app
        .execute(
            "clothing.get",
            json!({"member_id":"default","id":clothing["id"]}),
        )
        .unwrap();
    assert_eq!(c["name"], "Blue shirt");
    assert_eq!(c["display_asset_id"], preview["id"]);
    let historical = app
        .execute("wear.get", json!({"member_id":"default","id":wear["id"]}))
        .unwrap();
    assert_eq!(historical, wear);
    assert_eq!(
        historical["items"][0]["snapshot"]["display_asset_id"],
        asset["id"]
    );
    assert_eq!(
        app.execute(
            "assets.get",
            json!({"member_id":"default","id":asset["id"]})
        )
        .unwrap()["state"],
        "adopted"
    );
}
fn setup(
    endpoint: &str,
) -> (
    tempfile::TempDir,
    Application,
    serde_json::Value,
    serde_json::Value,
) {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let path = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 32, image::Rgb([100, 150, 200]))
        .save(&path)
        .unwrap();
    let a = app
        .execute("assets.import", json!({"member_id":"default","path":path}))
        .unwrap();
    let c = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"shirt","category":"top"}),
        )
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","asset_id":a["id"],"clothing_id":c["id"]}),
    )
    .unwrap();
    let p=app.execute("provider.save",json!({"name":"Seedream","endpoint":endpoint,"model":"doubao-seedream-5-0-flash-260915","adapter":"ark_seedream","capabilities":["image_generation"],"authentication":"none"})).unwrap();
    let j=app.execute("product.generate",json!({"member_id":"default","clothing_id":c["id"],"asset_id":a["id"],"provider_id":p["id"],"operation_key":"one","upload_original":true})).unwrap();
    (dir, app, j, c)
}
fn read_request(stream: &mut std::net::TcpStream) -> String {
    use std::io::Read;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buf = [0; 8192];
    loop {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..i]).into_owned();
            let len = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= i + 4 + len {
                return header;
            }
        }
    }
}
fn reply(stream: &mut std::net::TcpStream, value: serde_json::Value) {
    use std::io::Write;
    let body = value.to_string();
    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
}
#[test]
fn lost_provider_response_persists_unknown_and_refuses_same_job_after_restart() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/api/v3", server.local_addr().unwrap());
    let server_thread = std::thread::spawn(move || {
        let (mut s, _) = server.accept().unwrap();
        assert!(read_request(&mut s).starts_with("GET"));
        reply(
            &mut s,
            json!({"data":[{"id":"doubao-seedream-5-0-flash-260915"}]}),
        );
        drop(s);
        let (mut s, _) = server.accept().unwrap();
        assert!(read_request(&mut s).starts_with("POST"));
        drop(s);
    });
    let (dir, app, job, clothing) = setup(&endpoint);
    let result = app
        .execute(
            "product.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    assert_eq!(result["state"], "waiting_input");
    assert_eq!(result["error"]["code"], "provider_outcome_unknown");
    assert!(result["result"].is_null());
    server_thread.join().unwrap();
    drop(app);
    let app = Application::open(&dir.path().join("db")).unwrap();
    let result = app
        .execute(
            "product.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    assert_eq!(result["attempts"], 1);
    assert_eq!(
        app.execute("jobs.retry", json!({"member_id":"default","id":job["id"]}))
            .unwrap_err()
            .code,
        "submission_already_sent"
    );
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":"default","id":clothing["id"]})
        )
        .unwrap()["display_asset_id"],
        job["payload"]["base_display_asset_id"]
    );
}
#[test]
fn cancelled_inflight_generation_never_creates_result_or_changes_clothing() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/api/v3", server.local_addr().unwrap());
    let (sent, received) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let server_thread = std::thread::spawn(move || {
        let (mut s, _) = server.accept().unwrap();
        read_request(&mut s);
        reply(
            &mut s,
            json!({"data":[{"id":"doubao-seedream-5-0-flash-260915"}]}),
        );
        drop(s);
        let (mut s, _) = server.accept().unwrap();
        read_request(&mut s);
        sent.send(()).unwrap();
        resume
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let mut png = std::io::Cursor::new(vec![]);
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            80,
            60,
            image::Rgb([180, 190, 200]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        use base64::Engine;
        reply(
            &mut s,
            json!({"data":[{"b64_json":base64::engine::general_purpose::STANDARD.encode(png.into_inner())}]}),
        );
    });
    let (dir, app, job, c) = setup(&endpoint);
    let db = dir.path().join("db");
    let id = job["id"].clone();
    let worker = std::thread::spawn(move || {
        Application::open(&db)
            .unwrap()
            .execute("product.process", json!({"member_id":"default","id":id}))
            .unwrap()
    });
    received
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    app.execute("jobs.cancel", json!({"member_id":"default","id":job["id"]}))
        .unwrap();
    release.send(()).unwrap();
    assert_eq!(worker.join().unwrap()["state"], "cancelled");
    server_thread.join().unwrap();
    let result = app
        .execute("jobs.get", json!({"member_id":"default","id":job["id"]}))
        .unwrap();
    assert!(result["result"].is_null());
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":"default","id":c["id"]}))
            .unwrap()["display_asset_id"],
        job["payload"]["base_display_asset_id"]
    );
}
#[test]
fn limited_client_requires_explicit_generation_scope_and_member() {
    let (_dir, app, job, _) = setup("http://127.0.0.1:45678/api/v3");
    let member = job["member_id"].clone();
    let other = app
        .execute("member.create", json!({"name":"Other"}))
        .unwrap();
    let grant=app.execute("client.authorize",json!({"name":"Image client","mode":"limited","operations":["product.process","jobs.get"],"member_ids":[member],"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap();
    assert_eq!(
        app.execute_as_client(
            token,
            "product.adopt",
            json!({"member_id":member,"id":job["id"]})
        )
        .unwrap_err()
        .code,
        "scope_denied"
    );
    assert_eq!(
        app.execute_as_client(
            token,
            "product.process",
            json!({"member_id":other["member_id"],"id":job["id"]})
        )
        .unwrap_err()
        .code,
        "member_denied"
    );
    assert_eq!(
        app.execute_as_client(
            token,
            "jobs.get",
            json!({"member_id":member,"id":job["id"]})
        )
        .unwrap()["id"],
        job["id"]
    );
}
