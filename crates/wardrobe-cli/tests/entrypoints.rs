use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn cli(path: &std::path::Path, operation: &str, input: Value) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--trusted-local",
            "--db",
            path.to_str().unwrap(),
            operation,
            &input.to_string(),
        ])
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn cli_imported_image_is_available_through_mcp_without_gui() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let source = dir.path().join("image.heic");
    std::fs::write(
        &source,
        include_bytes!("../../wardrobe-core/tests/fixtures/generated-colors.heic"),
    )
    .unwrap();
    cli(&path, "space.default", json!({}));
    let imported = cli(
        &path,
        "assets.import",
        json!({"member_id":"default","path":source}),
    );
    assert_eq!(imported["result"]["display"]["width"], 64);
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":client_token(&path),"name":"assets.get","arguments":{"member_id":"default","id":imported["result"]["id"]}}});
    writeln!(mcp.stdin.take().unwrap(), "{request}").unwrap();
    let output = mcp.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let returned: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(returned, imported["result"]);
}
#[test]
fn cli_and_mcp_use_same_persisted_application_without_gui() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let created = cli(
        &path,
        "clothing.create",
        json!({"member_id":"default","name":"羊毛外套","category":"outerwear"}),
    );
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":client_token(&path),"name":"clothing.get","arguments":{"member_id":"default","id":created["result"]["id"]}}});
    writeln!(mcp.stdin.take().unwrap(), "{request}").unwrap();
    let output = mcp.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let returned: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(returned, created["result"]);
    assert_eq!(
        cli(
            &path,
            "clothing.create",
            json!({"member_id":"default","name":"","category":"top"})
        )["error"]["code"],
        "invalid_input"
    );
}
#[test]
fn mcp_edits_and_restores_same_clothing_as_cli_filters_without_gui() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let item = cli(
        &path,
        "clothing.create",
        json!({"member_id":"default","name":"棉衫","category":"top"}),
    )["result"]
        .clone();
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = mcp.stdin.take().unwrap();
    for (id, operation, patch) in [
        (
            1,
            "clothing.update",
            json!({"favorite":true,"material":["棉"]}),
        ),
        (2, "clothing.delete", json!({})),
        (3, "clothing.restore", json!({})),
    ] {
        let mut arguments = patch;
        arguments["member_id"] = json!("default");
        arguments["id"] = item["id"].clone();
        writeln!(stdin,"{}",json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"authorization_token":client_token(&path),"name":operation,"arguments":arguments}})).unwrap();
    }
    drop(stdin);
    let output = mcp.wait_with_output().unwrap();
    assert!(output.status.success());
    let filtered = cli(
        &path,
        "clothing.list",
        json!({"member_id":"default","favorite":true,"material":"棉"}),
    );
    assert_eq!(filtered["result"]["items"][0]["id"], item["id"]);
    assert_eq!(filtered["result"]["items"][0]["status"], "active");
}

#[test]
fn cli_member_profiles_and_mcp_inventory_share_member_scope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let a = cli(&path, "space.default", json!({}));
    let b = cli(&path, "member.create", json!({"name":"另一个成员"}));
    let created = cli(
        &path,
        "clothing.create",
        json!({"member_id":a["result"]["member_id"],"name":"私人衣物","category":"top"}),
    );
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":client_token(&path),"name":"clothing.get","arguments":{"member_id":b["result"]["member_id"],"id":created["result"]["id"]}}});
    writeln!(mcp.stdin.take().unwrap(), "{request}").unwrap();
    let output = mcp.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        cli(
            &path,
            "clothing.list",
            json!({"member_id":b["result"]["member_id"]})
        )["result"]["items"],
        json!([])
    );
}
#[test]
fn stdin_configuration_and_mcp_discovery_share_provider_contract_without_gui() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--trusted-local",
            "--db",
            path.to_str().unwrap(),
            "--trusted-local",
            "provider.save",
            "--input-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(),"{}",json!({"name":"本地","endpoint":"http://127.0.0.1:11434/v1","model":"local","capabilities":["text"],"authentication":"none"})).unwrap();
    let output = child.wait_with_output().unwrap();
    let saved: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(saved["result"]["name"], "本地");
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":client_token(&path),"name":"provider.list","arguments":{}}})).unwrap();
    let output = child.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let list: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(list["items"][0]["id"], saved["result"]["id"]);
}
#[test]
fn credential_command_requires_stdin_to_keep_secret_out_of_process_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let result = cli(
        &dir.path().join("db"),
        "provider.credential.set",
        json!({"id":"missing","api_key":"disposable-placeholder"}),
    );
    assert_eq!(result["error"]["code"], "secret_requires_stdin");
    assert!(!result.to_string().contains("disposable-placeholder"));
}
#[test]
fn unauthenticated_external_cli_cannot_read_inventory() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--db",
            dir.path().join("db").to_str().unwrap(),
            "clothing.list",
            r#"{"member_id":"default"}"#,
        ])
        .output()
        .unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["error"]["code"], "unauthorized");
}

