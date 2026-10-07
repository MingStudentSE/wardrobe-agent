//! Shared bounded transport for text-provider requests. Submission ledgers and
//! retries belong to the calling durable job, before each actual POST.
use crate::{ApiError, Application};
use serde_json::{json, Value};
use std::io::Read;

impl Application {
    pub(crate) fn provider_text_request(
        &self,
        provider: &Value,
        body: &Value,
    ) -> Result<reqwest::blocking::RequestBuilder, ApiError> {
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
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
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
        Ok(request)
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
