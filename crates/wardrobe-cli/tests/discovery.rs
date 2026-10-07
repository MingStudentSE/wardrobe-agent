use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn help_and_version_succeed_as_json_without_touching_storage() {
    for flag in ["--help", "--version"] {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let data = dir.path().join("data");
        let db = dir.path().join("explicit").join("wardrobe.sqlite");
        std::fs::create_dir(&home).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", db.to_str().unwrap(), flag])
            .env("HOME", &home)
            .env("WARDROBE_DATA_DIR", &data)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{flag} must exit 0, got {:?}: stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(envelope.get("error").is_none(), "{flag}: {envelope}");
        assert!(
            envelope["schema_version"].as_u64() == Some(1),
            "{flag} envelope must be versioned: {envelope}"
        );
        assert!(
            !db.exists() && !db.parent().unwrap().exists(),
            "{flag} must not create the database or its directory"
        );
        assert!(!data.exists(), "{flag} must not create WARDROBE_DATA_DIR");
        assert!(
            !home.join("Library").exists() && !home.join(".local").exists(),
            "{flag} must not write into HOME"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .arg("--version")
        .env("HOME", dir.path().join("home"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        envelope["result"]["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "--version must report the cargo package version"
    );
}

fn metadata_run(dir: &std::path::Path, args: &[&str]) -> (Option<i32>, Value) {
    let db = dir.join("explicit").join("wardrobe.sqlite");
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", db.to_str().unwrap()])
        .args(args)
        .env("HOME", dir.join("home"))
        .env("WARDROBE_DATA_DIR", dir.join("data"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        !db.exists() && !db.parent().unwrap().exists(),
        "metadata call {args:?} must not create the database or its directory"
    );
    (output.status.code(), envelope)
}

#[test]
fn help_and_schema_describe_registered_operations_without_storage() {
    let dir = tempfile::tempdir().unwrap();
    let (help_code, help) = metadata_run(dir.path(), &["help", "clothing.create"]);
    assert_eq!(help_code, Some(0), "help clothing.create: {help}");
    assert!(help.get("error").is_none(), "{help}");
    assert_eq!(help["schema_version"], 1);
    assert_eq!(help["result"]["name"], "clothing.create");
    assert!(help["result"]["description"].is_string(), "{help}");
    assert!(help["result"]["inputSchema"].is_object(), "{help}");
    assert!(help["result"]["outputSchema"].is_object(), "{help}");
    let (schema_code, schema) = metadata_run(dir.path(), &["schema", "clothing.create"]);
    assert_eq!(schema_code, Some(0), "schema clothing.create: {schema}");
    assert_eq!(schema["result"]["name"], "clothing.create");
    assert_eq!(
        schema["result"]["description"],
        help["result"]["description"]
    );
    assert_eq!(
        schema["result"]["inputSchema"],
        help["result"]["inputSchema"]
    );
    assert_eq!(
        schema["result"]["outputSchema"],
        help["result"]["outputSchema"]
    );

    let (bare_help_code, bare_help) = metadata_run(dir.path(), &["help"]);
    assert_eq!(bare_help_code, Some(0), "bare help: {bare_help}");
    let usage = bare_help["result"]["usage"].as_array().unwrap();
    assert!(
        usage
            .iter()
            .any(|u| u.as_str().unwrap().contains("help <operation>")),
        "usage must advertise help <operation>: {usage:?}"
    );
    assert!(
        usage
            .iter()
            .any(|u| u.as_str().unwrap().contains("schema <operation>")),
        "usage must advertise schema <operation>: {usage:?}"
    );
    assert_eq!(
        bare_help["result"]["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "help must carry the package version"
    );
    let trusted = bare_help["result"]["flags"]["--trusted-local"]
        .as_str()
        .unwrap();
    assert!(
        trusted.contains("维护") && trusted.contains("--client-stdin"),
        "--trusted-local must describe the human local maintenance bypass and point external AI to the scoped client path: {trusted}"
    );

    let (unknown_code, unknown) = metadata_run(dir.path(), &["schema", "not.an.operation"]);
    assert_eq!(
        unknown_code,
        Some(2),
        "unknown operation must exit 2: {unknown}"
    );
    assert_eq!(unknown["error"]["code"], "unsupported_operation");

    let (missing_code, missing) = metadata_run(dir.path(), &["schema"]);
    assert_eq!(
        missing_code,
        Some(2),
        "schema without operation must exit 2: {missing}"
    );
    assert_eq!(missing["error"]["code"], "invalid_arguments");
}

fn bounded_output(
    mut child: std::process::Child,
    seconds: u64,
    context: &str,
) -> (Option<i32>, String, String) {
    // 并发排空 stdout/stderr，避免大输出（如 capabilities 注册表）撑满管道造成死锁
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            let _ = std::io::Read::read_to_end(pipe, &mut buffer);
        }
        buffer
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            let _ = std::io::Read::read_to_end(pipe, &mut buffer);
        }
        buffer
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    let status = loop {
        match child.try_wait().unwrap() {
            Some(status) => break status,
            None => {
                if std::time::Instant::now() >= deadline {
                    child.kill().unwrap();
                    let _ = child.wait();
                    let _ = stdout_reader.join();
                    let _ = stderr_reader.join();
                    panic!("{context} blocked instead of exiting; killed");
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    };
    child.wait().unwrap();
    let stdout = String::from_utf8(stdout_reader.join().unwrap()).unwrap();
    let stderr = String::from_utf8(stderr_reader.join().unwrap()).unwrap();
    (status.code(), stdout, stderr)
}

#[test]
fn only_literal_mcp_command_switches_transport_and_version_matches_package() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();

    // --db mcp capabilities："mcp" 是 --db 的字面量值，capabilities 仍是 CLI 发现，不得进入 MCP 传输或阻塞
    let (code, body, _) = bounded_output(
        Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", "mcp", "capabilities"])
            .current_dir(dir.path())
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
        30,
        "--db mcp capabilities",
    );
    assert_eq!(code, Some(0), "--db mcp capabilities: {body}");
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert!(envelope.get("error").is_none(), "{envelope}");
    assert!(
        !envelope["result"]["operations"]
            .as_array()
            .unwrap()
            .is_empty(),
        "capabilities discovery must list registered operations"
    );

    // "mcp" 作为输入参数只是普通字面量，不得切换传输
    let (code, body, _) = bounded_output(
        Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", "literal.sqlite", "clothing.list", "mcp"])
            .current_dir(dir.path())
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
        30,
        "clothing.list mcp",
    );
    assert_eq!(code, Some(2), "clothing.list mcp: {body}");
    assert!(
        body.lines().count() == 1 && serde_json::from_str::<Value>(&body).is_ok(),
        "must stay single-line CLI JSON, not MCP transport: {body}"
    );
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        envelope["error"]["code"], "invalid_json",
        "\"mcp\" as input argument is a literal, not a transport switch"
    );

    // MCP serverInfo 版本必须来自 Cargo 包版本
    let db = dir.path().join("mcp.sqlite");
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", db.to_str().unwrap(), "mcp"])
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})
    )
    .unwrap();
    let (code, body, _) = bounded_output(child, 30, "mcp initialize");
    assert_eq!(code, Some(0), "mcp initialize: {body}");
    let response: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    assert_eq!(
        response["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn invalid_argument_combinations_fail_before_storage_with_invalid_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let data = dir.path().join("data");
    let invalid = |args: &[&str], context: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(args)
            .current_dir(dir.path())
            .env("HOME", &home)
            .env("WARDROBE_DATA_DIR", &data)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{context} must exit 2: {envelope}"
        );
        assert_eq!(
            envelope["error"]["code"], "invalid_arguments",
            "{context} must be a syntax error: {envelope}"
        );
        let created: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            created,
            vec!["home".to_string()],
            "{context} must not create any database or data directory"
        );
    };
    invalid(
        &[
            "--db",
            "a.sqlite",
            "--db",
            "b.sqlite",
            "clothing.list",
            r#"{"member_id":"default"}"#,
        ],
        "duplicate --db",
    );
    invalid(&["--db"], "missing --db value");
    invalid(&["--bogus", "clothing.list"], "unknown flag");
    invalid(
        &["clothing.list", r#"{"member_id":"default"}"#, "extra"],
        "excess positional argument",
    );
    invalid(
        &[
            "--db",
            "c.sqlite",
            "clothing.list",
            "--input-stdin",
            "--client-stdin",
        ],
        "conflicting stdin modes",
    );
    invalid(
        &["--db", "d.sqlite", "mcp", "extra"],
        "mcp with excess argument",
    );
    invalid(
        &["--db", "e.sqlite", "mcp", "--input-stdin"],
        "mcp with CLI stdin mode",
    );
}

fn assert_invalid(dir: &std::path::Path, args: &[&str], context: &str) {
    let home = dir.join("home");
    let data = dir.join("data");
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(args)
        .current_dir(dir)
        .env("HOME", &home)
        .env("WARDROBE_DATA_DIR", &data)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "{context} must exit 2: {envelope}"
    );
    assert_eq!(
        envelope["error"]["code"], "invalid_arguments",
        "{context} must be a syntax error: {envelope}"
    );
    let created: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        created,
        vec!["home".to_string()],
        "{context} must not create any database or data directory"
    );
}

