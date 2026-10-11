//! 注册表派生结果的抽检（issue #70）。
//!
//! 四张平行表的逐表焊接已由结构保证替代：
//! - effect ↔ 分发：同一注册条目（registry.rs 一个操作一行）导出，冻结快照
//!   （operation_registry::public_operation_contracts_are_frozen）逐字对照；
//! - 字段白名单 ↔ inputSchema：派生自同一 properties，全操作一致拒绝
//!   （operation_registry 逐字钉住）；
//! - scope 元数据 ↔ execute_with_grant：composite_scope / space_level_operation
//!   单一数据源（authorization.rs）。
//!
//! 本文件只保留：输出 schema 校验器自测、全操作示例有效性、
//! 派生元数据的抽样检查。各家族的运行时输出契约已迁入对应家族测试文件。

use serde_json::{json, Value};
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
            assert!(allowed.contains(value), "{context}: 字段 {key} 须为枚举值");
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
            declared_types.is_empty()
                || declared_types.iter().any(|t| {
                    t == actual_type
                        || (t == "number" && value.is_number())
                        || (t == "integer" && value.as_f64().is_some_and(|n| n.fract() == 0.0))
                }),
            "{context}: 字段 {key} 声明类型不符"
        );
    }
    for key in required {
        assert!(
            actual.contains_key(key),
            "{context}: 缺少 required 字段 {key}"
        );
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
fn output_contract_checker_handles_constants_nullable_types_and_required_fields() {
    let schema = json!({"type":"object","required":["count","flag"],"properties":{
        "count":{"type":"number"},"flag":{"const":false},
        "optional":{"type":["string","null"],"enum":["ready",null]}
    }});
    assert_result_matches_schema(&schema, &json!({"count":1,"flag":false}), "optional absent");
    assert_result_matches_schema(
        &schema,
        &json!({"count":1.5,"flag":false,"optional":null}),
        "nullable",
    );
    for invalid in [
        json!({"count":1}),
        json!({"count":null,"flag":false}),
        json!({"count":1,"flag":true}),
        json!({"count":1,"flag":false,"optional":"bad"}),
    ] {
        assert!(std::panic::catch_unwind(|| assert_result_matches_schema(
            &schema, &invalid, "invalid"
        ))
        .is_err());
    }
    let integer = json!({"type":"object","required":["n"],"properties":{"n":{"type":"integer"}}});
    assert_result_matches_schema(&integer, &json!({"n":1.0}), "integral number");
    assert!(std::panic::catch_unwind(|| assert_result_matches_schema(
        &integer,
        &json!({"n":1.5}),
        "fraction"
    ))
    .is_err());
    let enumeration = json!({"type":"object","properties":{"v":{"enum":["ready"]}}});
    assert!(std::panic::catch_unwind(|| assert_result_matches_schema(
        &enumeration,
        &json!({"v":null}),
        "enum null"
    ))
    .is_err());
}

/// 全操作示例有效性：request 字段必须在各自 inputSchema 内声明、
/// 类型/枚举相符；prerequisites 须为符号化字符串指令。
/// （category_and_member 版的推广：从一族扩到全部 214 个操作。）
#[test]
fn every_operation_examples_are_schema_valid_with_symbolic_prerequisites() {
    let descriptors = operations().as_array().unwrap().clone();
    assert!(descriptors.len() >= 200, "公开操作不应缩水");
    for descriptor in &descriptors {
        let name = descriptor["name"].as_str().unwrap();
        // sync.*/schedule.* 公开契约本就不含 examples（与缺失 effect 同源），缺席合法
        let examples = match descriptor.get("examples").and_then(|e| e.as_array()) {
            Some(examples) => examples,
            None => continue,
        };
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
    // 符号化占位抽查：示例值不得像真实业务数据
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

/// 派生元数据抽检：授权元数据与 effect 声明的抽样对照已知真值。
/// 全量逐表焊接已由单一数据源 + 冻结快照替代（见文件头）。
#[test]
fn derived_scope_and_effect_metadata_spot_checks() {
    let descriptors = operations().as_array().unwrap().clone();
    let find = |name: &str| -> Value {
        descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"))
            .clone()
    };
    for descriptor in &descriptors {
        let name = descriptor["name"].as_str().unwrap();
        let scope = descriptor
            .get("scope")
            .unwrap_or_else(|| panic!("{name} 缺少 scope 元数据"));
        assert_eq!(
            scope["primary"], descriptor["name"],
            "{name}: primary 授权名须与操作同名"
        );
    }
    // 授权通道抽检：privileged 前缀族 vs 外部可用族
    for name in ["diagnostics.summary", "client.authorize", "encryption.status"] {
        assert_eq!(
            find(name)["scope"]["access"],
            json!("trusted-local-only"),
            "{name}"
        );
    }
    for name in ["clothing.list", "feedback.record", "recommendation.generate"] {
        assert_eq!(find(name)["scope"]["access"], json!("external"), "{name}");
    }
    assert_eq!(
        find("capabilities")["scope"]["primary_grant_required"],
        json!(false)
    );
    assert_eq!(
        find("clothing.list")["scope"]["primary_grant_required"],
        json!(true)
    );
    // 成员限制语义抽检
    assert_eq!(find("clothing.list")["scope"]["member"]["grant"], json!("per-member_id"));
    assert_eq!(find("space.default")["scope"]["member"]["grant"], json!("all-members"));
    assert_eq!(find("provider.list")["scope"]["member"]["grant"], json!("space-level"));
    assert_eq!(find("member.list")["scope"]["member"]["grant"], json!("filtered"));
    assert_eq!(
        find("backup.create")["scope"]["member"]["grant"],
        json!("all-members")
    );
    // 组合 scope 渲染抽检（与 execute_with_grant 同一份数据）
    let feedback = find("feedback.record");
    let additional = feedback["scope"]["additional"]
        .as_array()
        .expect("feedback.record 须声明组合 scope");
    assert!(additional.iter().any(|condition| {
        condition["requires"] == json!(["wear.record"])
            && condition["when"]["reaction"] == json!(["actual_wore", "changed"])
    }));
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
    assert!(
        find("maintenance.worker.run")["scope"]["additional"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["requires_original_job_scope"] == json!(true)),
        "维护控制须声明原业务操作 scope 依赖"
    );
    assert!(
        find("maintenance.list")["scope"]["additional"].is_null(),
        "maintenance.list 无组合 scope"
    );
    // effect 声明抽检（派生自注册条目；分发行为由家族运行时测试证明）
    assert_eq!(find("category.list")["effect"]["kind"], json!("query"));
    assert_eq!(find("clothing.create")["effect"]["kind"], json!("mutation"));
    assert_eq!(find("assets.import")["effect"]["local_files"], json!(true));
    assert_eq!(
        find("learning.recompute")["effect"]["async"]["process"],
        json!("learning.worker.run")
    );
    assert_eq!(
        find("backup.create")["effect"]["async"]["process"],
        json!("maintenance.worker.run")
    );
    assert!(
        find("sync.status").get("effect").is_none(),
        "sync 家族公开契约本就不声明 effect"
    );
}

/// 运行时输出对照 outputSchema 的端到端样例保留在校验器覆盖的家族文件中；
/// 此处保留一个最小真实运行样例，证明公开 Core 契约要求输出带 schema_version=1。
#[test]
fn public_core_outputs_carry_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("registry.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let created = app
        .execute("category.create", json!({"member_id":member,"name":"户外"}))
        .unwrap();
    assert_eq!(
        created,
        json!({"schema_version":1,"id":created["id"],"name":"户外","count":0}),
        "category.create 公开结果"
    );
    let listed = app
        .execute("category.list", json!({"member_id":member}))
        .unwrap();
    let descriptors = operations();
    let schema_of = |name: &str| -> Value {
        Value::Object(
            descriptors
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["name"] == json!(name))
                .unwrap()["outputSchema"]
                .as_object()
                .unwrap()
                .clone(),
        )
    };
    assert_result_matches_schema(&schema_of("category.list"), &listed, "category.list");
}
