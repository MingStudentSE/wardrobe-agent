//! S3-compatible SyncStorage 适配器（AWS Signature V4，无 SDK 依赖）。
//!
//! 契约要点（与 LocalFolder 同一 `SyncStorage` 语义）：
//! - ListObjectsV2 `start-after` 分页：游标是单调递增的对象键，与本地文件夹一致。
//! - `If-None-Match: *` 实现 CreateOnly；`If-Match: <etag>` 实现 MatchVersion。
//!   能力在适配器构造时探测：服务器不执行条件写（返回 200 而不是 412）或
//!   不支持（501/405）时以 `storage_unsupported` 明确拒绝，绝不静默改语义。
//! - ETag 只作条件写协调的版本标识，绝不当内容 hash；完整性由同步协议的
//!   sha256 摘要（对象→marker 两步发布 + 读取校验）承担。
//! - 条件删除：S3 生态不统一（AWS 支持 If-Match DELETE，MinIO 静默忽略）。
//!   构造时探测：支持则按版本条件删除（412→storage_precondition），
//!   不支持则明确拒绝（storage_unsupported），两者都不会移除新发布的版本。
//! - 单对象上限 2MB（与 LocalFolder 一致）；响应体短于声明的 Content-Length
//!   视为传输损坏（读取期报错，部分字节不可见）。
//! - 错误映射：401/403→storage_auth；404→storage_not_found（条件写中
//!   指向已不存在的版本时→storage_precondition）；412→storage_precondition；
//!   501/405→storage_unsupported；5xx/网络/超时→storage_transient（首试 +
//!   最多 2 次自动重试，遵守 Retry-After）。
use super::storage::{
    valid_object_key, ObjectMeta, Page, StorageCapabilities, SyncStorage, WriteCondition,
};
use crate::ApiError;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;

/// 单对象大小上限，与 LocalFolder 一致。
const MAX_OBJECT: u64 = 2 * 1024 * 1024;
/// 重试预算：首次请求 + 最多 2 次自动重试（父规格）。
const MAX_ATTEMPTS: u32 = 3;
/// 单次重试等待上限（遵守 Retry-After，但为同步批次设界）。
const MAX_RETRY_WAIT: u64 = 30;

#[derive(Clone, Debug)]
pub struct S3Target {
    /// 形如 `http(s)://host:port` 的端点（不含 bucket）。
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    /// 适配器 key 根前缀（可为空）。领域键相对该前缀，切换后端不改领域键。
    pub prefix: String,
    pub path_style: bool,
}

#[derive(Clone)]
pub struct S3Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
}

// 凭据绝不进入日志/调试输出：secret 一律脱敏为固定占位。
impl std::fmt::Debug for S3Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Credentials")
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct S3Objects {
    base: reqwest::Url,
    target: S3Target,
    credentials: Arc<S3Credentials>,
    http: reqwest::blocking::Client,
    /// 探测得到的条件删除能力（true：DELETE 支持 If-Match）。
    conditional_delete: Arc<std::sync::atomic::AtomicBool>,
}

fn err(code: &str) -> ApiError {
    ApiError::new(code, "同步存储操作失败，保留本地数据后重试")
}

struct Request {
    method: &'static str,
    host: String,
    /// 已按段百分号编码的对象路径（含 bucket，path-style 时）。
    path: String,
    /// 未编码的查询参数（发送与签名前统一编码、排序）。
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// 只读响应体包装：强制 2MB 上限；提前 EOF（截断）报损坏而不是短读。
struct VerifiedBody {
    response: reqwest::blocking::Response,
    remaining: Option<u64>,
    total: u64,
}

impl Read for VerifiedBody {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.total > MAX_OBJECT {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "对象超过大小上限",
            ));
        }
        if let Some(remaining) = self.remaining {
            if remaining == 0 {
                return Ok(0);
            }
            let cap = out.len().min(remaining.min(usize::MAX as u64) as usize);
            let n = self.response.read(&mut out[..cap])?;
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "对象在传输中被截断",
                ));
            }
            self.remaining = Some(remaining - n as u64);
            self.total += n as u64;
            return Ok(n);
        }
        let n = self.response.read(out)?;
        self.total += n as u64;
        Ok(n)
    }
}

