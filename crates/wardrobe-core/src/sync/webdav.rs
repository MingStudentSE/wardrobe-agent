//! WebDAV 传输适配器：实现与 LocalFolder 相同的 `SyncStorage` 外部契约。
//!
//! 设计边界（ADR 0038）：
//! - `PROPFIND Depth:1` 列举映射为客户端排序分页枚举，游标语义与 LocalFolder
//!   一致（严格大于上一个键）。
//! - 条件写入走服务器判定：`CreateOnly` 用 `If-None-Match: *`；
//!   `MatchVersion(etag)` 用 RFC 4918 `If: <绝对URL> ([etag])` 状态令牌（弱比较，
//!   同时兼容强/弱 ETag）。ETag 只是条件写的透明令牌，绝不当作内容摘要；
//!   内容完整性由引擎的 sha256 发布标记（#27 Publication Marker Gate）负责。
//! - 不使用 MOVE，不依赖服务器原子改名；发布可见性复用 #27 的对象+.commit
//!   标记机制，`atomic_publication` 如实声明为 false。
//! - 能力探针：连接时用真实条件写入握手验证服务器确实执行条件判定；不执行的
//!   服务器（如 dufs）在连接期被明确拒绝（storage_unsupported），绝不静默降级。
//! - GET 全程流式读取；PUT 在协议上界内缓冲（引擎对象上界 2 MiB，与
//!   LocalFolder 的 take(2 MiB+1) 一致），超限在发送前以 storage_corrupt 拒绝，
//!   不发布任何字节。条件写重试后出现 412 时以内容一致性判定"首次已成功"，
//!   不一致才报 storage_precondition。
//! - 类型化错误：401/403→storage_unauthorized（不重试）、404→storage_not_found、
//!   412/409→storage_precondition、423→storage_locked、429/503→
//!   storage_rate_limited（遵守 Retry-After）、其余 5xx/网络/超时→
//!   storage_transient。自动重试预算按父规格：网络/限流类最多 2 次；条件写与
//!   幂等操作允许自动重试，无条件写（Any）不允许（结果不明确）。
use super::storage::{ObjectMeta, Page, StorageCapabilities, SyncStorage, WriteCondition};
use crate::ApiError;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{io::Read, time::Duration};

const MAX_OBJECT: u64 = 2 * 1024 * 1024;
const MAX_ATTEMPTS: usize = 3; // 1 次原始请求 + 最多 2 次自动重试（父规格预算）

#[derive(Clone, Debug)]
pub struct WebDavConfig {
    /// 服务器地址，如 `https://dav.example.com` 或 `http://127.0.0.1:5981`。
    pub url: String,
    /// 服务器上已存在的目录前缀；本应用的所有同步对象都在它下面。
    pub root: String,
    pub username: String,
    pub password: String,
    pub timeout: Duration,
}

pub struct WebDav {
    base: reqwest::Url,
    auth: String,
    client: reqwest::blocking::Client,
}

fn error(code: &str, message: &str) -> ApiError {
    ApiError::new(code, message)
}
fn invalid_key() -> ApiError {
    error("invalid_object_key", "对象键包含不可移植的字符或路径")
}
/// 与 LocalFolder 相同的键规则：相对、POSIX 分段、无盘符/反斜杠/控制字符。
fn validate_key(key: &str) -> Result<(), ApiError> {
    let portable = !key.is_empty()
        && !key.contains(':')
        && !key.chars().any(char::is_control)
        && std::path::Path::new(key)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        && key
            .split('/')
            .all(|p| !p.is_empty() && p != ".." && p != "." && !p.contains('\\'));
    if portable {
        Ok(())
    } else {
        Err(invalid_key())
    }
}
fn validate_segment(segment: &str) -> Result<(), ApiError> {
    if segment.is_empty()
        || segment == "."
        || segment == ".."
        || segment.contains('\\')
        || segment.contains(':')
        || segment.chars().any(char::is_control)
    {
        return Err(invalid_key());
    }
    Ok(())
}
fn encode_segment(segment: &str) -> String {
    let mut out = String::new();
    for byte in segment.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}