#[test]
fn metadata_flags_are_standalone_and_empty_command_stdin_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("home")).unwrap();
    assert_invalid(
        dir.path(),
        &["--help", "extra"],
        "--help with trailing argument",
    );
    assert_invalid(
        dir.path(),
        &["--version", "--bogus"],
        "--version with unknown flag",
    );
    assert_invalid(
        dir.path(),
        &["clothing.list", "{}", "--help"],
        "--help after operation and JSON",
    );
    assert_invalid(
        dir.path(),
        &["--trusted-local", "--help"],
        "--help is standalone except for the --db global",
    );
    assert_invalid(
        dir.path(),
        &["--db", "s.sqlite", "--client-stdin"],
        "stdin mode without a command",
    );
    assert_invalid(
        dir.path(),
        &["--db", "s.sqlite", "--input-stdin"],
        "input-stdin without a command",
    );
}

#[test]
fn duplicate_trusted_local_is_a_syntax_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("home")).unwrap();
    assert_invalid(
        dir.path(),
        &[
            "--trusted-local",
            "--trusted-local",
            "--db",
            "dup.sqlite",
            "capabilities",
            "{}",
        ],
        "duplicate --trusted-local",
    );
}

#[test]
fn mcp_startup_failure_keeps_the_jsonrpc_stream_clean() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let db = blocker.join("wardrobe.sqlite");
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", db.to_str().unwrap(), "mcp"])
        .env("HOME", dir.path().join("home"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})
    )
    .unwrap();
    let (code, stdout, stderr) = bounded_output(child, 30, "mcp with unwritable storage");
    assert_eq!(code, Some(1), "startup failure must exit 1");
    assert!(
        stdout.trim().is_empty(),
        "MCP startup failure must not write a CLI envelope into the JSON-RPC stream: {stdout}"
    );
    assert!(
        !stderr.trim().is_empty(),
        "startup failure must leave a nonsecret diagnostic on stderr"
    );
}

