use crate::{string, ApiError, Application};
use rusqlite::params;
use serde_json::{json, Value};
use uuid::Uuid;
const CAPABILITIES: &[&str] = &["text", "vision"];
impl Application {
    fn credential_entry(&self, id: &str) -> Result<keyring::Entry, ApiError> {
        self.provider_get(id)?;
        keyring::Entry::new("wardrobe-agent.provider", id).map_err(|_| credential_error())
    }
    pub(crate) fn provider_credential(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        fields(
            &input,
            if operation == "provider.credential.set" {
                &["id", "api_key"]
            } else {
                &["id"]
            },
        )?;
        let id = string(&input, "id")?;
        let entry = self.credential_entry(id)?;
        match operation {
            "provider.credential.set" => {
                let key = string(&input, "api_key")?;
                if key.is_empty() || key.len() > 8192 || key.contains(['\r', '\n']) {
                    return Err(ApiError::new("invalid_input", "凭据为空、过长或包含换行"));
                }
                entry.set_password(key).map_err(|_| credential_error())?;
            }
            "provider.credential.remove" => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(_) => return Err(credential_error()),
            },
            _ => {}
        }
        let configured = match entry.get_password() {
            Ok(_) => true,
            Err(keyring::Error::NoEntry) => false,
            Err(_) => return Err(credential_error()),
        };
        Ok(json!({"schema_version":1,"id":id,"configured":configured}))
    }
    pub(crate) fn provider_validate(&self, input: Value) -> Result<Value, ApiError> {
        use std::io::Read;
        fields(&input, &["id"])?;
        let id = string(&input, "id")?;
        let p = self.provider_get(id)?;
        if !p["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("text"))
        {
            return Err(ApiError::new(
                "unsupported_capability",
                "连接验证需要文本能力；视觉验证在真实图片任务中单独记录",
            ));
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ApiError::new("connection_error", "无法建立连接客户端"))?;
        let mut request=client.post(format!("{}/chat/completions",p["endpoint"].as_str().unwrap())).json(&json!({"model":p["model"],"messages":[{"role":"user","content":"Reply OK."}],"max_tokens":1}));
        if p["authentication"] == "bearer" {
            let key = match self.credential_entry(id)?.get_password() {
                Ok(key) => key,
                Err(keyring::Error::NoEntry) => {
                    return Err(ApiError::new("credential_missing", "请先配置系统安全凭据"))
                }
                Err(_) => return Err(credential_error()),
            };
            request = request.bearer_auth(key);
        }
        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                ApiError::new("connection_timeout", "连接超时，请检查端点或网络")
            } else {
                ApiError::new("connection_error", "连接失败，请检查端点、证书或网络")
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => ApiError::new(
                    "provider_authentication",
                    "供应商拒绝认证，请检查凭据与权限",
                ),
                404 => ApiError::new(
                    "provider_model_or_endpoint",
                    "模型或接口不存在，请检查端点与模型",
                ),
                429 => ApiError::new(
                    "provider_rate_limit",
                    "供应商限流或额度不足，请稍后重试或检查供应商账户",
                ),
                300..=399 => ApiError::new(
                    "provider_redirect",
                    "端点发生重定向，未发送凭据到新地址；请配置最终端点",
                ),
                _ => ApiError::new(
                    "provider_error",
                    "供应商返回调用错误，请检查能力与供应商状态",
                ),
            });
        }
        let mut body = Vec::new();
        response
            .take(1_048_577)
            .read_to_end(&mut body)
            .map_err(|_| ApiError::new("provider_response", "无法读取供应商响应"))?;
        if body.len() > 1_048_576 {
            return Err(ApiError::new("provider_response", "供应商响应过大"));
        }
        let result: Value = serde_json::from_slice(&body)
            .map_err(|_| ApiError::new("provider_response", "供应商未返回兼容JSON响应"))?;
        if !result["choices"][0]["message"]["content"].is_string() {
            return Err(ApiError::new("provider_response", "供应商响应缺少文本结果"));
        }
        let mut usage = json!({});
        for field in ["prompt_tokens", "completion_tokens", "total_tokens"] {
            if let Some(count) = result["usage"][field].as_u64() {
                usage[field] = json!(count);
            }
        }
        Ok(
            json!({"schema_version":1,"id":id,"verified_capabilities":["text"],"verified_at":chrono::Utc::now().to_rfc3339(),"usage":usage}),
        )
    }
    pub(crate) fn provider_save(&self, input: Value) -> Result<Value, ApiError> {
        fields(
            &input,
            &[
                "id",
                "name",
                "endpoint",
                "model",
                "capabilities",
                "authentication",
            ],
        )?;
        let name = text(&input, "name", 120)?;
        let model = text(&input, "model", 200)?;
        let endpoint = string(&input, "endpoint")?.trim().trim_end_matches('/');
        let url = reqwest::Url::parse(endpoint)
            .map_err(|_| ApiError::new("invalid_input", "端点必须是有效URL"))?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !(url.scheme() == "https" || (url.scheme() == "http" && local))
        {
            return Err(ApiError::new(
                "invalid_input",
                "端点需使用HTTPS，本机可用HTTP；不能包含凭据、查询参数或片段",
            ));
        }
        let authentication = string(&input, "authentication")?;
        if !["none", "bearer"].contains(&authentication) {
            return Err(ApiError::new("invalid_input", "认证方式必须是none或bearer"));
        }
        let capabilities = input["capabilities"]
            .as_array()
            .ok_or_else(|| ApiError::new("invalid_input", "能力必须是数组"))?;
        if capabilities.is_empty()
            || capabilities
                .iter()
                .any(|v| !CAPABILITIES.contains(&v.as_str().unwrap_or("")))
        {
            return Err(ApiError::new(
                "unsupported_capability",
                "当前兼容适配器支持文本和视觉，图片生成/试穿须使用专用适配器",
            ));
        }
        let id = match input.get("id") {
            Some(v) => {
                let id = v
                    .as_str()
                    .ok_or_else(|| ApiError::new("invalid_input", "id必须是字符串"))?;
                self.provider_get(id)?;
                id.to_string()
            }
            None => Uuid::now_v7().to_string(),
        };
        let existing = self.provider_get(&id).ok();
        let now = chrono::Utc::now().to_rfc3339();
        let doc = json!({"schema_version":1,"id":id,"name":name,"endpoint":endpoint,"model":model,"capabilities":capabilities,"adapter":"openai_compatible","authentication":authentication,"created_at":existing.as_ref().map(|v|v["created_at"].clone()).unwrap_or(json!(now)),"updated_at":now});
        let tx=self.begin_write()?;
        tx.execute("INSERT INTO providers(id,document) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET document=excluded.document",params![id,doc.to_string()])?;
        tx.commit()?;
        Ok(doc)
    }
    pub(crate) fn provider_get(&self, id: &str) -> Result<Value, ApiError> {
        let serialized = self
            .connection
            .query_row("SELECT document FROM providers WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    ApiError::new("not_found", "模型配置不存在")
                }
                _ => e.into(),
            })?;
        serde_json::from_str(&serialized)
            .map_err(|_| ApiError::new("storage_error", "模型配置损坏"))
    }
    pub(crate) fn provider_list(&self, input: Value) -> Result<Value, ApiError> {
        fields(&input, &[])?;
        let mut s = self
            .connection
            .prepare("SELECT document FROM providers ORDER BY id")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        let mut items = vec![];
        for row in rows {
            items.push(
                serde_json::from_str::<Value>(&row?)
                    .map_err(|_| ApiError::new("storage_error", "模型配置损坏"))?,
            );
        }
        Ok(json!({"schema_version":1,"items":items}))
    }
}
fn credential_error() -> ApiError {
    ApiError::new(
        "credential_store_unavailable",
        "系统安全凭据存储不可用，请解锁钥匙串或Secret Service；不会改存明文",
    )
}
fn text<'a>(input: &'a Value, key: &str, max: usize) -> Result<&'a str, ApiError> {
    let v = string(input, key)?.trim();
    if v.is_empty() || v.chars().count() > max {
        return Err(ApiError::new("invalid_input", "名称或模型长度无效"));
    }
    Ok(v)
}
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .unwrap()
        .keys()
        .any(|k| !allowed.contains(&k.as_str()))
    {
        return Err(ApiError::new("invalid_input", "模型操作包含不支持的字段"));
    }
    Ok(())
}
pub(crate) fn descriptors() -> Vec<Value> {
    let mut result:Vec<Value> = vec![("provider.save","保存模型端点和能力",json!({"type":"object","required":["name","endpoint","model","capabilities","authentication"],"properties":{"id":{"type":"string"},"name":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"capabilities":{"type":"array","items":{"enum":CAPABILITIES}},"authentication":{"enum":["none","bearer"]}},"additionalProperties":false})),("provider.list","读取模型配置，不包含凭据",json!({"type":"object","properties":{},"additionalProperties":false}))].into_iter().map(|(name,description,input)|json!({"name":name,"description":description,"inputSchema":input,"outputSchema":{"type":"object","required":["schema_version"],"properties":{"schema_version":{"const":1,"type":"integer"}}}})).collect();
    result.push(json!({"name":"provider.validate","description":"显式调用配置的文本模型验证连接，会使用供应商额度","inputSchema":{"type":"object","required":["id"],"properties":{"id":{"type":"string"}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","id","verified_capabilities","verified_at","usage"],"properties":{"schema_version":{"const":1,"type":"integer"},"id":{"type":"string"},"verified_capabilities":{"type":"array","items":{"const":"text"}},"verified_at":{"type":"string"},"usage":{"type":"object"}}}}));
    for name in [
        "provider.credential.set",
        "provider.credential.remove",
        "provider.credential.status",
    ] {
        result.push(json!({"name":name,"description":"管理系统安全凭据，只返回配置状态","inputSchema":{"type":"object","required":if name.ends_with("set"){vec!["id","api_key"]}else{vec!["id"]},"properties":{"id":{"type":"string"},"api_key":{"type":"string","writeOnly":true}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","id","configured"],"properties":{"schema_version":{"const":1,"type":"integer"},"id":{"type":"string"},"configured":{"type":"boolean"}}}}));
    }
    result
}
