use serde_json::{json, Value};
use std::path::Path;
use wardrobe_core::{operations, Application};

fn assert_result_matches_schema(schema: &Value, result: &Value, context: &str) {
    assert_eq!(
        schema["type"].as_str(),
        Some("object"),
        "{context}: outputSchema 须为 object"
    );
    let props = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{context}: outputSchema 须声明 properties"));
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|r| r.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    let actual = result
        .as_object()
        .unwrap_or_else(|| panic!("{context} 结果须为对象"));
    for (key, value) in actual {
        let prop = props
            .get(key)
            .unwrap_or_else(|| panic!("{context}: 实际结果字段 {key} 未在 outputSchema 声明"));
        if let Some(const_value) = prop.get("const") {
            assert_eq!(value, const_value, "{context}: 字段 {key} 须等于 const");
        }
        if let Some(allowed) = prop["enum"].as_array() {
            assert!(
                allowed.contains(value),
                "{context}: 字段 {key} 须为枚举值"
            );
        }
        let declared_types: Vec<String> = match prop.get("type") {
            Some(Value::String(t)) => vec![t.clone()],
            Some(Value::Array(ts)) => ts.iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            _ => vec![],
        };
        let actual_type = match value {
            Value::String(_) => "string",
            Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Bool(_) => "boolean",
            Value::Null => "null",
        };
        assert!(
            declared_types.is_empty() || declared_types.iter().any(|t| {
                t == actual_type
                    || (t == "number" && value.is_number())
                    || (t == "integer" && value.as_f64().is_some_and(|n| n.fract() == 0.0))
            }),
            "{context}: 字段 {key} 声明类型不符"
        );
    }
    for key in required {
        assert!(actual.contains_key(key), "{context}: 缺少 required 字段 {key}");

    }
}

#[test]
fn output_contract_checker_handles_constants_nullable_types_and_required_fields() {
    let schema = json!({"type":"object","required":["count","flag"],"properties":{
        "count":{"type":"number"},"flag":{"const":false},
        "optional":{"type":["string","null"],"enum":["ready",null]}
    }});
    assert_result_matches_schema(&schema, &json!({"count":1,"flag":false}), "optional absent");
    assert_result_matches_schema(&schema, &json!({"count":1.5,"flag":false,"optional":null}), "nullable");
    for invalid in [json!({"count":1}), json!({"count":null,"flag":false}),
        json!({"count":1,"flag":true}), json!({"count":1,"flag":false,"optional":"bad"})] {
        assert!(std::panic::catch_unwind(|| assert_result_matches_schema(&schema, &invalid, "invalid")).is_err());
    }
    let integer = json!({"type":"object","required":["n"],"properties":{"n":{"type":"integer"}}});
    assert_result_matches_schema(&integer, &json!({"n":1.0}), "integral number");
    assert!(std::panic::catch_unwind(|| assert_result_matches_schema(&integer, &json!({"n":1.5}), "fraction")).is_err());
    let enumeration = json!({"type":"object","properties":{"v":{"enum":["ready"]}}});
    assert!(std::panic::catch_unwind(|| assert_result_matches_schema(&enumeration, &json!({"v":null}), "enum null")).is_err());
}

#[test]
fn category_descriptors_declare_the_actual_public_output_schemas() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("registry.sqlite");
    let app = Application::open(&db).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let created = app
        .execute("category.create", json!({"member_id":member,"name":"户外"}))
        .unwrap();
    let custom = created["id"].as_str().unwrap().to_string();
    app.execute(
        "clothing.create",
        // 自定义分类以 category.create 返回的 ID 引用
        json!({"member_id":member,"name":"冲锋衣","category":custom}),
    )
    .unwrap();
    let listed = app
        .execute("category.list", json!({"member_id":member}))
        .unwrap();
    let deleted = app
        .execute("category.delete", json!({"member_id":member,"id":custom}))
        .unwrap();

    // 实际公开结果（Application 是 CLI/MCP 共享的结果接缝）
    // root 裁定：公开 Core 契约要求输出带 schema_version=1（application 契约测试）
    assert_eq!(
        created,
        json!({"schema_version":1,"id":custom,"name":"户外","count":0}),
        "category.create 公开结果"
    );
    let items = listed["items"].as_array().unwrap();
    let entry = items.iter().find(|i| i["id"] == json!(custom)).unwrap();
    assert_eq!(
        entry,
        &json!({"schema_version":1,"id":custom,"name":"户外","count":1})
    );
    assert!(
        items.iter().all(|i| i["schema_version"] == json!(1)),
        "列表条目同样版本化"
    );
    assert_eq!(
        deleted,
        json!({"schema_version":1,"deleted":custom,"moved":1}),
        "category.delete 公开结果"
    );

    // 描述符必须声明与公开结果一致且完整的 outputSchema；
    // 公开结果不含 schema_version，描述符不得虚构。
    let descriptors = operations().as_array().unwrap().clone();
    let schema_of = |name: &str| -> Value {
        let descriptor = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        Value::Object(
            descriptor["outputSchema"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} 必须声明 outputSchema"))
                .clone(),
        )
    };
    assert_result_matches_schema(&schema_of("category.create"), &created, "category.create");
    let list_schema = schema_of("category.list");
    assert_result_matches_schema(&list_schema, &listed, "category.list");
    let item_schema = Value::Object(
        list_schema["properties"]["items"]["items"]
            .as_object()
            .unwrap_or_else(|| panic!("category.list items 须声明条目 schema"))
            .clone(),
    );
    for item in items {
        assert_result_matches_schema(&item_schema, item, "category.list 条目");
    }
    assert_result_matches_schema(&schema_of("category.delete"), &deleted, "category.delete");
}

