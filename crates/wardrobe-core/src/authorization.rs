use crate::{string, ApiError, Application};
use rusqlite::params;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
fn privileged(operation: &str) -> bool {
    operation == "schedule.runtime.configure"
        || operation.starts_with("client.")
        || operation.starts_with("provider.credential.")
        || operation.starts_with("encryption.")
}
fn parse_grant(stored: &str) -> Result<Value, ApiError> {
    let grant: Value = serde_json::from_str(stored)
        .map_err(|_| ApiError::new("storage_error", "客户端授权损坏"))?;
    let required = [
        "schema_version",
        "id",
        "name",
        "mode",
        "operations",
        "member_ids",
        "expires_at",
        "revoked",
        "created_at",
    ];
    let valid = grant.as_object().is_some_and(|object| {
        required.iter().all(|field| object.contains_key(*field))
            && object
                .keys()
                .all(|field| required.contains(&field.as_str()) || field == "revoked_at")
    }) && grant["schema_version"] == 1
        && grant["id"].is_string()
        && grant["name"].is_string()
        && matches!(grant["mode"].as_str(), Some("full" | "limited"))
        && grant["operations"]
            .as_array()
            .is_some_and(|values| values.iter().all(Value::is_string))
        && (grant["member_ids"].is_null()
            || grant["member_ids"]
                .as_array()
                .is_some_and(|values| !values.is_empty() && values.iter().all(Value::is_string)))
        && (grant["expires_at"].is_null() || grant["expires_at"].is_string())
        && grant["revoked"].is_boolean()
        && grant["created_at"].is_string();
    if !valid {
        return Err(ApiError::new(
            "storage_error",
            "客户端授权字段损坏，未扩大授权范围",
        ));
    }
    Ok(grant)
}
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .unwrap()
        .keys()
        .any(|k| !allowed.contains(&k.as_str()))
    {
        return Err(ApiError::new("invalid_input", "客户端授权包含不支持的字段"));
    }
    Ok(())
}
impl Application {
    pub fn execute_as_client(
        &self,
        token: &str,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        if self.installation_changed()? {
            return Application::open(std::path::Path::new(self.connection.path().unwrap()))?
                .execute_as_client(token, operation, input);
        }
        if let Some(path) = self.redirected()? {
            return Application::open(&path)?.execute_as_client(token, operation, input);
        }
        self.check_schema()?;
        if !input.is_object() {
            return Err(ApiError::new("invalid_input", "输入必须是JSON对象"));
        }
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ApiError::new(
                "unauthorized",
                "外部客户端需要有效的本机授权令牌",
            ));
        }
        let stored = self
            .connection
            .query_row(
                "SELECT document FROM clients WHERE token_hash=?1",
                [hash(token)],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    ApiError::new("unauthorized", "客户端令牌无效")
                }
                _ => e.into(),
            })?;
        let grant = parse_grant(&stored)?;
        if grant["revoked"] == true {
            return Err(ApiError::new("authorization_revoked", "客户端授权已撤销"));
        }
        if let Some(expires) = grant["expires_at"].as_str() {
            let date = chrono::DateTime::parse_from_rfc3339(expires)
                .map_err(|_| ApiError::new("storage_error", "客户端授权到期时间损坏"))?;
            if date <= chrono::Utc::now() {
                return Err(ApiError::new("authorization_expired", "客户端授权已到期"));
            }
        }
        if privileged(operation) {
            return Err(ApiError::new(
                "scope_denied",
                "客户端不能登记授权或管理系统凭据，请使用可信本机设置",
            ));
        }
        if operation != "capabilities"
            && grant["mode"] != "full"
            && !grant["operations"]
                .as_array()
                .unwrap()
                .contains(&json!(operation))
        {
            return Err(ApiError::new("scope_denied", "操作超出客户端授权scope"));
        }
        if operation == "feedback.record"
            && matches!(input["reaction"].as_str(), Some("actual_wore" | "changed"))
            && grant["mode"] != "full"
            && !grant["operations"]
                .as_array()
                .unwrap()
                .contains(&json!("wear.record"))
        {
            return Err(ApiError::new(
                "scope_denied",
                "实际穿着反馈还需要wear.record授权",
            ));
        }
        if operation == "feedback.try_another"
            && grant["mode"] != "full"
            && ["feedback.record", "recommendation.change"]
                .iter()
                .any(|op| !grant["operations"].as_array().unwrap().contains(&json!(op)))
        {
            return Err(ApiError::new(
                "scope_denied",
                "Try Another还需要feedback.record与recommendation.change授权",
            ));
        }
        if !crate::operations()
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["name"] == operation)
        {
            return Err(ApiError::new("unsupported_operation", "此操作尚不可用"));
        }
        if grant["member_ids"].is_array() && crate::maintenance::is_global_operation(operation) {
            return Err(ApiError::new("member_denied", "全空间维护需要授权全部成员"));
        }
        if operation.starts_with("maintenance.") && operation != "maintenance.list" {
            let job = self.maintenance_raw_get(string(&input, "id")?)?;
            if grant["mode"] != "full"
                && !grant["operations"]
                    .as_array()
                    .unwrap()
                    .contains(&job["operation"])
            {
                return Err(ApiError::new(
                    "scope_denied",
                    "维护控制还需要原业务操作的授权scope",
                ));
            }
        }
        if let Some(members) = grant["member_ids"].as_array() {
            if input.get("member_id").is_some() {
                let member = self.member(&input)?;
                if !members.contains(&json!(member)) {
                    return Err(ApiError::new("member_denied", "成员超出客户端授权范围"));
                }
            } else if operation == "member.list" {
                let mut result = self.member_list()?;
                result["items"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|m| members.contains(&m["member_id"]));
                if !members.contains(&result["selected_member_id"]) {
                    result["selected_member_id"] = Value::Null;
                }
                return Ok(result);
            } else if !["capabilities", "provider.list", "provider.validate"].contains(&operation) {
                return Err(ApiError::new(
                    "member_denied",
                    "此操作需要明确授权的member_id，空间级操作须授权全部成员",
                ));
            }
        }
        #[cfg(test)]
        AFTER_AUTHORIZATION.with(|hook| {
            let callback = hook.borrow_mut().take();
            if let Some(callback) = callback {
                callback();
            }
        });
        if operation == "capabilities" {
            let mut result = self.execute_current(operation, input)?;
            result["operations"].as_array_mut().unwrap().retain(|o| {
                if grant["member_ids"].is_array()
                    && crate::maintenance::is_global_operation(o["name"].as_str().unwrap())
                {
                    return false;
                }
                if matches!(
                    o["name"].as_str(),
                    Some(
                        "schedule.create"
                            | "schedule.update"
                            | "schedule.worker.run"
                            | "schedule.tick"
                    )
                ) && grant["mode"] != "full"
                    && !["care.reminders", "recommendation.generate"]
                        .iter()
                        .any(|op| grant["operations"].as_array().unwrap().contains(&json!(op)))
                {
                    return false;
                }
                !privileged(o["name"].as_str().unwrap())
                    && (grant["mode"] == "full"
                        || o["name"] == "capabilities"
                        || grant["operations"].as_array().unwrap().contains(&o["name"]))
            });
            return Ok(result);
        }
        if operation == "provider.save" {
            return self.provider_save_from_client(input);
        }
        if matches!(
            operation,
            "schedule.create" | "schedule.update" | "schedule.worker.run"
        ) && grant["mode"] != "full"
        {
            let required = self.schedule_required_operation(operation, &input)?;
            if !grant["operations"]
                .as_array()
                .unwrap()
                .contains(&json!(required))
            {
                return Err(ApiError::new(
                    "scope_denied",
                    "定时计划执行还需要原业务操作授权",
                ));
            }
        }
        if operation == "recommendation.worker.run" && grant["mode"] != "full" {
            let member = self.member(&input)?;
            let scheduled: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM schedule_executions WHERE member_id=?1 AND json_extract(document,'$.job_id')=?2)",rusqlite::params![member,input["id"].as_str()],|r|r.get(0))?;
            if scheduled
                && !grant["operations"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("recommendation.generate"))
            {
                return Err(ApiError::new(
                    "scope_denied",
                    "定时推荐任务还需要recommendation.generate原业务授权",
                ));
            }
        }
        if operation == "schedule.tick"
            && grant["mode"] != "full"
            && self
                .schedule_tick_required_operations(&input)?
                .iter()
                .any(|op| !grant["operations"].as_array().unwrap().contains(&json!(op)))
        {
            return Err(ApiError::new(
                "scope_denied",
                "到期登记还需要计划原业务授权",
            ));
        }
        let mut result = self.execute_current(operation, input)?;
        if operation == "maintenance.list" && grant["mode"] != "full" {
            result["items"].as_array_mut().unwrap().retain(|job| {
                grant["operations"]
                    .as_array()
                    .unwrap()
                    .contains(&job["operation"])
            });
        }
        Ok(result)
    }

    pub(super) fn client_list(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &[])?;
        let mut statement = self
            .connection
            .prepare("SELECT document FROM clients ORDER BY id")?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        let mut items = vec![];
        for row in rows {
            items.push(parse_grant(&row?)?);
        }
        Ok(json!({"schema_version":1,"items":items}))
    }
    pub(super) fn client_revoke(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &["id"])?;
        let id = string(&input, "id")?;
        let tx = self.begin_write()?;
        let stored = tx
            .query_row("SELECT document FROM clients WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    ApiError::new("not_found", "客户端授权不存在")
                }
                _ => e.into(),
            })?;
        let mut doc = parse_grant(&stored)?;
        doc["revoked"] = json!(true);
        doc["revoked_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE clients SET document=?1 WHERE id=?2",
            params![doc.to_string(), id],
        )?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn client_authorize(&self, input: Value) -> Result<Value, ApiError> {
        fields(
            &input,
            &["name", "mode", "operations", "member_ids", "expires_at"],
        )?;
        let name = string(&input, "name")?.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(ApiError::new(
                "invalid_input",
                "客户端名称长度需为1到120字符",
            ));
        }
        let mode = string(&input, "mode")?;
        if !["full", "limited"].contains(&mode) {
            return Err(ApiError::new("invalid_input", "授权模式须为full或limited"));
        }
        let operations = input["operations"]
            .as_array()
            .ok_or_else(|| ApiError::new("invalid_input", "操作scope须为数组"))?;
        let discovered = crate::operations();
        let known = discovered.as_array().unwrap();
        if mode == "limited" && operations.is_empty() {
            return Err(ApiError::new("invalid_input", "限定授权至少选择一个操作"));
        }
        if operations.iter().any(|v| {
            v.as_str()
                .is_none_or(|name| privileged(name) || !known.iter().any(|o| o["name"] == name))
        }) {
            return Err(ApiError::new(
                "invalid_input",
                "scope包含未知或仅本机可信入口可用的操作",
            ));
        }
        if mode == "full" && !operations.is_empty() {
            return Err(ApiError::new(
                "invalid_input",
                "完整授权的operations应为空数组",
            ));
        }
        if input.get("member_ids").is_none() {
            return Err(ApiError::new("invalid_input", "须明确member_ids或null"));
        }
        let members = if input["member_ids"].is_null() {
            Value::Null
        } else {
            let values = input["member_ids"]
                .as_array()
                .ok_or_else(|| ApiError::new("invalid_input", "成员范围须为数组或null"))?;
            if values.is_empty() {
                return Err(ApiError::new("invalid_input", "至少授权一个成员"));
            }
            let mut ids = vec![];
            for value in values {
                let id = self.member(&json!({"member_id":value}))?;
                if !ids.contains(&json!(id)) {
                    ids.push(json!(id));
                }
            }
            json!(ids)
        };
        let expires = input
            .get("expires_at")
            .ok_or_else(|| ApiError::new("invalid_input", "须明确expires_at或null"))?;
        if !expires.is_null() {
            let value = expires
                .as_str()
                .ok_or_else(|| ApiError::new("invalid_input", "到期时间须为RFC3339字符串或null"))?;
            let date = chrono::DateTime::parse_from_rfc3339(value)
                .map_err(|_| ApiError::new("invalid_input", "到期时间无效"))?;
            if date <= chrono::Utc::now() {
                return Err(ApiError::new("invalid_input", "到期时间须在未来"));
            }
        }
        let mut random = [0u8; 32];
        getrandom::fill(&mut random)
            .map_err(|_| ApiError::new("security_error", "系统随机数不可用，未登记客户端"))?;
        let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let id = uuid::Uuid::now_v7().to_string();
        let doc = json!({"schema_version":1,"id":id,"name":name,"mode":mode,"operations":operations,"member_ids":members,"expires_at":expires,"revoked":false,"created_at":chrono::Utc::now().to_rfc3339()});
        let tx = self.begin_write()?;
        tx.execute(
            "INSERT INTO clients(id,token_hash,document) VALUES(?1,?2,?3)",
            params![id, hash(&token), doc.to_string()],
        )?;
        tx.commit()?;
        let mut output = doc;
        output["token"] = json!(token);
        Ok(output)
    }
}
fn grant_schema() -> Value {
    json!({"type":"object","required":["schema_version","id","name","mode","operations","member_ids","expires_at","revoked","created_at"],"properties":{"schema_version":{"type":"integer","const":1},"id":{"type":"string"},"name":{"type":"string"},"mode":{"enum":["full","limited"]},"operations":{"type":"array","items":{"type":"string"}},"member_ids":{"type":["array","null"],"items":{"type":"string"}},"expires_at":{"type":["string","null"]},"revoked":{"type":"boolean"},"created_at":{"type":"string"},"revoked_at":{"type":"string"}}})
}
pub(super) fn descriptors() -> Vec<Value> {
    let mut issued_schema = grant_schema();
    issued_schema["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("token"));
    issued_schema["properties"]["token"] =
        json!({"type":"string","readOnly":true,"description":"一次性连接令牌，不落普通配置或日志"});
    vec![
        json!({"name":"client.authorize","description":"仅可信本机入口登记客户端授权，一次性返回连接令牌","inputSchema":{"type":"object","required":["name","mode","operations","member_ids","expires_at"],"properties":{"name":{"type":"string"},"mode":{"enum":["full","limited"]},"operations":{"type":"array","items":{"type":"string"}},"member_ids":{"type":["array","null"],"items":{"type":"string"}},"expires_at":{"type":["string","null"]}},"additionalProperties":false},"outputSchema":issued_schema}),
        json!({"name":"client.list","description":"仅可信本机入口列出客户端，无连接令牌或hash","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","items"],"properties":{"schema_version":{"type":"integer","const":1},"items":{"type":"array","items":grant_schema()}}}}),
        json!({"name":"client.revoke","description":"仅可信本机入口撤销客户端授权","inputSchema":{"type":"object","required":["id"],"properties":{"id":{"type":"string"}},"additionalProperties":false},"outputSchema":grant_schema()}),
    ]
}

