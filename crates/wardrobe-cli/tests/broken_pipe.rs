//! CLI stdout broken-pipe 契约（#63）：消费者提前关闭管道（如 `wardrobe-cli capabilities | head`）
//! 时，CLI 必须按 CLI 惯例干净退出，不得走 `println!` panic（exit 101），
//! 也不得经 panic 钩子在数据目录 logs/ 留下 error 级 panic 现场。

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

fn log_events(dir: &std::path::Path) -> Vec<Value> {
    let mut events = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir.join("logs")) else {
        return events;
    };
    for entry in entries.flatten() {
        let Ok(content) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            if let Ok(event) = serde_json::from_str(line) {
                events.push(event);
            }
        }
    }
    events
}

fn panic_scenes(events: &[Value]) -> Vec<&Value> {
    events
        .iter()
        .filter(|e| {
            (e["event_type"] == "panic" || e["event_type"] == "process.panic")
                && e["level"] == "error"
        })
        .collect()
}

/// stdin 屏障保证确定性：子进程读到 stdin EOF 之前不会写 stdout，
/// 因此“关闭读端”必然先于“写输出”，写入必然得到 BrokenPipe（与输出大小、调度无关）。
#[test]
fn cli_exits_cleanly_when_the_stdout_pipe_is_closed_early() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "capabilities",
            "--input-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // 先关闭 stdout 读端（模拟 `| head` 退出后的空管道）
    drop(child.stdout.take());
    // 再放行输入：子进程读 stdin 至 EOF 后才执行并输出
    {
        let mut stdin = child.stdin.take().unwrap();
        write!(stdin, "{{}}").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout 管道关闭应干净退出而非 panic(101)：{:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("panicked"),
        "不得出现 panic 输出：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = log_events(dir.path());
    let scenes = panic_scenes(&events);
    assert!(
        scenes.is_empty(),
        "stdout broken pipe 不得经 panic 钩子留下日志现场：{scenes:?}"
    );
}

/// MCP stdio 传输同样只写 stdout JSON 行：客户端断开（关闭管道）后
/// 必须静默干净退出，不得 panic——传输断开不是程序错误。
#[test]
fn mcp_stdio_exits_cleanly_when_the_client_disconnects_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", db.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"ping"}}"#).unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "MCP 客户端断开 stdout 应干净退出而非 panic(101)：{:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let events = log_events(dir.path());
    let scenes = panic_scenes(&events);
    assert!(
        scenes.is_empty(),
        "MCP stdout 断开不得经 panic 钩子留下日志现场：{scenes:?}"
    );
}