#[test]
fn scope_metadata_reflects_actual_authorization_rules_for_every_descriptor() {
    let descriptors = operations().as_array().unwrap().clone();
    assert!(!descriptors.is_empty());
    let find = |name: &str| -> Value {
        descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"))
            .clone()
    };
    let mut trusted_local_only = vec![];
    for descriptor in &descriptors {
        let name = descriptor["name"].as_str().unwrap().to_string();
        let scope = descriptor
            .get("scope")
            .unwrap_or_else(|| panic!("{name} 缺少 scope 元数据"));
        assert!(
            matches!(
                scope["access"].as_str(),
                Some("trusted-local-only") | Some("external")
            ),
            "{name}: access 取值非法"
        );
        assert_eq!(
            scope["primary"], descriptor["name"],
            "{name}: primary 授权名须与操作同名（精确名授予）"
        );
        if scope["access"] == "trusted-local-only" {
            // 仅本机可信入口：外部一律拒绝，primary 授权判定不适用
            assert_eq!(
                scope["primary_grant_required"],
                json!(false),
                "{name}: 可信本地操作不得声称需要外部授权"
            );
        } else if name == "capabilities" {
            assert_eq!(
                scope["primary_grant_required"],
                json!(false),
                "capabilities 对有限授权免 primary 检查"
            );
        } else {
            assert_eq!(
                scope["primary_grant_required"],
                json!(true),
                "{name}: 外部有限授权须按精确名检查"
            );
        }
        assert!(
            scope["member"].is_object() && scope["member"]["grant"].is_string(),
            "{name}: 须声明成员限制语义"
        );
        if scope["access"] == "trusted-local-only" {
            trusted_local_only.push(name);
        }
    }
    // trusted-local-only 集合必须与特权规则精确一致，且不得为空
    assert!(
        !trusted_local_only.is_empty(),
        "必须标出仅本机可信入口可用的操作"
    );
    for name in &trusted_local_only {
        assert!(
            name.starts_with("client.")
                || name.starts_with("provider.credential.")
                || name.starts_with("encryption.")
                // ticket25 合并引入：精确名特权操作（无通用前缀）
                || name == "schedule.runtime.configure",
            "{name} 不应标为仅本机可信"
        );
    }
    let privileged_registered: Vec<String> = descriptors
        .iter()
        .map(|d| d["name"].as_str().unwrap().to_string())
        .filter(|n| {
            n.starts_with("client.")
                || n.starts_with("provider.credential.")
                || n.starts_with("encryption.")
                || n == "schedule.runtime.configure"
        })
        .collect();
    assert_eq!(
        trusted_local_only, privileged_registered,
        "trusted-local-only 集合须与特权操作一一对应"
    );

    // 组合 scope：feedback.record 的 reaction 条件分支
    let feedback_descriptor = find("feedback.record");
    let additional = feedback_descriptor["scope"]["additional"]
        .as_array()
        .expect("feedback.record 须声明组合 scope");
    assert!(additional.iter().any(|condition| {
        condition["requires"] == json!(["wear.record"])
            && condition["when"]["reaction"] == json!(["actual_wore", "changed"])
    }));
    let try_another_descriptor = find("feedback.try_another");
    let additional = try_another_descriptor["scope"]["additional"]
        .as_array()
        .expect("feedback.try_another 须声明组合 scope");
    assert!(additional
        .iter()
        .any(|condition| condition["requires"]
            == json!(["feedback.record", "recommendation.change"])));

    // 组合 scope：维护控制依赖原任务 operation（参数依赖）
    for name in [
        "maintenance.cancel",
        "maintenance.retry",
        "maintenance.worker.run",
    ] {
        let descriptor = find(name);
        let additional = descriptor["scope"]["additional"]
            .as_array()
            .unwrap_or_else(|| panic!("{name} 须声明任务依赖 scope"));
        assert!(
            additional
                .iter()
                .any(
                    |condition| condition["requires_original_job_scope"] == json!(true)
                        && condition["job_field"] == json!("id")
                ),
            "{name} 须声明原业务操作 scope 依赖"
        );
    }
    assert!(
        find("maintenance.list")["scope"]["additional"].is_null(),
        "maintenance.list 无组合 scope"
    );

    // 组合 scope：学习重算任务按 job kind 条件
    for name in ["learning.worker.run", "jobs.retry", "jobs.cancel"] {
        let descriptor = find(name);
        let additional = descriptor["scope"]["additional"]
            .as_array()
            .unwrap_or_else(|| panic!("{name} 须声明任务类型条件 scope"));
        assert!(additional.iter().any(|condition| {
            condition["when_job_kind"] == json!("learning_recompute")
                && condition["requires"] == json!(["learning.recompute"])
        }));
    }

    // 成员限制语义
    for name in ["capabilities", "provider.list", "provider.validate"] {
        assert_eq!(
            find(name)["scope"]["member"]["grant"],
            json!("space-level"),
            "{name} 为空间级操作"
        );
    }
    assert_eq!(
        find("member.list")["scope"]["member"]["grant"],
        json!("filtered"),
        "member.list 结果按授权成员过滤"
    );
    assert!(
        descriptors
            .iter()
            .any(|d| d["scope"]["member"]["grant"] == json!("all-members")
                && d["name"].as_str().unwrap().starts_with("backup.")),
        "全空间维护类操作须要求全部成员授权"
    );
    assert_eq!(
        find("clothing.list")["scope"]["member"]["grant"],
        json!("per-member_id"),
        "成员业务操作按 input.member_id 校验"
    );
}

