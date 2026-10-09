//! Web 栈行为测试：经真实 TCP/HTTP 走完整服务（tiny_http → 会话 → execute_as_client → 真实 SQLite）。
//! 测试边界遵循父规格：统一 Application API、真实 SQLite、临时资产目录；不 mock 业务成功。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;

pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}
impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

fn decode_chunked(mut raw: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    loop {
        let line_end = raw
            .windows(2)
            .position(|window| window == b"\r\n")
            .expect("分块响应应含块大小行");
        let size_text = String::from_utf8_lossy(&raw[..line_end]).into_owned();
        let size = usize::from_str_radix(size_text.trim().split(';').next().unwrap(), 16)
            .expect("块大小应为十六进制");
        raw = &raw[line_end + 2..];
        if size == 0 {
            break;
        }
        body.extend_from_slice(&raw[..size]);
        raw = &raw[size + 2..];
    }
    body
}

pub fn call(
    port: u16,
    host: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Response {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("应能连接本机服务");
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if let Some(bytes) = body {
        request.push_str(&format!("Content-Length: {}\r\n", bytes.len()));
    }
    for (key, value) in headers {
        request.push_str(&format!("{key}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    if let Some(bytes) = body {
        stream.write_all(bytes).unwrap();
    }
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("应读到完整响应");
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("响应应含头部结束标记");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let raw_body = &raw[split + 4..];
    let chunked = head.lines().any(|line| {
        line.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    });
    let body = if chunked {
        decode_chunked(raw_body)
    } else {
        raw_body.to_vec()
    };
    let status: u16 = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let mut mapped = HashMap::new();
    for line in head.lines().skip(1) {
        if let Some((key, value)) = line.split_once(':') {
            mapped.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Response {
        status,
        headers: mapped,
        body,
    }
}

pub fn host(port: u16) -> String {
    format!("127.0.0.1:{port}")
}
pub fn origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

pub fn start(port: u16, data: &Path, dist: Option<&Path>) -> u16 {
    let mut config = wardrobe_web::Config::new(port, data.to_path_buf());
    if let Some(directory) = dist {
        config = config.with_dist(directory.to_path_buf());
    }
    wardrobe_web::spawn(config).expect("服务应能启动").port
}

pub fn bootstrap_file(data: &Path) -> PathBuf {
    data.join("web-bootstrap-token")
}

/// 走真实引导流程：读本机引导令牌文件 → 恰好一次创建浏览器授权 → 建立会话。
pub fn bootstrap_and_login(port: u16, data: &Path) -> (String, String, String) {
    let token = std::fs::read_to_string(bootstrap_file(data)).unwrap();
    let body = format!("{{\"token\":\"{}\"}}", token.trim());
    let response = call(
        port,
        &host(port),
        "POST",
        "/auth/bootstrap",
        &[
            ("Origin", &origin(port)),
            ("Content-Type", "application/json"),
        ],
        Some(body.as_bytes()),
    );
    assert_eq!(response.status, 200, "引导应成功：{}", response.text());
    let cookie = response
        .headers
        .get("set-cookie")
        .expect("引导成功应设置会话 Cookie")
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let parsed = response.json();
    let connection_token = parsed["result"]["token"].as_str().unwrap().to_string();
    let state = call(
        port,
        &host(port),
        "GET",
        "/auth/state",
        &[("Cookie", &cookie)],
        None,
    );
    let csrf = state.json()["result"]["csrf"].as_str().unwrap().to_string();
    (cookie, csrf, connection_token)
}

pub fn session_post(
    port: u16,
    cookie: &str,
    csrf: Option<&str>,
    path: &str,
    body: String,
) -> Response {
    let mut headers = vec![
        ("Origin".to_string(), origin(port)),
        ("Cookie".to_string(), cookie.to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
    ];
    if let Some(token) = csrf {
        headers.push(("X-Wardrobe-CSRF".to_string(), token.to_string()));
    }
    let borrowed: Vec<(&str, &str)> = headers
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    call(
        port,
        &host(port),
        "POST",
        path,
        &borrowed,
        Some(body.as_bytes()),
    )
}

pub fn trusted_member(data: &Path, name: &str) -> String {
    let app = wardrobe_core::Application::open(&data.join("wardrobe.sqlite")).unwrap();
    let created = app
        .execute("member.create", serde_json::json!({"name": name}))
        .unwrap();
    created["member_id"].as_str().unwrap().to_string()
}

/// 标准 1×1 PNG（真实可解码图片，走核心真实压缩链路）。
pub const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0x64, 0xF8, 0xCF, 0x50,
    0x0F, 0x00, 0x03, 0x86, 0x01, 0x80, 0x5A, 0x34, 0x7D, 0x6B, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

// ---------- S0：服务启动、静态、健康、未鉴权拒绝 ----------

#[test]
fn s0_health_endpoint_reports_service_ok() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let response = call(port, &host(port), "GET", "/health", &[], None);
    assert_eq!(response.status, 200);
    let parsed = response.json();
    assert_eq!(parsed["ok"], serde_json::json!(true));
    assert_eq!(parsed["service"], serde_json::json!("wardrobe-web"));
}

#[test]
fn s0_forged_host_is_rejected_against_dns_rebinding() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let response = call(port, "attacker.example:80", "GET", "/health", &[], None);
    assert_eq!(response.status, 403, "伪造 Host 必须拒绝");
    let localhost_only = call(port, "localhost:80", "GET", "/health", &[], None);
    assert_eq!(localhost_only.status, 403, "不匹配服务端口的 Host 也拒绝");
}

#[test]
fn s0_unauthenticated_api_returns_structured_401() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let response = call(
        port,
        &host(port),
        "POST",
        "/api",
        &[
            ("Origin", &origin(port)),
            ("Content-Type", "application/json"),
        ],
        Some(br#"{"operation":"capabilities","input":{}}"#),
    );
    assert_eq!(response.status, 401);
    assert_eq!(
        response.json()["error"]["code"],
        serde_json::json!("unauthorized")
    );
}

#[test]
fn s0_static_dist_is_served_with_spa_fallback_and_404() {
    let data = tempfile::tempdir().unwrap();
    let dist = tempfile::tempdir().unwrap();
    std::fs::write(
        dist.path().join("index.html"),
        "<html><body>衣间 WEB</body></html>",
    )
    .unwrap();
    std::fs::create_dir_all(dist.path().join("assets")).unwrap();
    std::fs::write(dist.path().join("assets/app.js"), "console.log('衣间');").unwrap();
    let port = start(0, data.path(), Some(dist.path()));
    let index = call(port, &host(port), "GET", "/", &[], None);
    assert_eq!(index.status, 200);
    assert!(index.text().contains("衣间 WEB"));
    assert!(index.headers["content-type"].starts_with("text/html"));
    let script = call(port, &host(port), "GET", "/assets/app.js", &[], None);
    assert_eq!(script.status, 200);
    assert!(script.headers["content-type"].starts_with("text/javascript"));
    assert!(script.text().contains("衣间"));
    let fallback = call(port, &host(port), "GET", "/some/client/route", &[], None);
    assert_eq!(fallback.status, 200, "无扩展名路径回退到 SPA 入口");
    assert!(fallback.text().contains("衣间 WEB"));
    let missing = call(port, &host(port), "GET", "/missing.js", &[], None);
    assert_eq!(missing.status, 404);
    let traversal = call(port, &host(port), "GET", "/../wardrobe.sqlite", &[], None);
    assert_ne!(traversal.status, 200, "路径遍历必须拒绝");
}

#[test]
fn s0_missing_dist_serves_notice_page_instead_of_ui() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let response = call(port, &host(port), "GET", "/", &[], None);
    assert_eq!(response.status, 200);
    let page = response.text();
    assert!(page.contains("wardrobe-web"), "提示页应标明服务名");
    assert!(page.contains("web-dist"), "提示页应说明如何配置界面目录");
}

// ---------- S1/S2/S3：引导、会话、API 桥接、隔离、CSRF/Host 负向 ----------

#[test]
fn s1_bootstrap_requires_matching_one_time_token() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    assert!(
        bootstrap_file(data.path()).is_file(),
        "首启应生成引导令牌文件"
    );
    // 错误令牌：拒绝且不消费文件
    let wrong = call(
        port,
        &host(port),
        "POST",
        "/auth/bootstrap",
        &[
            ("Origin", &origin(port)),
            ("Content-Type", "application/json"),
        ],
        Some(br#"{"token":"0000000000000000000000000000000000000000000000000000000000000000"}"#),
    );
    assert_eq!(wrong.status, 403);
    assert!(
        bootstrap_file(data.path()).is_file(),
        "错误令牌不得消费引导文件"
    );
    // 正确令牌：成功、消费文件、发会话 Cookie 与一次性连接令牌
    let (cookie, _csrf, connection_token) = bootstrap_and_login(port, data.path());
    assert_eq!(connection_token.len(), 64);
    assert!(cookie.starts_with("wardrobe_session="));
    assert!(
        !bootstrap_file(data.path()).is_file(),
        "引导令牌用后必须作废"
    );
    assert!(
        !cookie.contains(&connection_token),
        "连接令牌不得进入 Cookie"
    );
    // 第二次引导：拒绝
    let again = call(
        port,
        &host(port),
        "POST",
        "/auth/bootstrap",
        &[
            ("Origin", &origin(port)),
            ("Content-Type", "application/json"),
        ],
        Some(&body_token(&connection_token)),
    );
    assert_eq!(again.status, 403, "引导令牌只能使用一次");
}

fn body_token(token: &str) -> Vec<u8> {
    serde_json::json!({"token": token}).to_string().into_bytes()
}

#[test]
fn s1_login_exchanges_connection_token_for_session_cookie() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let (_cookie, _csrf, connection_token) = bootstrap_and_login(port, data.path());
    let response = call(
        port,
        &host(port),
        "POST",
        "/auth/login",
        &[
            ("Origin", &origin(port)),
            ("Content-Type", "application/json"),
        ],
        Some(&body_token(&connection_token)),
    );
    assert_eq!(response.status, 200);
    let cookie = response.headers.get("set-cookie").unwrap();
    assert!(cookie.contains("HttpOnly"), "会话 Cookie 必须 HttpOnly");
    assert!(
        cookie.contains("SameSite=Strict"),
        "会话 Cookie 必须 SameSite=Strict"
    );
    let bad = call(
        port,
        &host(port),
        "POST",
        "/auth/login",
        &[
            ("Origin", &origin(port)),
            ("Content-Type", "application/json"),
        ],
        Some(&body_token(&"f".repeat(64))),
    );
    assert_eq!(bad.status, 401, "无效令牌登录必须拒绝");
}

#[test]
fn s1_state_and_logout_reflect_session_lifecycle() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let anonymous = call(port, &host(port), "GET", "/auth/state", &[], None);
    assert_eq!(
        anonymous.json()["result"]["authenticated"],
        serde_json::json!(false)
    );
    assert_eq!(
        anonymous.json()["result"]["bootstrap_required"],
        serde_json::json!(true)
    );
    let (cookie, csrf, connection_token) = bootstrap_and_login(port, data.path());
    let authenticated = call(
        port,
        &host(port),
        "GET",
        "/auth/state",
        &[("Cookie", &cookie)],
        None,
    );
    assert_eq!(
        authenticated.json()["result"]["authenticated"],
        serde_json::json!(true)
    );
    assert_eq!(
        authenticated.json()["result"]["csrf"].as_str().unwrap(),
        csrf,
        "csrf 应与会话绑定"
    );
    // 登出后再调用 API：401
    let logout = session_post(port, &cookie, Some(&csrf), "/auth/logout", "{}".to_string());
    assert_eq!(logout.status, 200);
    let after = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        r#"{"operation":"capabilities","input":{}}"#.to_string(),
    );
    assert_eq!(after.status, 401, "登出后会话立即失效");
    // 令牌仍可重新登录（复用语义与 CLI 一致）
    let relogin = call(
        port,
        &host(port),
        "POST",
        "/auth/login",
        &[
            ("Origin", &origin(port)),
            ("Content-Type", "application/json"),
        ],
        Some(&body_token(&connection_token)),
    );
    assert_eq!(relogin.status, 200, "连接令牌在撤销前可重复建立会话");
}