impl S3Objects {
    /// 构造并做真实能力探测（条件写必须真实可用，否则明确拒绝）。
    pub fn new(target: S3Target, credentials: S3Credentials) -> Result<Self, ApiError> {
        let target = normalize_target(target)?;
        let base = reqwest::Url::parse(&target.endpoint)
            .map_err(|_| ApiError::new("invalid_input", "对象存储端点必须是合法的 http(s) 地址"))?;
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            // 重定向意味着 endpoint/region 配置不符；按显式错误返回而不是跟随。
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| err("storage_transient"))?;
        let adapter = Self {
            base,
            target,
            credentials: Arc::new(credentials),
            http,
            conditional_delete: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        adapter.probe()?;
        Ok(adapter)
    }

    /// (签名与请求使用的主机头, 已编码路径)。
    fn object_location(&self, key: &str) -> (String, String) {
        let host = self.base.host_str().unwrap_or_default().to_string();
        let authority = match self.base.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.clone(),
        };
        if self.target.path_style {
            (authority, format!("/{}/{}", self.target.bucket, key))
        } else {
            (
                format!("{}.{}", self.target.bucket, authority),
                format!("/{key}"),
            )
        }
    }

    /// 对象路径段编码：保留 `/`，其余按 RFC3986 百分号编码（UTF-8 字节）。
    fn encode_component(value: &str, keep_slash: bool) -> String {
        let mut out = String::with_capacity(value.len());
        for byte in value.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(byte as char)
                }
                b'/' if keep_slash => out.push('/'),
                other => out.push_str(&format!("%{other:02X}")),
            }
        }
        out
    }

    /// 领域键 → 存储键（挂在适配器根前缀下）。
    fn full_key(&self, key: &str) -> Result<String, ApiError> {
        if !valid_object_key(key) {
            return Err(err("invalid_object_key"));
        }
        if self.target.prefix.is_empty() {
            Ok(key.to_string())
        } else {
            Ok(format!("{}/{}", self.target.prefix, key))
        }
    }

    fn relative_key(&self, full: &str) -> String {
        if self.target.prefix.is_empty() {
            full.to_string()
        } else {
            full.strip_prefix(&format!("{}/", self.target.prefix))
                .unwrap_or(full)
                .to_string()
        }
    }

    /// 发送签名请求；网络错误与 5xx 在预算内自动重试（遵守 Retry-After）。
    fn send(&self, request: Request) -> Result<reqwest::blocking::Response, ApiError> {
        let mut query = request.query.clone();
        query.sort();
        let canonical_query = query
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    Self::encode_component(k, false),
                    Self::encode_component(v, false)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let encoded_path = Self::encode_component(&request.path, true);
        let url = if canonical_query.is_empty() {
            format!("{}://{}{}", self.base.scheme(), request.host, encoded_path)
        } else {
            format!(
                "{}://{}{}?{}",
                self.base.scheme(),
                request.host,
                encoded_path,
                canonical_query
            )
        };
        let url: reqwest::Url = url
            .parse()
            .map_err(|_| err("storage_transient"))?;
        let (datestamp, amzdate) = amz_dates();
        let payload_hash = hex(&Sha256::digest(&request.body));
        let mut headers: Vec<(String, String)> = vec![
            ("host".into(), request.host.clone()),
            ("x-amz-content-sha256".into(), payload_hash.clone()),
            ("x-amz-date".into(), amzdate.clone()),
        ];
        // SigV4 要求头名一律小写并按小写序排序。
        headers = headers
            .into_iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v))
            .collect();
        headers.extend(
            request
                .headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.clone())),
        );
        headers.sort_by(|a, b| a.0.cmp(&b.0));
        let signed = headers
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>()
            .join(";");
        let canonical_headers = headers
            .iter()
            .map(|(k, v)| format!("{k}:{}\n", v.trim()))
            .collect::<String>();
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            request.method, encoded_path, canonical_query, canonical_headers, signed, payload_hash
        );
        let scope = format!("{datestamp}/{}/s3/aws4_request", self.target.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amzdate}\n{scope}\n{}",
            hex(&Sha256::digest(canonical_request.as_bytes()))
        );
        let signing_key = hmac_chain(&self.credentials.secret_access_key, &datestamp, &self.target.region);
        let mut mac = HmacSha256::new_from_slice(&signing_key).expect("hmac key");
        mac.update(string_to_sign.as_bytes());
        let signature = hex(&mac.finalize().into_bytes());
        if std::env::var("WARDROBE_S3_SIGN_DEBUG").is_ok() {
        eprintln!(
            "S3SIGN canonical_request=<<\n{}\n>> string_to_sign=<<\n{}\n>> signature={signature} url={url}",
            canonical_request, string_to_sign
        );
        }
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",
            self.credentials.access_key_id
        );
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let mut builder = self
                .http
                .request(
                    reqwest::Method::from_bytes(request.method.as_bytes())
                        .map_err(|_| err("storage_transient"))?,
                    url.clone(),
                )
                .header("x-amz-content-sha256", &payload_hash)
                .header("x-amz-date", &amzdate)
                .header(reqwest::header::AUTHORIZATION, &authorization);
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            if request.method == "PUT" || !request.body.is_empty() {
                builder = builder.body(request.body.clone());
            }
            match builder.send() {
                Err(error) if attempt < MAX_ATTEMPTS => {
                    std::thread::sleep(if error.is_timeout() {
                        Duration::from_millis(400)
                    } else {
                        Duration::from_millis(200)
                    });
                }
                Err(error) => {
                    let message = if error.is_timeout() {
                        "同步存储请求超时，保留本地数据后重试"
                    } else {
                        "同步存储网络不可达，保留本地数据后重试"
                    };
                    return Err(ApiError::new("storage_transient", message));
                }
                Ok(response) => {
                    let status = response.status();
                    if retryable_status(status) && attempt < MAX_ATTEMPTS {
                        let wait = retry_wait(&response);
                        drop(response);
                        std::thread::sleep(wait);
                    } else {
                        return Ok(response);
                    }
                }
            }
        }
    }

    /// 真实能力探测：条件写必须被服务器强制执行；条件删除按服务器实际
    /// 行为记录（不支持则领域调用被明确拒绝）。探测对象随后清理。
    fn probe(&self) -> Result<(), ApiError> {
        let key = self.full_key(&format!("wardrobe-probe/{}", uuid::Uuid::now_v7()))?;
        let body = b"wardrobe capability probe".to_vec();
        // 1. CreateOnly 首次发布必须成功。
        let (host, path) = self.object_location(&key);
        self.send(Request {
            method: "PUT",
            host,
            path,
            query: vec![],
            headers: vec![("If-None-Match".into(), "*".into())],
            body: body.clone(),
        })
        .and_then(|response| require_success(response, "put"))?;
        // 2. 重复 CreateOnly 必须被拒绝；服务器若返回 200 即忽略条件头，语义不可信。
        let (host, path) = self.object_location(&key);
        let duplicate = self.send(Request {
            method: "PUT",
            host,
            path,
            query: vec![],
            headers: vec![("If-None-Match".into(), "*".into())],
            body: b"must not overwrite".to_vec(),
        })?;
        if duplicate.status().as_u16() != 412 {
            return Err(ApiError::new(
                "storage_unsupported",
                "对象存储未强制条件写（If-None-Match），无法保证发布语义，已明确拒绝；请更换支持 S3 条件写的后端",
            ));
        }
        // 3. 条件删除探测：用陈旧版本尝试删除当前对象。
        let (host, path) = self.object_location(&key);
        let stale_delete = self.send(Request {
            method: "DELETE",
            host,
            path,
            query: vec![],
            headers: vec![(
                "If-Match".into(),
                "\"00000000000000000000000000000000\"".into(),
            )],
            body: vec![],
        })?;
        let (delete_body, delete_status) = read_body(stale_delete);
        self.conditional_delete.store(
            delete_status.as_u16() == 412
                || status_error_code(&delete_body).as_deref() == Some("PreconditionFailed"),
            std::sync::atomic::Ordering::Relaxed,
        );
        // 4. 无论探测结果如何清理探测对象。
        let (host, path) = self.object_location(&key);
        if let Ok(response) = self.send(Request {
            method: "DELETE",
            host,
            path,
            query: vec![],
            headers: vec![],
            body: vec![],
        }) {
            drop(response);
        }
        Ok(())
    }
}