fn client_token(path: &std::path::Path) -> String {
    let app = wardrobe_core::Application::open(path).unwrap();
    let result=app.execute("client.authorize",json!({"name":"测试客户端","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    result["token"].as_str().unwrap().to_string()
}
#[test]
fn external_cli_and_mcp_enforce_same_operation_and_member_grant() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = wardrobe_core::Application::open(&path).unwrap();
    let a = app.execute("space.default", json!({})).unwrap();
    let b = app
        .execute("member.create", json!({"name":"另一个成员"}))
        .unwrap();
    let grant=app.execute("client.authorize",json!({"name":"限定测试客户端","mode":"limited","operations":["clothing.list"],"member_ids":[a["member_id"]],"expires_at":null})).unwrap();
    let token = grant["token"].as_str().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--db",
            path.to_str().unwrap(),
            "clothing.create",
            "--client-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"token":token,"input":{"member_id":a["member_id"],"name":"不能写","category":"top"}})
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "scope_denied");
    assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for (id, params) in [
        (
            1,
            json!({"name":"clothing.list","arguments":{"member_id":a["member_id"]}}),
        ),
        (
            2,
            json!({"authorization_token":token,"name":"clothing.list","arguments":{"member_id":b["member_id"]}}),
        ),
        (
            3,
            json!({"authorization_token":token,"name":"clothing.list","arguments":{"member_id":a["member_id"]}}),
        ),
    ] {
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":params})
        )
        .unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(!body.contains(token));
    let results: Vec<Value> = body
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let error = |i: usize| -> Value {
        serde_json::from_str(results[i]["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    };
    assert_eq!(error(0)["error"]["code"], "unauthorized");
    assert_eq!(error(1)["error"]["code"], "member_denied");
    assert_eq!(results[2]["result"]["isError"], false);
}
#[test]
fn mcp_initialized_full_authorization_is_reused_for_multiple_writes_without_reauthorization() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let token = client_token(&path);
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"authorization_token":token}})
    )
    .unwrap();
    for id in [2, 3] {
        writeln!(stdin,"{}",json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"clothing.create","arguments":{"member_id":"default","name":format!("授权后写入{id}"),"category":"top"}}})).unwrap();
    }
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}})
    )
    .unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(!body.contains(&token));
    let responses: Vec<Value> = body
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 4);
    assert_eq!(responses[1]["result"]["isError"], false);
    assert_eq!(responses[2]["result"]["isError"], false);
    let tools = responses[3]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "assets.import"));
    assert!(!tools.iter().any(|tool| tool["name"] == "client.authorize"));
    assert_eq!(
        cli(&path, "clothing.list", json!({"member_id":"default"}))["result"]["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn authorized_cli_and_mcp_validate_only_explicit_vision_capability() {
    use std::io::Read;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(split) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..split]);
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|n| n.parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= split + 4 + length {
                        let body: Value =
                            serde_json::from_slice(&bytes[split + 4..split + 4 + length]).unwrap();
                        assert!(body["messages"][0]["content"][1]["image_url"]["url"]
                            .as_str()
                            .unwrap()
                            .starts_with("data:image/png;base64,"));
                        break;
                    }
                }
            }
            let body = r#"{"choices":[{"message":{"content":"blue square"}}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let p = cli(
        &path,
        "provider.save",
        json!({"name":"vision","endpoint":endpoint,"model":"fixture","capabilities":["vision"],"authentication":"none"}),
    );
    let grant = cli(
        &path,
        "client.authorize",
        json!({"name":"vision-validator","mode":"limited","operations":["provider.validate"],"member_ids":["default"],"expires_at":null}),
    );
    let token = grant["result"]["token"].as_str().unwrap();
    let input = json!({"id":p["result"]["id"],"capability":"vision"});
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--db",
            path.to_str().unwrap(),
            "provider.validate",
            "--client-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"token":token,"input":input})
    )
    .unwrap();
    let result: Value = serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
    assert_eq!(result["result"]["verified_capabilities"], json!(["vision"]));
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":"provider.validate","arguments":input}})).unwrap();
    let response: Value =
        serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
    let result: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["verified_capabilities"], json!(["vision"]));
    server.join().unwrap();
}
#[test]
fn cli_saved_outfit_and_immutable_version_are_the_same_through_mcp_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let clothing = cli(
        &path,
        "clothing.create",
        json!({"member_id":"default","name":"通勤外套","category":"outerwear"}),
    );
    let saved = cli(
        &path,
        "outfit.create",
        json!({"member_id":"default","name":"通勤","items":[{"clothing_id":clothing["result"]["id"],"role":"outer","layer":2}]}),
    );
    assert_eq!(saved["result"]["version"], 1);
    let mut process = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":client_token(&path),"name":"outfit.version","arguments":{"member_id":"default","id":saved["result"]["id"],"version":1}}});
    writeln!(process.stdin.take().unwrap(), "{request}").unwrap();
    let output = process.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let actual: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(actual, saved["result"]);
}
#[test]
fn cli_mcp_share_durable_batch_job_ids_cancellation_and_idempotent_commit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let source = dir.path().join("image.png");
    std::fs::write(
        &source,
        include_bytes!("../../wardrobe-core/tests/fixtures/generated-colors.png"),
    )
    .unwrap();
    cli(&path, "space.default", json!({}));
    let input = json!({"member_id":"default","operation_key":"batch-cli","paths":[source]});
    let batch = cli(&path, "imports.create", input.clone())["result"].clone();
    assert!(batch["id"].is_string());
    std::fs::remove_file(&source).unwrap();
    let token = client_token(&path);
    let mcp = |operation: &str, input: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", path.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":operation,"arguments":input}})).unwrap();
        let response: Value =
            serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let item = &batch["items"][0]["id"];
    assert_eq!(
        mcp("jobs.cancel", json!({"member_id":"default","id":item}))["state"],
        "cancelled"
    );
    assert_eq!(
        cli(
            &path,
            "jobs.retry",
            json!({"member_id":"default","id":item})
        )["result"]["state"],
        "queued"
    );
    let processed = mcp(
        "imports.process",
        json!({"member_id":"default","id":batch["id"]}),
    );
    assert_eq!(processed["items"][0]["state"], "waiting_input");
    let request = json!({"member_id":"default","id":batch["id"],"draft_id":batch["drafts"][0]["id"],"operation_key":"commit-cli"});
    let saved = mcp("imports.commit", request.clone());
    assert_eq!(saved["name"], "未命名单品");
    assert_eq!(
        cli(&path, "imports.commit", request)["result"]["id"],
        saved["id"]
    );
    assert_eq!(
        mcp(
            "imports.get",
            json!({"member_id":"default","id":batch["id"]})
        )["drafts"][0]["clothing_id"],
        saved["id"]
    );
    assert_eq!(
        cli(&path, "imports.create", input)["result"]["id"],
        batch["id"]
    );
}
#[test]
fn cli_mcp_share_immutable_image_edits_and_cancelled_cutout_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let source = dir.path().join("image.png");
    std::fs::write(
        &source,
        include_bytes!("../../wardrobe-core/tests/fixtures/generated-colors.png"),
    )
    .unwrap();
    cli(&path, "space.default", json!({}));
    let asset = cli(
        &path,
        "assets.import",
        json!({"member_id":"default","path":source}),
    )["result"]
        .clone();
    let token = client_token(&path);
    let mcp = |operation: &str, input: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", path.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":operation,"arguments":input}})).unwrap();
        let response: Value =
            serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let rotated = mcp(
        "images.rotate",
        json!({"member_id":"default","asset_id":asset["id"],"degrees":90}),
    );
    assert_ne!(rotated["id"], asset["id"]);
    assert_eq!(rotated["display"]["width"], 32);
    assert_eq!(rotated["display"]["height"], 64);
    assert_eq!(
        cli(
            &path,
            "assets.get",
            json!({"member_id":"default","id":asset["id"]})
        )["result"],
        asset
    );
    let restored = cli(
        &path,
        "images.restore",
        json!({"member_id":"default","asset_id":rotated["id"]}),
    )["result"]
        .clone();
    assert_eq!(restored["display"]["width"], 64);
    assert_eq!(
        mcp(
            "assets.get",
            json!({"member_id":"default","id":restored["id"]})
        ),
        restored
    );
    let job = mcp(
        "images.cutout",
        json!({"member_id":"default","asset_id":asset["id"],"operation_key":"cancelled"}),
    );
    assert_eq!(job["state"], "queued");
    let cancelled = cli(
        &path,
        "jobs.cancel",
        json!({"member_id":"default","id":job["id"]}),
    )["result"]
        .clone();
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(
        mcp("jobs.get", json!({"member_id":"default","id":job["id"]})),
        cancelled
    );
    assert_eq!(
        mcp(
            "images.worker.run",
            json!({"member_id":"default","job_id":job["id"]})
        )["error"]["code"],
        "job_unavailable"
    );
    assert_eq!(
        mcp(
            "assets.get",
            json!({"member_id":"default","id":asset["id"]})
        ),
        asset
    );
}
#[test]
fn cli_and_mcp_share_product_job_consent_idempotency_and_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    cli(&db, "space.default", json!({}));
    let source = dir.path().join("shirt.png");
    std::fs::write(
        &source,
        include_bytes!("../../wardrobe-core/tests/fixtures/generated-colors.png"),
    )
    .unwrap();
    let a = cli(
        &db,
        "assets.import",
        json!({"member_id":"default","path":source}),
    )["result"]
        .clone();
    let c = cli(
        &db,
        "clothing.create",
        json!({"member_id":"default","name":"shirt","category":"top"}),
    )["result"]
        .clone();
    cli(
        &db,
        "assets.adopt",
        json!({"member_id":"default","asset_id":a["id"],"clothing_id":c["id"]}),
    );
    let p=cli(&db,"provider.save",json!({"name":"Seedream","endpoint":"http://127.0.0.1:45678/api/v3","model":"doubao-seedream-5-0-flash-260915","adapter":"ark_seedream","capabilities":["image_generation"],"authentication":"none"}))["result"].clone();
    let mut input = json!({"member_id":"default","asset_id":a["id"],"clothing_id":c["id"],"provider_id":p["id"],"operation_key":"product-one","upload_original":false});
    assert_eq!(
        cli(&db, "product.generate", input.clone())["error"]["code"],
        "upload_consent_required"
    );
    input["upload_original"] = json!(true);
    let job = cli(&db, "product.generate", input.clone())["result"].clone();
    assert_eq!(
        cli(&db, "product.generate", input)["result"]["id"],
        job["id"]
    );
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", db.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let token = client_token(&db);
    let mut stdin = mcp.stdin.take().unwrap();
    writeln!(stdin,"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":"jobs.cancel","arguments":{"member_id":"default","id":job["id"]}}})).unwrap();
    drop(stdin);
    let output = mcp.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let result: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["state"], "cancelled");
    assert_eq!(
        cli(
            &db,
            "jobs.get",
            json!({"member_id":"default","id":job["id"]})
        )["result"]["state"],
        "cancelled"
    );
    assert_eq!(
        cli(
            &db,
            "clothing.get",
            json!({"member_id":"default","id":c["id"]})
        )["result"]["display_asset_id"],
        a["id"]
    );
}
#[test]
fn authorized_cli_mcp_share_actual_wear_retries_corrections_and_immutable_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let c = cli(
        &path,
        "clothing.create",
        json!({"member_id":"default","name":"实穿上装","category":"top"}),
    )["result"]
        .clone();
    let token = client_token(&path);
    let cli_authorized = |operation: &str, input: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", path.to_str().unwrap(), operation, "--client-stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        write!(
            child.stdin.take().unwrap(),
            "{}",
            json!({"token":token,"input":input})
        )
        .unwrap();
        let response: Value =
            serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    };
    let mcp = |operation: &str, input: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", path.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":operation,"arguments":input}})).unwrap();
        let response: Value =
            serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let request = json!({"member_id":"default","operation_key":"wear-1","local_date":"2020-01-02","timezone":"Asia/Shanghai","clothing_ids":[c["id"]]});
    let event = cli_authorized("wear.record", request.clone());
    assert_eq!(mcp("wear.record", request), event);
    let corrected = mcp(
        "wear.correct",
        json!({"member_id":"default","operation_key":"wear-correction","root_event_id":event["id"],"expected_revision":1,"local_date":"2020-01-03","timezone":"UTC","notes":"补记日期更正"}),
    );
    assert_eq!(corrected["revision"], 2);
    assert_eq!(
        cli_authorized("wear.get", json!({"member_id":"default","id":event["id"]})),
        event
    );
    assert_eq!(
        mcp("wear.summary", json!({"member_id":"default"}))["items"][0]["wearCount"],
        1
    );
    let history = cli_authorized(
        "wear.history",
        json!({"member_id":"default","root_event_id":event["id"]}),
    );
    assert_eq!(history["items"], json!([event, corrected]));
}
#[test]
fn space_crypto_uses_stdin_and_mcp_full_scope_does_not_expose_private_key_management() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("space.sqlite");
    let insecure = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--trusted-local",
            "--db",
            db.to_str().unwrap(),
            "encryption.status",
            "{}",
        ])
        .output()
        .unwrap();
    let response: Value = serde_json::from_slice(&insecure.stdout).unwrap();
    assert_eq!(response["error"]["code"], "secret_requires_stdin");
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--trusted-local",
            "--db",
            db.to_str().unwrap(),
            "encryption.recovery.create",
            "--input-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"password":"temporary CLI wrapping password"})
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let created: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(created["result"]["recovery_secret"].is_string());
    let token = client_token(&db);
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", db.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = mcp.stdin.take().unwrap();
    for request in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"authorization_token":token}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"encryption.recovery.create","arguments":{"member_id":"default","password":"temporary wrapping password"}}}),
    ] {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = mcp.wait_with_output().unwrap();
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(responses[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| !t["name"].as_str().unwrap().starts_with("encryption.")));
    assert_eq!(responses[2]["result"]["isError"], true);
    let denied: Value = serde_json::from_str(
        responses[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(denied["error"]["code"], "scope_denied");
}
#[test]
fn cli_mcp_real_multimage_flatlay_preview_and_local_save_keep_template_and_wear() {
    use std::io::Read;
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/api/v3", server.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = server.accept().unwrap();
            let mut bytes = vec![];
            let mut buf = [0; 8192];
            loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..i]);
                    let len = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= i + 4 + len {
                        if index == 1 {
                            let body: Value = serde_json::from_slice(&bytes[i + 4..]).unwrap();
                            assert_eq!(body["image"].as_array().unwrap().len(), 2);
                        }
                        break;
                    }
                }
            }
            let value = if index == 0 {
                json!({"data":[{"id":"doubao-seedream-5-0-flash-260915"}]})
            } else {
                use base64::Engine;
                json!({"data":[{"b64_json":base64::engine::general_purpose::STANDARD.encode(include_bytes!("../../wardrobe-core/tests/fixtures/generated-colors.png"))}]})
            };
            let body = value.to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    cli(&db, "space.default", json!({}));
    let source = dir.path().join("garment.png");
    std::fs::write(
        &source,
        include_bytes!("../../wardrobe-core/tests/fixtures/generated-colors.png"),
    )
    .unwrap();
    let mut items = vec![];
    for name in ["shirt", "pants"] {
        let c = cli(
            &db,
            "clothing.create",
            json!({"member_id":"default","name":name,"category":"top"}),
        )["result"]
            .clone();
        let a = cli(
            &db,
            "assets.import",
            json!({"member_id":"default","path":source}),
        )["result"]
            .clone();
        cli(
            &db,
            "assets.adopt",
            json!({"member_id":"default","asset_id":a["id"],"clothing_id":c["id"]}),
        );
        items.push(json!({"clothing_id":c["id"],"role":name,"layer":0}));
    }
    let outfit = cli(
        &db,
        "outfit.create",
        json!({"member_id":"default","name":"Real CLI flatlay","items":items}),
    )["result"]
        .clone();
    let provider=cli(&db,"provider.save",json!({"name":"Seedream","endpoint":endpoint,"model":"doubao-seedream-5-0-flash-260915","adapter":"ark_seedream","capabilities":["image_generation"],"authentication":"none"}))["result"].clone();
    let job=cli(&db,"flatlay.generate",json!({"member_id":"default","outfit_id":outfit["id"],"version":1,"provider_id":provider["id"],"operation_key":"flatlay-cli","upload_images":true}))["result"].clone();
    assert_eq!(job["kind"], "outfit_flatlay");
    let token = client_token(&db);
    let mcp = |operation: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", db.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":operation,"arguments":{"member_id":"default","id":job["id"]}}})).unwrap();
        let result: Value =
            serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
        serde_json::from_str::<Value>(result["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    assert_eq!(mcp("flatlay.process")["state"], "waiting_input");
    thread.join().unwrap();
    assert_eq!(mcp("flatlay.preview")["sync_scope"], "local_only");
    assert_eq!(mcp("flatlay.save")["state"], "succeeded");
    assert_eq!(
        cli(
            &db,
            "outfit.get",
            json!({"member_id":"default","id":outfit["id"]})
        )["result"],
        outfit
    );
    assert_eq!(
        cli(&db, "wear.list", json!({"member_id":"default"}))["result"]["items"],
        json!([])
    );
}
