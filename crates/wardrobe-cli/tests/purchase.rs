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
    assert!(
        output.status.success(),
        "{operation} 应成功: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["result"].clone()
}

fn client_token(path: &std::path::Path) -> String {
    cli(
        path,
        "client.authorize",
        json!({"name":"候选分析客户端","mode":"limited","operations":["purchase.candidate.create","purchase.candidate.list","purchase.analysis.run","purchase.analysis.list"],"member_ids":null,"expires_at":null}),
    )["token"]
        .as_str()
        .unwrap()
        .to_string()
}

/// CLI 契约：候选草稿经共享 Application API 创建，候选不入衣物库存；
/// 分析引用真实库存且版本化保留。
#[test]
fn cli_candidate_draft_and_analysis_roundtrip_stays_out_of_inventory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let stock = cli(
        &path,
        "clothing.create",
        json!({"member_id":"default","name":"库存白衬衫","category":"top","color":["white"]}),
    );
    let candidate = cli(
        &path,
        "purchase.candidate.create",
        json!({"member_id":"default","name":"候选白衬衫","category":"top","color":["white"]}),
    );
    assert_eq!(candidate["domain"], "purchase_candidate");
    let correction = cli(
        &path,
        "purchase.candidate.update",
        json!({"member_id":"default","id":candidate["id"],"facts":{"description":"确认"},"confirm_correction":true}),
    );
    assert_eq!(correction["facts"]["description"], "确认");
    let analysis = cli(
        &path,
        "purchase.analysis.run",
        json!({"member_id":"default","candidate_id":candidate["id"]}),
    );
    assert_eq!(analysis["version"], 1);
    assert_eq!(analysis["counts_toward_inventory"], false);
    let cited = analysis["duplicate_risk"]["items"][0]["clothing_id"]
        .as_str()
        .unwrap();
    assert_eq!(json!(cited), stock["id"]);
    let inventory = cli(
        &path,
        "clothing.list",
        json!({"member_id":"default","limit":100}),
    );
    assert_eq!(inventory["items"].as_array().unwrap().len(), 1);
    let drafts = cli(
        &path,
        "purchase.candidate.list",
        json!({"member_id":"default","state":"draft"}),
    );
    assert_eq!(drafts["items"].as_array().unwrap().len(), 1);
    let history = cli(
        &path,
        "purchase.analysis.list",
        json!({"member_id":"default","candidate_id":candidate["id"]}),
    );
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
}

/// 有限授权客户端：scope 内操作可执行，scope 外拒绝；CLI 与 MCP 返回一致。
#[test]
fn limited_client_scope_and_mcp_parity_for_purchase_operations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    cli(
        &path,
        "clothing.create",
        json!({"member_id":"default","name":"库存鞋","category":"shoes"}),
    );
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
        v
    };
    let created = authorized(
        "purchase.candidate.create",
        json!({"member_id":"default","name":"候选包","category":"bag"}),
    );
    assert!(created.get("error").is_none());
    let candidate_id = created["result"]["id"].clone();
    let analysis = authorized(
        "purchase.analysis.run",
        json!({"member_id":"default","candidate_id":candidate_id}),
    );
    assert!(analysis.get("error").is_none());
    // scope 外操作拒绝。
    let denied = authorized(
        "purchase.candidate.dismiss",
        json!({"member_id":"default","id":candidate_id}),
    );
    assert_eq!(denied["error"]["code"], "scope_denied");
    // MCP 同源：initialize 带 token 后 tools/list 暴露 purchase 操作；
    // tools/call 与 CLI 结果一致。
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
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})
    )
    .unwrap();
    writeln!(stdin, "{}", json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"purchase.analysis.list","arguments":{"member_id":"default","candidate_id":candidate_id}}
    }))
    .unwrap();
    drop(stdin);
    let stdout = child.wait_with_output().unwrap().stdout;
    let mut lines = stdout.split(|b| *b == b'\n').filter(|l| !l.is_empty());
    let mut responses = std::collections::BTreeMap::<u64, Value>::new();
    for line in lines {
        let value: Value = serde_json::from_slice(line).unwrap();
        if let Some(id) = value["id"].as_u64() {
            responses.insert(id, value);
        }
    }
    let tools: Vec<String> = responses[&2]["result"]["tools"]
        .as_array()
        .expect("tools/list 须返回工具列表")
        .iter()
        .filter_map(|t| t["name"].as_str().map(str::to_owned))
        .collect();
    assert!(tools.contains(&"purchase.analysis.run".to_string()));
    assert!(tools.contains(&"purchase.candidate.create".to_string()));

    let returned: Value = serde_json::from_str(
        responses[&3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert!(returned.get("error").is_none(), "{}", returned);
    assert_eq!(
        returned["items"].as_array().unwrap().len(),
        analysis["result"]["version"].as_u64().unwrap() as usize,
        "MCP 与 CLI 共享同一候选分析账本"
    );
}

/// 操作发现契约：schema 命令暴露完整描述符（含 scope 元数据与语义红线效果）。
#[test]
fn purchase_descriptors_are_discoverable_with_scope_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["schema", "purchase.analysis.run"])
        .output()
        .unwrap();
    let descriptor: Value =
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["result"].clone();
    assert_eq!(descriptor["name"], "purchase.analysis.run");
    assert_eq!(descriptor["effect"]["kind"], "mutation");
    assert_eq!(descriptor["effect"]["network"], "none");
    assert_eq!(descriptor["scope"]["member"]["grant"], "per-member_id");
    assert_eq!(descriptor["scope"]["access"], "external");
    let capabilities = cli(&path, "capabilities", json!({}));
    let names: Vec<String> = capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap().to_string())
        .collect();
    for operation in [
        "purchase.candidate.create",
        "purchase.candidate.recognize",
        "purchase.candidate.process",
        "purchase.candidate.get",
        "purchase.candidate.list",
        "purchase.candidate.update",
        "purchase.candidate.dismiss",
        "purchase.analysis.run",
        "purchase.analysis.get",
        "purchase.analysis.list",
    ] {
        assert!(
            names.contains(&operation.to_string()),
            "{operation} 必须已注册"
        );
    }
}
