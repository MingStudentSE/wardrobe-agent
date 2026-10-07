//! 测试支持：真实 S3 的供给与受控故障注入。
//!
//! - `ensure_bucket`：独立于产品适配器的最小 SigV4 实现（互为印证），
//!   仅用于测试环境准备。
//! - `FaultProxy`：本地真实 HTTP 代理，向前转发到 MinIO，可注入
//!   503-SlowDown（带 Retry-After）与 GET 响应体截断。注入的是真实
//!   协议字节流上的受控故障，不是协议 mock。
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub fn unique(scope: &str) -> String {
    format!("{scope}-{}", uuid::Uuid::now_v7().simple())
}

// ---------- 最小 SigV4（测试供给专用，独立于产品适配器） ----------

mod mini_sig {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};

    fn hmac<K: AsRef<[u8]>>(key: K, msg: &[u8]) -> Vec<u8> {
        let key = key.as_ref();
        let mut k = key.to_vec();
        if k.len() > 64 {
            k = Sha256::digest(&k).to_vec();
        }
        k.resize(64, 0);
        let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
        let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
        let mut inner = Sha256::new();
        inner.update(&ipad);
        inner.update(msg);
        let inner = inner.finalize();
        let mut outer = Sha256::new();
        outer.update(&opad);
        outer.update(inner);
        outer.finalize().to_vec()
    }

    pub fn put(
        host: &str,
        path: &str,
        access: &str,
        secret: &str,
        body: &[u8],
    ) -> Result<u16, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let (datestamp, amzdate) = {
            let days = now / 86400;
            // days since 1970-01-01 -> y/m/d（civil from days 算法）
            let z = days as i64 + 719468;
            let era = z.div_euclid(146097);
            let doe = z.rem_euclid(146097);
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let y = yoe + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = if m <= 2 { y + 1 } else { y };
            (format!("{y:04}{m:02}{d:02}"), format!("{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z", now / 3600 % 24, now / 60 % 60, now % 60))
        };
        let payload_hash = hex(&Sha256::digest(body));
        let canonical = format!(
            "PUT\n{path}\n\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amzdate}\n\nhost;x-amz-content-sha256;x-amz-date\n{payload_hash}"
        );
        let scope = format!("{datestamp}/us-east-1/s3/aws4_request");
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amzdate}\n{scope}\n{}",
            hex(&Sha256::digest(canonical.as_bytes()))
        );
        let k = hmac(
            hmac(
                hmac(
                    hmac(format!("AWS4{secret}"), datestamp.as_bytes()),
                    b"us-east-1",
                ),
                b"s3",
            ),
            b"aws4_request",
        );
        let signature = hex(&hmac(&k, string_to_sign.as_bytes()));
        let request = format!(
            "PUT {path} HTTP/1.1\r\nhost: {host}\nx-amz-content-sha256: {payload_hash}\nx-amz-date: {amzdate}\nauthorization: AWS4-HMAC-SHA256 Credential={access}/{scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={signature}\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        let mut stream = std::net::TcpStream::connect(host).map_err(|e| e.to_string())?;
        stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
        stream.write_all(body).map_err(|e| e.to_string())?;
        let mut response = vec![];
        stream.read_to_end(&mut response).map_err(|e| e.to_string())?;
        let head = String::from_utf8_lossy(&response);
        head.split("\r\n")
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| format!("bad response: {head}"))
    }


    /// 带 query 的签名 GET（ListObjectsV2 / GET 对象共用）。
    pub fn get(
        host: &str,
        path: &str,
        query: &[(String, String)],
        access: &str,
        secret: &str,
    ) -> Result<(u16, Vec<u8>), String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let (datestamp, amzdate) = civil(now);
        let payload_hash = hex(&Sha256::digest(b""));
        let mut sorted: Vec<(String, String)> = query.to_vec();
        sorted.sort();
        let canonical_query = sorted
            .iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let canonical = format!(
            "GET\n{path}\n{canonical_query}\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amzdate}\n\nhost;x-amz-content-sha256;x-amz-date\n{payload_hash}"
        );
        let scope = format!("{datestamp}/us-east-1/s3/aws4_request");
        let sts = format!(
            "AWS4-HMAC-SHA256\n{amzdate}\n{scope}\n{}",
            hex(&Sha256::digest(canonical.as_bytes()))
        );
        let k = hmac(
            hmac(hmac(hmac(format!("AWS4{secret}"), datestamp.as_bytes()), b"us-east-1"), b"s3"),
            b"aws4_request",
        );
        let signature = hex(&hmac(&k, sts.as_bytes()));
        let url = if canonical_query.is_empty() {
            format!("GET {path} HTTP/1.1\r\n")
        } else {
            format!("GET {path}?{canonical_query} HTTP/1.1\r\n")
        };
        let request = format!(
            "{url}host: {host}\nx-amz-content-sha256: {payload_hash}\nx-amz-date: {amzdate}\nauthorization: AWS4-HMAC-SHA256 Credential={access}/{scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={signature}\nconnection: close\r\n\r\n"
        );
        let mut stream = std::net::TcpStream::connect(host).map_err(|e| e.to_string())?;
        stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
        let mut response = vec![];
        stream.read_to_end(&mut response).map_err(|e| e.to_string())?;
        let split = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("no header terminator")?;
        let status: u16 = String::from_utf8_lossy(&response[..split])
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .ok_or("bad status")?;
        Ok((status, response[split + 4..].to_vec()))
    }

    fn encode(value: &str) -> String {
        let mut out = String::new();
        for byte in value.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(byte as char)
                }
                other => out.push_str(&format!("%{other:02X}")),
            }
        }
        out
    }

    fn civil(seconds: u64) -> (String, String) {
        let days = (seconds / 86400) as i64;
        let secs_of_day = seconds % 86400;
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

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// 确保 bucket 存在（409 = 已存在，同样视为可用）。
pub fn ensure_bucket(endpoint: &str, bucket: &str, access: &str, secret: &str) {
    let host = endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string();
    match mini_sig::put(&host, &format!("/{bucket}"), access, secret, b"") {
        Ok(200) | Ok(409) => {}
        other => panic!("ensure_bucket {bucket} failed on {endpoint}: {other:?}"),
    }
}


/// 列举 bucket 中 `prefix/` 下全部对象并读取字节（独立最小签名实现，互为印证）。
pub fn list_objects(
    endpoint: &str,
    bucket: &str,
    access: &str,
    secret: &str,
    prefix: &str,
) -> Vec<(String, Vec<u8>)> {
    let host = endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string();
    let mut out = Vec::new();
    let mut marker = String::new();
    loop {
        let mut query: Vec<(String, String)> = vec![
            ("encoding-type".into(), "url".into()),
            ("list-type".into(), "2".into()),
            ("max-keys".into(), "1000".into()),
            ("prefix".into(), format!("{prefix}/")),
        ];
        if !marker.is_empty() {
            query.push(("start-after".into(), marker.clone()));
        }
        let (status, body) =
            mini_sig::get(&host, &format!("/{bucket}/"), &query, access, secret)
                .expect("list objects");
        assert_eq!(status, 200, "list failed: {}", String::from_utf8_lossy(&body));
        let text = String::from_utf8_lossy(&body).to_string();
        let truncated = text.contains("<IsTruncated>true</IsTruncated>");
        let mut rest = text.as_str();
        let mut last = None;
        while let Some(position) = rest.find("<Contents>") {
            let remainder = &rest[position + "<Contents>".len()..];
            let end = remainder.find("</Contents>").unwrap();
            let entry = &remainder[..end];
            let key_start = entry.find("<Key>").unwrap() + 5;
            let key_end = entry.find("</Key>").unwrap();
            let key = percent_decode(&entry[key_start..key_end].replace("&amp;", "&"));
            last = Some(key.clone());
            out.push((key, Vec::new()));
            rest = &remainder[end + 11..];
        }
        if !truncated {
            break;
        }
        marker = last.unwrap_or_default();
    }
    // 读取每个对象的字节。
    for (key, bytes) in out.iter_mut() {
        let (status, body) =
            mini_sig::get(&host, &format!("/{bucket}/{key}"), &[], access, secret).expect("get object");
        assert_eq!(status, 200, "get {key} failed");
        *bytes = body;
    }
    out
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

// ---------- 受控故障注入代理 ----------

#[derive(Debug, Clone, Copy, Default)]
pub struct ProxyStats {
    pub slowdown: u64,
    pub forwarded: u64,
    pub truncated: u64,
}

struct ProxyState {
    slowdown_remaining: usize,
    truncate_next_get: Option<usize>,
    stats: ProxyStats,
}

pub struct FaultProxy {
    port: u16,
    state: Arc<Mutex<ProxyState>>,
    counters: Arc<(AtomicU64, AtomicU64, AtomicU64)>,
}

impl FaultProxy {
    /// 启动本地代理；`slowdown_first` 个请求收到 503 + Retry-After: 0。
    pub fn spawn(upstream: &str, slowdown_first: usize) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(ProxyState {
            slowdown_remaining: slowdown_first,
            truncate_next_get: None,
            stats: ProxyStats::default(),
        }));
        let counters = Arc::new((AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)));
        let upstream = upstream.to_string();
        let worker_state = state.clone();
        let worker_counters = counters.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let upstream = upstream.clone();
                let state = worker_state.clone();
                let counters = worker_counters.clone();
                // 串行处理即可：契约测试单请求在途。
                handle(&mut stream, &upstream, &state, &counters);
            }
        });
        Self {
            port,
            state,
            counters,
        }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// 下一次 GET 转发时：保留上游声明的 Content-Length，但只回送 `bytes`
    /// 字节后断开连接（模拟传输截断）。
    pub fn truncate_next_get(&self, bytes: usize) {
        self.state.lock().unwrap().truncate_next_get = Some(bytes);
    }

    pub fn stats(&self) -> ProxyStats {
        ProxyStats {
            slowdown: self.counters.0.load(Ordering::SeqCst),
            forwarded: self.counters.1.load(Ordering::SeqCst),
            truncated: self.counters.2.load(Ordering::SeqCst),
        }
    }
}

