use crate::{string, ApiError, Application};
use rusqlite::params;
use rusqlite::OptionalExtension;
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
        // 诊断查询含日志原文与底层原因：仅本机可信入口，外部客户端永远无法获得
        || operation.starts_with("diagnostics.")
}
/// 空间级操作：受限成员授权无须 member_id 也可执行——
/// scope_metadata 与 execute_with_grant 从这里取用，不再各写一份清单（#70）。
pub(crate) fn space_level_operation(operation: &str) -> bool {
    ["capabilities", "provider.list", "provider.validate"].contains(&operation)
}
/// 组合 scope 规则：scope_metadata 渲染与 execute_with_grant 执行的单一数据源。
/// 分支位置与错误文案保持既有执行语义；只收编第二份手写拷贝（#70）。
/// （schedule 族按原业务操作的条件 scope 是执行分支特有，未在元数据声明，保持原状。）
pub(crate) enum CompositeScope {
    /// 输入字段取特定值时 requires 其他操作 scope
    FieldValue { field: &'static str, values: &'static [&'static str], requires: &'static [&'static str], message: &'static str },
    /// 无条件 requires
    Plain { requires: &'static [&'static str], message: &'static str },
    /// 按持久任务 kind 条件 requires（id_field 决定任务 ID 取自哪个输入字段）
    JobKind { kind: &'static str, id_field: JobIdField, requires: &'static [&'static str], message: &'static str },
    /// 维护控制需要原任务的业务操作 scope（按 id 字段取任务）
    OriginalJobScope,
}
pub(crate) enum JobIdField {
    /// learning.worker.run 的 job_id
    JobId,
    /// jobs.retry / jobs.cancel 的 id
    Id,
}
pub(crate) fn composite_scope(operation: &str) -> &'static [CompositeScope] {
    const FEEDBACK_RECORD: &[CompositeScope] = &[CompositeScope::FieldValue {
        field: "reaction",
        values: &["actual_wore", "changed"],
        requires: &["wear.record"],
        message: "实际穿着反馈还需要wear.record授权",
    }];
    const FEEDBACK_TRY_ANOTHER: &[CompositeScope] = &[CompositeScope::Plain {
        requires: &["feedback.record", "recommendation.change"],
        message: "Try Another还需要feedback.record与recommendation.change授权",
    }];
    const LEARNING_WORKER: &[CompositeScope] = &[CompositeScope::JobKind {
        kind: "learning_recompute",
        id_field: JobIdField::JobId,
        requires: &["learning.recompute"],
        message: "管理学习重算任务还需要learning.recompute授权",
    }];
    const LEARNING_JOB_ACTION: &[CompositeScope] = &[CompositeScope::JobKind {
        kind: "learning_recompute",
        id_field: JobIdField::Id,
        requires: &["learning.recompute"],
        message: "管理学习重算任务还需要learning.recompute授权",
    }];
    const MAINTENANCE: &[CompositeScope] = &[CompositeScope::OriginalJobScope];
    match operation {
        "feedback.record" => FEEDBACK_RECORD,
        "feedback.try_another" => FEEDBACK_TRY_ANOTHER,
        "learning.worker.run" => LEARNING_WORKER,
        "jobs.retry" | "jobs.cancel" => LEARNING_JOB_ACTION,
        op if op.starts_with("maintenance.") && op != "maintenance.list" => MAINTENANCE,
        _ => &[],
    }
}
/// 描述符 scope 元数据：与上方 privileged()、maintenance::is_global_operation()
/// 及 execute_as_client 中的精确授权条件同源派生；只描述既有执行语义，不改变授权。
/// 成员约束在 member_ids 为数组时生效（含 full 模式）；有效输入不含 member_id 的
/// 操作（除显式空间级豁免）受限成员授权无法执行，按 all-members 标注。
/// 组合条件由 composite_scope 渲染，与 execute_with_grant 执行同一份数据。
pub(crate) fn scope_metadata(operation: &str, input_schema: &Value) -> Value {
    let access = if privileged(operation) {
        "trusted-local-only"
    } else {
        "external"
    };
    // capabilities 对有限授权免 primary 检查（execute_as_client 精确豁免）；
    // 仅本机可信入口的操作外部一律拒绝，primary/member 不参与判定。
    let primary_grant_required = access == "external" && operation != "capabilities";
    let member_grant = if crate::maintenance::is_global_operation(operation) {
        // 受限成员授权直接 member_denied，须授权全部成员
        "all-members"
    } else if space_level_operation(operation) {
        "space-level"
    } else if operation == "member.list" {
        "filtered"
    } else if input_schema["properties"].get("member_id").is_some() {
        // 受限成员授权必须提供且仅能使用授权内 member_id
        "per-member_id"
    } else {
        // 有效输入没有 member_id：受限成员授权在执行分支 member_denied
        "all-members"
    };
    let mut scope = json!({
        "access": access,
        "primary": operation,
        "primary_grant_required": primary_grant_required,
        "member": {"grant": member_grant},
    });
    let additional: Vec<Value> = composite_scope(operation)
        .iter()
        .map(|rule| match rule {
            CompositeScope::FieldValue { field, values, requires, .. } => {
                json!({"when": {*field: values}, "requires": requires})
            }
            CompositeScope::Plain { requires, .. } => json!({"requires": requires}),
            CompositeScope::JobKind { kind, requires, .. } => {
                json!({"when_job_kind": kind, "requires": requires})
            }
            CompositeScope::OriginalJobScope => {
                json!({"requires_original_job_scope": true, "job_field": "id"})
            }
        })
        .collect();
    if !additional.is_empty() {
        scope["additional"] = Value::Array(additional);
    }
    scope
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
fn check_grant_operation(grant: &Value, operation: &str) -> Result<(), ApiError> {
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
            "客户端不能登记授权、管理系统凭据或读取诊断，请使用可信本机设置",
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
    Ok(())
}

fn require_granted_member(grant: &Value, member: &str) -> Result<(), ApiError> {
    if grant["member_ids"]
        .as_array()
        .is_some_and(|members| !members.contains(&json!(member)))
    {
        return Err(ApiError::new("member_denied", "成员超出客户端授权范围"));
    }
    Ok(())
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
        self.execute_as_client_traced(token, operation, input, crate::diagnostics::new_trace_id())
    }
    fn execute_as_client_traced(
        &self,
        token: &str,
        operation: &str,
        input: Value,
        trace_id: String,
    ) -> Result<Value, ApiError> {
        if self.installation_changed()? {
            return Application::open(std::path::Path::new(self.connection.path().unwrap()))?
                .execute_as_client_traced(token, operation, input, trace_id);
        }
        if let Some(path) = self.redirected()? {
            return Application::open(&path)?
                .execute_as_client_traced(token, operation, input, trace_id);
        }
        self.logged(trace_id, operation, input, |app, operation, input| {
            app.authorized_execute(token, operation, input)
        })
    }
    fn authorized_execute(
        &self,
        token: &str,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
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
        self.execute_as_grant_id(string(&parse_grant(&stored)?, "id")?, operation, input)
    }

    // Deferred tools retain a grant ID, never the raw bearer token. Resolve it
    // afresh for each dispatch and reuse the same scope/member/revocation checks.
    pub(super) fn execute_as_grant_id(
        &self,
        grant_id: &str,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        if self.installation_changed()? {
            return Application::open(std::path::Path::new(self.connection.path().unwrap()))?
                .execute_as_grant_id(grant_id, operation, input);
        }
        if let Some(path) = self.redirected()? {
            return Application::open(&path)?.execute_as_grant_id(grant_id, operation, input);
        }
        self.check_schema()?;
        if !input.is_object() {
            return Err(ApiError::new("invalid_input", "输入必须是JSON对象"));
        }
        let stored = self
            .connection
            .query_row(
                "SELECT document FROM clients WHERE id=?1",
                [grant_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    ApiError::new("unauthorized", "原始客户端授权不存在")
                }
                _ => error.into(),
            })?;
        self.execute_with_grant(parse_grant(&stored)?, operation, input)
    }

    pub(crate) fn check_deferred_grant(
        &self,
        grant_id: &str,
        operation: &str,
        member: &str,
    ) -> Result<(), ApiError> {
        self.check_schema()?;
        let stored = self
            .connection
            .query_row(
                "SELECT document FROM clients WHERE id=?1",
                [grant_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    ApiError::new("unauthorized", "原始客户端授权不存在")
                }
                _ => error.into(),
            })?;
        let grant = parse_grant(&stored)?;
        check_grant_operation(&grant, operation)?;
        require_granted_member(&grant, member)
    }

