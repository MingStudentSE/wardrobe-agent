use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};

/// 每次调用都是全新CLI进程：读到的状态即落盘状态。
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

fn full_client_token(path: &std::path::Path) -> String {
    let app = wardrobe_core::Application::open(path).unwrap();
    let result=app.execute("client.authorize",json!({"name":"测试客户端","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
    result["token"].as_str().unwrap().to_string()
}

/// 每次调用都启动独立的stdin MCP服务进程。
fn mcp_call(
    path: &std::path::Path,
    token: &str,
    operation: &str,
    arguments: Value,
) -> (bool, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":operation,"arguments":arguments}})
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(!body.contains(token), "响应不得回显连接令牌");
    let response: Value = serde_json::from_str(body.trim()).unwrap();
    let payload: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    (response["result"]["isError"] == true, payload)
}

fn mcp_tool_descriptors(path: &std::path::Path, token: &str) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"authorization_token":token}})
    )
    .unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})
    )
    .unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(!body.contains(token), "发现结果不得回显连接令牌");
    let responses: Vec<Value> = body
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 2);
    responses[1]["result"]["tools"].as_array().unwrap().clone()
}

fn plan_input(member: &str, device: &str, name: &str, local_time: &str) -> Value {
    json!({
        "member_id":member,
        "name":name,
        "kind":"recommendation",
        "local_time":local_time,
        "timezone":"Asia/Shanghai",
        "weekdays":[1,2,3,4,5],
        "execution_device_id":device,
        "occasion":"daily",
        "target_day_offset":1,
        "enabled":true
    })
}

