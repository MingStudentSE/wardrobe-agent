use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

// ticket-23 场景与旅行计划：CLI 子进程契约测试。
// scenario.* 经共享 operations 注册源自动暴露到 CLI/MCP；本测试只覆盖
// 外部行为：发现（help/schema/capabilities）、真实库存计划、去重打包清单、
// 固定/排除/无解原因、乐观锁修改，以及 MCP tools/list 含新操作。

fn cli(path: &std::path::Path, operation: &str, input: Value) -> (Option<i32>, Value) {
    cli_raw(path, &[operation, &input.to_string()])
}

fn cli_raw(path: &std::path::Path, args: &[&str]) -> (Option<i32>, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--trusted-local", "--db", path.to_str().unwrap()])
        .args(args)
        .env("HOME", path.parent().unwrap().join("home"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (output.status.code(), envelope)
}

fn cli_ok(path: &std::path::Path, operation: &str, input: Value) -> Value {
    let (code, envelope) = cli(path, operation, input);
    assert_eq!(code, Some(0), "{operation} 应成功：{envelope}");
    assert!(envelope.get("error").is_none(), "{operation}: {envelope}");
    envelope["result"].clone()
}

#[test]
fn scenario_operations_are_discoverable_and_plan_travel_through_cli() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let member = cli_ok(&path, "space.default", json!({}))["member_id"]
        .as_str()
        .unwrap()
        .to_string();

    // 发现：help/schema 暴露新操作，capabilities 列出全部 scenario.*
    let (bare_code, bare) = cli_raw(&path, &["help"]);
    assert_eq!(bare_code, Some(0), "{bare}");
    assert!(bare.get("error").is_none(), "bare help 必须成功：{bare}");
    let (help_code, plan_help) = cli_raw(&path, &["help", "scenario.plan"]);
    assert_eq!(help_code, Some(0), "{plan_help}");
    let plan_help = plan_help["result"].clone();
    assert!(
        plan_help["description"]
            .as_str()
            .unwrap_or("")
            .contains("不记录实际穿着"),
        "scenario.plan help 须声明不产生穿着：{plan_help}"
    );
    let (schema_code, schema) = cli_raw(&path, &["schema", "scenario.plan"]);
    assert_eq!(schema_code, Some(0), "{schema}");
    let schema = schema["result"].clone();
    assert_eq!(
        schema["inputSchema"], plan_help["inputSchema"],
        "schema 与 help 同源"
    );
    let capabilities = cli_ok(&path, "capabilities", json!({}));
    let names: Vec<&str> = capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|op| op["name"].as_str())
        .collect();
    for expected in [
        "scenario.plan",
        "scenario.get",
        "scenario.list",
        "scenario.update",
        "scenario.delete",
        "scenario.worker.run",
    ] {
        assert!(names.contains(&expected), "capabilities 须包含 {expected}");
    }

    // 真实库存：三日旅行，低温手动天气 → 逐天方案 + 去重打包清单
    let created: Vec<Value> = [
        ("速干上衣A", "top"),
        ("速干上衣B", "top"),
        ("徒步长裤A", "bottom"),
        ("徒步长裤B", "bottom"),
        ("徒步鞋", "shoes"),
        ("轻薄羽绒", "outerwear"),
    ]
    .iter()
    .map(|(name, category)| {
        cli_ok(
            &path,
            "clothing.create",
            json!({"member_id":member,"name":name,"category":category}),
        )
    })
    .collect();
    let top_a = created[0]["id"].as_str().unwrap();
    let excluded_top = created[1]["id"].as_str().unwrap();

    let plan = cli_ok(
        &path,
        "scenario.plan",
        json!({
            "member_id":member,
            "operation_key":"cli-travel-1",
            "timezone":"Asia/Shanghai",
            "kind":"travel",
            "title":"川西三日",
            "start_date":"2026-11-01",
            "days":3,
            "excluded_ids":[excluded_top],
            "weather":{"mode":"manual","temperature_c":2}
        }),
    );
    assert_eq!(plan["kind"], "travel");
    assert_eq!(plan["days_count"], 3);
    assert_eq!(plan["creates_wear"], false);
    assert_eq!(
        plan["days"].as_array().unwrap().len(),
        3,
        "CLI 计划逐天输出：{plan}"
    );
    for day in plan["days"].as_array().unwrap() {
        let ids: Vec<&str> = day["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["clothing_id"].as_str())
            .collect();
        assert!(!ids.contains(&excluded_top), "被排除单品不出现在任何一天");
        assert_eq!(day["complete"], json!(true), "低温完整方案含外层：{day}");
    }
    let packing = plan["packing"]["items"].as_array().unwrap();
    assert_eq!(
        plan["packing"]["total"],
        json!(packing.len() as u64),
        "打包清单已去重"
    );
    assert!(
        packing
            .iter()
            .any(|item| item["clothing_id"] == created[5]["id"]),
        "低温打包清单含外层"
    );

    // 查看与修改：排除原方案内单品 → 版本递增、重算生效
    let plan_id = plan["id"].as_str().unwrap().to_string();
    let listed = cli_ok(&path, "scenario.list", json!({"member_id":member}));
    assert!(listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == json!(plan_id)));
    let got = cli_ok(
        &path,
        "scenario.get",
        json!({"member_id":member,"id":plan_id}),
    );
    assert_eq!(got["version"], json!(1));
    let updated = cli_ok(
        &path,
        "scenario.update",
        json!({"member_id":member,"id":plan_id,"expected_version":1,"excluded_ids":[top_a]}),
    );
    assert_eq!(updated["version"], json!(2));
    for day in updated["days"].as_array().unwrap() {
        let ids: Vec<&str> = day["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["clothing_id"].as_str())
            .collect();
        assert!(!ids.contains(&top_a), "修改后原固定单品不再出现");
    }
    let (_stale_code, stale) = cli(
        &path,
        "scenario.update",
        json!({"member_id":member,"id":plan_id,"expected_version":1}),
    );
    assert_eq!(stale["error"]["code"], "version_conflict", "{stale}");

    // 计划不产生穿着
    let wears = cli_ok(&path, "wear.list", json!({"member_id":member}));
    assert_eq!(wears["items"], json!([]), "场景计划不记录实际穿着");

    // 未知计划与不支持字段：类型化错误
    let (_missing_code, missing) = cli(
        &path,
        "scenario.get",
        json!({"member_id":member,"id":"none"}),
    );
    assert_eq!(missing["error"]["code"], "not_found", "{missing}");
    let (_bogus_code, bogus) = cli(
        &path,
        "scenario.plan",
        json!({"member_id":member,"operation_key":"x","timezone":"UTC","kind":"event","nope":1}),
    );
    assert_eq!(bogus["error"]["code"], "invalid_input", "{bogus}");
}

#[test]
fn cli_reports_shortage_and_constraint_conflicts_with_typed_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let member = cli_ok(&path, "space.default", json!({}))["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let dress = cli_ok(
        &path,
        "clothing.create",
        json!({"member_id":member,"name":"婚礼连衣裙","category":"dress"}),
    );
    // 无鞋：婚礼方案给出短缺原因，不虚构
    let plan = cli_ok(
        &path,
        "scenario.plan",
        json!({
            "member_id":member,
            "operation_key":"cli-wedding-short",
            "timezone":"UTC",
            "kind":"event",
            "occasion":"wedding",
            "fixed_ids":[dress["id"]]
        }),
    );
    let reasons = plan["shortage_reasons"].as_array().unwrap();
    assert!(
        reasons
            .iter()
            .any(|reason| reason.as_str().unwrap_or("").contains("缺少")),
        "缺鞋须给缺少角色原因：{reasons:?}"
    );
    // 固定+排除冲突
    let (_conflict_code, conflict) = cli(
        &path,
        "scenario.plan",
        json!({
            "member_id":member,"operation_key":"cli-conflict","timezone":"UTC","kind":"event",
            "occasion":"interview","fixed_ids":[dress["id"]],"excluded_ids":[dress["id"]]
        }),
    );
    assert_eq!(
        conflict["error"]["code"], "constraint_conflict",
        "{conflict}"
    );
    // 待洗固定单品被可用性校验拒绝
    cli_ok(
        &path,
        "care.mark_dirty",
        json!({"member_id":member,"operation_key":"cli-dirty","clothing_id":dress["id"],"local_date":"2026-10-07","timezone":"UTC","dirty":true}),
    );
    let (_dirty_code, dirty) = cli(
        &path,
        "scenario.plan",
        json!({
            "member_id":member,"operation_key":"cli-dirty-plan","timezone":"UTC","kind":"event",
            "occasion":"wedding","fixed_ids":[dress["id"]]
        }),
    );
    assert_eq!(dirty["error"]["code"], "constraint_conflict", "{dirty}");
}

#[test]
fn mcp_tools_list_exposes_scenario_operations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    cli_ok(&path, "space.default", json!({}));
    let token = {
        let app = wardrobe_core::Application::open(&path).unwrap();
        app.execute(
            "client.authorize",
            json!({"name":"MCP 发现客户端","mode":"full","operations":[],"member_ids":null,"expires_at":null}),
        )
        .unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", path.to_str().unwrap(), "mcp"])
        .env("HOME", dir.path().join("home"))
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
    let stdout = String::from_utf8(output.stdout).unwrap();
    let listing = stdout
        .lines()
        .find_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            if value["id"] == json!(2) {
                Some(value)
            } else {
                None
            }
        })
        .expect("tools/list 响应");
    let names: Vec<String> = listing["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect();
    assert!(
        names.iter().any(|name| name == "scenario.plan"),
        "MCP tools/list 须包含 scenario.plan：{names:?}"
    );
    assert!(
        names.iter().any(|name| name == "scenario.worker.run"),
        "MCP tools/list 须包含 scenario.worker.run"
    );
}
