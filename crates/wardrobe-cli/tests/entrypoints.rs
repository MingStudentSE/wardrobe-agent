use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn authorized_cli_and_mcp_return_the_same_derived_insights_and_persistent_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let a=cli(&path,"clothing.create",json!({"member_id":"default","name":"洞察红蓝上衣","category":"top","color":["red","blue"]}))["result"].clone();
    for key in ["one", "two"] {
        cli(
            &path,
            "wear.record",
            json!({"member_id":"default","operation_key":key,"local_date":"2020-01-01","timezone":"Asia/Shanghai","clothing_ids":[a["id"]]}),
        );
    }
    let token = client_token(&path);
    let authorized = |operation: &str, input: Value| {
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
        let v: Value = serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
        assert!(v.get("error").is_none());
        v["result"].clone()
    };
    assert_eq!(
        authorized(
            "insights.configure",
            json!({"member_id":"default","stale_days":91,"expected_revision":0})
        )["stale_days"],
        91
    );
    let query = json!({"member_id":"default","as_of_date":"2020-04-01","timezone":"Asia/Shanghai"});
    let expected = authorized("insights.summary", query.clone());
    assert_eq!(expected["stale_days"], 91);
    assert_eq!(expected["frequency"][0]["wearCount"], 2);
    assert_eq!(expected["frequency"][0]["wearDays"], 1);
    assert_eq!(expected["stale"].as_array().unwrap().len(), 1);
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":"insights.summary","arguments":query}})).unwrap();
    let response: Value =
        serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
    let returned: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(returned, expected);
}
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

#[test]
fn authorized_cli_mcp_share_recognition_proposal_review_and_restart() {
    use std::io::Read;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut b = vec![];
        loop {
            let mut chunk = [0; 4096];
            let n = s.read(&mut chunk).unwrap();
            assert!(n > 0);
            b.extend_from_slice(&chunk[..n]);
            if let Some(split) = b.windows(4).position(|w| w == b"\r\n\r\n") {
                let len: usize = String::from_utf8_lossy(&b[..split])
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|v| v.parse().unwrap())
                    })
                    .unwrap();
                if b.len() >= split + 4 + len {
                    let body: Value =
                        serde_json::from_slice(&b[split + 4..split + 4 + len]).unwrap();
                    assert_eq!(body["model"], "http-fixture");
                    break;
                }
            }
        }
        let body = r#"{"model":"http-fixture","choices":[{"message":{"content":"White shirt; brand uncertain."}}]}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let source = dir.path().join("fixture.png");
    std::fs::write(
        &source,
        include_bytes!("../../wardrobe-core/tests/fixtures/generated-colors.png"),
    )
    .unwrap();
    let c = cli(
        &db,
        "clothing.create",
        json!({"member_id":"default","name":"手工保护","category":"top"}),
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
        json!({"member_id":"default","clothing_id":c["id"],"asset_id":a["id"]}),
    );
    let p=cli(&db,"provider.save",json!({"name":"HTTP fixture","endpoint":endpoint,"model":"http-fixture","capabilities":["vision"],"authentication":"none"}))["result"].clone();
    let token = client_token(&db);
    let mcp = |op: &str, input: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", db.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":op,"arguments":input}})).unwrap();
        let response: Value =
            serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
        assert_ne!(response["result"]["isError"], true, "{response}");
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let request = json!({"member_id":"default","operation_key":"cli-recognition","clothing_id":c["id"],"provider_id":p["id"],"asset_ids":[a["id"]],"upload_role":"product_display","confirm_upload":true});
    let job = cli(&db, "recognition.create", request.clone())["result"].clone();
    assert_eq!(mcp("recognition.create", request), job);
    let done = mcp(
        "recognition.process",
        json!({"member_id":"default","id":job["id"]}),
    );
    server.join().unwrap();
    assert_eq!(done["state"], "succeeded");
    let suggestion = mcp(
        "recognition.get",
        json!({"member_id":"default","id":done["result"]["suggestion_id"]}),
    );
    assert_eq!(suggestion["state"], "pending_review");
    assert_eq!(
        cli(
            &db,
            "clothing.get",
            json!({"member_id":"default","id":c["id"]})
        )["result"]["description"],
        Value::Null
    );
    let reviewed = mcp(
        "recognition.review",
        json!({"member_id":"default","id":suggestion["id"],"facts":{"description":"人工核实","color":["white"]},"confirm_review":true}),
    );
    assert_eq!(reviewed["state"], "reviewed");
    let saved = cli(
        &db,
        "clothing.get",
        json!({"member_id":"default","id":c["id"]}),
    )["result"]
        .clone();
    assert_eq!(saved["name"], "手工保护");
    assert_eq!(saved["description"], "人工核实");
    assert_eq!(
        mcp(
            "recognition.get",
            json!({"member_id":"default","id":suggestion["id"]})
        ),
        reviewed
    );
}