#[cfg(test)]
thread_local! {
    static AFTER_AUTHORIZATION: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
mod restore_boundary_tests {
    use super::*;
    #[test]
    fn restore_after_grant_validation_never_reopens_as_trusted() {
        let installation = tempfile::tempdir().unwrap();
        let archive = tempfile::tempdir().unwrap();
        let db = installation.path().join("wardrobe.sqlite");
        let app = Application::open(&db).unwrap();
        let package = archive.path().join("backup");
        let queued = app
            .execute("backup.create", json!({"directory":package}))
            .unwrap();
        app.execute("maintenance.worker.run", json!({"id":queued["id"]}))
            .unwrap();
        let grant = app.execute("client.authorize",json!({"name":"deterministic authorization seam","mode":"full","operations":[],"member_ids":null,"expires_at":null})).unwrap();
        let restorer = Application::open(&db).unwrap();
        AFTER_AUTHORIZATION.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                let queued = restorer
                    .execute(
                        "backup.restore",
                        json!({"directory":package,"confirm_overwrite":true}),
                    )
                    .unwrap();
                restorer
                    .execute("maintenance.worker.run", json!({"id":queued["id"]}))
                    .unwrap();
            }))
        });
        let result = app.execute_as_client(grant["token"].as_str().unwrap(), "clothing.create", json!({"member_id":"default","name":"must not bypass lost authorization","category":"top"}));
        assert_eq!(result.unwrap_err().code, "data_restored");
        assert!(app
            .execute("clothing.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
