//! #41 CLI logs/doctor 子命令契约：JSON 信封、过滤语义与 GUI 检查同源、
//! 日志系统不污染 MCP stdio 会话。

use serde_json::{json, Value};
use std::process::{Command, Stdio};

fn cli(path: &std::path::Path, args: &[&str]) -> Value {
    let mut argv: Vec<&str> = vec!["--db", path.to_str().unwrap()];
    argv.extend_from_slice(args);
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(&argv)
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout 不是单个 JSON：{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn trusted(path: &std::path::Path, operation: &str, input: Value) -> Value {
    cli(
        path,
        &[
            "--trusted-local",
            operation,
            &input.to_string(),
        ],
    )
}

#[test]
fn logs_filters_by_level_trace_and_limit_with_contract_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    trusted(&db, "space.default", json!({}));
    trusted(
        &db,
        "clothing.create",
        json!({"member_id":"default","name":"CLI日志验收","category":"top"}),
    );
    let invalid = trusted(
        &db,
        "clothing.create",
        json!({"member_id":"default","name":" ","category":"top"}),
    );
    assert_eq!(invalid["error"]["code"], "invalid_input");
    // 错误对象带 trace_id（加法变更，既有 {code,message} 语义不变）
    assert!(invalid["error"]["trace_id"].is_string());

    let logs = |extra: &[&str]| cli(&db, &{
        let mut argv = vec!["logs"];
        argv.extend_from_slice(extra);
        argv
    });
    let all = logs(&["--limit", "500"])["result"].clone();
    assert_eq!(all["schema_version"], 1);
    assert!(all["next_cursor"].is_null() || all["next_cursor"].is_string());
    let items = all["items"].as_array().unwrap();
    assert!(items.len() >= 3, "至少有 open/space.default/clothing 事件");
    for event in items {
        assert!(event["ts"].is_string(), "事件带时间戳");
        assert!(event["run_id"].is_string(), "事件带运行会话");
        assert!(event["op"].is_string() && event["result"].is_string());
    }

    // 级别过滤
    let warns = logs(&["--level", "warn", "--limit", "500"])["result"]["items"]
        .as_array()
        .unwrap()
        .len();
    assert!(warns >= 1, "invalid_input 记为 warn");
    let errors = logs(&["--level", "error", "--limit", "500"])["result"]["items"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(errors, 0, "健康会话无 error");

    // trace 过滤：错误对象的 trace_id 能查回失败操作事件
    let trace = invalid["error"]["trace_id"].as_str().unwrap();
    let by_trace = logs(&["--trace", trace])["result"]["items"].clone();
    let events = by_trace.as_array().unwrap();
    assert!(!events.is_empty());
    assert!(events
        .iter()
        .all(|event| event["trace_id"].as_str() == Some(trace)));
    let failure = events
        .iter()
        .find(|event| event["op"] == "clothing.create")
        .unwrap();
    assert_eq!(failure["result"], "invalid_input");
    // 输入校验类拒绝本身就有可见中文消息；真实原因（reason）只由系统级错误携带
    assert!(failure["detail"]["reason"].is_null());

    // limit 生效
    assert_eq!(
        logs(&["--limit", "1"])["result"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // op 前缀过滤
    let op_logs = logs(&["--op", "clothing.", "--limit", "500"]);
    let clothing = op_logs["result"]["items"].as_array().unwrap();
    assert!(clothing.iter().all(|event| event["op"]
        .as_str()
        .unwrap()
        .starts_with("clothing.")));
    // 非法旗标值
    let bad = cli(&db, &["logs", "--level", "loud"]);
    assert_eq!(bad["error"]["code"], "invalid_arguments");
}

#[test]
fn doctor_outputs_summary_and_checks_same_as_application_source() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    trusted(&db, "space.default", json!({}));
    let report = cli(&db, &["doctor"])["result"].clone();
    assert_eq!(report["schema_version"], 1);
    let summary = &report["summary"];
    assert!(summary["run_id"].is_string());
    assert_eq!(summary["database"]["matches_application"], true);
    assert!(summary["jobs"].is_object());
    let checks = &report["checks"];
    let items = checks["items"].as_array().unwrap();
    for expected in [
        "data_directory",
        "database",
        "database_wal",
        "logs",
        "disk_space",
        "installation_generation",
        "encryption",
        "model_credentials",
        "stuck_jobs",
        "pending_move_marker",
    ] {
        assert!(
            items.iter().any(|item| item["id"] == expected),
            "doctor 缺少检查项 {expected}"
        );
    }
    // 与 Application API diagnostics.checks 同源：同环境重跑一次核心输出一致
    let via_app = trusted(&db, "diagnostics.checks", json!({}))["result"].clone();
    let doctor_ids: Vec<&str> = items
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    let app_ids: Vec<&str> = via_app["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(doctor_ids, app_ids, "doctor 与 GUI checks 检查项同源");
    let doctor_database = items.iter().find(|i| i["id"] == "database").unwrap();
    let app_database = via_app["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "database")
        .unwrap();
    assert_eq!(doctor_database["status"], app_database["status"]);
    // 健康环境无 fail
    assert!(items.iter().all(|item| item["status"] != "fail"));
}

#[test]
fn doctor_still_reports_checks_when_database_is_broken() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    trusted(&db, "space.default", json!({}));
    // 数据库由更高版本创建 → Application::open 失败，doctor 仍输出体检报告
    {
        let fixture = rusqlite::Connection::open(&db).unwrap();
        fixture.execute_batch("PRAGMA user_version=99;").unwrap();
    }
    let report = cli(&db, &["doctor"])["result"].clone();
    let items = report["checks"]["items"].as_array().unwrap();
    let database = items.iter().find(|i| i["id"] == "database").unwrap();
    assert_eq!(database["status"], "fail");
    assert!(report["summary"]["database"]["opened"] == false);
}

#[test]
fn mcp_stdio_session_is_not_polluted_by_logging() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    trusted(&db, "space.default", json!({}));
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 每行都是合法 JSON-RPC（日志只写文件，不占 stdout）
    for line in stdout.lines() {
        let value: Value = serde_json::from_str(line)
            .unwrap_or_else(|_| panic!("MCP stdout 行不是 JSON：{line}"));
        assert!(value["jsonrpc"] == "2.0", "非 JSON-RPC 行：{line}");
    }
}