#[test]
fn cli_mcp_real_multimage_tryon_preview_and_local_save_keep_template_and_wear() {
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
                            assert_eq!(body["image"].as_array().unwrap().len(), 3);
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
        json!({"member_id":"default","name":"Real CLI tryon","items":items}),
    )["result"]
        .clone();
    let provider=cli(&db,"provider.save",json!({"name":"Seedream","endpoint":endpoint,"model":"doubao-seedream-5-0-flash-260915","adapter":"ark_seedream","capabilities":["image_generation","virtual_tryon"],"authentication":"none"}))["result"].clone();
    let person_path = dir.path().join("person.png");
    std::fs::write(
        &person_path,
        include_bytes!("../../wardrobe-core/tests/fixtures/fictional-model.png"),
    )
    .unwrap();
    let person=cli(&db,"tryon.person.import",json!({"member_id":"default","path":person_path,"person_kind":"model","rights_confirmed":true,"operation_key":"person"}))["result"].clone();
    let job=cli(&db,"tryon.generate",json!({"member_id":"default","person_id":person["id"],"outfit_id":outfit["id"],"version":1,"provider_id":provider["id"],"operation_key":"tryon-cli","upload_images":true}))["result"].clone();
    assert_eq!(job["kind"], "virtual_tryon");
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
    assert_eq!(mcp("tryon.process")["state"], "waiting_input");
    thread.join().unwrap();
    assert_eq!(mcp("tryon.preview")["sync_scope"], "local_only");
    assert_eq!(mcp("tryon.save")["state"], "succeeded");
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
#[test]
fn cli_mcp_share_durable_care_threshold_chronology_and_reminder_availability() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let c = cli(
        &path,
        "clothing.create",
        json!({"member_id":"default","name":"洗护衬衫","category":"top"}),
    )["result"]
        .clone();
    let conf = cli(
        &path,
        "care.configure",
        json!({"member_id":"default","clothing_id":c["id"],"wash_after_wears":1,"expected_revision":0,"operation_key":"config"}),
    );
    assert_eq!(conf["result"]["revision"], 1);
    cli(
        &path,
        "wear.record",
        json!({"member_id":"default","clothing_ids":[c["id"]],"local_date":"2020-01-01","timezone":"UTC","operation_key":"wear"}),
    );
    let status = cli(
        &path,
        "care.status",
        json!({"member_id":"default","clothing_id":c["id"]}),
    )["result"]
        .clone();
    assert_eq!(status["dirty"], true);
    let token = client_token(&path);
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":"care.record","arguments":{"member_id":"default","clothing_id":c["id"],"action":"wash","local_date":"2020-01-02","timezone":"UTC","operation_key":"clean"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"authorization_token":token,"name":"care.reminders","arguments":{"member_id":"default"}}}),
    ];
    let mut stdin = mcp.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = mcp.wait_with_output().unwrap();
    assert!(output.status.success());
    let lines = String::from_utf8(output.stdout).unwrap();
    let responses: Vec<Value> = lines
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let event: Value = serde_json::from_str(
        responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(event["action"], "wash");
    let reminders: Value = serde_json::from_str(
        responses[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reminders["items"], json!([]));
    assert_eq!(
        cli(
            &path,
            "care.status",
            json!({"member_id":"default","clothing_id":c["id"]})
        )["result"]["wears_since_clean"],
        0
    );
    assert_eq!(
        cli(
            &path,
            "care.get",
            json!({"member_id":"default","id":event["id"]})
        )["result"],
        event
    );
}
#[test]
fn authorized_cli_pairing_generate_equals_fresh_mcp_pairing_list_without_wear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    // Output contract (GLOSSARY 默认成员别名): inputs may use the "default" alias,
    // but public outputs carry the canonical UUID from the shared member resolver.
    let member = cli(&path, "space.default", json!({}))["result"]["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let anchor = cli(
        &path,
        "clothing.create",
        json!({"member_id":member,"name":"锚点上衣","category":"top"}),
    )["result"]
        .clone();
    for (name, category) in [("裤子", "bottom"), ("鞋", "shoes")] {
        cli(
            &path,
            "clothing.create",
            json!({"member_id":member,"name":name,"category":category}),
        );
    }
    let token = client_token(&path);
    let input = json!({"member_id":member,"operation_key":"pair-cli-1","anchor_id":anchor["id"],"timezone":"UTC","limit":2});
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--db",
            path.to_str().unwrap(),
            "pairing.generate",
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
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "pairing.generate 子进程失败");
    let generated: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(generated.get("error").is_none(), "{generated}");
    let intent = generated["result"].clone();
    assert_eq!(intent["member_id"], json!(member));
    assert_eq!(
        intent["pairing"],
        json!({"schema_version":1,"anchor_id":anchor["id"]})
    );
    assert_eq!(intent["creates_wear"], false);
    let proposals = intent["proposals"].as_array().unwrap();
    assert!(!proposals.is_empty(), "{intent}");
    for proposal in proposals {
        assert_eq!(
            proposal["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["clothing_id"] == anchor["id"])
                .count(),
            1,
            "{proposal}"
        );
    }
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":"pairing.list","arguments":{"member_id":member,"anchor_id":anchor["id"]}}});
    writeln!(mcp.stdin.take().unwrap(), "{request}").unwrap();
    let output = mcp.wait_with_output().unwrap();
    assert!(output.status.success(), "mcp 子进程失败");
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(!body.contains(&token));
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["result"]["isError"], false);
    let listed: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(listed["schema_version"], 1);
    assert_eq!(listed["member_id"], json!(member));
    assert_eq!(listed["items"], json!([intent]));
    assert_eq!(listed["next_cursor"], Value::Null);
    assert_eq!(
        cli(&path, "wear.list", json!({"member_id":member}))["result"]["items"],
        json!([])
    );
}