    fn execute_with_grant(
        &self,
        grant: Value,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        check_grant_operation(&grant, operation)?;
        // 输入字段条件组合 scope（composite_scope 单一来源；分支位置与文案保持既有语义）
        for rule in composite_scope(operation) {
            if let CompositeScope::FieldValue { field, values, requires, message } = rule {
                if matches!(input[field].as_str(), Some(value) if values.contains(&value))
                    && grant["mode"] != "full"
                    && !requires
                        .iter()
                        .all(|op| grant["operations"].as_array().unwrap().contains(&json!(op)))
                {
                    return Err(ApiError::new("scope_denied", message));
                }
            }
            if let CompositeScope::Plain { requires, message } = rule {
                if grant["mode"] != "full"
                    && !requires
                        .iter()
                        .all(|op| grant["operations"].as_array().unwrap().contains(&json!(op)))
                {
                    return Err(ApiError::new("scope_denied", message));
                }
            }
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
        if composite_scope(operation)
            .iter()
            .any(|rule| matches!(rule, CompositeScope::OriginalJobScope))
        {
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
            if ["sync.configure", "sync.run", "sync.process", "sync.status"].contains(&operation) {
                return Err(ApiError::new(
                    "member_denied",
                    "同步目标和运行操作需要明确授权全部成员",
                ));
            }

            if ["jobs.get", "jobs.cancel", "jobs.retry"].contains(&operation) {
                if let Some(id) = input["id"].as_str() {
                    let kind: Option<String> = self
                        .connection
                        .query_row("SELECT kind FROM jobs WHERE id=?1", [id], |r| r.get(0))
                        .optional()?;
                    if kind.as_deref().is_some_and(crate::jobs::is_space_job) {
                        return Err(ApiError::new(
                            "member_denied",
                            "空间同步任务需要明确授权全部成员",
                        ));
                    }
                }
            }
            if input.get("member_id").is_some() {
                let member = self.member(&input)?;
                require_granted_member(&grant, &member)?;
                if operation == "jobs.list" {
                    let mut result = self.execute(operation, input)?;
                    result["items"].as_array_mut().unwrap().retain(|job| {
                        !crate::jobs::is_space_job(job["kind"].as_str().unwrap_or(""))
                    });
                    return Ok(result);
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
            } else if !space_level_operation(operation) {
                return Err(ApiError::new(
                    "member_denied",
                    "此操作需要明确授权的member_id，空间级操作须授权全部成员",
                ));
            }
        }
        if matches!(
            operation,
            "stylist.worker.run" | "jobs.retry" | "jobs.cancel"
        ) {
            let member = self.member(&input)?;
            let job = self.job_get(json!({"member_id":member,"id":string(&input,"id")?}))?;
            if job["kind"] == "stylist_query" {
                let expected = crate::stylist::StylistOrigin::Client {
                    grant_id: string(&grant, "id")?.to_owned(),
                };
                let actual: crate::stylist::StylistOrigin =
                    serde_json::from_value(job["payload"]["origin"].clone())
                        .map_err(|_| ApiError::new("storage_error", "任务授权来源损坏"))?;
                if actual != expected {
                    return Err(ApiError::new(
                        "scope_denied",
                        "客户端不能管理其他授权来源的着装问答",
                    ));
                }
                self.check_deferred_grant(string(&grant, "id")?, "stylist.ask", &member)?;
            }
        }
        if grant["mode"] != "full" {
            // 按任务 kind 条件组合 scope（composite_scope 单一来源）
            for rule in composite_scope(operation) {
                if let CompositeScope::JobKind { kind, id_field, requires, message } = rule {
                    let id = match id_field {
                        JobIdField::JobId => input["job_id"].clone(),
                        JobIdField::Id => input["id"].clone(),
                    };
                    let job = self.job_get(json!({"member_id":self.member(&input)?,"id":id}))?;
                    if job["kind"] == json!(kind)
                        && !requires
                            .iter()
                            .all(|op| grant["operations"].as_array().unwrap().contains(&json!(op)))
                    {
                        return Err(ApiError::new("scope_denied", message));
                    }
                }
            }
        }
        if operation == "stylist.ask" {
            self.check_deferred_grant(string(&grant, "id")?, operation, &self.member(&input)?)?;
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
                    && (grant["member_ids"].is_null()
                        || !["sync.configure", "sync.run", "sync.process", "sync.status"]
                            .contains(&o["name"].as_str().unwrap()))
                    && (grant["mode"] == "full"
                        || o["name"] == "capabilities"
                        || grant["operations"].as_array().unwrap().contains(&o["name"]))
            });
            return Ok(result);
        }
        if operation == "stylist.ask" {
            return self.stylist_enqueue(
                input,
                crate::stylist::StylistOrigin::Client {
                    grant_id: string(&grant, "id")?.to_owned(),
                },
            );
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
        json!({"name":"client.authorize","description":"仅可信本机入口登记客户端授权，一次性返回连接令牌","examples":[{"request":{"name":"<1-120字客户端名称>","mode":"limited","operations":["recommendation.generate"],"member_ids":null,"expires_at":null},"prerequisites":["仅可信本机入口可用；外部客户端不能自我授权","完整授权operations须为空数组；限定授权至少一个已知非特权操作","令牌仅在本次响应中一次性显示，不落普通配置或日志"]}],"inputSchema":{"type":"object","required":["name","mode","operations","member_ids","expires_at"],"properties":{"name":{"type":"string"},"mode":{"enum":["full","limited"]},"operations":{"type":"array","items":{"type":"string"}},"member_ids":{"type":["array","null"],"items":{"type":"string"}},"expires_at":{"type":["string","null"]}},"additionalProperties":false},"outputSchema":issued_schema}),
        json!({"name":"client.list","description":"仅可信本机入口列出客户端，无连接令牌或hash","examples":[{"request":{},"prerequisites":["列出全部客户端授权记录；不含连接令牌或令牌hash"]}],"inputSchema":{"type":"object","properties":{},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","items"],"properties":{"schema_version":{"type":"integer","const":1},"items":{"type":"array","items":grant_schema()}}}}),
        json!({"name":"client.revoke","description":"仅可信本机入口撤销客户端授权","examples":[{"request":{"id":"<client.list返回的授权ID>"},"prerequisites":["撤销后该客户端令牌立即失效","授权记录保留revoked状态与撤销时间"]}],"inputSchema":{"type":"object","required":["id"],"properties":{"id":{"type":"string"}},"additionalProperties":false},"outputSchema":grant_schema()}),
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