#[test]
fn cli_schedule_lifecycle_pause_reopen_and_mcp_share_identity_with_declared_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let device = cli(&path, "schedule.device", json!({}))["result"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let capabilities = cli(&path, "capabilities", json!({}))["result"].clone();
    // 能力声明中list与runtime包装的输出schema_version是常量1，实际输出必须一致。
    for operation in ["schedule.list", "schedule.runtime.get"] {
        let descriptor = capabilities["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == operation)
            .unwrap();
        let schema = &descriptor["outputSchema"];
        assert_eq!(schema["properties"]["schema_version"]["const"], 1);
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "schema_version"));
    }
    let created = cli(
        &path,
        "schedule.create",
        plan_input("default", &device, "工作日晨间推荐", "07:30"),
    )["result"]
        .clone();
    assert_eq!(created["schema_version"], 1);
    assert_eq!(created["revision"], 1);
    assert_eq!(created["target_day_offset"], 1);
    assert_eq!(created["timezone"], "Asia/Shanghai");
    assert_eq!(created["catchup_minutes"], 120);
    assert!(created["id"].is_string());
    // 动态时刻只验证存在；跨写入绝不比较整体时间戳。
    assert!(created["eligible_since"].is_string());
    let edited = cli(
        &path,
        "schedule.update",
        json!({
            "member_id":"default","id":created["id"],"expected_revision":1,
            "name":"工作日晨间提醒","local_time":"08:15"
        }),
    )["result"]
        .clone();
    assert_eq!(edited["revision"], 2);
    assert_eq!(edited["name"], "工作日晨间提醒");
    assert_eq!(edited["local_time"], "08:15");
    let stale = cli(
        &path,
        "schedule.update",
        json!({"member_id":"default","id":created["id"],"expected_revision":1,"name":"过期修改"}),
    );
    assert_eq!(stale["error"]["code"], "revision_conflict");
    let paused = cli(
        &path,
        "schedule.update",
        json!({"member_id":"default","id":created["id"],"expected_revision":2,"enabled":false}),
    )["result"]
        .clone();
    assert_eq!(paused["revision"], 3);
    assert_eq!(paused["enabled"], false);
    // 暂停写入后由全新CLI进程重新打开同一SQLite，验证暂停已持久化。
    let reopened = cli(&path, "schedule.list", json!({"member_id":"default"}))["result"].clone();
    assert_eq!(reopened["schema_version"], 1);
    let stored = reopened["plans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == created["id"])
        .unwrap();
    assert_eq!(stored["enabled"], false);
    assert_eq!(stored["revision"], 3);
    assert_eq!(stored["name"], "工作日晨间提醒");
    // 实际runtime包装输出与能力声明常量一致。
    assert_eq!(
        cli(&path, "schedule.runtime.get", json!({}))["result"]["schema_version"],
        1
    );
    let token = full_client_token(&path);
    let tools = mcp_tool_descriptors(&path, &token);
    let listed =
        tools.iter().find(|t| t["name"] == "schedule.list").unwrap()["outputSchema"].clone();
    assert_eq!(
        listed,
        capabilities["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == "schedule.list")
            .unwrap()["outputSchema"]
    );
    let (ok, mcp_list) = mcp_call(
        &path,
        &token,
        "schedule.list",
        json!({"member_id":"default"}),
    );
    assert!(!ok, "{mcp_list}");
    assert_eq!(mcp_list["schema_version"], 1);
    let shared = mcp_list["plans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == created["id"])
        .unwrap();
    assert_eq!(shared["id"], stored["id"]);
    assert_eq!(shared["revision"], stored["revision"]);
    assert_eq!(shared["enabled"], false);
    // CLI与MCP读的是同一份只读持久化快照，允许整体比较。
    assert_eq!(shared, stored);
}

#[test]
fn restricted_second_member_is_isolated_from_first_member_plans_and_mutations_denied() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli(&path, "space.default", json!({}));
    let device = cli(&path, "schedule.device", json!({}))["result"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let created = cli(
        &path,
        "schedule.create",
        plan_input("default", &device, "第一位成员计划", "07:30"),
    )["result"]
        .clone();
    cli(
        &path,
        "schedule.update",
        json!({
            "member_id":"default","id":created["id"],"expected_revision":1,
            "name":"已改名计划","local_time":"09:05"
        }),
    );
    let paused = cli(
        &path,
        "schedule.update",
        json!({"member_id":"default","id":created["id"],"expected_revision":2,"enabled":false}),
    )["result"]
        .clone();
    assert_eq!(paused["revision"], 3);
    let second = cli(&path, "member.create", json!({"name":"第二位成员"}))["result"]["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    // 公开API没有schedule.get；成员计划读取就是list/history/pending/notifications。
    let second_plan = cli(
        &path,
        "schedule.create",
        plan_input(&second, &device, "第二成员计划", "08:00"),
    )["result"]
        .clone();
    let capabilities = cli(&path, "capabilities", json!({}))["result"].clone();
    let operations = [
        "schedule.list",
        "schedule.history",
        "schedule.pending",
        "schedule.notifications",
        "schedule.create",
    ];
    // 授权scope必须逐项来自公开能力清单，而非臆造操作。
    for operation in operations {
        assert!(capabilities["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["name"] == operation));
    }
    let grant = cli(
        &path,
        "client.authorize",
        json!({
            "name":"受限日程客户端","mode":"limited","operations":operations,
            "member_ids":[second],"expires_at":null
        }),
    );
    assert!(grant.get("error").is_none(), "{grant}");
    let restricted = grant["result"]["token"].as_str().unwrap().to_string();
    let mcp =
        |operation: &str, arguments: Value| mcp_call(&path, &restricted, operation, arguments);
    let (ok, own) = mcp("schedule.list", json!({"member_id":second}));
    assert!(!ok, "{own}");
    assert_eq!(own["plans"].as_array().unwrap().len(), 1);
    assert_eq!(own["plans"][0]["id"], second_plan["id"]);
    // 成员限定读取不得泄露第一位成员的计划；跨成员直接查询被类型化拒绝。
    let (_, history) = mcp(
        "schedule.history",
        json!({"member_id":second,"id":created["id"]}),
    );
    assert_eq!(history["executions"], json!([]));
    let (_, pending) = mcp("schedule.pending", json!({"member_id":second}));
    assert_eq!(pending["items"], json!([]));
    let (_, notifications) = mcp("schedule.notifications", json!({"member_id":second}));
    assert_eq!(notifications["items"], json!([]));
    let (_, denied_list) = mcp("schedule.list", json!({"member_id":"default"}));
    assert_eq!(denied_list["error"]["code"], "member_denied");
    let (_, denied_history) = mcp(
        "schedule.history",
        json!({"member_id":"default","id":created["id"]}),
    );
    assert_eq!(denied_history["error"]["code"], "member_denied");
    // 越权修改：scope不含schedule.update，返回类型化拒绝。
    let (_, denied_update) = mcp(
        "schedule.update",
        json!({
            "member_id":second,"id":created["id"],
            "expected_revision":3,"name":"越权修改"
        }),
    );
    assert_eq!(denied_update["error"]["code"], "scope_denied");
    // 即使授予schedule.create，缺少原业务scope也必须类型化拒绝。
    let (_, denied_create) = mcp(
        "schedule.create",
        plan_input(&second, &device, "未授权新建", "08:00"),
    );
    assert_eq!(denied_create["error"]["code"], "scope_denied");
    // CLI侧同样只经标准输入携带令牌的受限通道读取，结果一致。
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args([
            "--db",
            path.to_str().unwrap(),
            "schedule.list",
            "--client-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"token":restricted,"input":{"member_id":second}})
    )
    .unwrap();
    let via_stdin: Value =
        serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
    assert_eq!(via_stdin["result"]["plans"][0]["id"], second_plan["id"]);
    // 拒绝之后第一位成员计划未变，第二成员也没有留下任何写入痕迹。
    let after = cli(&path, "schedule.list", json!({"member_id":"default"}))["result"].clone();
    let kept = after["plans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == created["id"])
        .unwrap();
    assert_eq!(kept["revision"], 3);
    assert_eq!(kept["enabled"], false);
    assert_eq!(kept["name"], "已改名计划");
    assert_eq!(kept["local_time"], "09:05");
    let second_after = cli(&path, "schedule.list", json!({"member_id":second}))["result"].clone();
    assert_eq!(second_after["plans"].as_array().unwrap().len(), 1);
    assert_eq!(second_after["plans"][0]["id"], second_plan["id"]);
}

#[test]
fn scheduler_rejects_profile_connection_instead_of_silently_ignoring_it() {
    // 调度是本机维护通道：--profile 连接选择不受支持，必须显式拒绝，
    // 不能静默忽略后打开默认数据库（未被授权的连接路径）。
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .env("WARDROBE_DATA_DIR", dir.path())
        .args(["--profile", "不存在的配置", "--trusted-local", "scheduler", "--once"])
        .output()
        .unwrap();
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["error"]["code"], "invalid_arguments");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        !dir.path().join("wardrobe.sqlite").exists(),
        "拒绝后不得创建或打开任何数据库"
    );
}