#[test]
fn pairing_only_member_limited_grant_denies_scope_and_foreign_member_via_cli_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let member = cli(&path, "space.default", json!({}))["result"]["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let foreign = cli(&path, "member.create", json!({"name":"未授权成员"}))["result"]["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let anchor = cli(
        &path,
        "clothing.create",
        json!({"member_id":member,"name":"授权锚点","category":"top"}),
    )["result"]
        .clone();
    for (name, category) in [("授权裤子", "bottom"), ("授权鞋", "shoes")] {
        cli(
            &path,
            "clothing.create",
            json!({"member_id":member,"name":name,"category":category}),
        );
    }
    let pair_token = cli(
        &path,
        "client.authorize",
        json!({"name":"配对限定客户端","mode":"limited","operations":["pairing.generate","pairing.list"],"member_ids":[member],"expires_at":null}),
    )["result"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let unscoped_token = cli(
        &path,
        "client.authorize",
        json!({"name":"无配对scope客户端","mode":"limited","operations":["clothing.list"],"member_ids":[member],"expires_at":null}),
    )["result"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let cli_client = |token: &str, operation: &str, input: Value| {
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
        let output = child.wait_with_output().unwrap();
        let body: Value = serde_json::from_slice(&output.stdout).unwrap();
        (output.status, body)
    };
    let mcp_call = |token: &str, operation: &str, input: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", path.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(
            child.stdin.take().unwrap(),
            "{}",
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":operation,"arguments":input}})
        )
        .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "mcp 子进程失败");
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let input = json!({"member_id":member,"operation_key":"pair-grant-1","anchor_id":anchor["id"],"timezone":"UTC"});
    let (status, generated) = cli_client(&pair_token, "pairing.generate", input.clone());
    assert_eq!(status.code(), Some(0));
    assert!(generated.get("error").is_none(), "{generated}");
    let intent = generated["result"].clone();
    assert_eq!(intent["pairing"]["anchor_id"], anchor["id"]);
    let listed = mcp_call(&pair_token, "pairing.list", json!({"member_id":member}));
    assert_eq!(listed["items"], json!([intent]));
    let (status, denied) = cli_client(
        &pair_token,
        "pairing.generate",
        json!({"member_id":foreign,"operation_key":"pair-foreign","anchor_id":anchor["id"],"timezone":"UTC"}),
    );
    assert_eq!(status.code(), Some(2));
    assert_eq!(denied["error"]["code"], "member_denied");
    assert!(!denied.to_string().contains(&pair_token));
    let denied = mcp_call(&pair_token, "pairing.list", json!({"member_id":foreign}));
    assert_eq!(denied["error"]["code"], "member_denied");
    let (status, denied) = cli_client(&unscoped_token, "pairing.generate", input);
    assert_eq!(status.code(), Some(2));
    assert_eq!(denied["error"]["code"], "scope_denied");
    assert!(!denied.to_string().contains(&unscoped_token));
    let denied = mcp_call(
        &unscoped_token,
        "pairing.generate",
        json!({"member_id":member,"operation_key":"pair-unscoped","anchor_id":anchor["id"],"timezone":"UTC"}),
    );
    assert_eq!(denied["error"]["code"], "scope_denied");
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
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"authorization_token":unscoped_token}})
    )
    .unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})
    )
    .unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(!body.contains(&unscoped_token));
    let responses: Vec<Value> = body
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "clothing.list"));
    assert!(tools
        .iter()
        .all(|tool| tool["name"] != "pairing.generate" && tool["name"] != "pairing.list"));
}

