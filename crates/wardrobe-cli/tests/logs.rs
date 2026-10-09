//! wardrobe-cli logs 子命令：JSON 封套契约、过滤参数与 diagnostics.logs.query 行为一致、
//! 游标分页与类型化校验错误。先例：entrypoints.rs、logging.rs。

use serde_json::{json, Value};
use std::process::{Command, Output};

fn run(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn cli_json(args: &[&str]) -> Value {
    let output = run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    serde_json::from_slice(&output.stdout).unwrap()
}

/// 可信本机入口制造操作（产生日志事件）。
fn trusted(path: &std::path::Path, operation: &str, input: Value) -> Value {
    cli_json(&[
        "--trusted-local",
        "--db",
        path.to_str().unwrap(),
        operation,
        &input.to_string(),
    ])
}

/// logs 子命令：CLI 即可信本机入口，无需 --trusted-local。
fn logs(path: &std::path::Path, extra: &[&str]) -> Output {
    let mut args: Vec<String> = ["--db", path.to_str().unwrap(), "logs"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.extend(extra.iter().map(|s| s.to_string()));
    run(&args)
}

/// logs 查询的断言辅助：封套无错、exit 0，返回 result 部分。
fn query(path: &std::path::Path, extra: &[&str]) -> Value {
    let output = logs(path, extra);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

#[test]
fn logs_subcommand_returns_the_json_envelope_with_query_items() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    trusted(&db, "space.default", json!({}));
    trusted(
        &db,
        "clothing.create",
        json!({"member_id":"default","name":"日志上衣","category":"top"}),
    );
    let output = logs(&db, &[]);
    assert_eq!(output.status.code(), Some(0), "logs 子命令应成功");
    // stdout 恰好一个 {"schema_version":1,...} 封套（from_slice 拒绝任何额外字节）
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["schema_version"], 1);
    let result = &response["result"];
    assert_eq!(result["schema_version"], 1);
    let items = result["items"].as_array().unwrap();
    assert!(!items.is_empty(), "{result}");
    for item in items {
        assert!(item["ts"].is_string(), "{item}");
        assert!(item["level"].is_string(), "{item}");
        assert!(item["event_type"].is_string(), "{item}");
        assert!(item["run_id"].is_string(), "{item}");
    }
    // 无过滤且事件少于一页上限：next_cursor 为 null 表示末页
    assert_eq!(result["next_cursor"], Value::Null);
}

#[test]
fn logs_filters_narrow_results_consistent_with_the_diagnostics_query() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    trusted(&db, "space.default", json!({}));
    for name in ["过滤上衣", "过滤裤子"] {
        trusted(
            &db,
            "clothing.create",
            json!({"member_id":"default","name":name,"category":"top"}),
        );
    }
    // 失败操作：错误封套携带 trace_id（加法变更），logs --trace-id 应能精确回查该次调用
    let rejected = trusted(
        &db,
        "clothing.create",
        json!({"member_id":"default","name":" ","category":"top"}),
    );
    let trace_id = rejected["error"]["trace_id"].as_str().unwrap().to_string();

    let all_items = query(&db, &[])["items"].as_array().unwrap().clone();
    // 四个制造进程各写 run_start + 操作事件；查询进程自身的 run_start 在读取前已写入，同样可见
    assert_eq!(all_items.len(), 9, "{all_items:?}");

    // --level error：只含错误级事件，含失败操作的 operation 事件
    let errors = query(&db, &["--level", "error"])["items"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!errors.is_empty());
    assert!(
        errors.iter().all(|event| event["level"] == "error"),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|event| event["operation"] == "clothing.create"
                && event["result"] == "invalid_input")
    );

    // --trace-id：错误输出里的链路标识可精确回查该次调用的全部事件
    let traced = query(&db, &["--trace-id", &trace_id])["items"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!traced.is_empty(), "{traced:?}");
    assert!(
        traced
            .iter()
            .all(|event| event["trace_id"] == trace_id.as_str()),
        "{traced:?}"
    );
    assert!(
        traced
            .iter()
            .any(|event| event["operation"] == "clothing.create"
                && event["result"] == "invalid_input")
    );

    // --operation 前缀：三次 clothing.create，排除 space.default 与 run_start
    let creations = query(&db, &["--operation", "clothing.create"])["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(creations.len(), 3, "{creations:?}");
    assert!(creations.iter().all(|event| event["operation"]
        .as_str()
        .unwrap()
        .starts_with("clothing.create")));

    // --run-id：首个运行会话的事件（run_start + space.default）共享同一 run_id
    let first_run = all_items[0]["run_id"].as_str().unwrap().to_string();
    let same_run = query(&db, &["--run-id", &first_run])["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(same_run.len(), 2, "{same_run:?}");
    assert!(same_run
        .iter()
        .all(|event| event["run_id"] == first_run.as_str()));

    // --keyword：大小写不敏感子串匹配事件文本（错误码在事件里，输入值不在）
    let keyword = query(&db, &["--keyword", "INVALID_INPUT"])["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(keyword.len(), 1, "{keyword:?}");
    assert_eq!(keyword[0]["result"], "invalid_input");

    // --since/--until：时间窗过滤（远期边界）。用操作前缀隔离制造阶段的事件，
    // 查询进程自身产生的日志（diagnostics.logs.query）不进入断言，避免跨进程计数竞态
    let recent = query(
        &db,
        &[
            "--operation",
            "clothing.create",
            "--since",
            "2000-01-01T00:00:00Z",
        ],
    );
    assert_eq!(recent["items"].as_array().unwrap().len(), 3, "{recent:?}");
    let ancient = query(
        &db,
        &[
            "--operation",
            "clothing.create",
            "--until",
            "2000-01-01T00:00:00Z",
        ],
    );
    assert_eq!(ancient["items"].as_array().unwrap().len(), 0, "{ancient:?}");
    assert_eq!(ancient["next_cursor"], Value::Null);
}

#[test]
fn logs_pagination_follows_next_cursor_until_null_on_the_last_page() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    trusted(&db, "space.default", json!({}));
    for index in 0..4 {
        trusted(
            &db,
            "clothing.create",
            json!({"member_id":"default","name":format!("分页{index}"),"category":"top"}),
        );
    }
    // 只分页 clothing.create 事件：查询自身产生的日志不进入该过滤结果
    let ground_truth = query(&db, &["--operation", "clothing.create"])["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(ground_truth.len(), 4, "{ground_truth:?}");

    let mut collected: Vec<Value> = Vec::new();
    let mut extra: Vec<String> = ["--operation", "clothing.create", "--limit", "2"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for _ in 0..10 {
        let flags: Vec<&str> = extra.iter().map(String::as_str).collect();
        let result = query(&db, &flags);
        let items = result["items"].as_array().unwrap().clone();
        assert!(items.len() <= 2, "{items:?}");
        collected.extend(items);
        match result["next_cursor"].as_str() {
            // 续页须保留查询条件（GLOSSARY 分页游标）
            Some(cursor) => {
                extra.truncate(2);
                extra.extend([
                    "--limit".into(),
                    "2".into(),
                    "--cursor".into(),
                    cursor.into(),
                ]);
            }
            None => break,
        }
    }
    assert_eq!(collected.len(), 4, "{collected:?}");
    assert_eq!(collected, ground_truth);
}

#[test]
fn logs_rejects_invalid_arguments_with_typed_errors() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    trusted(&db, "space.default", json!({}));
    let expect_rejected = |extra: &[&str], code: &str| {
        let output = logs(&db, extra);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(2), "{code}: {stdout}");
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["code"], code, "{response}");
        assert!(response["error"]["message"].is_string(), "{response}");
    };
    expect_rejected(&["--nope", "x"], "invalid_arguments");
    expect_rejected(&["--level"], "invalid_arguments");
    expect_rejected(
        &["--level", "info", "--level", "error"],
        "invalid_arguments",
    );
    expect_rejected(&["--limit", "abc"], "invalid_arguments");
    expect_rejected(&["extra"], "invalid_arguments");
    // 穿透到 diagnostics.logs.query 的业务校验同样是类型化错误封套
    expect_rejected(&["--level", "bogus"], "invalid_input");
    expect_rejected(&["--since", "not-a-time"], "invalid_input");
    expect_rejected(&["--limit", "0"], "invalid_input");
    expect_rejected(&["--cursor", "bogus"], "invalid_input");
}

#[test]
fn help_advertises_the_logs_subcommand_and_schema_still_describes_the_operation() {
    let help = cli_json(&["help"]);
    assert!(help.get("error").is_none(), "{help}");
    let usage = help["result"]["usage"].as_array().unwrap();
    assert!(
        usage.iter().any(|u| u.as_str().unwrap().contains("logs")),
        "usage 必须介绍 logs 子命令：{usage:?}"
    );
    let schema = cli_json(&["schema", "diagnostics.logs.query"]);
    assert_eq!(schema["result"]["name"], "diagnostics.logs.query");
}