#[test]
fn s2_api_executes_as_client_and_trusted_reads_back_same_data() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let (cookie, csrf, _token) = bootstrap_and_login(port, data.path());
    let created = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"clothing.create","input":{"member_id":"default","name":"Web 栈创建的衬衫","category":"top"}})
            .to_string(),
    );
    assert_eq!(created.status, 200, "{}", created.text());
    assert!(
        created.json()["result"]["id"].is_string(),
        "应返回真实持久 ID"
    );
    let id = created.json()["result"]["id"].as_str().unwrap().to_string();
    // 另一个进程视角（等价 CLI）读同一数据库：一致
    let app = wardrobe_core::Application::open(&data.path().join("wardrobe.sqlite")).unwrap();
    let read_back = app
        .execute(
            "clothing.get",
            serde_json::json!({"member_id":"default","id":id}),
        )
        .unwrap();
    assert_eq!(read_back["name"], serde_json::json!("Web 栈创建的衬衫"));
}

#[test]
fn s2_limited_grant_member_isolation_over_http() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let member_a = trusted_member(data.path(), "姐姐");
    let member_b = trusted_member(data.path(), "妹妹");
    let app = wardrobe_core::Application::open(&data.path().join("wardrobe.sqlite")).unwrap();
    app.execute(
        "clothing.create",
        serde_json::json!({"member_id":member_a,"name":"姐姐的外套","category":"outerwear"}),
    )
    .unwrap();
    let grant = app
        .execute(
            "client.authorize",
            serde_json::json!({"name":"受限浏览器","mode":"limited","operations":["clothing.list"],"member_ids":[member_a],"expires_at":null}),
        )
        .unwrap();
    let token = grant["token"].as_str().unwrap().to_string();
    let login = call(
        port,
        &host(port),
        "POST",
        "/auth/login",
        &[
            ("Origin", &origin(port)),
            ("Content-Type", "application/json"),
        ],
        Some(&body_token(&token)),
    );
    assert_eq!(login.status, 200);
    let cookie = login
        .headers
        .get("set-cookie")
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let state = call(
        port,
        &host(port),
        "GET",
        "/auth/state",
        &[("Cookie", &cookie)],
        None,
    );
    let csrf = state.json()["result"]["csrf"].as_str().unwrap().to_string();
    // 授权内成员：可见且只有自己的数据
    let listed = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"clothing.list","input":{"member_id":member_a}}).to_string(),
    );
    assert_eq!(listed.status, 200);
    let listed_json = listed.json();
    let items = listed_json["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], serde_json::json!("姐姐的外套"));
    // 越权成员：member_denied，且读不到对方数据
    let denied = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"clothing.list","input":{"member_id":member_b}}).to_string(),
    );
    assert_eq!(denied.status, 200, "业务错误按 CLI 语义返回信封");
    assert_eq!(
        denied.json()["error"]["code"],
        serde_json::json!("member_denied")
    );
}