fn require_success(
    response: reqwest::blocking::Response,
    purpose: &'static str,
) -> Result<(), ApiError> {
    if response.status().is_success() {
        return Ok(());
    }
    let (body, status) = read_body(response);
    Err(map_status(status, &body, purpose))
}

fn retryable_status(status: reqwest::StatusCode) -> bool {
    [429, 500, 502, 503, 504].contains(&status.as_u16())
}

fn retry_wait(response: &reqwest::blocking::Response) -> Duration {
    Duration::from_secs(
        response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0)
            .min(MAX_RETRY_WAIT)
            .max(0),
    )
}

fn read_body(response: reqwest::blocking::Response) -> (Vec<u8>, reqwest::StatusCode) {
    let status = response.status();
    let body = response.bytes().map(|b| b.to_vec()).unwrap_or_default();
    (body, status)
}

/// 从错误 XML 提取 S3 <Code>（不提取 <Message>，避免服务器回显敏感信息）。
fn status_error_code(body: &[u8]) -> Option<String> {
    let xml = std::str::from_utf8(body).ok()?;
    let start = xml.find("<Code>")? + "<Code>".len();
    let end = xml[start..].find("</Code>")? + start;
    Some(xml[start..end].to_string())
}

/// PUT 成功后提取 ETag 作为版本标识（仅用于条件写协调）。
fn etag_of_put(response: reqwest::blocking::Response) -> Result<String, ApiError> {
    if !response.status().is_success() {
        let (body, status) = read_body(response);
        return Err(map_status(status, &body, "put"));
    }
    response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim_matches('"').to_string())
        .ok_or_else(|| err("storage_corrupt"))
}

