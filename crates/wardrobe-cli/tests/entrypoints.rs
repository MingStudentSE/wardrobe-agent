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