#[test]
fn pairing_generate_rejects_foreign_unknown_and_care_unavailable_anchor_via_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let member = cli(&path, "space.default", json!({}))["result"]["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let foreign = cli(&path, "member.create", json!({"name":"外来锚点成员"}))["result"]
        ["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let anchor = cli(
        &path,
        "clothing.create",
        json!({"member_id":member,"name":"可用锚点","category":"top"}),
    )["result"]["id"]
        .clone();
    for (name, category) in [("可用裤子", "bottom"), ("可用鞋", "shoes")] {
        cli(
            &path,
            "clothing.create",
            json!({"member_id":member,"name":name,"category":category}),
        );
    }
    let foreign_anchor = cli(
        &path,
        "clothing.create",
        json!({"member_id":foreign,"name":"别成员上衣","category":"top"}),
    )["result"]["id"]
        .clone();
    let dirty = cli(
        &path,
        "clothing.create",
        json!({"member_id":member,"name":"待洗锚点","category":"top"}),
    )["result"]["id"]
        .clone();
    cli(
        &path,
        "care.configure",
        json!({"member_id":member,"clothing_id":dirty,"wash_after_wears":1,"expected_revision":0,"operation_key":"config-dirty"}),
    );
    cli(
        &path,
        "wear.record",
        json!({"member_id":member,"operation_key":"wear-dirty","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[dirty]}),
    );
    let token = client_token(&path);
    let mcp = |operation: &str, input: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", path.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(
            child.stdin.take().unwrap(),
            "{}",
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":operation,"arguments":input}})
        )
        .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "mcp 子进程失败");
        let body = String::from_utf8(output.stdout).unwrap();
        assert!(!body.contains(&token));
        let response: Value = serde_json::from_str(&body).unwrap();
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let generate = |key: &str, anchor: &Value| {
        mcp(
            "pairing.generate",
            json!({"member_id":member,"operation_key":key,"anchor_id":anchor,"timezone":"UTC"}),
        )
    };
    let ok = generate("pair-ok", &anchor);
    assert_eq!(ok["pairing"]["anchor_id"], anchor, "{ok}");
    assert!(!ok["proposals"].as_array().unwrap().is_empty());
    for (key, bad, code) in [
        ("pair-foreign", &foreign_anchor, "not_found"),
        ("pair-unknown", &json!("不存在的锚点ID"), "not_found"),
        ("pair-dirty", &dirty, "constraint_conflict"),
    ] {
        let rejected = generate(key, bad);
        assert_eq!(rejected["error"]["code"], code, "{rejected}");
    }
    // Unfiltered member history proves rejected anchors left no durable intent
    // at all — not even under a different anchor filter.
    let listed = mcp("pairing.list", json!({"member_id":member}));
    assert_eq!(listed["items"], json!([ok]));
}