fn handle(
    client: &mut std::net::TcpStream,
    upstream: &str,
    state: &Arc<Mutex<ProxyState>>,
    counters: &Arc<(AtomicU64, AtomicU64, AtomicU64)>,
) {
    // 读取请求头。
    let mut request = vec![0u8; 0];
    let mut buffer = [0u8; 4096];
    let header_end;
    loop {
        let n = match client.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        request.extend_from_slice(&buffer[..n]);
        if let Some(pos) = find_header_end(&request) {
            // pos 匹配的是「最后一个头的 CRLF + 空行的 CRLF」四字节窗口；
            // 头块（含每个头自身的 CRLF）实际结束于 pos+2，正文从 pos+4 开始。
            header_end = pos + 2;
            break;
        }
        if request.len() > 1024 * 1024 {
            return;
        }
    }
    let head = String::from_utf8_lossy(&request[..header_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default().to_string();
    let method = request_line.split_whitespace().next().unwrap_or_default().to_string();
    let mut content_length = 0usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        if name.trim().eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    // 读取请求体（若有）。
    while request.len() < header_end + 2 + content_length {
        let n = match client.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        request.extend_from_slice(&buffer[..n]);
    }

    let (slowdown, _forwarded) = {
        let mut state = state.lock().unwrap();
        if state.slowdown_remaining > 0 {
            state.slowdown_remaining -= 1;
            state.stats.slowdown += 1;
            (true, false)
        } else {
            state.stats.forwarded += 1;
            (false, true)
        }
    };
    if slowdown {
        let body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Error><Code>SlowDown</Code><Message>Please reduce your request rate.</Message></Error>";
        let response = format!(
            "HTTP/1.1 503 Slow Down\r\ncontent-type: application/xml\r\ncontent-length: {}\r\nretry-after: 0\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = client.write_all(response.as_bytes());
        let _ = client.flush();
        counters.0.fetch_add(1, Ordering::SeqCst);
        return;
    }
    counters.1.fetch_add(1, Ordering::SeqCst);

    // 转发到上游：剥离客户端的 Connection 头并注入 close，
    // 让上游响应后主动关闭连接，代理透传到 EOF 即完成。
    let head_text = String::from_utf8_lossy(&request[..header_end]).to_string();
    // split 的最后一个元素是终止 CRLF 产生的空串；connection 头必须插到它之前。
    let mut lines: Vec<&str> = head_text.split("\r\n").collect();
    let trailing_empty = lines.last().map(|l| l.is_empty()).unwrap_or(false);
    let body_part = if trailing_empty { lines.pop(); true } else { false };
    lines.retain(|line| !(line.len() > 11 && line[..11].to_ascii_lowercase() == "connection:"));
    lines.push("connection: close");
    let mut rebuilt = lines.join("\r\n");
    rebuilt.push_str("\r\n\r\n");
    let mut forwarded = rebuilt.into_bytes();
    if body_part {
        // 正文从 header_end（头块末尾）再跳过空行 CRLF 的 2 字节开始。
        forwarded.extend_from_slice(&request[header_end + 2..]);
    }

    let Ok(mut upstream_stream) = std::net::TcpStream::connect(upstream) else {
        return;
    };
    let _ = upstream_stream.set_read_timeout(Some(std::time::Duration::from_secs(15)));
    let _ = upstream_stream.set_write_timeout(Some(std::time::Duration::from_secs(15)));
    if upstream_stream.write_all(&forwarded).is_err() {
        return;
    }
    let truncate = {
        let mut state = state.lock().unwrap();
        if method == "GET" {
            state.truncate_next_get.take()
        } else {
            None
        }
    };
    match truncate {
        Some(keep) => {
            // 读完整响应头 + 全部响应体，然后按原 Content-Length 只回送部分字节。
            let mut response = vec![];
            let mut chunk = [0u8; 8192];
            loop {
                match upstream_stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => response.extend_from_slice(&chunk[..n]),
                }
            }
            let Some(pos) = find_header_end(&response) else { return };
            let head = String::from_utf8_lossy(&response[..pos + 2]).to_string();
            let body_start = pos + 4;
            let head = head
                .lines()
                .filter(|line| {
                    let lower = line.to_ascii_lowercase();
                    !lower.starts_with("transfer-encoding:") && !lower.starts_with("connection:")
                })
                .map(|line| format!("{line}\r\n"))
                .collect::<String>();
            let _ = client.write_all(
                format!("{head}connection: close\r\n\r\n").as_bytes(),
            );
            let end = (body_start + keep).min(response.len());
            let _ = client.write_all(&response[body_start..end]);
            let _ = client.flush();
            counters.2.fetch_add(1, Ordering::SeqCst);
            let _ = client.shutdown(std::net::Shutdown::Both);
        }
        None => {
            // 透传直到上游EOF（connection: close 保证到达）。
            let _ = std::io::copy(&mut upstream_stream, client);
            let _ = client.flush();
        }
    }
    use std::net::Shutdown;
    let _ = upstream_stream.shutdown(Shutdown::Both);
    let _ = client.shutdown(Shutdown::Both);
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| w == b"\r\n\r\n")
}