#[test]
fn s3_missing_or_forged_origin_and_csrf_are_rejected() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let (cookie, csrf, _token) = bootstrap_and_login(port, data.path());
    let body = r#"{"operation":"capabilities","input":{}}"#.to_string();
    // 缺失 Origin
    let no_origin = call(
        port,
        &host(port),
        "POST",
        "/api",
        &[
            ("Cookie", &cookie),
            ("X-Wardrobe-CSRF", &csrf),
            ("Content-Type", "application/json"),
        ],
        Some(body.as_bytes()),
    );
    assert_eq!(no_origin.status, 403, "缺失 Origin 的变更请求必须拒绝");
    // 跨源 Origin
    let evil = call(
        port,
        &host(port),
        "POST",
        "/api",
        &[
            ("Origin", "http://attacker.example"),
            ("Cookie", &cookie),
            ("X-Wardrobe-CSRF", &csrf),
            ("Content-Type", "application/json"),
        ],
        Some(body.as_bytes()),
    );
    assert_eq!(evil.status, 403, "跨源 Origin 必须拒绝");
    // 同源但缺失 CSRF 头
    let no_csrf = call(
        port,
        &host(port),
        "POST",
        "/api",
        &[
            ("Origin", &origin(port)),
            ("Cookie", &cookie),
            ("Content-Type", "application/json"),
        ],
        Some(body.as_bytes()),
    );
    assert_eq!(no_csrf.status, 403, "缺少会话 CSRF 令牌必须拒绝");
    // 错误 CSRF 头
    let bad_csrf = session_post(
        port,
        &cookie,
        Some("00000000000000000000000000000000"),
        "/api",
        body,
    );
    assert_eq!(bad_csrf.status, 403, "错误 CSRF 令牌必须拒绝");
    // 全部正确：放行
    let good = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        r#"{"operation":"capabilities","input":{}}"#.to_string(),
    );
    assert_eq!(good.status, 200, "{}", good.text());
    assert!(
        good.json()["result"]["operations"].is_array(),
        "合法会话应得到 capabilities：{}",
        good.text()
    );
}

