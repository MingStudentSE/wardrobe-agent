use serde_json::{json, Value};
use wardrobe_core::{operations, Application};
trait MaintenanceTestApplication {
    fn execute_and_wait(
        &self,
        operation: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, wardrobe_core::ApiError>;
}
impl MaintenanceTestApplication for Application {
    fn execute_and_wait(
        &self,
        operation: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, wardrobe_core::ApiError> {
        let submitted = self.execute(operation, input)?;
        if let Some(id) = submitted["job_id"].as_str() {
            let job = self.execute("maintenance.worker.run", json!({"id":id}))?;
            if job["state"] == "succeeded" {
                Ok(job["result"].clone())
            } else {
                Err(wardrobe_core::ApiError::custom(
                    job["error"]["code"].as_str().unwrap_or("job_cancelled"),
                    job["error"]["message"].as_str().unwrap_or("任务未完成"),
                ))
            }
        } else {
            Ok(submitted)
        }
    }
}

#[test]
fn custom_categories_and_deletion_preserve_clothing_and_are_member_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let category = app
        .execute(
            "category.create",
            json!({"member_id":"default","name":"汉服"}),
        )
        .unwrap();
    let id = category["id"].as_str().unwrap();
    assert!(app
        .execute(
            "category.create",
            json!({"member_id":"default","name":"汉服"})
        )
        .is_err());
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"测试衣物","category":id}),
        )
        .unwrap();
    let patched = app
        .execute(
            "clothing.update",
            json!({"member_id":"default","id":item["id"],"category":id}),
        )
        .unwrap();
    assert_eq!(patched["category"], id);
    let result = app
        .execute("category.delete", json!({"member_id":"default","id":id}))
        .unwrap();
    assert_eq!(result["moved"], 1);
    assert_eq!(
        app.execute(
            "clothing.get",
            json!({"member_id":"default","id":item["id"]})
        )
        .unwrap()["category"],
        "other"
    );
    assert!(app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"无效","category":id})
        )
        .is_err());
    assert!(app
        .execute(
            "category.delete",
            json!({"member_id":"default","id":"other"})
        )
        .is_err());
    app.execute(
        "category.delete",
        json!({"member_id":"default","id":"dress"}),
    )
    .unwrap();
    assert!(!app
        .execute("category.list", json!({"member_id":"default"}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "dress"));
    let second = app
        .execute("member.create", json!({"name":"另一个衣橱"}))
        .unwrap();
    assert!(app
        .execute("category.list", json!({"member_id":second["member_id"]}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "dress"));
}
#[test]
fn custom_categories_survive_portable_backup_restore() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("source")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let category = app
        .execute(
            "category.create",
            json!({"member_id":"default","name":"家居服"}),
        )
        .unwrap();
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"睡衣","category":category["id"]}),
        )
        .unwrap();
    app.execute_and_wait(
        "backup.create",
        json!({"directory":dir.path().join("backup")}),
    )
    .unwrap();
    let restored = Application::open(&dir.path().join("restored")).unwrap();
    restored.execute("space.default", json!({})).unwrap();
    restored
        .execute_and_wait(
            "backup.preview",
            json!({"directory":dir.path().join("backup")}),
        )
        .unwrap();
    restored
        .execute_and_wait(
            "backup.restore",
            json!({"directory":dir.path().join("backup"),"confirm_overwrite":true}),
        )
        .unwrap();
    assert!(restored
        .execute("category.list", json!({"member_id":"default"}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == category["id"]));
    assert_eq!(
        restored
            .execute(
                "clothing.get",
                json!({"member_id":"default","id":item["id"]})
            )
            .unwrap()["category"],
        category["id"]
    );
}

// ==== 自 registry_contracts.rs 迁入的运行时/内容契约（issue #70 增量5）====

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