#[test]
fn capabilities_is_grant_exempt_and_filtered_for_limited_clients() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("cap.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let grant = app
        .execute(
            "client.authorize",
            json!({"name":"仅衣物列表","mode":"limited","operations":["clothing.list"],"member_ids":null,"expires_at":null}),
        )
        .unwrap();
    let token = grant["token"].as_str().unwrap();
    // 与授权无关的 capabilities 调用成功（primary 豁免）
    let capabilities = app
        .execute_as_client(token, "capabilities", json!({}))
        .unwrap();
    let names: Vec<&str> = capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"capabilities"));
    assert!(names.contains(&"clothing.list"), "授权内操作必须出现");
    assert!(
        !names.contains(&"clothing.create"),
        "授权外业务操作不得出现"
    );
    assert!(
        !names.contains(&"client.authorize"),
        "仅本机可信入口的操作不得出现"
    );
    // 描述符 scope 与公开发现一致
    let listed = capabilities["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == json!("clothing.list"))
        .unwrap();
    assert_eq!(listed["scope"]["primary_grant_required"], json!(true));
}

#[test]
fn category_effects_describe_actual_dispatcher_behavior() {
    let descriptors = operations().as_array().unwrap().clone();
    for (name, kind) in [
        ("category.list", "query"),
        ("category.create", "mutation"),
        ("category.delete", "mutation"),
    ] {
        let descriptor = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        let effect = descriptor
            .get("effect")
            .unwrap_or_else(|| panic!("{name} 缺少 effect 元数据"));
        assert_eq!(effect["kind"], json!(kind), "{name} 实际调度行为");
        assert_eq!(effect["network"], json!("none"), "{name} 不联网");
        assert_eq!(effect["local_files"], json!(false), "{name} 不读本地文件");
        assert!(effect["async"].is_null(), "{name} 非异步任务");
    }
}

#[test]
fn member_grant_semantics_match_enforcement_and_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("member.sqlite")).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap().to_string();
    // 有效输入拒绝注入 member_id（root 公开探针一致）
    let injected = app.execute("space.default", json!({"member_id":member}));
    assert_eq!(injected.unwrap_err().code, "invalid_input");

    // 受限成员的 full 授权：space.default 无法执行（有效输入无 member_id）
    let grant = app
        .execute(
            "client.authorize",
            json!({"name":"单成员完整","mode":"full","operations":[],"member_ids":[member],"expires_at":null}),
        )
        .unwrap();
    let token = grant["token"].as_str().unwrap();
    assert_eq!(
        app.execute_as_client(token, "space.default", json!({}))
            .unwrap_err()
            .code,
        "member_denied",
        "无 member_id 输入的操作对受限成员授权必须 member_denied"
    );
    let descriptors = operations().as_array().unwrap().clone();
    let scope_of = |name: &str| {
        descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap()
            .clone()
    };
    assert_eq!(
        scope_of("space.default")["scope"]["member"]["grant"],
        json!("all-members"),
        "space.default 元数据须与执行一致"
    );
    // per-member_id 操作：授权内成员可执行
    let created = app
        .execute_as_client(
            token,
            "clothing.create",
            json!({"member_id":member,"name":"受限可写","category":"top"}),
        )
        .unwrap();
    assert!(created.get("error").is_none(), "{created}");
    assert_eq!(
        scope_of("clothing.create")["scope"]["member"]["grant"],
        json!("per-member_id")
    );
    // 授权范围外成员被拒
    let other = app
        .execute("member.create", json!({"name":"其他成员"}))
        .unwrap();
    let other_member = other["member_id"].as_str().unwrap().to_string();
    assert_eq!(
        app.execute_as_client(
            token,
            "clothing.create",
            json!({"member_id":other_member,"name":"越权","category":"top"})
        )
        .unwrap_err()
        .code,
        "member_denied"
    );
}

