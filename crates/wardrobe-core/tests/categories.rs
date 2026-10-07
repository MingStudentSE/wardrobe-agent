use serde_json::json;
use wardrobe_core::Application;
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
                Err(wardrobe_core::ApiError {
                    code: job["error"]["code"]
                        .as_str()
                        .unwrap_or("job_cancelled")
                        .into(),
                    message: job["error"]["message"]
                        .as_str()
                        .unwrap_or("任务未完成")
                        .into(),
                })
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
