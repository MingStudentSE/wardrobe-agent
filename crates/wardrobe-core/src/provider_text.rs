//! Shared bounded transport for text-provider requests. Submission ledgers and
//! retries belong to the calling durable job, before each actual POST.
use crate::{ApiError, Application};
use serde_json::{json, Value};
use std::io::Read;

/// 带记录能力的文本模型请求：调用方只负责构造与提交；每次真实发送的
/// provider_request 事件（trace、结果类别、HTTP 状态、耗时）由 `send` 统一记入，
/// 调用点不再手工组装台账，也不再知道端点类别词汇。
pub(crate) struct ProviderTextRequest {
    request: reqwest::blocking::RequestBuilder,
}

impl ProviderTextRequest {
    /// 一次真实发送：计时重置 → POST → 按结果类别（ok/http_error/outcome_unknown）
    /// 记一条事件。网络失败按「结果未知」返回（http 状态不可知），是否重发、
    /// 如何入账由调用方的持久任务台账处置。
    pub(crate) fn send(
        &self,
        app: &Application,
        log: &crate::diagnostics::ProviderRequestLog,
    ) -> Result<reqwest::blocking::Response, ApiError> {
        log.begin_send();
        let response = match self.request.try_clone() {
            Some(request) => request.send(),
            None => return Err(ApiError::new("provider_error", "无法建立模型请求")),
        };
        match response {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    log.record(app, "chat_completions", "ok", None);
                } else {
                    log.record(app, "chat_completions", "http_error", Some(status.as_u16()));
                }
                Ok(response)
            }
            Err(_) => {
                log.record(app, "chat_completions", "outcome_unknown", None);
                Err(ApiError::new(
                    "provider_network",
                    "模型连接失败，结果未知，不盲重发",
                ))
            }
        }
    }
}

impl Application {
    pub(crate) fn provider_text_request(
        &self,
        provider: &Value,
        body: &Value,
    ) -> Result<ProviderTextRequest, ApiError> {
        if provider["adapter"] != "openai_compatible"
            || !provider["capabilities"]
                .as_array()
                .is_some_and(|values| values.contains(&json!("text")))
        {
            return Err(ApiError::new(
                "unsupported_capability",
                "需要配置兼容聊天接口的文本模型",
            ));
        }
        let client = crate::http_client::build(crate::http_client::OutboundProfile::Chat)
            .map_err(|_| ApiError::new("provider_network", "模型连接不可用"))?;
        let endpoint = provider["endpoint"]
            .as_str()
            .ok_or_else(|| ApiError::new("storage_error", "模型地址损坏"))?;
        let mut request = client
            .post(format!("{endpoint}/chat/completions"))
            .json(body);
        if provider["authentication"] == "bearer" {
            let key = super::credential_entry_for(provider)?
                .get_password()
                .map_err(|_| ApiError::new("credential_missing", "文本模型凭据不可用"))?;
            request = request.bearer_auth(key);
        }
        Ok(ProviderTextRequest { request })
    }
}

pub(crate) fn read_chat_response(response: reqwest::blocking::Response) -> Result<Value, ApiError> {
    let mut bytes = Vec::new();
    response
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|_| ApiError::new("provider_response", "模型响应读取失败"))?;
    if bytes.len() > 1_048_576 {
        return Err(ApiError::new("provider_response", "模型响应过大"));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::new("provider_response", "模型响应不是JSON"))
}