#[test]
fn pairing_history_pages_exact_ids_across_subprocess_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let member = cli(&path, "space.default", json!({}))["result"]["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let foreign = cli(&path, "member.create", json!({"name":"分页外来成员"}))["result"]
        ["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let item = |name: &str, category: &str, owner: &str| {
        cli(
            &path,
            "clothing.create",
            json!({"member_id":owner,"name":name,"category":category}),
        )["result"]["id"]
            .clone()
    };
    let a1 = item("历史锚点一", "top", &member);
    let a2 = item("历史锚点二", "top", &member);
    item("历史裤子", "bottom", &member);
    item("历史鞋", "shoes", &member);
    let f_anchor = item("外来锚点", "top", &foreign);
    item("外来裤子", "bottom", &foreign);
    item("外来鞋", "shoes", &foreign);
    let token = client_token(&path);
    // Every generator is its own subprocess that exits before anything is read;
    // the reader below is yet another fresh process over the same SQLite file.
    let generate = |operation: &str, input: Value| {
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
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{operation} 子进程失败");
        let body: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(body.get("error").is_none(), "{body}");
        body["result"].clone()
    };
    let mut anchored = vec![];
    for key in ["h1", "h2", "h3"] {
        anchored.push(
            generate(
                "pairing.generate",
                json!({"member_id":member,"operation_key":key,"anchor_id":a1,"timezone":"UTC"}),
            )["id"]
                .clone(),
        );
    }
    let other = generate(
        "pairing.generate",
        json!({"member_id":member,"operation_key":"h-other","anchor_id":a2,"timezone":"UTC"}),
    );
    let foreign_intent = generate(
        "pairing.generate",
        json!({"member_id":foreign,"operation_key":"fh","anchor_id":f_anchor,"timezone":"UTC"}),
    );
    let unmarked = generate(
        "recommendation.generate",
        json!({"member_id":member,"operation_key":"oh","timezone":"UTC"}),
    );
    let mcp_list = |input: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", path.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(
            child.stdin.take().unwrap(),
            "{}",
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":"pairing.list","arguments":input}})
        )
        .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "mcp 子进程失败");
        let body = String::from_utf8(output.stdout).unwrap();
        assert!(!body.contains(&token));
        let response: Value = serde_json::from_str(&body).unwrap();
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let ids = |page: &Value| -> Vec<Value> {
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].clone())
            .collect()
    };
    let mut expected: Vec<Value> = anchored.clone();
    expected.push(other["id"].clone());
    expected.sort_by(|a, b| b.as_str().unwrap().cmp(a.as_str().unwrap()));
    let page1 = mcp_list(json!({"member_id":member,"limit":2}));
    assert_eq!(ids(&page1), expected[..2]);
    assert_eq!(page1["next_cursor"], expected[1]);
    let page2 = mcp_list(json!({"member_id":member,"limit":2,"cursor":page1["next_cursor"]}));
    assert_eq!(ids(&page2), expected[2..]);
    assert_eq!(page2["next_cursor"], Value::Null);
    let mut a1_expected = anchored.clone();
    a1_expected.sort_by(|a, b| b.as_str().unwrap().cmp(a.as_str().unwrap()));
    let p1 = mcp_list(json!({"member_id":member,"anchor_id":a1,"limit":2}));
    assert_eq!(ids(&p1), a1_expected[..2]);
    assert_eq!(p1["items"][0]["pairing"]["anchor_id"], json!(a1));
    let p2 =
        mcp_list(json!({"member_id":member,"anchor_id":a1,"limit":2,"cursor":p1["next_cursor"]}));
    assert_eq!(ids(&p2), a1_expected[2..]);
    assert_eq!(p2["next_cursor"], Value::Null);
    // Invented or query-mismatched cursors fail identically without leaking why.
    let bad = mcp_list(json!({"member_id":member,"cursor":foreign_intent["id"]}));
    assert_eq!(bad["error"]["code"], "invalid_input");
    let bad = mcp_list(json!({"member_id":member,"anchor_id":a1,"cursor":other["id"]}));
    assert_eq!(bad["error"]["code"], "invalid_input");
    assert_ne!(foreign_intent["id"], unmarked["id"]);
    // Exact per-page equality above already excludes the foreign member's pair
    // and the unmarked ordinary intent from every page of member history.
}

