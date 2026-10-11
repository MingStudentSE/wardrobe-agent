//! #74 MCP 错误出口带 trace_id：initialize / tools/list 的 JSON-RPC 错误帧
//! `data` 直接序列化完整 ApiError（code/message/trace_id），与 tools/call 的
//! `{"error":…}` 一致；外部客户端可按 trace_id 回查日志链路，不再在缝合线丢字段。

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn spawn_mcp(db: &std::path::Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", db.to_str().unwrap(), "mcp"])
        .env_remove("WARDROBE_DATA_DIR")
        .env_remove("WARDROBE_PROFILES_PATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

fn frames(mut child: std::process::Child, requests: &[String]) -> Vec<Value> {
    {
        let mut stdin = child.stdin.take().unwrap();
        for request in requests {
            writeln!(stdin, "{request}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("MCP stdout 每行都是协议帧"))
        .collect()
}

#[test]
fn mcp_error_frames_carry_trace_id_like_tools_call() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mcp-trace.sqlite");
    // 未经授权的 initialize（令牌形状合法但库中无此授权）：错误必须带 trace_id。
    let responses = frames(
        spawn_mcp(&db),
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"authorization_token":"f".repeat(64)}})
                .to_string(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string(),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"clothing.list","arguments":{"member_id":"default"}}})
                .to_string(),
        ],
    );
    let initialize = responses
        .iter()
        .find(|frame| frame["id"] == json!(1))
        .expect("initialize 应答");
    assert_eq!(initialize["error"]["code"], json!(-32001), "{initialize}");
    assert!(
        initialize["error"]["data"]["trace_id"].is_string(),
        "initialize 错误应携带 trace_id: {initialize}"
    );
    let tools_list = responses
        .iter()
        .find(|frame| frame["id"] == json!(2))
        .expect("tools/list 应答");
    assert_eq!(tools_list["error"]["code"], json!(-32001), "{tools_list}");
    assert!(
        tools_list["error"]["data"]["trace_id"].is_string(),
        "tools/list 错误应携带 trace_id: {tools_list}"
    );
    // tools/call 的错误信封（既有形态）同样钉住：content text 内是完整 ApiError。
    let tools_call = responses
        .iter()
        .find(|frame| frame["id"] == json!(3))
        .expect("tools/call 应答");
    assert_eq!(tools_call["result"]["isError"], json!(true), "{tools_call}");
    let text = tools_call["result"]["content"][0]["text"]
        .as_str()
        .expect("tools/call 错误文本");
    let payload: Value = serde_json::from_str(text).expect("tools/call 错误文本应为 JSON");
    assert_eq!(payload["error"]["code"], json!("unauthorized"), "{payload}");
    assert!(
        payload["error"]["trace_id"].is_string(),
        "tools/call 错误应携带 trace_id: {payload}"
    );
}