#[test]
fn s3_privileged_operations_never_enter_session_api() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let (cookie, csrf, _token) = bootstrap_and_login(port, data.path());
    for operation in [
        "client.list",
        "client.authorize",
        "provider.credential.status",
        "encryption.status",
        "schedule.runtime.configure",
    ] {
        let response = session_post(
            port,
            &cookie,
            Some(&csrf),
            "/api",
            serde_json::json!({"operation":operation,"input":{}}).to_string(),
        );
        let code = response.json()["error"]["code"].clone();
        assert!(
            code == serde_json::json!("scope_denied")
                || code == serde_json::json!("unsupported_operation"),
            "{operation} 必须被核心拒绝（scope_denied/unsupported_operation），实际 {code}"
        );
    }
}

// ---------- S4：真实照片上传、导入、预览与任务收据 ----------

#[test]
fn s4_upload_imports_real_photo_and_preview_serves_authorized_bytes() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let (cookie, csrf, _token) = bootstrap_and_login(port, data.path());
    let uploaded = call(
        port,
        &host(port),
        "POST",
        &format!("/api/upload?member=default"),
        &[
            ("Origin", &origin(port)),
            ("Cookie", &cookie),
            ("X-Wardrobe-CSRF", &csrf),
            ("Content-Type", "image/png"),
        ],
        Some(PNG_1X1),
    );
    assert_eq!(uploaded.status, 200, "{}", uploaded.text());
    let uploaded_json = uploaded.json();
    let asset_id = uploaded_json["result"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("上传应返回资产：{}", uploaded.text()))
        .to_string();
    assert!(uploaded.json()["result"]["display"]["width"].is_number());
    // 预览：走 assets.preview 成员授权，返回 data_url
    let preview = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"assets.preview","input":{"member_id":"default","id":asset_id,"variant":"display"}})
            .to_string(),
    );
    assert_eq!(preview.status, 200, "{}", preview.text());
    let preview_json = preview.json();
    let data_url = preview_json["result"]["data_url"].as_str().unwrap();
    assert!(data_url.starts_with("data:image/webp;base64,"));
    // 上传后临时目录不残留
    let tmp = data.path().join("web-uploads-tmp");
    assert!(
        std::fs::read_dir(&tmp).unwrap().next().is_none(),
        "上传临时文件必须清理"
    );
}

