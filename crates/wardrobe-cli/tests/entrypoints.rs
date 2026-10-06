use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn cli(path: &std::path::Path, operation: &str, input: Value) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
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
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"clothing.get","arguments":{"member_id":"default","id":created["result"]["id"]}}});
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
        writeln!(stdin,"{}",json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":operation,"arguments":arguments}})).unwrap();
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
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"clothing.get","arguments":{"member_id":b["result"]["member_id"],"id":created["result"]["id"]}}});
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
