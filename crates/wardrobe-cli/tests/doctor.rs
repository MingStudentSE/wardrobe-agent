//! wardrobe-cli doctor 子命令：= diagnostics.summary + diagnostics.checks 同源输出
//! （JSON 封套，先例 logs.rs）；数据库无法打开时降级输出文件系统级体检与失败原因。
//! 先例：logs.rs、entrypoints.rs。

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

/// 可信本机入口制造操作与对照输出。
fn trusted(path: &std::path::Path, operation: &str, input: Value) -> Value {
    cli_json(&[
        "--trusted-local",
        "--db",
        path.to_str().unwrap(),
        operation,
        &input.to_string(),
    ])
}

#[test]
fn doctor_outputs_the_same_summary_and_checks_as_the_diagnostics_operations() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    trusted(&db, "space.default", json!({}));
    trusted(
        &db,
        "clothing.create",
        json!({"member_id":"default","name":"体检上衣","category":"top"}),
    );

    let output = run(&["--db".to_string(), db.to_str().unwrap().to_string(), "doctor".to_string()]);
    assert_eq!(output.status.code(), Some(0), "doctor 应成功");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // stdout 恰好一个 {"schema_version":1,...} 封套（from_slice 拒绝任何额外字节）
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["schema_version"], 1);
    let report = &response["result"];
    assert_eq!(report["error"], Value::Null, "正常打开无错误：{report}");
    assert!(report["summary"].is_object(), "{report}");

    // 同源一致性：checks 与直接执行 diagnostics.checks 逐字节同构。
    // 例外：disk_space 的 free_bytes 随两次调用之间磁盘实际占用漂移（与 core 全量测试并行时
    // 必然发生），只断言该项存在且结论字段同源；其余九项仍逐字节同源。
    let direct = trusted(&db, "diagnostics.checks", json!({}));
    assert!(direct.get("error").is_none(), "{direct}");
    let doctor_items = report["checks"]["checks"].as_array().unwrap();
    let direct_items = direct["result"]["checks"].as_array().unwrap();
    assert_eq!(doctor_items.len(), direct_items.len(), "检查项数量同源");
    for (doctor_item, direct_item) in doctor_items.iter().zip(direct_items.iter()) {
        if doctor_item["id"] == "disk_space" {
            assert_eq!(doctor_item["id"], direct_item["id"], "disk_space 存在且同源");
            continue;
        }
        assert_eq!(doctor_item, direct_item, "doctor 与 checks 操作同源");
    }
    assert_eq!(
        report["checks"]["counts"], direct["result"]["counts"],
        "doctor 与 checks 操作同源：counts"
    );
    // summary 稳定字段与直接执行 diagnostics.summary 一致（run_id/最近错误/日志占用属各运行会话）
    let summary_direct = trusted(&db, "diagnostics.summary", json!({}))["result"].clone();
    for field in [
        "schema_version",
        "database_schema_version",
        "database_user_version",
        "installation_generation",
        "jobs_by_state",
        "provider_outcome_unknown",
    ] {
        assert_eq!(
            report["summary"][field], summary_direct[field],
            "doctor 与 summary 操作同源：{field}"
        );
    }
    assert!(
        stdout.lines().count() == 1,
        "doctor stdout 只有一行 JSON：{stdout}"
    );
}

#[test]
fn doctor_degrades_to_filesystem_checks_when_database_cannot_open() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    // 直连接缝：制造「由更新版本应用创建」的数据库（过高模式版本）
    {
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .pragma_update(None, "user_version", wardrobe_core::DATABASE_SCHEMA_VERSION + 1)
            .unwrap();
    }
    let output = run(&["--db".to_string(), db.to_str().unwrap().to_string(), "doctor".to_string()]);
    assert_eq!(output.status.code(), Some(0), "降级报告也是成功的 doctor 运行");
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = &response["result"];
    assert_eq!(report["error"]["code"], "unsupported_schema", "{report}");
    assert!(report["summary"].is_null(), "{report}");
    let items = report["checks"]["checks"].as_array().unwrap();
    assert_eq!(items.len(), 10, "{report}");
    let schema = items.iter().find(|i| i["id"] == "database_schema").unwrap();
    assert_eq!(schema["status"], "fail", "{schema}");
    let data_directory = items.iter().find(|i| i["id"] == "data_directory").unwrap();
    assert_eq!(data_directory["status"], "pass", "{data_directory}");
}

#[test]
fn doctor_rejects_input_and_extra_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wardrobe.sqlite");
    for args in [
        vec!["--db", db.to_str().unwrap(), "doctor", "{}"],
        vec!["--db", db.to_str().unwrap(), "doctor", "--input-stdin"],
        vec!["--db", db.to_str().unwrap(), "doctor", "extra"],
    ] {
        let output = run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["code"], "invalid_arguments", "{args:?}");
        assert_eq!(output.status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn help_documents_the_doctor_subcommand() {
    let response = cli_json(&["help"]);
    let usage = response["result"]["usage"].as_array().unwrap();
    assert!(
        usage.iter().any(|u| u.as_str().unwrap_or("").contains("doctor")),
        "usage 应包含 doctor：{usage:?}"
    );
    let doctor = &response["result"]["doctor"];
    assert_eq!(
        doctor["usage"],
        json!("wardrobe-cli [--db <数据库路径>] doctor"),
        "{doctor}"
    );
}