#[test]
fn member_family_effects_describe_actual_dispatcher_behavior() {
    let descriptors = operations().as_array().unwrap().clone();
    let effects = [
        ("space.default", "query", true),
        ("member.list", "query", false),
        ("member.create", "mutation", false),
        ("member.update", "mutation", false),
        ("member.select", "mutation", false),
    ];
    for (name, kind, needs_member) in effects {
        let descriptor = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        let effect = descriptor
            .get("effect")
            .unwrap_or_else(|| panic!("{name} 缺少 effect 元数据"));
        assert_eq!(effect["kind"], json!(kind), "{name}");
        assert_eq!(effect["network"], json!("none"), "{name} 不联网");
        match (name, needs_member) {
            ("space.default", _) => {
                // 初始化属于 Application::open 迁移，不是 space.default 调度效果
                assert!(
                    effect.get("initializes").is_none(),
                    "space.default 调度不得声称负责初始化: {effect}"
                );
                assert_eq!(descriptor["scope"]["member"]["grant"], json!("all-members"));
            }
            (_, true) => assert_eq!(
                descriptor["scope"]["member"]["grant"],
                json!("all-members"),
                "{name} 有效输入不含 member_id"
            ),
            ("member.list", _) => assert_eq!(
                descriptor["scope"]["member"]["grant"],
                json!("filtered"),
                "member.list 按授权成员过滤"
            ),
            _ => {}
        }
    }
}

fn assert_request_matches_input_schema(schema: &Value, request: &Value, context: &str) {
    assert!(request.is_object(), "{context}: 请求须为对象");
    let properties = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{context}: inputSchema 须声明 properties"));
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|values| values.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    for key in &required {
        assert!(request.get(key).is_some(), "{context}: 缺少必填字段 {key}");
    }
    for (key, value) in request.as_object().unwrap() {
        let property = properties
            .get(key)
            .unwrap_or_else(|| panic!("{context}: 示例含未声明字段 {key}"));
        match property["type"].as_str() {
            Some("string") => assert!(value.is_string(), "{context}: {key} 须为字符串"),
            Some("integer") => {
                assert!(
                    value.is_i64() || value.is_u64(),
                    "{context}: {key} 须为整数"
                )
            }
            Some("boolean") => assert!(value.is_boolean(), "{context}: {key} 须为布尔"),
            Some("array") => assert!(value.is_array(), "{context}: {key} 须为数组"),
            _ => {}
        }
        if let Some(allowed) = property["enum"].as_array() {
            assert!(allowed.contains(value), "{context}: {key} 须为声明的枚举值");
        }
    }
}

#[test]
fn initialization_is_an_open_precondition_not_a_space_default_dispatch_effect() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("fresh.sqlite")).unwrap();
    // 公开边界：open 迁移已完成初始化，无需任何 space.default 调度
    let space = app.execute("space.default", json!({})).unwrap();
    assert!(space["member_id"].is_string(), "{space}");
    let second = app.execute("space.default", json!({})).unwrap();
    assert_eq!(space, second, "再次调用为纯查询");
    let descriptors = operations().as_array().unwrap().clone();
    let descriptor = descriptors
        .iter()
        .find(|d| d["name"] == json!("space.default"))
        .unwrap();
    assert!(
        descriptor["effect"].get("initializes").is_none(),
        "元数据不得把 open 迁移归因于 space.default 调度"
    );
}

#[test]
fn category_and_member_examples_are_schema_valid_with_symbolic_prerequisites() {
    let descriptors = operations().as_array().unwrap().clone();
    let family = [
        "space.default",
        "member.list",
        "member.create",
        "member.update",
        "member.select",
        "category.list",
        "category.create",
        "category.delete",
    ];
    for name in family {
        let descriptor = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        let examples = descriptor
            .get("examples")
            .and_then(|e| e.as_array())
            .unwrap_or_else(|| panic!("{name} 缺少 examples"));
        assert!(!examples.is_empty(), "{name} 至少一个示例");
        for example in examples {
            let request = example
                .get("request")
                .unwrap_or_else(|| panic!("{name} 示例须含 request"));
            assert_request_matches_input_schema(&descriptor["inputSchema"], request, name);
            let prerequisites = example
                .get("prerequisites")
                .and_then(|p| p.as_array())
                .unwrap_or_else(|| panic!("{name} 示例须含 prerequisites"));
            assert!(
                !prerequisites.is_empty() && prerequisites.iter().all(|p| p.is_string()),
                "{name}: prerequisites 须为符号化字符串指令"
            );
        }
    }
    // 符号化占位：示例值不得像真实业务数据
    let create = descriptors
        .iter()
        .find(|d| d["name"] == json!("category.create"))
        .unwrap();
    let request = &create["examples"][0]["request"];
    assert!(
        request["name"].as_str().unwrap().contains('<'),
        "示例须使用 <符号占位> 而非真实名称: {request}"
    );
}