#[test]
fn s4_upload_rejects_oversize_bad_type_and_unknown_member() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let (cookie, csrf, _token) = bootstrap_and_login(port, data.path());
    let oversized = vec![0u8; 20 * 1024 * 1024 + 1];
    let too_big = call(
        port,
        &host(port),
        "POST",
        "/api/upload?member=default",
        &[
            ("Origin", &origin(port)),
            ("Cookie", &cookie),
            ("X-Wardrobe-CSRF", &csrf),
            ("Content-Type", "image/png"),
        ],
        Some(&oversized),
    );
    assert_eq!(too_big.status, 413, "超过 20MiB 必须拒绝");
    let wrong_type = call(
        port,
        &host(port),
        "POST",
        "/api/upload?member=default",
        &[
            ("Origin", &origin(port)),
            ("Cookie", &cookie),
            ("X-Wardrobe-CSRF", &csrf),
            ("Content-Type", "text/html"),
        ],
        Some(b"<html></html>"),
    );
    assert_eq!(wrong_type.status, 400, "非图片类型必须拒绝");
    let unknown_member = call(
        port,
        &host(port),
        "POST",
        "/api/upload?member=ghost",
        &[
            ("Origin", &origin(port)),
            ("Cookie", &cookie),
            ("X-Wardrobe-CSRF", &csrf),
            ("Content-Type", "image/png"),
        ],
        Some(PNG_1X1),
    );
    assert_eq!(unknown_member.status, 200);
    assert_eq!(
        unknown_member.json()["error"]["code"],
        serde_json::json!("not_found"),
        "未知成员按核心语义拒绝"
    );
    let tmp = data.path().join("web-uploads-tmp");
    assert!(
        std::fs::read_dir(&tmp).unwrap().next().is_none(),
        "拒绝的上传不得落残留"
    );
}

