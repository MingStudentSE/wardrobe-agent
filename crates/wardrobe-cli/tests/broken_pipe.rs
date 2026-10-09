//! CLI stdout broken-pipe 契约（#63）：消费者提前关闭管道（如 `wardrobe-cli capabilities | head`）
//! 时，CLI 必须按 CLI 惯例干净退出，不得走 `println!` panic（exit 101），
//! 也不得经 panic 钩子在数据目录 logs/ 留下 error 级 panic 现场。
//! 例外是错误出口 fail()（如未知操作）：其输出遇 BrokenPipe 仍必须以
//! 非零退出（exit 1）可见，不得退化为干净退出 0。

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Output, Stdio};

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

/// 共享驱动（#63 两场景的 spawn→关 stdout→喂 stdin→等待 形状）：
/// 先关闭 stdout 读端（模拟 `| head` 退出后的空管道），再放行 stdin 输入。
/// stdin 屏障保证确定性：子进程读到 stdin EOF 之前不会写 stdout，
/// 因此「关闭读端」必然先于「写输出」，写入必然得到 BrokenPipe（与输出大小、调度无关）。
fn run_cli_with_stdout_closed(dir: &std::path::Path, args: &[&str], stdin_body: &str) -> Output {
    let db = dir.join("wardrobe.sqlite");
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .arg("--db")
        .arg(&db)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(stdin_body.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}

/// 共享断言：正常输出路径必须干净退出 0、无 panic 输出、无 panic 日志现场。
fn assert_clean_exit(scenario: &str, dir: &std::path::Path, output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{scenario}应干净退出而非 panic(101)：{:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("panicked"),
        "{scenario}不得出现 panic 输出：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = log_events(dir);
    let scenes = panic_scenes(&events);
    assert!(
        scenes.is_empty(),
        "{scenario}不得经 panic 钩子留下日志现场：{scenes:?}"
    );
}

#[test]
fn cli_exits_cleanly_when_the_stdout_pipe_is_closed_early() {
    let dir = tempfile::tempdir().unwrap();
    let output = run_cli_with_stdout_closed(dir.path(), &["capabilities", "--input-stdin"], "{}");
    assert_clean_exit("stdout 管道关闭", dir.path(), &output);
}

/// MCP stdio 传输同样只写 stdout JSON 行：客户端断开（关闭管道）后
/// 必须静默干净退出，不得 panic——传输断开不是程序错误。
#[test]
fn mcp_stdio_exits_cleanly_when_the_client_disconnects_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let output = run_cli_with_stdout_closed(
        dir.path(),
        &["mcp"],
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n",
    );
    assert_clean_exit("MCP 客户端断开 stdout", dir.path(), &output);
}

/// 错误出口 fail() 的契约例外：`schema <未知操作>` 在打印错误 JSON 前不读
/// stdin、不开数据库，stdout 读端已关闭时写入必得 BrokenPipe——此时错误
/// 仍必须以非零退出（exit 1）可见，绝不允许复用普通输出的 exit 0 分支。
#[test]
fn error_exit_stays_nonzero_when_the_stdout_pipe_is_closed() {
    let dir = tempfile::tempdir().unwrap();
    let output = run_cli_with_stdout_closed(dir.path(), &["schema", "no-such-operation"], "");
    assert_eq!(
        output.status.code(),
        Some(1),
        "fail() 错误输出遇 BrokenPipe 应以 exit 1 可见，不得退化为 0：{:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