#[test]
fn stdin_and_secret_transport_guards_fail_without_panic_or_token_echo() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let db = dir.path().join("stdin.sqlite");
    let run_stdin = |args: &[&str], body: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(args)
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(mut pipe) = child.stdin.take() {
            let _ = pipe.write_all(body.as_bytes());
        }
        let (code, stdout, stderr) = bounded_output(child, 30, &format!("stdin case {args:?}"));
        let envelope: Value = serde_json::from_str(&stdout).unwrap();
        (code, envelope, stderr)
    };

    let (code, envelope, stderr) = run_stdin(
        &[
            "--trusted-local",
            "--db",
            db.to_str().unwrap(),
            "clothing.create",
            "--input-stdin",
        ],
        "{not json",
    );
    assert_eq!(code, Some(2), "malformed stdin: {envelope}");
    assert_eq!(envelope["error"]["code"], "invalid_json");
    assert!(
        stderr.trim().is_empty(),
        "diagnostics must stay nonsecret: {stderr}"
    );

    let oversized = format!("{{\"name\":\"{}\"}}", "x".repeat(1_048_577));
    let (code, envelope, _) = run_stdin(
        &[
            "--trusted-local",
            "--db",
            db.to_str().unwrap(),
            "clothing.create",
            "--input-stdin",
        ],
        &oversized,
    );
    assert_eq!(code, Some(2), "oversize stdin: {envelope}");
    assert_eq!(envelope["error"]["code"], "invalid_input");

    let token = "wardrobe-invalid-token-echo-check";
    let (code, envelope, _) = run_stdin(
        &[
            "--db",
            db.to_str().unwrap(),
            "clothing.list",
            "--client-stdin",
        ],
        &json!({"token":token,"input":{"member_id":"default"}}).to_string(),
    );
    assert_eq!(code, Some(2), "invalid token: {envelope}");
    assert_eq!(envelope["error"]["code"], "unauthorized");
    assert!(
        !envelope.to_string().contains(token),
        "token must not be echoed: {envelope}"
    );

    let secret_db = dir.path().join("secret.sqlite");
    let (code, envelope, _) = run_stdin(
        &[
            "--db",
            secret_db.to_str().unwrap(),
            "provider.credential.set",
            "{}",
        ],
        "",
    );
    assert_eq!(code, Some(2), "secret without stdin: {envelope}");
    assert_eq!(envelope["error"]["code"], "secret_requires_stdin");
    let crypto_db = dir.path().join("crypto.sqlite");
    let (code, envelope, _) = run_stdin(
        &[
            "--db",
            crypto_db.to_str().unwrap(),
            "encryption.status",
            "{}",
        ],
        "",
    );
    assert_eq!(code, Some(2), "encryption without stdin: {envelope}");
    assert_eq!(envelope["error"]["code"], "secret_requires_stdin");
    assert!(
        !secret_db.exists() && !crypto_db.exists(),
        "secret stdin-only rejection must happen before storage opens"
    );
}