fn map_status(status: reqwest::StatusCode, body: &[u8], purpose: &str) -> ApiError {
    let server_code = status_error_code(body).unwrap_or_default();
    match status.as_u16() {
        401 | 403 => ApiError::new(
            "storage_auth",
            &format!("同步存储认证失败（{server_code}），请检查对象存储访问密钥后重新配置"),
        ),
        404 if purpose == "conditional-put" => err("storage_precondition"),
        404 => ApiError::new(
            "storage_not_found",
            &format!("同步存储对象或存储桶不存在（{server_code}）"),
        ),
        412 => err("storage_precondition"),
        405 | 501 => ApiError::new(
            "storage_unsupported",
            "对象存储不支持同步所需的条件写能力，已明确拒绝，不改变发布语义",
        ),
        500..=599 => ApiError::new("storage_transient", "同步存储暂时不可用，保留本地数据后重试"),
        300..=399 => ApiError::new(
            "storage_transient",
            "同步存储返回重定向，请检查 endpoint/region 配置",
        ),
        _ => ApiError::new(
            "storage_transient",
            &format!("同步存储拒绝请求（{} {server_code}）", status.as_u16()),
        ),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hmac_chain(secret: &str, datestamp: &str, region: &str) -> Vec<u8> {
    fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
    let date = hmac(format!("AWS4{secret}").as_bytes(), datestamp.as_bytes());
    let region = hmac(&date, region.as_bytes());
    let service = hmac(&region, b"s3");
    hmac(&service, b"aws4_request")
}

/// (datestamp=yyyymmdd, amzdate=yyyymmddThhmmssZ)
fn amz_dates() -> (String, String) {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = seconds.div_euclid(86400);
    let secs_of_day = seconds.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    let datestamp = format!("{y:04}{m:02}{d:02}");
    let time = format!(
        "{:02}{:02}{:02}",
        secs_of_day / 3600,
        secs_of_day / 60 % 60,
        secs_of_day % 60
    );
    (datestamp.clone(), format!("{datestamp}T{time}Z"))
}

fn normalize_target(target: S3Target) -> Result<S3Target, ApiError> {
    let endpoint = target.endpoint.trim().trim_end_matches('/').to_string();
    if reqwest::Url::parse(&endpoint).is_err() {
        return Err(ApiError::new(
            "invalid_input",
            "对象存储端点必须是合法的 http(s) 地址",
        ));
    }
    let bucket = target.bucket.trim().to_string();
    if bucket.is_empty()
        || bucket.len() > 63
        || !bucket
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
    {
        return Err(ApiError::new(
            "invalid_input",
            "对象存储 bucket 名不合法（小写字母、数字、点、连字符）",
        ));
    }
    let prefix = target.prefix.trim().trim_matches('/').to_string();
    if !prefix.is_empty() && !valid_object_key(&prefix) {
        return Err(err("invalid_object_key"));
    }
    let region = if target.region.trim().is_empty() {
        "us-east-1".to_string()
    } else {
        target.region.trim().to_string()
    };
    Ok(S3Target {
        endpoint,
        region,
        bucket,
        prefix,
        path_style: target.path_style,
    })
}

impl SyncStorage for S3Objects {
    fn capabilities(&self) -> StorageCapabilities {
        // 条件写已由构造期探测强制验证；单对象 PUT 与 head 的 If-Match CAS
        // 都是原子操作，发布标记不依赖 rename（#27 marker 可见性规则）。
        StorageCapabilities {
            conditional_write: true,
            atomic_publication: true,
        }
    }

    fn list(&self, prefix: &str, cursor: Option<&str>, limit: usize) -> Result<Page, ApiError> {
        if !(1..=1000).contains(&limit) {
            return Err(err("invalid_cursor"));
        }
        if !valid_object_key(prefix) {
            return Err(err("invalid_object_key"));
        }
        if let Some(c) = cursor {
            if !c.starts_with(&format!("{prefix}/")) || !valid_object_key(c) {
                return Err(err("invalid_cursor"));
            }
        }
        let full_prefix = self.full_key(prefix)?;
        let mut query = vec![
            ("encoding-type".into(), "url".into()),
            ("list-type".into(), "2".into()),
            ("max-keys".into(), limit.to_string()),
            ("prefix".into(), format!("{full_prefix}/")),
        ];
        if let Some(c) = cursor {
            query.push(("start-after".into(), self.full_key(c)?));
        }
        let (host, path) = self.object_location("");
        let response = self.send(Request {
            method: "GET",
            host,
            path,
            query,
            headers: vec![],
            body: vec![],
        })?;
        if !response.status().is_success() {
            let (body, status) = read_body(response);
            return Err(map_status(status, &body, "list"));
        }
        let body = response.bytes().map_err(|_| err("storage_transient"))?;
        let xml = String::from_utf8_lossy(&body).to_string();
        let truncated = xml.contains("<IsTruncated>true</IsTruncated>");
        let mut items = vec![];
        let mut rest = xml.as_str();
        let mut last_key = None;
        while let Some(position) = rest.find("<Contents>") {
            let remainder = &rest[position + "<Contents>".len()..];
            let end = remainder
                .find("</Contents>")
                .ok_or_else(|| err("storage_corrupt"))?;
            let entry = &remainder[..end];
            let key = percent_decode(&xml_unescape(
                xml_value(entry, "Key").ok_or_else(|| err("storage_corrupt"))?,
            ));
            let etag = xml_unescape(
                xml_value(entry, "ETag").ok_or_else(|| err("storage_corrupt"))?,
            )
            .trim_matches('"')
            .to_string();
            let size: u64 = xml_value(entry, "Size")
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| err("storage_corrupt"))?;
            if size > MAX_OBJECT {
                return Err(err("storage_corrupt"));
            }
            let relative = self.relative_key(&key);
            last_key = Some(relative.clone());
            items.push(ObjectMeta {
                key: relative,
                version: etag,
                bytes: size,
            });
            rest = &remainder[end + "</Contents>".len()..];
        }
        let cursor = if truncated {
            Some(last_key.ok_or_else(|| err("storage_corrupt"))?)
        } else {
            // 与 LocalFolder 同语义：非截断页没有后续游标。
            None
        };
        Ok(Page { items, cursor })
    }

    fn stat(&self, key: &str) -> Result<ObjectMeta, ApiError> {
        let full = self.full_key(key)?;
        let (host, path) = self.object_location(&full);
        let response = self.send(Request {
            method: "HEAD",
            host,
            path,
            query: vec![],
            headers: vec![],
            body: vec![],
        })?;
        if !response.status().is_success() {
            let (body, status) = read_body(response);
            return Err(map_status(status, &body, "stat"));
        }
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim_matches('"').to_string())
            .ok_or_else(|| err("storage_corrupt"))?;
        let bytes: u64 = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| err("storage_corrupt"))?;
        if bytes > MAX_OBJECT {
            return Err(err("storage_corrupt"));
        }
        Ok(ObjectMeta {
            key: key.to_string(),
            version: etag,
            bytes,
        })
    }

    fn get(&self, key: &str) -> Result<Box<dyn Read>, ApiError> {
        let full = self.full_key(key)?;
        let (host, path) = self.object_location(&full);
        let response = self.send(Request {
            method: "GET",
            host,
            path,
            query: vec![],
            headers: vec![],
            body: vec![],
        })?;
        if !response.status().is_success() {
            let (body, status) = read_body(response);
            return Err(map_status(status, &body, "get"));
        }
        Ok(Box::new(VerifiedBody {
            remaining: response.content_length(),
            response,
            total: 0,
        }))
    }

    fn put(
        &self,
        key: &str,
        data: &mut dyn Read,
        condition: WriteCondition,
    ) -> Result<ObjectMeta, ApiError> {
        let full = self.full_key(key)?;
        let mut bytes = vec![];
        data.take(MAX_OBJECT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| err("storage_transient"))?;
        if bytes.len() as u64 > MAX_OBJECT {
            return Err(err("storage_corrupt"));
        }
        let (headers, purpose): (Vec<(String, String)>, &'static str) = match condition {
            WriteCondition::CreateOnly => (vec![("If-None-Match".into(), "*".into())], "put"),
            WriteCondition::MatchVersion(_) => {
                let version = match &condition {
                    WriteCondition::MatchVersion(v) => v.clone(),
                    _ => unreachable!(),
                };
                (
                    vec![("If-Match".into(), format!("\"{version}\""))],
                    "conditional-put",
                )
            }
            WriteCondition::Any => (vec![], "put"),
        };
        let (host, path) = self.object_location(&full);
        let response = self.send(Request {
            method: "PUT",
            host,
            path,
            query: vec![],
            headers,
            body: bytes.clone(),
        })?;
        let etag = etag_of_put(response).map_err(|e| {
            // If-Match 指向已不存在的版本时，S3 返回 404——按条件失败语义映射。
            if e.code == "storage_not_found" && purpose == "conditional-put" {
                err("storage_precondition")
            } else {
                e
            }
        })?;
        Ok(ObjectMeta {
            key: key.to_string(),
            version: etag,
            bytes: bytes.len() as u64,
        })
    }

    fn delete(&self, key: &str, condition: WriteCondition) -> Result<(), ApiError> {
        match condition {
            WriteCondition::CreateOnly => return Err(err("storage_unsupported")),
            WriteCondition::MatchVersion(_)
                if !self
                    .conditional_delete
                    .load(std::sync::atomic::Ordering::Relaxed) =>
            {
                // 该 S3 服务器忽略 DELETE 的 If-Match（探测证实），静默删除
                // 会移除新发布的版本；按既定降级明确拒绝。
                return Err(ApiError::new(
                    "storage_unsupported",
                    "对象存储不支持条件删除，已明确拒绝以保持删除语义；可使用无条件删除或更换支持 If-Match DELETE 的后端",
                ));
            }
            _ => {}
        }
        let full = self.full_key(key)?;
        // 与 LocalFolder 的 remove_file 语义对齐：对象不存在必须是
        // storage_not_found，而不是 S3 的幂等 204。
        self.stat(key)?;
        let headers = match &condition {
            WriteCondition::MatchVersion(version) => {
                vec![("If-Match".into(), format!("\"{version}\""))]
            }
            _ => vec![],
        };
        let (host, path) = self.object_location(&full);
        let response = self.send(Request {
            method: "DELETE",
            host,
            path,
            query: vec![],
            headers,
            body: vec![],
        })?;
        if !response.status().is_success() {
            let (body, status) = read_body(response);
            return Err(map_status(status, &body, "delete"));
        }
        Ok(())
    }
}

fn xml_value<'a>(entry: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = entry.find(&open)? + open.len();
    let end = entry.find(&close)?;
    if start > end {
        return None;
    }
    Some(&entry[start..end])
}

/// XML 实体反转义（键与 ETag 经 XML 转义；Key 另有 percent 编码需再解码）。
fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
