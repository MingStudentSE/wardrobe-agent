use serde_json::{json, Value};
use wardrobe_core::Application;
fn setup() -> (tempfile::TempDir, Application, Value, Value) {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let path = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 32, image::Rgb([200, 210, 220]))
        .save(&path)
        .unwrap();
    let mut items = vec![];
    for name in ["shirt", "pants"] {
        let c = app
            .execute(
                "clothing.create",
                json!({"member_id":"default","name":name,"category":"top"}),
            )
            .unwrap();
        let a = app
            .execute("assets.import", json!({"member_id":"default","path":path}))
            .unwrap();
        app.execute(
            "assets.adopt",
            json!({"member_id":"default","asset_id":a["id"],"clothing_id":c["id"]}),
        )
        .unwrap();
        items.push(json!({"clothing_id":c["id"],"role":name,"layer":0}));
    }
    let outfit = app
        .execute(
            "outfit.create",
            json!({"member_id":"default","name":"Two pieces","items":items}),
        )
        .unwrap();
    let p=app.execute("provider.save",json!({"name":"Seedream","endpoint":"http://127.0.0.1:45678/api/v3","model":"doubao-seedream-5-0-flash-260915","adapter":"ark_seedream","capabilities":["image_generation"],"authentication":"none"})).unwrap();
    (dir, app, outfit, p)
}
#[test]
fn person_is_private_and_tryon_preflight_requires_consent_and_all_garments() {
    let (_dir, app, outfit, p) = setup();
    let person=app.execute("tryon.person.import",json!({"member_id":"default","path":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fictional-model.png"),"person_kind":"model","rights_confirmed":true,"operation_key":"person"})).unwrap();
    assert_eq!(person["result"]["sync_scope"], "local_only");
    assert!(!app
        .execute("assets.sync_manifest", json!({"member_id":"default"}))
        .unwrap()
        .to_string()
        .contains(person["result"]["blob_hash"].as_str().unwrap()));
    let input = json!({"member_id":"default","person_id":person["id"],"outfit_id":outfit["id"],"version":1,"provider_id":p["id"],"operation_key":"tryon","upload_images":true});
    assert_eq!(
        app.execute("tryon.generate", input.clone())
            .unwrap_err()
            .code,
        "unsupported_capability"
    );
    app.execute("provider.save",json!({"id":p["id"],"name":"Try-on","endpoint":p["endpoint"],"model":p["model"],"adapter":"ark_seedream","capabilities":["image_generation","virtual_tryon"],"authentication":"none"})).unwrap();
    let mut no = input.clone();
    no["upload_images"] = json!(false);
    assert_eq!(
        app.execute("tryon.generate", no).unwrap_err().code,
        "upload_consent_required"
    );
    let job = app.execute("tryon.generate", input).unwrap();
    assert_eq!(job["kind"], "virtual_tryon");
    assert_eq!(job["payload"]["inputs"].as_array().unwrap().len(), 2);
    assert_eq!(job["payload"]["person"]["id"], person["id"]);
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
                return String::from_utf8_lossy(&bytes).into_owned();
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
fn person_first_http_result_compare_save_remains_private_after_restart() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/api/v3", server.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = server.accept().unwrap();
        assert!(read_request(&mut stream).starts_with("GET"));
        reply(
            &mut stream,
            json!({"data":[{"id":"doubao-seedream-5-0-flash-260915"}]}),
        );
        drop(stream);
        let (mut stream, _) = server.accept().unwrap();
        let request = read_request(&mut stream);
        let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["image"].as_array().unwrap().len(), 3);
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(
                body["image"][0]
                    .as_str()
                    .unwrap()
                    .split(',')
                    .nth(1)
                    .unwrap(),
            )
            .unwrap();
        let person = image::load_from_memory(&bytes).unwrap();
        assert_eq!((person.width(), person.height()), (96, 160));
        assert!(body["prompt"]
            .as_str()
            .unwrap()
            .contains("wear EVERY garment"));

        assert!(body["image"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.as_str().unwrap().starts_with("data:image/png;base64,")));
        assert!(body.get("sequential_image_generation").is_none());
        assert_eq!(body["size"], "2K");
        let mut png = std::io::Cursor::new(vec![]);
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            96,
            64,
            image::Rgb([180, 190, 200]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        reply(
            &mut stream,
            json!({"data":[{"b64_json":base64::engine::general_purpose::STANDARD.encode(png.into_inner())}]}),
        );
    });
    let (dir, app, outfit, p) = setup();
    app.execute("provider.save",json!({"id":p["id"],"name":"Seedream","endpoint":endpoint,"model":p["model"],"adapter":"ark_seedream","capabilities":["image_generation","virtual_tryon"],"authentication":"none"})).unwrap();
    let person=app.execute("tryon.person.import",json!({"member_id":"default","path":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fictional-model.png"),"person_kind":"self","rights_confirmed":true,"operation_key":"self"})).unwrap();
    let job=app.execute("tryon.generate",json!({"member_id":"default","person_id":person["id"],"outfit_id":outfit["id"],"version":1,"provider_id":p["id"],"operation_key":"two-garments","upload_images":true})).unwrap();
    let done = app
        .execute(
            "tryon.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    thread.join().unwrap();
    assert_eq!(done["state"], "waiting_input");
    assert_eq!(done["result"]["sync_scope"], "local_only");
    assert_eq!(done["result"]["role"], "tryon");
    assert_eq!(done["result"]["width"], 96);
    let comparison = app
        .execute(
            "tryon.compare",
            json!({"member_id":"default","ids":[job["id"]]}),
        )
        .unwrap();
    assert_eq!(comparison["items"].as_array().unwrap().len(), 1);
    assert!(comparison["items"][0]["before"]["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));
    let preview = app
        .execute(
            "tryon.preview",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    assert!(preview["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/webp;base64,"));
    assert_eq!(
        app.execute("jobs.retry", json!({"member_id":"default","id":job["id"]}))
            .unwrap_err()
            .code,
        "submission_already_sent"
    );
    app.execute("tryon.save", json!({"member_id":"default","id":job["id"]}))
        .unwrap();
    drop(app);
    let app = Application::open(&dir.path().join("db")).unwrap();
    assert_eq!(
        app.execute("jobs.get", json!({"member_id":"default","id":job["id"]}))
            .unwrap()["state"],
        "succeeded"
    );
    assert_eq!(
        app.execute(
            "outfit.get",
            json!({"member_id":"default","id":outfit["id"]})
        )
        .unwrap(),
        outfit
    );
    assert_eq!(
        app.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    assert!(!app
        .execute("assets.sync_manifest", json!({"member_id":"default"}))
        .unwrap()
        .to_string()
        .contains(done["result"]["blob_hash"].as_str().unwrap()));
}

fn tryon_setup() -> (tempfile::TempDir, Application, Value, Value) {
    let (d, a, o, p) = setup();
    let p=a.execute("provider.save",json!({"id":p["id"],"name":"Tryon","endpoint":p["endpoint"],"model":p["model"],"adapter":"ark_seedream","capabilities":["image_generation","virtual_tryon"],"authentication":"none"})).unwrap();
    (d, a, o, p)
}
#[test]
fn all_inputs_preflight_rejects_missing_oversized_count_and_small_resolution_without_truncation() {
    let (_dir, app, outfit, p) = tryon_setup();
    let person=app.execute("tryon.person.import",json!({"member_id":"default","path":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fictional-model.png"),"person_kind":"model","rights_confirmed":true,"operation_key":"person"})).unwrap();
    let original = &outfit["items"][0];
    let one = json!({"clothing_id":original["clothing_id"],"role":"top","layer":0});
    let single = app
        .execute(
            "outfit.create",
            json!({"member_id":"default","name":"Single garment","items":[one]}),
        )
        .unwrap();
    let make = |o: &Value, key: &str| json!({"member_id":"default","person_id":person["id"],"outfit_id":o["id"],"version":1,"provider_id":p["id"],"operation_key":key,"upload_images":true});
    let single_job = app
        .execute("tryon.generate", make(&single, "single"))
        .unwrap();
    assert_eq!(single_job["payload"]["inputs"].as_array().unwrap().len(), 1);
    let missing = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"Missing picture","category":"top"}),
        )
        .unwrap();
    let unpictured=app.execute("outfit.create",json!({"member_id":"default","name":"Missing garment photo","items":[one,{"clothing_id":missing["id"],"role":"bottom","layer":1}]})).unwrap();
    assert_eq!(
        app.execute("tryon.generate", make(&unpictured, "missing"))
            .unwrap_err()
            .code,
        "image_missing"
    );
    let mut many = vec![];
    for i in 0..11 {
        let c = app
            .execute(
                "clothing.create",
                json!({"member_id":"default","name":format!("Many {i}"),"category":"top"}),
            )
            .unwrap();
        many.push(json!({"clothing_id":c["id"],"role":"other","layer":i}));
    }
    let oversized = app
        .execute(
            "outfit.create",
            json!({"member_id":"default","name":"Eleven pieces","items":many}),
        )
        .unwrap();
    assert_eq!(
        app.execute("tryon.generate", make(&oversized, "many"))
            .unwrap_err()
            .code,
        "image_count_limit"
    );
    let tiny_path = _dir.path().join("tiny.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([255, 255, 255]))
        .save(&tiny_path)
        .unwrap();
    let tiny_asset = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":tiny_path}),
        )
        .unwrap();
    let tiny_clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"Tiny resolution","category":"top"}),
        )
        .unwrap();
    app.execute("assets.adopt",json!({"member_id":"default","clothing_id":tiny_clothing["id"],"asset_id":tiny_asset["id"]})).unwrap();
    let tiny=app.execute("outfit.create",json!({"member_id":"default","name":"Tiny source","items":[{"clothing_id":tiny_clothing["id"],"role":"top","layer":0}]})).unwrap();
    assert_eq!(
        app.execute("tryon.generate", make(&tiny, "tiny"))
            .unwrap_err()
            .code,
        "image_limit"
    );
    assert_eq!(
        app.execute("jobs.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let fallback = app
        .execute(
            "outfit.render_local",
            json!({"member_id":"default","id":unpictured["id"]}),
        )
        .unwrap();
    assert_eq!(fallback["missing_images"], 1);
}
#[test]
fn immutable_version_inputs_and_scope_survive_later_template_edits() {
    let (_dir, app, outfit, p) = tryon_setup();
    let person=app.execute("tryon.person.import",json!({"member_id":"default","path":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fictional-model.png"),"person_kind":"model","rights_confirmed":true,"operation_key":"person"})).unwrap();
    let changed=app.execute("outfit.update",json!({"member_id":"default","id":outfit["id"],"expected_version":1,"name":"Changed now","items":[{"clothing_id":outfit["items"][0]["clothing_id"],"role":"top","layer":0}]})).unwrap();
    assert_eq!(changed["version"], 2);
    let job=app.execute("tryon.generate",json!({"member_id":"default","person_id":person["id"],"outfit_id":outfit["id"],"version":1,"provider_id":p["id"],"operation_key":"historical","upload_images":true})).unwrap();
    assert_eq!(job["payload"]["inputs"].as_array().unwrap().len(), 2);
    assert_eq!(
        job["payload"]["inputs"][1]["asset_id"],
        outfit["items"][1]["snapshot"]["display_asset_id"]
    );
    let grant=app.execute("client.authorize",json!({"name":"Flatlay reader","mode":"limited","operations":["tryon.preview"],"member_ids":[job["member_id"]],"expires_at":null})).unwrap();
    assert_eq!(
        app.execute_as_client(
            grant["token"].as_str().unwrap(),
            "tryon.generate",
            json!({"member_id":"default"})
        )
        .unwrap_err()
        .code,
        "scope_denied"
    );
    let other = app
        .execute("member.create", json!({"name":"Other"}))
        .unwrap();
    assert_eq!(
        app.execute_as_client(
            grant["token"].as_str().unwrap(),
            "tryon.preview",
            json!({"member_id":other["member_id"],"id":job["id"]})
        )
        .unwrap_err()
        .code,
        "member_denied"
    );
}
#[test]
fn provider_failure_never_loses_saved_outfit_or_offline_collage_and_does_not_resubmit() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/api/v3", server.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let (mut s, _) = server.accept().unwrap();
        read_request(&mut s);
        reply(
            &mut s,
            json!({"data":[{"id":"doubao-seedream-5-0-flash-260915"}]}),
        );
        drop(s);
        let (mut s, _) = server.accept().unwrap();
        read_request(&mut s);
        drop(s);
    });
    let (dir, app, outfit, p) = tryon_setup();
    let person=app.execute("tryon.person.import",json!({"member_id":"default","path":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fictional-model.png"),"person_kind":"self","rights_confirmed":true,"operation_key":"person"})).unwrap();
    app.execute("provider.save",json!({"id":p["id"],"name":"Seedream","endpoint":endpoint,"model":p["model"],"adapter":"ark_seedream","capabilities":["image_generation","virtual_tryon"],"authentication":"none"})).unwrap();
    let before = app
        .execute(
            "outfit.render_local",
            json!({"member_id":"default","id":outfit["id"]}),
        )
        .unwrap();
    let job=app.execute("tryon.generate",json!({"member_id":"default","person_id":person["id"],"outfit_id":outfit["id"],"version":1,"provider_id":p["id"],"operation_key":"uncertain","upload_images":true})).unwrap();
    let done = app
        .execute(
            "tryon.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    thread.join().unwrap();
    assert_eq!(done["state"], "waiting_input");
    assert_eq!(done["error"]["code"], "provider_outcome_unknown");
    drop(app);
    let app = Application::open(&dir.path().join("db")).unwrap();
    assert_eq!(
        app.execute(
            "tryon.process",
            json!({"member_id":"default","id":job["id"]})
        )
        .unwrap()["attempts"],
        1
    );
    assert_eq!(
        app.execute("jobs.retry", json!({"member_id":"default","id":job["id"]}))
            .unwrap_err()
            .code,
        "submission_already_sent"
    );
    assert_eq!(
        app.execute(
            "outfit.get",
            json!({"member_id":"default","id":outfit["id"]})
        )
        .unwrap(),
        outfit
    );
    assert_eq!(
        app.execute(
            "outfit.render_local",
            json!({"member_id":"default","id":outfit["id"]})
        )
        .unwrap(),
        before
    );
}
#[test]
fn cancelled_inflight_generation_never_creates_result_or_changes_outfit() {
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
    let (dir, app, outfit, p) = tryon_setup();
    app.execute("provider.save",json!({"id":p["id"],"name":"Tryon","endpoint":endpoint,"model":p["model"],"adapter":"ark_seedream","capabilities":["image_generation","virtual_tryon"],"authentication":"none"})).unwrap();
    let person=app.execute("tryon.person.import",json!({"member_id":"default","path":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fictional-model.png"),"person_kind":"model","rights_confirmed":true,"operation_key":"person"})).unwrap();
    let job=app.execute("tryon.generate",json!({"member_id":"default","person_id":person["id"],"outfit_id":outfit["id"],"version":1,"provider_id":p["id"],"operation_key":"cancel","upload_images":true})).unwrap();
    let db = dir.path().join("db");
    let id = job["id"].clone();
    let worker = std::thread::spawn(move || {
        Application::open(&db)
            .unwrap()
            .execute("tryon.process", json!({"member_id":"default","id":id}))
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
        app.execute(
            "outfit.get",
            json!({"member_id":"default","id":outfit["id"]})
        )
        .unwrap(),
        outfit
    );
    assert_eq!(
        app.execute("jobs.retry", json!({"member_id":"default","id":job["id"]}))
            .unwrap_err()
            .code,
        "submission_already_sent"
    );
}

#[test]
fn person_rights_scope_and_ten_total_reference_boundary_are_enforced_before_submission() {
    let (dir, app, _outfit, p) = tryon_setup();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fictional-model.png");
    let mut input = json!({"member_id":"default","path":path,"person_kind":"model","rights_confirmed":false,"operation_key":"person"});
    assert_eq!(
        app.execute("tryon.person.import", input.clone())
            .unwrap_err()
            .code,
        "person_consent_required"
    );
    input["rights_confirmed"] = json!(true);
    let person = app.execute("tryon.person.import", input.clone()).unwrap();
    assert_eq!(
        app.execute("tryon.person.import", input.clone()).unwrap()["id"],
        person["id"]
    );
    input["person_kind"] = json!("self");
    assert_eq!(
        app.execute("tryon.person.import", input).unwrap_err().code,
        "idempotency_conflict"
    );
    let other = app
        .execute("member.create", json!({"name":"Other"}))
        .unwrap();
    assert_eq!(
        app.execute(
            "tryon.person.preview",
            json!({"member_id":other["member_id"],"id":person["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    let mut items = vec![];
    for i in 0..10 {
        let c = app
            .execute(
                "clothing.create",
                json!({"member_id":"default","name":format!("Piece{i}"),"category":"top"}),
            )
            .unwrap();
        let a = app
            .execute(
                "assets.import",
                json!({"member_id":"default","path":dir.path().join("shirt.png")}),
            )
            .unwrap();
        app.execute(
            "assets.adopt",
            json!({"member_id":"default","clothing_id":c["id"],"asset_id":a["id"]}),
        )
        .unwrap();
        items.push(json!({"clothing_id":c["id"],"role":"other","layer":i}));
    }
    let nine = app
        .execute(
            "outfit.create",
            json!({"member_id":"default","name":"Nine","items":&items[..9]}),
        )
        .unwrap();
    let ten = app
        .execute(
            "outfit.create",
            json!({"member_id":"default","name":"Ten","items":items}),
        )
        .unwrap();
    let make = |outfit: &Value, key: &str| json!({"member_id":"default","person_id":person["id"],"outfit_id":outfit["id"],"version":1,"provider_id":p["id"],"operation_key":key,"upload_images":true});
    let job = app.execute("tryon.generate", make(&nine, "nine")).unwrap();
    assert_eq!(job["payload"]["inputs"].as_array().unwrap().len(), 9);
    assert_eq!(
        app.execute("tryon.generate", make(&ten, "ten"))
            .unwrap_err()
            .code,
        "image_count_limit"
    );
    assert_eq!(
        app.execute(
            "tryon.compare",
            json!({"member_id":"default","ids":["a","b","c","d","e"]})
        )
        .unwrap_err()
        .code,
        "comparison_limit"
    );
    assert_eq!(
        app.execute("tryon.compare", json!({"member_id":"default","ids":[]}))
            .unwrap_err()
            .code,
        "comparison_limit"
    );
}