#[test]
fn provider_family_effects_and_examples_match_actual_authorization_and_dispatcher() {
    let descriptors = operations().as_array().unwrap().clone();
    let find = |name: &str| -> Value {
        descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"))
            .clone()
    };
    let effects = [
        ("provider.list", "query", "none"),
        ("provider.save", "mutation", "none"),
        // validate 实际发起 chat/completions 网络请求（视觉仅发送内置测试图）并持久化验证结果
        ("provider.validate", "mutation", "explicit-provider"),
        ("provider.credential.set", "mutation", "none"),
        ("provider.credential.remove", "mutation", "none"),
        ("provider.credential.status", "query", "none"),
    ];
    for (name, kind, network) in effects {
        let descriptor = find(name);
        let effect = descriptor
            .get("effect")
            .unwrap_or_else(|| panic!("{name} 缺少 effect 元数据"));
        assert_eq!(effect["kind"], json!(kind), "{name}");
        assert_eq!(effect["network"], json!(network), "{name} 实际网络行为");
        assert!(effect["async"].is_null(), "{name} 非异步任务");
        // scope：credential 三操作仅本机可信；validate/save/list 外部可用
        let access = descriptor["scope"]["access"].as_str().unwrap();
        let expected_access = if name.starts_with("provider.credential.") {
            "trusted-local-only"
        } else {
            "external"
        };
        assert_eq!(access, expected_access, "{name} 授权通道");
        // examples
        let examples = descriptor
            .get("examples")
            .and_then(|e| e.as_array())
            .unwrap_or_else(|| panic!("{name} 缺少 examples"));
        assert!(!examples.is_empty(), "{name} 至少一个示例");
        for example in examples {
            let request = example
                .get("request")
                .unwrap_or_else(|| panic!("{name} 示例须含 request"));
            assert_request_matches_input_schema(&descriptor["inputSchema"], request, name);
            assert!(
                example
                    .get("prerequisites")
                    .and_then(|p| p.as_array())
                    .is_some_and(|p| !p.is_empty() && p.iter().all(|v| v.is_string())),
                "{name} 示例须含符号化前置说明"
            );
        }
    }
    // validate 示例必须声明付费/网络语义，且不自动执行
    let validate = find("provider.validate");
    let text = validate["examples"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|e| e["prerequisites"].as_array().unwrap().clone())
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<String>>()
        .join(" ");
    assert!(
        text.contains("额度") && text.contains("不自动执行"),
        "validate 示例须显式声明额度消耗且不自动执行: {text}"
    );
    // credential 示例必须指示 stdin 通道
    let set = find("provider.credential.set");
    let text = set["examples"][0]["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<String>();
    assert!(text.contains("stdin"), "凭据示例须指示 stdin: {text}");
}

fn assert_family_effects_and_examples(descriptors: &[Value], family: &[(&str, &str, bool)]) {
    for (name, kind, local_files) in family {
        let descriptor = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        let effect = descriptor
            .get("effect")
            .unwrap_or_else(|| panic!("{name} 缺少 effect 元数据"));
        assert_eq!(effect["kind"], json!(kind), "{name} 实际调度行为");
        assert_eq!(effect["network"], json!("none"), "{name} 不联网");
        assert_eq!(
            effect["local_files"],
            json!(local_files),
            "{name} 本地文件访问"
        );
        assert!(effect["async"].is_null(), "{name} 非异步任务");
        let examples = descriptor
            .get("examples")
            .and_then(|e| e.as_array())
            .unwrap_or_else(|| panic!("{name} 缺少 examples"));
        assert!(!examples.is_empty(), "{name} 至少一个示例");
        for example in examples {
            let request = example
                .get("request")
                .unwrap_or_else(|| panic!("{name} 示例须含 request"));
            assert_request_matches_input_schema(&descriptor["inputSchema"], request, name);
            assert!(
                example
                    .get("prerequisites")
                    .and_then(|p| p.as_array())
                    .is_some_and(|p| !p.is_empty() && p.iter().all(|v| v.is_string())),
                "{name} 示例须含符号化前置说明"
            );
        }
    }
}

#[test]
fn clothing_effects_and_examples_match_actual_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("l.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    // 真实生命周期：软删除可恢复；历史保留不可变快照
    let created = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"羊毛外套","category":"outerwear"}),
        )
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    app.execute("clothing.archive", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap()["status"],
        json!("archived")
    );
    app.execute("clothing.activate", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap()["status"],
        json!("active")
    );
    app.execute("clothing.delete", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap()["status"],
        json!("deleted")
    );
    app.execute("clothing.restore", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap()["status"],
        json!("active")
    );
    app.execute(
        "clothing.update",
        json!({"member_id":member,"id":id,"favorite":true}),
    )
    .unwrap();
    let history = app
        .execute("clothing.history", json!({"member_id":member,"id":id}))
        .unwrap();
    let entries = history["items"].as_array().unwrap();
    assert!(entries.len() >= 2, "更新后历史含快照: {history}");
    assert_eq!(entries[0]["operation"], json!("clothing.create"));
    assert_eq!(
        entries[0]["snapshot"]["favorite"],
        json!(false),
        "历史首条快照不可变"
    );
    assert_eq!(entries.last().unwrap()["snapshot"]["favorite"], json!(true));

    let descriptors = operations().as_array().unwrap().clone();
    let family = [
        ("clothing.create", "mutation", false),
        ("clothing.get", "query", false),
        ("clothing.list", "query", false),
        ("clothing.history", "query", false),
        ("clothing.archive", "mutation", false),
        ("clothing.activate", "mutation", false),
        ("clothing.delete", "mutation", false),
        ("clothing.restore", "mutation", false),
        ("clothing.update", "mutation", false),
    ];
    assert_family_effects_and_examples(&descriptors, &family);
}