#[test]
fn pairing_save_feedback_and_try_another_retain_anchor_without_wear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let member = cli(&path, "space.default", json!({}))["result"]["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let anchor = cli(
        &path,
        "clothing.create",
        json!({"member_id":member,"name":"保存锚点","category":"top"}),
    )["result"]["id"]
        .clone();
    for (name, category) in [
        ("备用上衣", "top"),
        ("裤子甲", "bottom"),
        ("裤子乙", "bottom"),
        ("鞋甲", "shoes"),
        ("鞋乙", "shoes"),
    ] {
        cli(
            &path,
            "clothing.create",
            json!({"member_id":member,"name":name,"category":category}),
        );
    }
    // try_another is a primary scope that additionally requires both composite
    // components (authorization.rs): the positive token lists all three.
    let token = cli(
        &path,
        "client.authorize",
        json!({"name":"配对保存客户端","mode":"limited","operations":["pairing.generate","pairing.list","recommendation.save","feedback.record","recommendation.change","feedback.try_another"],"member_ids":[member],"expires_at":null}),
    )["result"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    // Composite denials: primary present, exactly one component missing each.
    let missing_change = cli(
        &path,
        "client.authorize",
        json!({"name":"缺换套组件客户端","mode":"limited","operations":["feedback.try_another","feedback.record"],"member_ids":[member],"expires_at":null}),
    )["result"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let missing_record = cli(
        &path,
        "client.authorize",
        json!({"name":"缺反馈组件客户端","mode":"limited","operations":["feedback.try_another","recommendation.change"],"member_ids":[member],"expires_at":null}),
    )["result"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    // Primary denial: both components present, try_another itself unlisted.
    let no_primary = cli(
        &path,
        "client.authorize",
        json!({"name":"无主scope客户端","mode":"limited","operations":["feedback.record","recommendation.change"],"member_ids":[member],"expires_at":null}),
    )["result"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let mcp = |token: &str, operation: &str, input: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", path.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(
            child.stdin.take().unwrap(),
            "{}",
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":operation,"arguments":input}})
        )
        .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "mcp 子进程失败");
        let body = String::from_utf8(output.stdout).unwrap();
        assert!(!body.contains(token));
        let response: Value = serde_json::from_str(&body).unwrap();
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let intent = mcp(
        &token,
        "pairing.generate",
        json!({"member_id":member,"operation_key":"save-anchor","anchor_id":anchor,"timezone":"UTC","limit":1}),
    );
    assert_eq!(intent["pairing"]["anchor_id"], anchor);
    let saved = mcp(
        &token,
        "recommendation.save",
        json!({"member_id":member,"id":intent["id"],"proposal_index":0,"name":"已存搭配","operation_key":"save-1"}),
    );
    assert_eq!(saved["version"], 1, "{saved}");
    assert!(saved["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["clothing_id"] == anchor));
    assert_eq!(
        cli(
            &path,
            "outfit.get",
            json!({"member_id":member,"id":saved["id"]})
        )["result"],
        saved
    );
    assert_eq!(
        cli(&path, "wear.list", json!({"member_id":member}))["result"]["items"],
        json!([])
    );
    let liked = mcp(
        &token,
        "feedback.record",
        json!({"member_id":member,"operation_key":"fb-like","intent_id":intent["id"],"proposal_index":0,"reaction":"like"}),
    );
    assert_eq!(liked["revision"], 1, "{liked}");
    assert_eq!(
        cli(
            &path,
            "feedback.get",
            json!({"member_id":member,"id":liked["id"]})
        )["result"],
        liked
    );
    let history = cli(
        &path,
        "feedback.history",
        json!({"member_id":member,"root_id":liked["root_id"]}),
    )["result"]
        .clone();
    assert_eq!(history["items"], json!([liked.clone()]));
    // Every rejected try_another leaves no feedback at all — public list still
    // shows only the like root.
    let try_input = json!({"member_id":member,"operation_key":"try-denied","intent_id":intent["id"],"proposal_index":0});
    for denied_token in [&missing_change, &missing_record, &no_primary] {
        let denied = mcp(denied_token, "feedback.try_another", try_input.clone());
        assert_eq!(denied["error"]["code"], "scope_denied", "{denied}");
    }
    let untouched = cli(&path, "feedback.list", json!({"member_id":member}))["result"].clone();
    assert_eq!(untouched["items"], json!([liked.clone()]));
    let attempt = mcp(
        &token,
        "feedback.try_another",
        json!({"member_id":member,"operation_key":"try-1","intent_id":intent["id"],"proposal_index":0}),
    );
    assert_eq!(attempt["feedback"]["reaction"], "skip", "{attempt}");
    let continuation = &attempt["recommendation"];
    assert_ne!(continuation["id"], intent["id"]);
    // The continuation is itself a recognized pairing intent for the same anchor.
    assert_eq!(
        continuation["pairing"],
        json!({"schema_version":1,"anchor_id":anchor})
    );
    assert_eq!(continuation["controls"]["source_intent"], intent["id"]);
    let continuations = continuation["proposals"].as_array().unwrap();
    assert!(!continuations.is_empty(), "{continuation}");
    for proposal in continuations {
        assert_eq!(
            proposal["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["clothing_id"] == anchor)
                .count(),
            1,
            "{proposal}"
        );
    }
    let mut expected_ids = vec![intent["id"].clone(), continuation["id"].clone()];
    expected_ids.sort_by(|a, b| b.as_str().unwrap().cmp(a.as_str().unwrap()));
    let listed = mcp(&token, "pairing.list", json!({"member_id":member}));
    let listed_ids: Vec<Value> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].clone())
        .collect();
    assert_eq!(listed_ids, expected_ids);
    // The successful skip is the only feedback added, as the newest root.
    let mut roots = vec![liked["id"].clone(), attempt["feedback"]["id"].clone()];
    roots.sort_by(|a, b| b.as_str().unwrap().cmp(a.as_str().unwrap()));
    let after = cli(&path, "feedback.list", json!({"member_id":member}))["result"].clone();
    let after_ids: Vec<Value> = after["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].clone())
        .collect();
    assert_eq!(after_ids, roots);
    assert_eq!(
        cli(&path, "wear.list", json!({"member_id":member}))["result"]["items"],
        json!([])
    );
}

#[test]
fn real_authorized_cli_and_stdin_mcp_share_persistent_offline_recommendation_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    for (name, category) in [
        ("上衣A", "top"),
        ("上衣B", "top"),
        ("裤子", "bottom"),
        ("鞋", "shoes"),
    ] {
        let r = cli(
            &path,
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        );
        assert!(r.get("error").is_none());
    }
    let token = client_token(&path);
    let input = json!({"member_id":"default","operation_key":"recommend-cli","timezone":"UTC"});
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--db",
            path.to_str().unwrap(),
            "recommendation.generate",
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
    let generated: Value =
        serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
    assert!(generated.get("error").is_none(), "{generated}");
    let expected = &generated["result"];
    assert_eq!(expected["actual_count"], 2);
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":"recommendation.get","arguments":{"member_id":"default","id":expected["id"]}}})).unwrap();
    let response: Value =
        serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
    let actual: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&actual, expected);
    assert_eq!(
        cli(&path, "wear.list", json!({"member_id":"default"}))["result"]["items"],
        json!([])
    );
}
