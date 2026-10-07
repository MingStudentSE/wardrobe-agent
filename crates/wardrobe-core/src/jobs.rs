use crate::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),kind TEXT NOT NULL,operation_key TEXT NOT NULL,document TEXT NOT NULL,UNIQUE(member_id,kind,operation_key)); CREATE INDEX IF NOT EXISTS jobs_member ON jobs(member_id,id);")?;
    Ok(())
}
fn submission_hash(mut payload: Value, kind: &str) -> String {
    use sha2::{Digest, Sha256};
    if kind == "recognition" {
        if let Some(object) = payload.as_object_mut() {
            object.remove("submission");
            object.remove("transferred_images");
        }
    }
    Sha256::digest(payload.to_string().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
/// Installation-local metadata only; never copied into portable facts.
pub(super) fn retain_submission_guards(tx: &Transaction<'_>) -> Result<(), ApiError> {
    let space: String = tx.query_row("SELECT id FROM workspace", [], |row| row.get(0))?;
    let mut statement=tx.prepare("SELECT document FROM jobs WHERE json_type(document,'$.submitted_at')='text' OR (kind='recognition' AND json_extract(document,'$.payload.submission') IN ('submitted','received','unknown'))")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        let doc: Value = serde_json::from_str(&raw)
            .map_err(|_| ApiError::new("storage_error", "提交记录损坏，不能安全覆盖"))?;
        let kind = doc["kind"]
            .as_str()
            .ok_or_else(|| ApiError::new("storage_error", "提交类型损坏"))?;
        let hash = submission_hash(doc["payload"].clone(), kind);
        let submitted = doc["submitted_at"]
            .as_str()
            .or_else(|| doc["updated_at"].as_str())
            .ok_or_else(|| ApiError::new("storage_error", "提交时间缺失"))?;
        tx.execute("INSERT INTO local_submission_guards VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(space_id,member_id,kind,operation_key) DO UPDATE SET submission_count=MAX(submission_count,excluded.submission_count),last_outcome=excluded.last_outcome",params![space,doc["member_id"].as_str(),kind,doc["operation_key"].as_str(),hash,submitted,doc["submission_count"].as_u64().unwrap_or(1),doc["last_outcome"].as_str().unwrap_or("unknown")])?;
    }
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
    let guard:Option<String>=tx.query_row("SELECT payload_hash FROM local_submission_guards WHERE space_id=(SELECT id FROM workspace) AND member_id=?1 AND kind=?2 AND operation_key=?3",params![member,kind,key],|row|row.get(0)).optional()?;
    if let Some(hash) = guard {
        if hash != submission_hash(payload, kind) {
            return Err(ApiError::new(
                "idempotency_conflict",
                "恢复前已提交的同一意图不能改变输入；请先核实供应商并明确创建新意图",
            ));
        }
        return Err(ApiError::new(
            "submission_already_sent",
            "本机恢复前此意图已经提交；恢复不会授权再次扣用额度，请核实供应商记录或明确创建新意图",
        ));
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
            if ([
                "product_display",
                "outfit_flatlay",
                "virtual_tryon",
                "text_recommendation",
                "stylist_query",
            ]
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
    ["get","list","cancel","retry"].iter().map(|action|{let required=if *action=="list"{vec!["member_id"]}else{vec!["member_id","id"]};let output=if *action=="list"{json!({"type":"object","required":["schema_version","items"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":output_schema()}}})}else{output_schema()};let (effect,examples)=match *action {
            "get"=>(json!({"kind":"query","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<任务收据或任务列表中的任务ID>"},"prerequisites":["读取持久任务文档（含payload/result/error与进度）","查询不重发任务；结果未知时先查询再决定"]}])),"list"=>(json!({"kind":"query","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["按任务ID倒序返回当前成员全部本机持久任务文档"]}])),"cancel"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<任务ID>"},"prerequisites":["已完成（succeeded）任务不能取消","取消不保证撤回供应商已消耗额度"]}])),"retry"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<失败或取消的任务ID>"},"prerequisites":["仅failed/cancelled任务可重试并回到queued","已提交供应商的生成/推荐任务拒绝盲重发（submission_already_sent）；请核对供应商记录后显式创建新意图","本示例不自动执行"]}])),_=>unreachable!()};json!({"name":format!("jobs.{action}"),"description":"本机持久任务查询与逐项取消重试","effect":effect,"examples":examples,"inputSchema":{"type":"object","required":required,"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}},"outputSchema":output})}).collect()
}