#[test]
fn assets_effects_and_examples_match_actual_local_file_semantics() {
    let descriptors = operations().as_array().unwrap().clone();
    let family = [
        ("assets.import", "mutation", true),
        ("assets.get", "query", false),
        ("assets.preview", "query", true),
        ("assets.rebuild_thumbnail", "mutation", true),
        ("assets.adopt", "mutation", false),
        ("assets.reorder", "mutation", false),
        ("assets.detach", "mutation", false),
        ("assets.sync_manifest", "query", false),
    ];
    assert_family_effects_and_examples(&descriptors, &family);
    // sync_manifest 仅 SELECT 重算清单，不落库（root 独立审查）
    let manifest = descriptors
        .iter()
        .find(|d| d["name"] == json!("assets.sync_manifest"))
        .unwrap();
    assert_eq!(manifest["effect"]["kind"], json!("query"));
    // preview 条件性变更：缩略图缺失时同步重建（本地 blob 写 + 文档更新）
    let preview = descriptors
        .iter()
        .find(|d| d["name"] == json!("assets.preview"))
        .unwrap();
    let conditional = preview["effect"]["conditional_effects"]
        .as_array()
        .expect("preview 须声明条件性 effect");
    assert!(conditional.iter().any(|c| {
        c["when"]["variant"] == json!("thumbnail")
            && c["when"]["thumbnail_file_missing"] == json!(true)
            && c["effect"]["kind"] == json!("mutation")
            && c["effect"]["local_files"] == json!(true)
    }));
    let text = preview["examples"][0]["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<String>();
    assert!(
        text.contains("重建"),
        "preview 示例须说明缩略图缺失时同步重建: {text}"
    );
    // 语义前置：导入读本地文件写受管字节；采纳仅压缩展示图参与同步，原图保持本地
    let import = descriptors
        .iter()
        .find(|d| d["name"] == json!("assets.import"))
        .unwrap();
    let text = import["examples"][0]["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<String>();
    assert!(text.contains("本地") && text.contains("受管"), "{text}");
    let adopt = descriptors
        .iter()
        .find(|d| d["name"] == json!("assets.adopt"))
        .unwrap();
    let text = adopt["examples"][0]["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<String>();
    assert!(
        text.contains("原图保持本地") && text.contains("压缩"),
        "{text}"
    );
}

#[test]
fn outfits_effects_and_examples_match_actual_versioning() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("o.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"外套","category":"outerwear"}),
        )
        .unwrap();
    let items = json!([{"clothing_id":clothing["id"],"role":"outer","layer":2}]);
    let created = app
        .execute(
            "outfit.create",
            json!({"member_id":member,"name":"通勤","items":items}),
        )
        .unwrap();
    assert_eq!(created["version"], json!(1));
    let id = created["id"].as_str().unwrap().to_string();
    let updated = app
        .execute(
            "outfit.update",
            json!({"member_id":member,"id":id,"name":"通勤二","items":items,"expected_version":1}),
        )
        .unwrap();
    assert_eq!(
        updated["version"],
        json!(2),
        "编辑以 expected_version 乐观推进版本"
    );
    let v1 = app
        .execute(
            "outfit.version",
            json!({"member_id":member,"id":id,"version":1}),
        )
        .unwrap();
    assert_eq!(v1["name"], json!("通勤"), "版本快照不可变");
    app.execute("outfit.delete", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(
        app.execute("outfit.get", json!({"member_id":member,"id":id}))
            .unwrap()["status"],
        json!("deleted")
    );
    app.execute("outfit.restore", json!({"member_id":member,"id":id}))
        .unwrap();

    let descriptors = operations().as_array().unwrap().clone();
    let family = [
        ("outfit.create", "mutation", false),
        ("outfit.get", "query", false),
        ("outfit.list", "query", false),
        ("outfit.update", "mutation", false),
        ("outfit.version", "query", false),
        ("outfit.favorite", "mutation", false),
        ("outfit.clone", "mutation", false),
        ("outfit.delete", "mutation", false),
        ("outfit.restore", "mutation", false),
        ("outfit.render_local", "query", true),
    ];
    assert_family_effects_and_examples(&descriptors, &family);
}

#[test]
fn wears_effects_and_examples_match_actual_event_sourcing() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("w.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"衬衫","category":"top"}),
        )
        .unwrap();
    let record = json!({"member_id":member,"operation_key":"wear-x","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[clothing["id"]]});
    let event = app.execute("wear.record", record.clone()).unwrap();
    let again = app.execute("wear.record", record).unwrap();
    assert_eq!(event["id"], again["id"], "同 operation_key 幂等");
    let corrected = app
        .execute("wear.correct", json!({"member_id":member,"operation_key":"wear-c","root_event_id":event["id"],"expected_revision":1,"local_date":"2020-01-02","timezone":"UTC"}))
        .unwrap();
    assert_eq!(corrected["revision"], json!(2));

    let descriptors = operations().as_array().unwrap().clone();
    let family = [
        ("wear.record", "mutation", false),
        ("wear.correct", "mutation", false),
        ("wear.revoke", "mutation", false),
        ("wear.get", "query", false),
        ("wear.list", "query", false),
        ("wear.history", "query", false),
        ("wear.summary", "query", false),
    ];
    assert_family_effects_and_examples(&descriptors, &family);
}

#[test]
fn care_effects_and_examples_match_actual_completion_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("c.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"洗护衫","category":"top"}),
        )
        .unwrap();
    let cid = clothing["id"].as_str().unwrap().to_string();
    app.execute("care.configure", json!({"member_id":member,"clothing_id":cid,"wash_after_wears":1,"expected_revision":0,"operation_key":"cfg"})).unwrap();
    app.execute("wear.record", json!({"member_id":member,"operation_key":"w1","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[cid]})).unwrap();
    let status = app
        .execute("care.status", json!({"member_id":member,"clothing_id":cid}))
        .unwrap();
    assert_eq!(status["dirty"], json!(true), "达到阈值后待洗");
    let recorded = app
        .execute("care.record", json!({"member_id":member,"clothing_id":cid,"operation_key":"wash-1","local_date":"2020-01-02","timezone":"UTC","action":"wash"}))
        .unwrap();
    assert!(recorded.get("error").is_none(), "{recorded}");
    let status = app
        .execute("care.status", json!({"member_id":member,"clothing_id":cid}))
        .unwrap();
    assert_eq!(status["wears_since_clean"], json!(0), "洗护完成即重置计数");

    let descriptors = operations().as_array().unwrap().clone();
    let family = [
        ("care.configure", "mutation", false),
        ("care.mark_dirty", "mutation", false),
        ("care.record", "mutation", false),
        ("care.correct", "mutation", false),
        ("care.revoke", "mutation", false),
        ("care.status", "query", false),
        ("care.get", "query", false),
        ("care.list", "query", false),
        ("care.history", "query", false),
        ("care.dirty_list", "query", false),
        ("care.reminders", "query", false),
        ("care.availability", "query", false),
    ];
    assert_family_effects_and_examples(&descriptors, &family);
}

#[test]
fn insights_effects_and_examples_match_actual_statistics_reads() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("i.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let configured = app
        .execute(
            "insights.configure",
            json!({"member_id":member,"stale_days":91,"expected_revision":0}),
        )
        .unwrap();
    assert_eq!(configured["stale_days"], json!(91));
    let summary = app
        .execute(
            "insights.summary",
            json!({"member_id":member,"as_of_date":"2020-04-01","timezone":"Asia/Shanghai"}),
        )
        .unwrap();
    assert_eq!(summary["stale_days"], json!(91), "统计读取持久化配置");

    let descriptors = operations().as_array().unwrap().clone();
    let family = [
        ("insights.summary", "query", false),
        ("insights.configure", "mutation", false),
    ];
    assert_family_effects_and_examples(&descriptors, &family);
}

#[test]
fn preview_rebuilds_missing_thumbnail_for_temporary_generated_asset() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("w.sqlite");
    let app = Application::open(&db).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let source = dir.path().join("generated-colors.png");
    std::fs::write(
        &source,
        include_bytes!("../tests/fixtures/generated-colors.png"),
    )
    .unwrap();
    let imported = app
        .execute("assets.import", json!({"member_id":member,"path":source}))
        .unwrap();
    let id = imported["id"].as_str().unwrap().to_string();
    let doc = app
        .execute("assets.get", json!({"member_id":member,"id":id}))
        .unwrap();
    let original_thumbnail = doc["thumbnail"]["local_path"].as_str().unwrap().to_string();
    assert!(Path::new(&original_thumbnail).is_file(), "导入即生成缩略图");
    // 仅删除测试临时受管缩略图，制造缺失条件（不动用户文件）
    std::fs::remove_file(&original_thumbnail).unwrap();
    let preview = app
        .execute(
            "assets.preview",
            json!({"member_id":member,"id":id,"variant":"thumbnail"}),
        )
        .unwrap();
    assert!(preview["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/webp;base64,"));
    let doc = app
        .execute("assets.get", json!({"member_id":member,"id":id}))
        .unwrap();
    let rebuilt = doc["thumbnail"]["local_path"].as_str().unwrap();
    assert!(
        Path::new(rebuilt).is_file(),
        "preview 条件性重建缩略图并持久化"
    );
    // 默认 display 读取语义保持
    let display = app
        .execute(
            "assets.preview",
            json!({"member_id":member,"id":id,"variant":"display"}),
        )
        .unwrap();
    assert!(display["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/webp;base64,"));
}

#[test]
fn recommendation_family_effects_examples_and_runtime_output_contract() {
    // 真实运行时输出：规则策略同步产出意图；save/change/replace 实际执行；推荐永不产生穿着
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("rec.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let _top = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"上衣","category":"top"}),
        )
        .unwrap();
    let bottom = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"裤子","category":"bottom"}),
        )
        .unwrap();
    let generated = app
        .execute(
            "recommendation.generate",
            json!({"member_id":member,"operation_key":"rec-1","timezone":"UTC","strategy":"rules","limit":5,"occasion":"daily"}),
        )
        .unwrap();
    assert_eq!(generated["creates_wear"], json!(false), "推荐永不产生穿着");
    let proposals = generated["proposals"]
        .as_array()
        .expect("proposals 数组")
        .clone();
    assert!(
        !proposals.is_empty(),
        "有上下装时规则策略必须产出建议: {generated}"
    );
    let intent_id = generated["id"].as_str().unwrap().to_string();
    let got = app
        .execute(
            "recommendation.get",
            json!({"member_id":member,"id":intent_id}),
        )
        .unwrap();
    let listed = app
        .execute("recommendation.list", json!({"member_id":member}))
        .unwrap();
    let descriptors = operations().as_array().unwrap().clone();
    let schema_of = |name: &str| -> Value {
        let d = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        Value::Object(
            d["outputSchema"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} 必须声明 outputSchema"))
                .clone(),
        )
    };
    assert_result_matches_schema(
        &schema_of("recommendation.generate"),
        &generated,
        "generate",
    );
    assert_result_matches_schema(&schema_of("recommendation.get"), &got, "get");
    assert_result_matches_schema(&schema_of("recommendation.list"), &listed, "list");
    // 无条件执行采纳/调整/替换，实际公开输出对照声明 outputSchema
    let saved = app
        .execute(
            "recommendation.save",
            json!({"member_id":member,"id":intent_id,"name":"采纳搭配","operation_key":"save-1","proposal_index":0}),
        )
        .unwrap();
    assert_result_matches_schema(&schema_of("recommendation.save"), &saved, "save");
    let changed = app
        .execute(
            "recommendation.change",
            json!({"member_id":member,"id":intent_id,"operation_key":"change-1","proposal_index":0}),
        )
        .unwrap();
    assert_result_matches_schema(&schema_of("recommendation.change"), &changed, "change");
    let replaced = app
        .execute(
            "recommendation.replace",
            json!({"member_id":member,"id":intent_id,"operation_key":"replace-1","proposal_index":0,"clothing_id":bottom["id"]}),
        )
        .unwrap();
    assert_result_matches_schema(&schema_of("recommendation.replace"), &replaced, "replace");
    // 推荐与采纳从不产生穿着事实
    let wears = app
        .execute("wear.list", json!({"member_id":member}))
        .unwrap();
    assert_eq!(wears["items"], json!([]), "推荐/采纳不得产生 WearEvent");

    // effects：generate 同步 mutation；strategy=ai 为条件入队（worker 真实调用供应商）
    let family = [
        ("recommendation.generate", "mutation", false),
        ("recommendation.get", "query", false),
        ("recommendation.list", "query", false),
        ("recommendation.save", "mutation", false),
        ("recommendation.change", "mutation", false),
        ("recommendation.replace", "mutation", false),
    ];
    assert_family_effects_and_examples(&descriptors, &family);
    let generate = descriptors
        .iter()
        .find(|d| d["name"] == json!("recommendation.generate"))
        .unwrap();
    let conditional = generate["effect"]["conditional_effects"]
        .as_array()
        .expect("generate 须声明 strategy=ai 条件入队");
    assert!(conditional.iter().any(|c| {
        c["when"]["strategy"] == json!("ai")
            && c["effect"]["async"]["process"] == json!("recommendation.worker.run")
    }));
    // worker.run 自身同步处理任务并真实调用供应商，故不在 network=none 家族断言内
    let worker = descriptors
        .iter()
        .find(|d| d["name"] == json!("recommendation.worker.run"))
        .unwrap();
    assert_eq!(
        worker["effect"]["kind"],
        json!("mutation"),
        "worker.run 持久化任务状态"
    );
    assert_eq!(
        worker["effect"]["network"],
        json!("explicit-provider"),
        "worker.run 执行已排队 AI 生成时会真实调用供应商"
    );
    assert_eq!(worker["effect"]["local_files"], json!(false));
    assert!(
        worker["effect"]["async"].is_null(),
        "worker.run 是同步任务处理，不是异步入队"
    );
    let worker_examples = worker["examples"].as_array().expect("worker.run 须有示例");
    assert!(!worker_examples.is_empty(), "worker.run 至少一个示例");
    for example in worker_examples {
        assert_request_matches_input_schema(
            &worker["inputSchema"],
            &example["request"],
            "recommendation.worker.run",
        );
        assert!(
            example["prerequisites"]
                .as_array()
                .is_some_and(|p| !p.is_empty()),
            "worker.run 示例须含符号化前置说明"
        );
    }
    let gen_example = generate["examples"][0]["request"].clone();
    assert_eq!(
        gen_example["strategy"],
        json!("rules"),
        "示例默认离线策略，不自动执行付费调用"
    );
    // remaining：worker.run(strategy=ai) 真实供应商运行时证明需本地假供应商，未在本测试覆盖
}
