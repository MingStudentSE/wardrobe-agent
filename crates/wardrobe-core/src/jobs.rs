use crate::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
/// Whole-space task payloads require all-member authorization even through generic job APIs.
pub(super) fn is_space_job(kind: &str) -> bool {
    kind == "sync"
}
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),kind TEXT NOT NULL,operation_key TEXT NOT NULL,document TEXT NOT NULL,UNIQUE(member_id,kind,operation_key)); CREATE INDEX IF NOT EXISTS jobs_member ON jobs(member_id,id);")?;
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    member: &str,
    kind: &str,
    key: &str,
    payload: Value,
) -> Result<Value, ApiError> {
    let existing: Option<String> = tx
        .query_row(
            "SELECT document FROM jobs WHERE member_id=?1 AND kind=?2 AND operation_key=?3",
            params![member, kind, key],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(stored) = existing {
        let doc: Value = serde_json::from_str(&stored)
            .map_err(|_| ApiError::new("storage_error", "任务记录损坏"))?;
        if doc["payload"] != payload {
            return Err(ApiError::new(
                "idempotency_conflict",
                "同一操作标识不能替换任务输入",
            ));
        }
        return Ok(doc);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let doc = json!({"schema_version":1,"id":uuid::Uuid::now_v7().to_string(),"member_id":member,"kind":kind,"operation_key":key,"state":"queued","progress":0,"attempts":0,"next_due":0,"lease_token":null,"lease_expires":0,"payload":payload,"result":null,"error":null,"created_at":now,"updated_at":now});
    save(tx, &doc)?;
    Ok(doc)
}
pub(super) fn save(tx: &Transaction<'_>, doc: &Value) -> Result<(), ApiError> {
    tx.execute("INSERT INTO jobs VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET document=excluded.document",params![doc["id"].as_str(),doc["member_id"].as_str(),doc["kind"].as_str(),doc["operation_key"].as_str(),doc.to_string()])?;
    Ok(())
}
impl Application {
    /// Trusted worker seam; external clients use the authorized Application API.
    pub fn job_enqueue(
        &self,
        member: &str,
        kind: &str,
        key: &str,
        payload: Value,
    ) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let doc = insert(&tx, member, kind, key, payload)?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn job_get(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM jobs WHERE member_id=?1 AND id=?2",
                params![member, string(&input, "id")?],
                |r| r.get(0),
            )
            .optional()?;
        serde_json::from_str(&stored.ok_or_else(|| ApiError::new("not_found", "任务不存在"))?)
            .map_err(|_| ApiError::new("storage_error", "任务记录损坏"))
    }
    pub(super) fn job_list(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let mut statement = self
            .connection
            .prepare("SELECT document FROM jobs WHERE member_id=?1 ORDER BY id DESC")?;
        let mut items = vec![];
        for row in statement.query_map([member], |r| r.get::<_, String>(0))? {
            let doc: Value = serde_json::from_str(&row?)
                .map_err(|_| ApiError::new("storage_error", "任务记录损坏"))?;
            items.push(doc);
        }
        Ok(json!({"schema_version":1,"items":items}))
    }
    pub fn job_claim(&self, member: &str, id: &str, lease_seconds: u64) -> Result<Value, ApiError> {
        if !(1..=300).contains(&lease_seconds) {
            return Err(ApiError::new("invalid_input", "lease须为1–300秒"));
        }
        let tx = self.begin_write()?;
        let mut doc = self.job_get(json!({"member_id":member,"id":id}))?;
        let now = chrono::Utc::now().timestamp_millis();
        let eligible = doc["state"] == "queued"
            || (doc["state"] == "running" && doc["lease_expires"].as_i64().unwrap_or(0) <= now);
        if !eligible || doc["next_due"].as_i64().unwrap_or(0) > now {
            return Err(ApiError::new(
                "job_unavailable",
                "任务尚未到期、已结束或其他worker正在处理",
            ));
        }
        doc["state"] = json!("running");
        doc["progress"] = json!(0);
        doc["lease_token"] = json!(uuid::Uuid::now_v7().to_string());
        doc["lease_expires"] = json!(now + lease_seconds as i64 * 1000);
        doc["attempts"] = json!(doc["attempts"].as_u64().unwrap_or(0) + 1);
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        save(&tx, &doc)?;
        tx.commit()?;
        Ok(doc)
    }
    pub fn job_finish(
        &self,
        member: &str,
        id: &str,
        lease: &str,
        state: &str,
        result: Value,
        error: Value,
    ) -> Result<Value, ApiError> {
        if !["succeeded", "waiting_input", "failed"].contains(&state) {
            return Err(ApiError::new("invalid_input", "无效任务结束状态"));
        }
        let tx = self.begin_write()?;
        let mut doc = self.job_get(json!({"member_id":member,"id":id}))?;
        if doc["state"] != "running"
            || doc["lease_token"] != lease
            || doc["lease_expires"].as_i64().unwrap_or(0) < chrono::Utc::now().timestamp_millis()
        {
            return Err(ApiError::new("lease_lost", "任务已取消或lease已失效"));
        }
        doc["state"] = json!(state);
        doc["progress"] = json!(if state == "failed" { 0 } else { 100 });
        doc["result"] = result;
        doc["error"] = error;
        doc["lease_token"] = Value::Null;
        doc["lease_expires"] = json!(0);
        // Only real network tasks supply transient classification; local decode failures never loop automatically.
        if state == "failed"
            && doc["error"]["retryable"] == true
            && doc["attempts"].as_u64().unwrap_or(3) < 3
        {
            doc["state"] = json!("queued");
            let delay = doc["error"]["retry_after_ms"]
                .as_i64()
                .unwrap_or(1000 * (1_i64 << doc["attempts"].as_u64().unwrap_or(1)))
                .clamp(1000, 300_000);
            doc["next_due"] = json!(chrono::Utc::now().timestamp_millis() + delay);
        }
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        save(&tx, &doc)?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn job_action(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let mut doc = self.job_get(input)?;
        if operation == "jobs.cancel" {
            if doc["state"] == "succeeded" {
                return Err(ApiError::new("invalid_state", "已完成任务不能取消"));
            }
            doc["state"] = json!("cancelled");
        } else {
            if (["product_display", "outfit_flatlay", "virtual_tryon"]
                .contains(&doc["kind"].as_str().unwrap_or(""))
                && doc["submitted_at"].is_string())
                || (doc["kind"] == "recognition" && doc["payload"]["submission"] != "not_submitted")
            {
                return Err(ApiError::new(
                    "submission_already_sent",
                    "此生成任务已经提交，不能盲重发；请检查供应商记录，显式创建新生成意图",
                ));
            }
            if !["failed", "cancelled"].contains(&doc["state"].as_str().unwrap_or("")) {
                return Err(ApiError::new("invalid_state", "仅失败或取消任务可重试"));
            }
            doc["state"] = json!("queued");
            doc["error"] = Value::Null;
            doc["progress"] = json!(0);
            doc["attempts"] = json!(0);
            doc["next_due"] = json!(0);
        }
        doc["lease_token"] = Value::Null;
        doc["lease_expires"] = json!(0);
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        save(&tx, &doc)?;
        tx.commit()?;
        Ok(doc)
    }
}
pub(super) fn output_schema() -> Value {
    json!({"type":"object","required":["schema_version","id","member_id","kind","operation_key","state","progress","attempts","next_due","lease_expires","lease_token","payload","result","error","created_at","updated_at"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"kind":{"type":"string"},"operation_key":{"type":"string"},"state":{"enum":["queued","running","waiting_input","succeeded","failed","cancelled"]},"progress":{"type":"integer","minimum":0,"maximum":100},"attempts":{"type":"integer","minimum":0},"next_due":{"type":"integer"},"lease_expires":{"type":"integer"},"lease_token":{"type":["string","null"]},"submitted_at":{"type":"string"},"payload":{},"result":{},"error":{"type":["object","null"]},"created_at":{"type":"string"},"updated_at":{"type":"string"}}})
}
pub(super) fn descriptors() -> Vec<Value> {
    ["get","list","cancel","retry"].iter().map(|action|{let required=if *action=="list"{vec!["member_id"]}else{vec!["member_id","id"]};let output=if *action=="list"{json!({"type":"object","required":["schema_version","items"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":output_schema()}}})}else{output_schema()};json!({"name":format!("jobs.{action}"),"description":"本机持久任务查询与逐项取消重试","inputSchema":{"type":"object","required":required,"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}},"outputSchema":output})}).collect()
}
