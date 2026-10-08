//! 日志核心与入口契约：CLI stdout JSON 契约、MCP stdio 会话不受日志影响；
//! 进程 panic 经钩子留下日志现场。先例：entrypoints.rs、external_agent_workflow.rs。

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

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

fn log_events(dir: &std::path::Path) -> Vec<Value> {
    let mut events = Vec::new();
    for entry in std::fs::read_dir(dir.join("logs")).expect("logs 目录应存在") {
        let content = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            events.push(serde_json::from_str(line).unwrap());
        }
    }
    events
}

#[test]
fn logging_keeps_the_cli_stdout_json_contract() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    // 成功调用：stdout 恰好一个 {"schema_version":1,...} 封套（from_slice 拒绝任何额外字节）
    let ok = cli(&db, "space.default", json!({}));
    assert!(ok.get("error").is_none(), "{ok}");
    assert_eq!(ok["schema_version"], 1);
    // 失败调用：错误输出含 trace_id（加法变更），封套契约不变
    let rejected = cli(
        &db,
        "clothing.create",
        json!({"member_id":"default","name":" ","category":"top"}),
    );
    assert_eq!(rejected["error"]["code"], "invalid_input", "{rejected}");
    assert!(rejected["error"]["trace_id"].is_string(), "{rejected}");
    // 日志只写文件，不占 stdout：日志文件确实产生
    assert!(
        std::fs::read_dir(dir.path().join("logs")).unwrap().count() > 0,
        "CLI 执行应留下日志文件"
    );
    let events = log_events(dir.path());
    assert!(
        events
            .iter()
            .any(|e| e["operation"] == "clothing.create" && e["result"] == "invalid_input"),
        "{events:?}"
    );
}

#[test]
fn logging_does_not_pollute_the_mcp_stdio_session() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    cli(&db, "space.default", json!({}));
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", db.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})
    )
    .unwrap();
    writeln!(stdin, "{}", json!({"jsonrpc":"2.0","id":2,"method":"ping"})).unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    // 启用日志后，stdout 仍然恰好是逐行 JSON-RPC，无任何日志字节混入
    let text = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
    assert_eq!(lines.len(), 2, "stdout 应只有两条 JSON-RPC 响应：{text}");
    let first: Value = serde_json::from_str(lines[0]).unwrap();
    let second: Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(first["jsonrpc"], "2.0");
    assert_eq!(first["id"], 1);
    assert!(first["result"]["protocolVersion"].is_string(), "{first}");
    assert_eq!(second["id"], 2);
    // MCP 会话同样落日志
    assert!(!log_events(dir.path()).is_empty());
}