#[test]
fn db_value_consumes_flag_like_tokens_and_separator_forces_literals() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();

    // "--help" 被 --db 无条件消耗为字面量路径值；--version 元数据照常执行，且不创建该路径
    let (code, body, _) = bounded_output(
        Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", "--help", "--version"])
            .current_dir(dir.path())
            .env("HOME", &home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
        30,
        "--db --help --version",
    );
    assert_eq!(code, Some(0), "--db --help --version: {body}");
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        envelope["result"]["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "{envelope}"
    );
    assert!(
        !dir.path().join("--help").exists(),
        "flag-like --db value must never become a created file"
    );

    // -- 之后的 "--help" 只是字面量 JSON 输入槽：命令 + JSON，产生 invalid_json 而非元数据
    let db = dir.path().join("separator.sqlite");
    let (code, body, _) = bounded_output(
        Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args([
                "--db",
                db.to_str().unwrap(),
                "clothing.list",
                "--",
                "--help",
            ])
            .env("HOME", &home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
        30,
        "clothing.list -- --help",
    );
    assert_eq!(code, Some(2), "clothing.list -- --help: {body}");
    assert!(
        body.lines().count() == 1 && serde_json::from_str::<Value>(&body).is_ok(),
        "must stay single-line CLI JSON: {body}"
    );
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        envelope["error"]["code"], "invalid_json",
        "literal after -- must be parsed as JSON input, not metadata: {envelope}"
    );
    assert!(
        envelope["result"].is_null() && !body.contains("usage"),
        "separator case must not emit help metadata: {envelope}"
    );
}

#[test]
fn storage_failures_exit_1_while_json_errors_stay_2() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let db = dir.path().join("storage-exit.sqlite");
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(args)
            .env("HOME", &home)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        (output.status.code(), envelope)
    };
    let setup = run(&[
        "--trusted-local",
        "--db",
        db.to_str().unwrap(),
        "space.default",
        "{}",
    ]);
    assert_eq!(setup.0, Some(0), "space.default setup: {}", setup.1);

    // 一次性真实 SQLite 上，导入不存在的临时路径是存储失败：必须 exit 1
    let missing = dir.path().join("does-not-exist.heic");
    let (code, envelope) = run(&[
        "--trusted-local",
        "--db",
        db.to_str().unwrap(),
        "assets.import",
        &json!({"member_id":"default","path":missing}).to_string(),
    ]);
    assert_eq!(code, Some(1), "storage failure must exit 1: {envelope}");
    assert_eq!(envelope["error"]["code"], "storage_error");

    // 既有 invalid_json 仍是 exit 2
    let (code, envelope) = run(&[
        "--trusted-local",
        "--db",
        db.to_str().unwrap(),
        "clothing.create",
        "{bad json",
    ]);
    assert_eq!(code, Some(2), "invalid_json must stay exit 2: {envelope}");
    assert_eq!(envelope["error"]["code"], "invalid_json");
}