fn decode_percent(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("zz"),
                16,
            ) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn decode_entities(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
/// 从 multistatus XML 中按本地名提取单个属性值（命名空间前缀无关）。
fn extract_tag(block: &str, local_name: &str) -> Option<String> {
    let mut search = 0;
    while let Some(found) = block[search..].find('<') {
        let start = search + found;
        let end = block[start..].find('>')? + start;
        let name = block[start + 1..end].trim();
        let local = name.rsplit(':').next().unwrap_or(name);
        if local.eq_ignore_ascii_case(local_name)
            && !name.starts_with('/')
            && !name.ends_with('/')
            && !name.contains(' ')
        {
            let close = format!("</{name}>");
            let value_end = block[end..].find(&close)? + end;
            return Some(block[end + 1..value_end].to_string());
        }
        search = end;
    }
    None
}

impl WebDav {
    pub fn connect(config: WebDavConfig) -> Result<Self, ApiError> {
        let mut url = reqwest::Url::parse(&config.url)
            .map_err(|_| error("invalid_input", "WebDAV 地址不是合法 URL"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(error("invalid_input", "WebDAV 地址必须是 http(s)"));
        }
        url.set_path("/");
        for segment in config.root.split('/').filter(|s| !s.is_empty()) {
            validate_segment(segment)?;
            url.path_segments_mut()
                .map_err(|_| error("invalid_input", "WebDAV 地址没有路径"))?
                .push(&encode_segment(segment));
        }
        url.set_path(&format!("{}/", url.path().trim_end_matches('/')));
        let client =
            crate::http_client::build(crate::http_client::OutboundProfile::WebDav {
                timeout: config.timeout,
            })
            .map_err(|_| error("storage_error", "无法初始化 WebDAV 客户端"))?;
        let webdav = Self {
            base: url,
            auth: format!(
                "Basic {}",
                STANDARD.encode(format!("{}:{}", config.username, config.password))
            ),
            client,
        };
        webdav.capability_probe()?;
        Ok(webdav)
    }
    fn url_for(&self, key: &str) -> Result<reqwest::Url, ApiError> {
        validate_key(key)?;
        // join 相对解析：base 以 "/" 结尾，键路径整体拼接不会出现重复斜杠
        //（重复斜杠会让 mod_dav 归一化后无法匹配 If 头的标记 URI，条件被忽略）。
        let encoded = key
            .split('/')
            .map(encode_segment)
            .collect::<Vec<_>>()
            .join("/");
        let url = self
            .base
            .join(&encoded)
            .map_err(|_| error("invalid_input", "对象键无法映射为 URL"))?;
        Ok(url)
    }
    /// 真实条件写入握手：建探针对象 → 重复创建必须被拒（412）→ 过期条件写
    /// 必须被拒（412）→ 清理。任何一步被无视都视为能力缺失，明确拒绝。
    fn capability_probe(&self) -> Result<(), ApiError> {
        let key = format!(".wardrobe-probe/{}", uuid::Uuid::now_v7());
        let url = self.url_for(&key)?;
        self.ensure_parents(&url)?;
        let created = self.execute_url(&url, |client, url| {
            client
                .put(url.clone())
                .header("If-None-Match", "*")
                .body(b"probe".to_vec())
        })?;
        if !created.status().is_success() {
            return Err(self.status_error(created.status(), "能力探针创建失败"));
        }
        let replayed = self.execute_url(&url, |client, url| {
            client
                .put(url.clone())
                .header("If-None-Match", "*")
                .body(b"probe".to_vec())
        })?;
        if replayed.status().as_u16() != 412 {
            let _ = self.raw_delete(&url);
            return Err(error(
                "storage_unsupported",
                "WebDAV 服务器未执行 If-None-Match 条件判定（重复创建被放行），多设备会互相覆盖；已拒绝该服务器",
            ));
        }
        let stale = self.execute_url(&url, |client, url| {
            client
                .put(url.clone())
                .header("If", format!("<{}> ([\"probe-stale-etag\"])", url.as_str()))
                .body(b"probe".to_vec())
        })?;
        if stale.status().as_u16() != 412 {
            let _ = self.raw_delete(&url);
            return Err(error(
                "storage_unsupported",
                "WebDAV 服务器未执行 If 条件判定（过期条件被放行），发布游标可能被回写；已拒绝该服务器",
            ));
        }
        self.raw_delete(&url)
    }
    /// 逐级 MKCOL 建目录链：从主机根开始覆盖配置根（root）的全部段，再加键的
    /// 祖先集合（不含末段对象本身）。405（已存在）是并发竞争的合法结果。
    fn ensure_parents(&self, url: &reqwest::Url) -> Result<(), ApiError> {
        let mut prefix = self.base.clone();
        prefix.set_path("/");
        let root_segments: Vec<String> = self
            .base
            .path()
            .split('/')
            .filter(|s| !s.is_empty())
            .map(decode_percent)
            .collect();
        let target = url
            .path()
            .strip_prefix(self.base.path())
            .ok_or_else(invalid_key)?
            .trim_end_matches('/')
            .to_string();
        let key_segments: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();
        let method = reqwest::Method::from_bytes(b"MKCOL")
            .map_err(|_| error("storage_error", "无法构造 WebDAV 请求"))?;
        let mut chain = root_segments.clone();
        if key_segments.len() > 1 {
            chain.extend(
                key_segments[..key_segments.len() - 1]
                    .iter()
                    .map(|s| s.to_string()),
            );
        }
        for segment in chain {
            prefix
                .path_segments_mut()
                .map_err(|_| error("invalid_input", "WebDAV 地址没有路径"))?
                .push(&encode_segment(&segment));
            let response = self.execute_url(&prefix, |client, url| {
                client.request(method.clone(), url.clone())
            })?;
            let status = response.status().as_u16();
            if status != 201 && status != 405 && status != 409 {
                return Err(match status {
                    409 => error("storage_precondition", "WebDAV 父目录竞争"),
                    423 => error("storage_locked", "WebDAV 目录被写锁占用"),
                    401 | 403 => error("storage_unauthorized", "WebDAV 认证失败"),
                    _ => error("storage_transient", "创建 WebDAV 目录失败"),
                });
            }
        }
        Ok(())
    }
    fn execute_url(
        &self,
        url: &reqwest::Url,
        build: impl Fn(&reqwest::blocking::Client, &reqwest::Url) -> reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response, ApiError> {
        for attempt in 0..MAX_ATTEMPTS {
            let request = build(&self.client, url)
                .header(reqwest::header::AUTHORIZATION, &self.auth)
                .build()
                .map_err(|_| error("storage_error", "无法构造 WebDAV 请求"))?;
            match self.client.execute(request) {
                Ok(response) => {
                    let status = response.status();
                    match status.as_u16() {
                        429 | 503 => {
                            if attempt + 1 == MAX_ATTEMPTS {
                                return Err(error(
                                    "storage_rate_limited",
                                    "WebDAV 服务器限流，自动重试已用尽；本地数据已保留",
                                ));
                            }
                            let wait = response
                                .headers()
                                .get(reqwest::header::RETRY_AFTER)
                                .and_then(|v| v.to_str().ok())
                                .and_then(|v| v.trim().parse::<u64>().ok())
                                .unwrap_or(1)
                                .min(30);
                            std::thread::sleep(Duration::from_secs(wait));
                        }
                        500..=599 => {
                            if attempt + 1 == MAX_ATTEMPTS {
                                return Err(error(
                                    "storage_transient",
                                    "WebDAV 服务器暂时不可用，自动重试已用尽；本地数据已保留",
                                ));
                            }
                            std::thread::sleep(Duration::from_millis(200 * (attempt as u64 + 1)));
                        }
                        _ => return Ok(response),
                    }
                }
                Err(_) if attempt + 1 < MAX_ATTEMPTS => {
                    std::thread::sleep(Duration::from_millis(200 * (attempt as u64 + 1)));
                }
                Err(_) => {
                    return Err(error(
                        "storage_transient",
                        "WebDAV 连接失败，自动重试已用尽；本地数据已保留，可重试",
                    ))
                }
            }
        }
        Err(error(
            "storage_transient",
            "WebDAV 服务器持续不可用；本地数据已保留",
        ))
    }
    fn status_error(&self, status: reqwest::StatusCode, message: &str) -> ApiError {
        match status.as_u16() {
            401 | 403 => error(
                "storage_unauthorized",
                "WebDAV 认证失败：请检查用户名与 OS 安全存储中的密码",
            ),
            404 => error("storage_not_found", "WebDAV 对象不存在"),
            409 | 412 => error(
                "storage_precondition",
                "WebDAV 对象已被其他设备更新，条件写入被拒绝",
            ),
            423 => error(
                "storage_locked",
                "WebDAV 对象被服务器端写锁占用（423 Locked）",
            ),
            _ => error("storage_transient", message),
        }
    }
    fn raw_delete(&self, url: &reqwest::Url) -> Result<(), ApiError> {
        let response = self.execute_url(url, |client, url| client.delete(url.clone()))?;
        let status = response.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        Err(self.status_error(status, "删除探针失败"))
    }
    fn parse_multistatus(xml: &str) -> Vec<(String, String, Option<u64>)> {
        let mut entries = vec![];
        let lower = xml.to_ascii_lowercase();
        // 开标签可能带属性（如 <D:response xmlns:...>），按前缀匹配。
        for (offset, _) in lower.match_indices("<d:response") {
            let Some(open_end_rel) = lower[offset..].find('>') else {
                continue;
            };
            let after = offset + open_end_rel + 1;
            let Some(close) = lower[after..].find("</d:response>") else {
                continue;
            };
            let block = &xml[after..after + close];
            let Some(href) = extract_tag(block, "href") else {
                continue;
            };
            let href = decode_percent(&decode_entities(&href));
            let etag = decode_entities(&extract_tag(block, "getetag").unwrap_or_default());
            let length =
                extract_tag(block, "getcontentlength").and_then(|v| v.trim().parse::<u64>().ok());
            entries.push((href, etag, length));
        }
        entries
    }
    /// 重试后出现 412 时判定"首次尝试是否已经完整落地"：远端内容与本次上传
    /// 完全一致才算幂等成功；否则是真实竞争。
    fn verify_remote_equals(&self, key: &str, body: &[u8]) -> Result<bool, ApiError> {
        let mut remote = vec![];
        self.get(key)?
            .take(MAX_OBJECT + 1)
            .read_to_end(&mut remote)
            .map_err(|_| error("storage_transient", "读取远端内容失败"))?;
        Ok(remote.as_slice() == body)
    }
}

impl SyncStorage for WebDav {
    fn capabilities(&self) -> StorageCapabilities {
        // 条件写入已由连接探针验证；WebDAV 无原子改名，发布可见性由
        // #27 的 .commit 标记机制保证（不使用 MOVE）。
        StorageCapabilities {
            conditional_write: true,
            atomic_publication: false,
        }
    }
    fn list(&self, prefix: &str, cursor: Option<&str>, limit: usize) -> Result<Page, ApiError> {
        if !(1..=1000).contains(&limit) {
            return Err(error("invalid_cursor", "非法分页参数"));
        }
        validate_key(prefix)?;
        if let Some(cursor) = cursor {
            validate_key(cursor)?;
            if !cursor.starts_with(&format!("{prefix}/")) {
                return Err(error("invalid_cursor", "游标不属于该前缀"));
            }
        }
        let encoded = prefix
            .split('/')
            .map(encode_segment)
            .collect::<Vec<_>>()
            .join("/");
        let mut url = self
            .base
            .join(&encoded)
            .map_err(|_| error("invalid_input", "前缀无法映射为 URL"))?;
        url.set_path(&format!("{}/", url.path().trim_end_matches('/')));
        let propfind = reqwest::Method::from_bytes(b"PROPFIND")
            .map_err(|_| error("storage_error", "无法构造 WebDAV 请求"))?;
        let body = r#"<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:getetag/><D:getcontentlength/><D:resourcetype/></D:prop></D:propfind>"#.as_bytes().to_vec();
        let response = self.execute_url(&url, |client, url| {
            client
                .request(propfind.clone(), url.clone())
                .header("Depth", "1")
                .header(reqwest::header::CONTENT_TYPE, "application/xml")
                .body(body.clone())
        })?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Ok(Page {
                items: vec![],
                cursor: None,
            });
        }
        if !(status.is_success() || status.as_u16() == 207) {
            return Err(self.status_error(status, "列举 WebDAV 目录失败"));
        }
        let xml = response
            .text()
            .map_err(|_| error("storage_transient", "WebDAV 列举响应不完整"))?;
        let base_path = url.path().to_string();
        let parsed = Self::parse_multistatus(&xml);
        let mut keys = vec![];
        for (href, etag, length) in parsed {
            let path = match reqwest::Url::parse(&href) {
                Ok(absolute) => absolute.path().to_string(),
                Err(_) => href.clone(),
            };
            let relative = match path.strip_prefix(base_path.as_str()) {
                Some(relative) if !relative.is_empty() => relative,
                _ => continue, // 目录自身或前缀之外
            };
            if relative.ends_with('/') || relative.contains('/') {
                continue; // 只接受直接子项
            }
            let key = format!("{prefix}/{}", decode_percent(relative));
            if validate_key(&key).is_err() {
                continue;
            }
            if cursor.is_none_or(|c| key.as_str() > c) {
                keys.push(ObjectMeta {
                    key,
                    version: etag,
                    bytes: length.unwrap_or(0),
                });
            }
        }
        keys.sort_by(|a, b| a.key.cmp(&b.key));
        let more = keys.len() > limit;
        keys.truncate(limit);
        let cursor = if more {
            keys.last().map(|v| v.key.clone())
        } else {
            None
        };
        Ok(Page {
            items: keys,
            cursor,
        })
    }
    fn stat(&self, key: &str) -> Result<ObjectMeta, ApiError> {
        let url = self.url_for(key)?;
        let response = self.execute_url(&url, |client, url| client.head(url.clone()))?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Err(error("storage_not_found", "WebDAV 对象不存在"));
        }
        if !status.is_success() {
            return Err(self.status_error(status, "查询 WebDAV 对象失败"));
        }
        Ok(ObjectMeta {
            key: key.into(),
            version: response
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string(),
            bytes: response
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        })
    }
    fn get(&self, key: &str) -> Result<Box<dyn Read>, ApiError> {
        let url = self.url_for(key)?;
        let response = self.execute_url(&url, |client, url| client.get(url.clone()))?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Err(error("storage_not_found", "WebDAV 对象不存在"));
        }
        if !status.is_success() {
            return Err(self.status_error(status, "读取 WebDAV 对象失败"));
        }
        Ok(Box::new(response))
    }
    fn put(
        &self,
        key: &str,
        data: &mut dyn Read,
        condition: WriteCondition,
    ) -> Result<ObjectMeta, ApiError> {
        let url = self.url_for(key)?;
        self.ensure_parents(&url)?;
        // 协议上界 2 MiB（与 LocalFolder 相同）：超限在发送前整体拒绝，不发布。
        let mut body = vec![];
        data.take(MAX_OBJECT + 1)
            .read_to_end(&mut body)
            .map_err(|_| error("storage_transient", "读取待上传数据失败"))?;
        if body.len() as u64 > MAX_OBJECT {
            return Err(error(
                "storage_corrupt",
                "对象超过 2 MiB 传输上界，拒绝上传",
            ));
        }
        let if_none_match = matches!(condition, WriteCondition::CreateOnly);
        let if_header = match &condition {
            WriteCondition::MatchVersion(etag) => Some(format!(
                "<{}> ([{}])",
                url.as_str(),
                etag.trim().trim_start_matches("W/")
            )),
            _ => None,
        };
        let mut response = None;
        for attempt in 0..MAX_ATTEMPTS {
            let mut request = self.client.put(url.clone());
            if if_none_match {
                request = request.header("If-None-Match", "*");
            }
            if let Some(header) = &if_header {
                request = request.header("If", header.clone());
            }
            let built = request
                .header(reqwest::header::AUTHORIZATION, &self.auth)
                .body(body.clone())
                .build()
                .map_err(|_| error("storage_error", "无法构造 WebDAV 请求"))?;
            match self.client.execute(built) {
                Ok(ok) => {
                    response = Some(ok);
                    break;
                }
                Err(_) if attempt + 1 < MAX_ATTEMPTS => {
                    // 条件写在重试时由服务器重新判定，语义安全；412 后再以内容
                    // 一致性区分"首次已成功"与真实竞争。
                    std::thread::sleep(Duration::from_millis(200 * (attempt as u64 + 1)));
                }
                Err(_) => {
                    return Err(error(
                        "storage_transient",
                        "WebDAV 上传失败，自动重试已用尽；本地数据已保留",
                    ))
                }
            }
        }
        let response = response.ok_or_else(|| error("storage_transient", "WebDAV 上传无响应"))?;
        let status = response.status();
        if status.as_u16() == 412 || status.as_u16() == 409 {
            if self.verify_remote_equals(key, &body)? {
                return self.stat(key);
            }
            return Err(error(
                "storage_precondition",
                "WebDAV 条件写入被拒绝：对象已被其他设备发布",
            ));
        }
        if !status.is_success() && status.as_u16() != 207 {
            return Err(self.status_error(status, "上传 WebDAV 对象失败"));
        }
        self.stat(key)
    }
    fn delete(&self, key: &str, condition: WriteCondition) -> Result<(), ApiError> {
        let url = self.url_for(key)?;
        if matches!(condition, WriteCondition::CreateOnly) {
            return Err(error("storage_unsupported", "创建条件不适用于删除"));
        }
        let if_header = match &condition {
            WriteCondition::MatchVersion(etag) => Some(format!(
                "<{}> ([{}])",
                url.as_str(),
                etag.trim().trim_start_matches("W/")
            )),
            _ => None,
        };
        let response = self.execute_url(&url, |client, url| {
            let mut request = client.delete(url.clone());
            if let Some(header) = &if_header {
                request = request.header("If", header.clone());
            }
            request
        })?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        // 与 LocalFolder 一致：删除不存在的对象是 storage_not_found。
        Err(self.status_error(status, "删除 WebDAV 对象失败"))
    }
}
