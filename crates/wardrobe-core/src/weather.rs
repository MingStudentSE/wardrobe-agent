use crate::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::io::Read;
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS weather_preferences(member_id TEXT PRIMARY KEY REFERENCES members(id),document TEXT NOT NULL);")?;
    Ok(())
}
fn bad(s: &str) -> ApiError {
    ApiError::new("invalid_input", s)
}
fn fields(v: &Value, keys: &[&str]) -> Result<(), ApiError> {
    if v.as_object()
        .is_none_or(|m| m.keys().any(|k| !keys.contains(&k.as_str())))
    {
        return Err(bad("天气包含不支持字段"));
    }
    Ok(())
}
fn present(mut v: Value) -> Value {
    if let Some(time) = v["cache"]["obtained_at"].as_str() {
        v["cache"]["stale"] = json!(chrono::DateTime::parse_from_rfc3339(time)
            .map(|t| chrono::Utc::now().signed_duration_since(t) > chrono::Duration::hours(3))
            .unwrap_or(true));
    }
    v
}
impl Application {
    pub(super) fn weather_execute(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        if op == "weather.configure" {
            fields(
                &input,
                &[
                    "member_id",
                    "mode",
                    "location",
                    "latitude",
                    "longitude",
                    "endpoint",
                    "usage",
                ],
            )?;
            let mode = string(&input, "mode")?;
            let config = match mode {
                "off" => {
                    if input.as_object().unwrap().len() != 2 {
                        return Err(bad("关闭天气时只提供成员与模式"));
                    }
                    json!({"mode":"off"})
                }
                "automatic" => {
                    let location = string(&input, "location")?;
                    if location.trim().is_empty() || location.chars().count() > 120 {
                        return Err(bad("请输入120字内地点名称"));
                    }
                    let lat = input["latitude"]
                        .as_f64()
                        .filter(|x| x.is_finite() && (-90.0..=90.0).contains(x))
                        .ok_or_else(|| bad("纬度须为-90至90"))?;
                    let lon = input["longitude"]
                        .as_f64()
                        .filter(|x| x.is_finite() && (-180.0..=180.0).contains(x))
                        .ok_or_else(|| bad("经度须为-180至180"))?;
                    if string(&input, "usage")? != "noncommercial" {
                        return Err(bad(
                            "免费Open-Meteo仅非商业用途；商业部署须配置获授权服务后再接入",
                        ));
                    }
                    let endpoint = input
                        .get("endpoint")
                        .and_then(Value::as_str)
                        .unwrap_or("https://api.open-meteo.com/v1/forecast");
                    let url = reqwest::Url::parse(endpoint).map_err(|_| bad("天气端点须为URL"))?;
                    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
                    if (!local && url.scheme() != "https")
                        || !matches!(url.scheme(), "https" | "http")
                        || !url.username().is_empty()
                        || url.password().is_some()
                        || url.query().is_some()
                        || url.fragment().is_some()
                    {
                        return Err(bad("天气端点须HTTPS或本机HTTP，不含凭据或查询"));
                    }
                    json!({"mode":mode,"location":location,"latitude":lat,"longitude":lon,"endpoint":endpoint,"usage":"noncommercial","attribution":"Weather data by Open-Meteo · CC BY 4.0","source_url":"https://open-meteo.com/"})
                }
                _ => return Err(bad("天气可关闭或配置自动获取")),
            };
            let tx = self.begin_write()?;
            let doc = json!({"schema_version":1,"member_id":member,"config":config,"cache":null,"last_error":null});
            tx.execute("INSERT INTO weather_preferences VALUES(?1,?2) ON CONFLICT(member_id) DO UPDATE SET document=excluded.document",params![member,doc.to_string()])?;
            tx.commit()?;
            return Ok(doc);
        }
        fields(&input, &["member_id"])?;
        let original: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM weather_preferences WHERE member_id=?1",
                [&member],
                |r| r.get(0),
            )
            .optional()?;
        let mut doc=original.as_ref().map(|s|serde_json::from_str(s).map_err(|_|ApiError::new("storage_error","天气配置损坏"))).transpose()?.unwrap_or(json!({"schema_version":1,"member_id":member,"config":{"mode":"off"},"cache":null,"last_error":null}));
        if op == "weather.get" || doc["config"]["mode"] == "off" {
            return Ok(present(doc));
        }
        if op != "weather.refresh" {
            return Err(ApiError::new("unsupported_operation", "天气操作不可用"));
        }
        let config = doc["config"].clone();
        let result = (|| -> Result<Value, ApiError> {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| ApiError::new("weather_network", "无法建立天气连接"))?;
            let response = client
                .get(config["endpoint"].as_str().unwrap())
                .query(&[
                    ("latitude", config["latitude"].to_string()),
                    ("longitude", config["longitude"].to_string()),
                    ("current", "temperature_2m".to_owned()),
                    ("timeformat", "unixtime".to_owned()),
                    ("timezone", "GMT".to_owned()),
                ])
                .send()
                .map_err(|_| {
                    ApiError::new("weather_network", "天气连接失败，可使用缓存或手动温度")
                })?;
            if !response.status().is_success() {
                return Err(ApiError::new(
                    "weather_provider",
                    "天气服务返回错误，可继续规则推荐",
                ));
            }
            let mut bytes = vec![];
            response
                .take(65537)
                .read_to_end(&mut bytes)
                .map_err(|_| ApiError::new("weather_response", "天气响应读取失败"))?;
            if bytes.len() > 65536 {
                return Err(ApiError::new("weather_response", "天气响应过大"));
            }
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| ApiError::new("weather_response", "天气响应不是JSON"))?;
            let temp = value["current"]["temperature_2m"]
                .as_f64()
                .filter(|t| t.is_finite() && (-80.0..=60.0).contains(t))
                .ok_or_else(|| ApiError::new("weather_response", "天气温度不可用"))?;
            if value["current_units"]["temperature_2m"] != "°C"
                || value["current_units"]["time"] != "unixtime"
            {
                return Err(ApiError::new("weather_response", "天气单位不匹配"));
            }
            let ts = value["current"]["time"]
                .as_i64()
                .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                .ok_or_else(|| ApiError::new("weather_response", "天气模型时间不可用"))?;
            Ok(
                json!({"temperature_c":temp,"location":config["location"],"obtained_at":chrono::Utc::now().to_rfc3339(),"valid_time":ts.to_rfc3339(),"source":"open_meteo_model","stale":false,"attribution":config["attribution"],"source_url":config["source_url"]}),
            )
        })();
        match result {
            Ok(cache) => {
                doc["cache"] = cache;
                doc["last_error"] = Value::Null;
            }
            Err(error) => {
                doc["last_error"] = json!({"code":error.code,"message":error.message,"occurred_at":chrono::Utc::now().to_rfc3339()});
            }
        }
        let tx = self.begin_write()?;
        let changed = tx.execute(
            "UPDATE weather_preferences SET document=?1 WHERE member_id=?2 AND document=?3",
            params![doc.to_string(), member, original],
        )?;
        tx.commit()?;
        if changed == 0 {
            return self.weather_execute("weather.get", json!({"member_id":member}));
        }
        Ok(present(doc))
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    ["configure","get","refresh"].into_iter().map(|op|{let mut properties=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];if op=="configure"{properties["mode"]=json!({"enum":["off","automatic"]});required.push("mode");properties["location"]=json!({"type":"string","maxLength":120});properties["latitude"]=json!({"type":"number","minimum":-90,"maximum":90});properties["longitude"]=json!({"type":"number","minimum":-180,"maximum":180});properties["endpoint"]=json!({"type":"string"});properties["usage"]=json!({"const":"noncommercial"});}json!({"name":format!("weather.{op}"),"description":"明确配置地点的天气估计；失败不阻断手工/离线搭配；免费服务仅非商业","inputSchema":{"type":"object","required":required,"properties":properties,"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id","config","cache","last_error"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"config":{"type":"object"},"cache":{},"last_error":{}}}})}).collect()
}