#[test]
fn s4_batch_upload_drives_real_job_receipts_to_commit() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let (cookie, csrf, _token) = bootstrap_and_login(port, data.path());
    let encoded = base64::engine::general_purpose::STANDARD.encode(PNG_1X1);
    let batch = serde_json::json!({
        "member_id": "default",
        "operation_key": "web-batch-1",
        "images": [
            {"name": "照片一.png", "type": "image/png", "data": encoded},
            {"name": "照片二.png", "type": "image/png", "data": encoded}
        ]
    });
    let created = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api/batch-upload",
        batch.to_string(),
    );
    assert_eq!(created.status, 200, "{}", created.text());
    let batch_id = created.json()["result"]["id"].as_str().unwrap().to_string();
    // 幂等：同 operation_key 重放返回同一批次
    let replay = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api/batch-upload",
        batch.to_string(),
    );
    assert_eq!(
        replay.json()["result"]["id"],
        serde_json::json!(batch_id),
        "同幂等键重放应返回原批次：{}",
        replay.text()
    );
    // 逐项处理（真实压缩链路），任务收据可见
    let mut item_ids = vec![];
    let created_json = created.json();
    for draft in created_json["result"]["drafts"]
        .as_array()
        .unwrap_or_else(|| panic!("批量应返回草稿：{}", created.text()))
    {
        for item in draft["item_ids"].as_array().unwrap() {
            item_ids.push(item.as_str().unwrap().to_string());
        }
    }
    assert_eq!(item_ids.len(), 2);
    for item in &item_ids {
        let processed = session_post(
            port,
            &cookie,
            Some(&csrf),
            "/api",
            serde_json::json!({"operation":"imports.process","input":{"member_id":"default","id":batch_id,"item_id":item}})
                .to_string(),
        );
        assert_eq!(processed.status, 200, "{}", processed.text());
    }
    let after = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"imports.get","input":{"member_id":"default","id":batch_id}})
            .to_string(),
    );
    for job in after.json()["result"]["items"].as_array().unwrap() {
        assert_eq!(
            job["state"],
            serde_json::json!("waiting_input"),
            "处理完成应待确认入库"
        );
        assert!(job["asset_id"].is_string(), "任务收据应包含资产");
    }
    // 逐任务查询：jobs.list 可见收据
    let jobs = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"jobs.list","input":{"member_id":"default"}}).to_string(),
    );
    assert!(jobs.json()["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|job| job["kind"] == serde_json::json!("import")));
    // 提交第一个草稿入库
    let draft_id = created.json()["result"]["drafts"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let committed = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"imports.commit","input":{"member_id":"default","id":batch_id,"draft_id":draft_id,"operation_key":"commit-web-1","facts":{"category":"top"}}})
            .to_string(),
    );
    assert_eq!(committed.status, 200, "{}", committed.text());
    let clothes = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"clothing.list","input":{"member_id":"default"}})
            .to_string(),
    );
    assert_eq!(
        clothes.json()["result"]["items"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn s4_batch_upload_rejects_oversize_and_leaves_no_residue() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let (cookie, csrf, _token) = bootstrap_and_login(port, data.path());
    let encoded = base64::engine::general_purpose::STANDARD.encode(PNG_1X1);
    let too_many: Vec<serde_json::Value> = (0..101)
        .map(|_| serde_json::json!({"name":"a.png","type":"image/png","data":encoded}))
        .collect();
    let rejected = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api/batch-upload",
        serde_json::json!({"member_id":"default","operation_key":"too-many","images":too_many})
            .to_string(),
    );
    assert_eq!(rejected.status, 400, "超过 100 文件必须拒绝");
    let tmp = data.path().join("web-uploads-tmp");
    assert!(
        std::fs::read_dir(&tmp).unwrap().next().is_none(),
        "拒绝的批量不得残留临时文件"
    );
}

