use serde_json::json;
use wardrobe_core::Application;
trait PortableTestApplication {
    fn execute_and_wait(
        &self,
        operation: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, wardrobe_core::ApiError>;
}
impl PortableTestApplication for Application {
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
                Err(wardrobe_core::ApiError::of(job["error"]["code"]
                        .as_str()
                        .unwrap_or("job_cancelled")
                        .to_owned(), job["error"]["message"]
                        .as_str()
                        .unwrap_or("任务未完成")
                        .to_owned()))
            }
        } else {
            Ok(submitted)
        }
    }
}
#[test]
fn portable_backup_preserves_learning_preference_and_schedule_plan_across_restore_and_reopen() {
    let source = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let app = Application::open(&source.path().join("wardrobe.sqlite")).unwrap();
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"学习组合衬衫","category":"top","color":["blue"]}),
        )
        .unwrap();
    let memory = app
        .execute_and_wait(
            "learning.set",
            json!({"member_id":"default","operation_key":"portable-pref","dimension":"color","value":"blue","weight":3,"note":"我明确喜欢蓝色"}),
        )
        .unwrap();
    assert_eq!(memory["source"]["kind"], "explicit_user");
    let profile_before = app
        .execute_and_wait("learning.get", json!({"member_id":"default"}))
        .unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let plan = app
        .execute(
            "schedule.create",
            json!({"member_id":"default","name":"前夜准备","kind":"recommendation","local_time":"21:30","timezone":"Asia/Shanghai","weekdays":[1,2,3,4,5],"execution_device_id":device["id"],"occasion":"work","target_day_offset":1,"enabled":true}),
        )
        .unwrap();
    assert_eq!(plan["revision"], 1);
    let package = archive.path().join("combined-backup");
    let backup = app
        .execute_and_wait(
            "backup.create",
            json!({"directory":package,"include_originals":false}),
        )
        .unwrap();
    assert_eq!(backup["includes_originals"], false);
    let preview = app
        .execute_and_wait("backup.preview", json!({"directory":package}))
        .unwrap();
    assert_eq!(preview["valid"], true);
    for (table, count) in [
        ("clothing", 1),
        ("learning_memories", 1),
        ("learning_profiles", 0),
        ("learning_state", 0),
        ("learning_history", 1),
        ("learning_receipts", 1),
        ("schedule_plans", 1),
        ("schedule_executions", 0),
    ] {
        assert_eq!(
            preview["incoming"][table], count,
            "preview incoming {table}"
        );
        assert_eq!(preview["current"][table], count, "preview current {table}");
    }
    let restored = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    restored
        .execute_and_wait(
            "backup.restore",
            json!({"directory":package,"confirm_overwrite":true}),
        )
        .unwrap();
    drop(restored);
    let reopened = Application::open(&target.path().join("wardrobe.sqlite")).unwrap();
    assert_eq!(
        reopened
            .execute_and_wait("learning.get", json!({"member_id":"default"}))
            .unwrap(),
        profile_before
    );
    let listed = reopened
        .execute_and_wait("schedule.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(listed["plans"], json!([plan]));
    assert_eq!(
        reopened
            .execute_and_wait(
                "clothing.get",
                json!({"member_id":"default","id":garment["id"]})
            )
            .unwrap(),
        garment
    );
}
