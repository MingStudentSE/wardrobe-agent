//! Ticket-22 首片：Stylist 问答的落库排队接缝。只做校验、安全快照与共享任务持久化；
//! dispatch（stylist.ask → stylist_enqueue）与只读工具环工作线程由根在后续接线，
//! 本模块不实现任何工作线程、不发起网络请求、不读取凭据。
use super::StylistOrigin;
use crate::{jobs, string, ApiError, Application};
use serde_json::{json, Value};

fn invalid(message: &str) -> ApiError {
    ApiError::new("invalid_input", message)
}

/// 输入字段闭合校验；必填性由下方逐字段读取兜底。
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    let object = input.as_object().ok_or_else(|| invalid("输入须为对象"))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("着装问答包含不支持的字段"));
    }
    Ok(())
}

impl Application {
    /// 校验并排队一次 Stylist 问答：真实成员、非空幂等键（1–200字节）、
    /// 去除首尾空白后非空且不超过4000字符的问题、有效IANA时区，
    /// 以及一个已存储且具备文本能力的 openai_compatible 模型配置。
    /// 只读取 providers 表文档做安全快照（表内不含原始密钥，绝不附加原始令牌），
    /// 不调用 provider.validate、不发起网络请求。origin 必填、随载荷不可变地持久化；
    /// 重放同一 (member, kind, operation_key) 返回已存任务的真实状态。
    pub(crate) fn stylist_enqueue(
        &self,
        input: Value,
        origin: StylistOrigin,
    ) -> Result<Value, ApiError> {
        fields(
            &input,
            &[
                "member_id",
                "operation_key",
                "provider_id",
                "question",
                "timezone",
            ],
        )?;
        let member = self.member(&input)?;
        let key = string(&input, "operation_key")?;
        if key.trim().is_empty() || key.len() > 200 {
            return Err(invalid("操作标识须为1–200字节"));
        }
        let question = string(&input, "question")?.trim();
        if question.is_empty() || question.chars().count() > 4000 {
            return Err(invalid("问题去除首尾空白后须为1–4000字符"));
        }
        let timezone = string(&input, "timezone")?;
        timezone
            .parse::<chrono_tz::Tz>()
            .map_err(|_| invalid("时区须为有效IANA名称"))?;
        let provider_id = string(&input, "provider_id")?;
        let provider = self.provider_get(provider_id)?;
        if provider["adapter"] != "openai_compatible" {
            return Err(ApiError::new(
                "unsupported_capability",
                "需要配置兼容聊天接口的文本模型",
            ));
        }
        let capabilities = provider["capabilities"]
            .as_array()
            .ok_or_else(|| ApiError::new("storage_error", "模型配置损坏"))?;
        if !capabilities.contains(&json!("text")) {
            return Err(ApiError::new(
                "unsupported_capability",
                "需要配置兼容聊天接口的文本模型",
            ));
        }
        let origin_value = serde_json::to_value(&origin)
            .map_err(|_| ApiError::new("storage_error", "来源记录失败"))?;
        let payload = json!({
            "schema_version": 1,
            "member_id": &member,
            "provider_id": provider_id,
            "question": question,
            "timezone": timezone,
            "provider_binding": provider,
            "origin": origin_value,
        });
        let job = self.job_enqueue(&member, "stylist_query", key, payload)?;
        Ok(json!({
            "schema_version": 1,
            "member_id": &member,
            "job_id": job["id"].clone(),
            "state": job["state"].clone(),
        }))
    }
}

/// 供根接线汇总的描述符：stylist.ask 与 stylist.worker.run 的闭合 JSON Schema。
/// 只描述排队的只读首轮工具环（不写 WearEvents）；无任何工作线程实现。
pub(super) fn descriptors() -> Vec<Value> {
    let receipt = json!({"type":"object","required":["schema_version","member_id","job_id","state"],"additionalProperties":false,"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"job_id":{"type":"string"},"state":{"enum":["queued","running","waiting_input","succeeded","failed","cancelled"]}}});
    let job_output = jobs::output_schema();
    vec![
        json!({"name":"stylist.ask","description":"排队一次只读衣物工具环问答：先写本机持久任务并立即返回回执，再由本机任务调用已配置的文本模型；工具环只读，不写 WearEvents","inputSchema":{"type":"object","required":["member_id","operation_key","provider_id","question","timezone"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"operation_key":{"type":"string","minLength":1,"maxLength":200,"description":"幂等标识，1–200字节"},"provider_id":{"type":"string"},"question":{"type":"string","minLength":1,"maxLength":4000,"description":"自然语言问题，去除首尾空白后须为1–4000字符"},"timezone":{"type":"string","description":"有效IANA时区名称，例如Asia/Shanghai"}}},"outputSchema":receipt}),
        json!({"name":"stylist.worker.run","description":"本机驱动一次已排队的只读工具环问答任务，返回任务对象的当前状态","inputSchema":{"type":"object","required":["member_id","id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}},"outputSchema":job_output}),
    ]
}