// ---------- S5：备份与恢复经 Web 会话（maintenance 通道，ADR0022 边界不变） ----------

#[test]
fn s5_backup_and_preview_drive_through_web_session() {
    let data = tempfile::tempdir().unwrap();
    let port = start(0, data.path(), None);
    let (cookie, csrf, _token) = bootstrap_and_login(port, data.path());
    let backup_dir = data.path().join("backups").join("pkg-1");
    let clothing = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"clothing.create","input":{"member_id":"default","name":"备份前的大衣","category":"outerwear"}})
            .to_string(),
    );
    assert!(
        clothing.json()["result"]["id"].is_string(),
        "{}",
        clothing.text()
    );
    let queued = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"backup.create","input":{"directory":backup_dir.display().to_string(),"include_originals":false}})
            .to_string(),
    );
    assert_eq!(queued.status, 200, "{}", queued.text());
    let job = queued.json()["result"]["id"].as_str().unwrap().to_string();
    let driven = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"maintenance.worker.run","input":{"id":job}}).to_string(),
    );
    assert_eq!(
        driven.json()["result"]["state"],
        serde_json::json!("succeeded"),
        "{}",
        driven.text()
    );
    let preview_queued = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"backup.preview","input":{"directory":backup_dir.display().to_string()}})
            .to_string(),
    );
    assert_eq!(preview_queued.status, 200, "{}", preview_queued.text());
    let preview_job = preview_queued.json()["result"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let preview_done = session_post(
        port,
        &cookie,
        Some(&csrf),
        "/api",
        serde_json::json!({"operation":"maintenance.worker.run","input":{"id":preview_job}})
            .to_string(),
    );
    assert_eq!(
        preview_done.json()["result"]["state"],
        serde_json::json!("succeeded"),
        "{}",
        preview_done.text()
    );
    let incoming = preview_done.json()["result"]["result"]["incoming"]["clothing"]
        .as_u64()
        .unwrap_or_else(|| panic!("备份清单应含衣物计数：{}", preview_done.text()));
    assert_eq!(incoming, 1, "备份应包含会话创建的单品");
}
