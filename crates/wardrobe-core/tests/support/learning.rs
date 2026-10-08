use serde_json::{json, Value};
use wardrobe_core::{ApiError, Application};
/// Test/client convenience only: the public endpoint itself always returns the durable receipt.
pub trait LearningTestSeam {
    fn execute_and_wait(&self, operation: &str, input: Value) -> Result<Value, ApiError>;
}
impl LearningTestSeam for Application {
    fn execute_and_wait(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        let result = self.execute(operation, input.clone())?;
        if operation != "learning.recompute" {
            return Ok(result);
        }
        let job = self.execute(
            "learning.worker.run",
            json!({"member_id":input["member_id"],"job_id":result["job_id"]}),
        )?;
        if job["state"] != "succeeded" {
            return Err(ApiError::of(job["error"]["code"]
                    .as_str()
                    .unwrap_or("recompute_failed")
                    .to_owned(), job["error"]["message"]
                    .as_str()
                    .unwrap_or("重算未完成")
                    .to_owned()));
        }
        self.execute("learning.get", json!({"member_id":input["member_id"]}))
    }
}
