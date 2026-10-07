//! 本机 Web 服务实现（ADR0029）：
//! - 只绑定 127.0.0.1；Host 白名单 + POST Origin 同源 + 会话 CSRF 三层防护
//! - 一切业务经 `Application::execute_as_client`（复用既有授权模型，特权操作天然拒绝）
//! - 首启一次性引导令牌（0600）→ 恰好一次创建浏览器 client grant
//! - 媒体只经回环上传到数据目录临时文件，再走既有 assets.import / imports.create
//! 本模块不含任何第二套业务逻辑；成功/业务错误信封与 CLI 一致。

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use wardrobe_core::Application;

const SESSION_COOKIE: &str = "wardrobe_session";
const MAX_JSON_BODY: usize = 1024 * 1024;
const MAX_UPLOAD_BODY: usize = 20 * 1024 * 1024;
const MAX_BATCH_BODY: usize = 64 * 1024 * 1024;
const MAX_BATCH_FILES: usize = 100;
const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
const SESSION_TTL: Duration = Duration::from_secs(12 * 3600);
const IMAGE_TYPES: &[(&str, &str)] = &[
    ("image/jpeg", "jpg"),
    ("image/png", "png"),
    ("image/webp", "webp"),
    ("image/heic", "heic"),
    ("image/heif", "heif"),
];

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub data_dir: PathBuf,
    pub web_dist: Option<PathBuf>,
}

impl Config {
    pub fn new(port: u16, data_dir: PathBuf) -> Self {
        Self {
            port,
            data_dir,
            web_dist: None,
        }
    }
    pub fn with_dist(mut self, web_dist: PathBuf) -> Self {
        self.web_dist = Some(web_dist);
        self
    }
}

pub struct RunningService {
    pub port: u16,
}

struct Session {
    grant_token: String,
    csrf: String,
    last_seen: Instant,
}

struct AppState {
    app: Mutex<Application>,
    sessions: Mutex<HashMap<String, Session>>,
    web_dist: Option<PathBuf>,
    upload_tmp: PathBuf,
    bootstrap_file: PathBuf,
    port: u16,
}

enum Body {
    Json(Value),
    Html(String),
    Bytes(Vec<u8>, String),
}

struct Resp {
    status: u16,
    body: Body,
    set_cookie: Option<String>,
}

impl Resp {
    fn json(status: u16, value: Value) -> Self {
        Self {
            status,
            body: Body::Json(value),
            set_cookie: None,
        }
    }
    fn error(status: u16, code: &str, message: &str) -> Self {
        Self::json(status, json!({"error":{"code":code,"message":message}}))
    }
    fn with_cookie(mut self, cookie: String) -> Self {
        self.set_cookie = Some(cookie);
        self
    }
}

pub fn spawn(config: Config) -> std::io::Result<RunningService> {
    std::fs::create_dir_all(&config.data_dir)?;
    let database = config.data_dir.join("wardrobe.sqlite");
    let application = Application::open(&database).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("无法打开衣橱数据：{}", error.message),
        )
    })?;
    let upload_tmp = config.data_dir.join("web-uploads-tmp");
    std::fs::create_dir_all(&upload_tmp)?;
    cleanup_stale_uploads(&upload_tmp);
    let bootstrap_file = config.data_dir.join("web-bootstrap-token");
    ensure_bootstrap_token(&bootstrap_file, &application);
    let server = arc_server(
        tiny_http::Server::http(("127.0.0.1", config.port))
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::AddrInUse, format!("{error}")))?,
    );
    let port = match server.server_addr() {
        tiny_http::ListenAddr::IP(address) => address.port(),
        tiny_http::ListenAddr::Unix(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "不支持 Unix 套接字",
            ))
        }
    };
    let state = AppState {
        app: Mutex::new(application),
        sessions: Mutex::new(HashMap::new()),
        web_dist: config.web_dist,
        upload_tmp,
        bootstrap_file,
        port,
    };
    let state = std::sync::Arc::new(state);
    eprintln!("[wardrobe-web] 衣橱本机服务已启动：http://127.0.0.1:{port}/");
    eprintln!("[wardrobe-web] 数据目录：{}", config.data_dir.display());
    match &state.web_dist {
        Some(directory) => eprintln!("[wardrobe-web] 界面目录：{}", directory.display()),
        None => eprintln!(
            "[wardrobe-web] 未配置界面目录（--web-dist 或 WARDROBE_WEB_DIST），首页将显示提示页；API 仍可用。"
        ),
    }
    for index in 0..8 {
        let server = server.clone();
        let state = state.clone();
        std::thread::Builder::new()
            .name(format!("wardrobe-web-{index}"))
            .spawn(move || loop {
                match server.recv() {
                    Ok(request) => {
                        let response = handle(&state, request);
                        if let Err(error) = response {
                            eprintln!("[wardrobe-web] 响应失败：{error}");
                        }
                    }
                    Err(error) => {
                        eprintln!("[wardrobe-web] 接收请求失败：{error}");
                        break;
                    }
                }
            })?;
    }
    Ok(RunningService { port })
}

fn arc_server(server: tiny_http::Server) -> std::sync::Arc<tiny_http::Server> {
    std::sync::Arc::new(server)
}

fn cleanup_stale_uploads(directory: &Path) {
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ignore = if path.is_dir() {
                std::fs::remove_dir_all(&path).is_err()
            } else {
                std::fs::remove_file(&path).is_err()
            };
            if ignore {
                eprintln!("[wardrobe-web] 清理残留上传文件失败：{}", path.display());
            }
        }
    }
}

fn ensure_bootstrap_token(file: &Path, application: &Application) {
    let has_clients = application
        .execute("client.list", json!({}))
        .ok()
        .and_then(|result| {
            result["items"]
                .as_array()
                .map(|items| !items.is_empty())
        })
        .unwrap_or(false);
    if has_clients {
        if file.exists() {
            let _ = std::fs::remove_file(file);
        }
        return;
    }
    if file.exists() {
        return;
    }
    let token = random_hex(32);
    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let mut options = std::fs::OpenOptions::new();
        let mut file_handle = options
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(file)
            .expect("应能写入引导令牌文件");
        file_handle.write_all(token.as_bytes()).ok();
    }
    #[cfg(not(unix))]
    {
        std::fs::write(file, &token).ok();
    }
    eprintln!(
        "[wardrobe-web] 首次使用：本机尚无客户端授权。请把引导令牌文件 {} 的内容粘贴到网页首启页（仅限本机操作员，使用一次后作废）。",
        file.display()
    );
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("系统随机数不可用");
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for index in 0..a.len() {
        diff |= a[index] ^ b[index];
    }
    diff == 0
}

fn header<'a>(request: &'a tiny_http::Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|candidate| candidate.field.as_str().to_ascii_lowercase() == name.to_ascii_lowercase())
        .map(|found| found.value.as_str())
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            ) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_map(url: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(query) = url.split_once('?').map(|(_, query)| query) {
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                map.insert(percent_decode(key), percent_decode(value));
            }
        }
    }
    map
}

fn read_body(
    request: &mut tiny_http::Request,
    limit: usize,
) -> Result<Vec<u8>, &'static str> {
    let declared = request.body_length();
    if let Some(length) = declared {
        if length > limit {
            // 排空请求体再回应答，浏览器与测试客户端才能读到 413 状态。
            let mut remaining = length;
            let mut sink = [0u8; 65536];
            let reader = request.as_reader();
            while remaining > 0 {
                let want = remaining.min(sink.len());
                match reader.read(&mut sink[..want]) {
                    Ok(0) => break,
                    Ok(read) => remaining -= read,
                    Err(_) => break,
                }
            }
            return Err("payload_too_large");
        }
    }
    let mut buffer = Vec::new();
    {
        let reader = request.as_reader();
        reader
            .take(limit as u64 + 1)
            .read_to_end(&mut buffer)
            .map_err(|_| "invalid_body")?;
    }
    if buffer.len() > limit {
        return Err("payload_too_large");
    }
    Ok(buffer)
}

fn session_cookie(request: &tiny_http::Request) -> Option<String> {
    let cookies = header(request, "Cookie")?;
    for pair in cookies.split(';') {
        if let Some((name, value)) = pair.trim().split_once('=') {
            if name == SESSION_COOKIE {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn session_cookie_value(session_id: &str) -> String {
    format!("{SESSION_COOKIE}={session_id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200")
}

fn cleared_cookie() -> String {
    format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

/// 返回 (grant_token, csrf)。过期会话在此清除。
fn session_of(state: &AppState, request: &tiny_http::Request) -> Option<(String, String)> {
    let session_id = session_cookie(request)?;
    let mut sessions = state.sessions.lock().unwrap();
    let session = sessions.get_mut(&session_id)?;
    if session.last_seen.elapsed() > SESSION_TTL {
        sessions.remove(&session_id);
        return None;
    }
    session.last_seen = Instant::now();
    Some((session.grant_token.clone(), session.csrf.clone()))
}

fn host_allowed(state: &AppState, request: &tiny_http::Request) -> bool {
    match header(request, "Host") {
        Some(host) => {
            let host = host.to_ascii_lowercase();
            host == format!("127.0.0.1:{}", port_of(state))
                || host == format!("localhost:{}", port_of(state))
        }
        None => false,
    }
}

fn port_of(state: &AppState) -> u16 {
    state.port
}

fn origin_allowed(request: &tiny_http::Request) -> bool {
    let Some(host) = header(request, "Host") else {
        return false;
    };
    match header(request, "Origin") {
        Some(origin) => origin.to_ascii_lowercase() == format!("http://{}", host.to_ascii_lowercase()),
        None => false,
    }
}

fn handle(
    state: &AppState,
    mut request: tiny_http::Request,
) -> Result<(), std::io::Error> {
    let method = request.method().to_string();
    let url = request.url().to_string();
    let path = percent_decode(url.split('?').next().unwrap_or("/")).trim_end_matches('?').to_string();
    let response = if !host_allowed(state, &request) {
        Resp::error(403, "forbidden_host", "仅允许通过 127.0.0.1 或 localhost 访问本机服务")
    } else if method == "GET" || method == "HEAD" {
        route_get(state, &request, &path)
    } else if method == "POST" {
        if !origin_allowed(&request) {
            Resp::error(403, "forbidden_origin", "变更请求必须来自同源页面")
        } else {
            route_post(state, &mut request, &path, &url)
        }
    } else {
        Resp::error(405, "method_not_allowed", "此服务仅支持 GET 与 POST")
    };
    let status = response.status;
    let outcome = respond(request, response);
    eprintln!("[wardrobe-web] {method} {path} → {status}");
    outcome
}

fn route_get(state: &AppState, request: &tiny_http::Request, path: &str) -> Resp {
    match path {
        "/health" => Resp::json(
            200,
            json!({"schema_version":1,"ok":true,"service":"wardrobe-web"}),
        ),
        "/auth/state" => auth_state(state, request),
        "/api" | "/api/upload" | "/api/batch-upload" | "/auth/login" | "/auth/logout"
        | "/auth/bootstrap" => Resp::error(405, "method_not_allowed", "此路径仅支持 POST"),
        _ => serve_static(state, path),
    }
}

fn route_post(
    state: &AppState,
    request: &mut tiny_http::Request,
    path: &str,
    url: &str,
) -> Resp {
    match path {
        "/health" => Resp::error(405, "method_not_allowed", "health 仅支持 GET"),
        "/auth/state" => Resp::error(405, "method_not_allowed", "auth/state 仅支持 GET"),
        "/auth/bootstrap" => auth_bootstrap(state, request),
        "/auth/login" => auth_login(state, request),
        "/auth/logout" => auth_logout(state, request),
        "/api" => api_bridge(state, request),
        "/api/upload" => api_upload(state, request, url),
        "/api/batch-upload" => api_batch_upload(state, request),
        _ => Resp::error(404, "not_found", "此服务没有该路径"),
    }
}

fn auth_state(state: &AppState, request: &tiny_http::Request) -> Resp {
    let bootstrap_required = state.bootstrap_file.exists();
    let session = session_of(state, request);
    let payload = match session {
        Some((_grant, csrf)) => json!({
            "schema_version":1,
            "result":{
                "authenticated":true,
                "bootstrap_required":bootstrap_required,
                "bootstrap_token_path":Value::Null,
                "csrf":csrf,
            }
        }),
        None => json!({
            "schema_version":1,
            "result":{
                "authenticated":false,
                "bootstrap_required":bootstrap_required,
                "bootstrap_token_path": if bootstrap_required {
                    json!(state.bootstrap_file.display().to_string())
                } else {
                    Value::Null
                },
                "csrf":Value::Null,
            }
        }),
    };
    Resp::json(200, payload)
}

fn auth_bootstrap(state: &AppState, request: &mut tiny_http::Request) -> Resp {
    let body = match read_body(request, MAX_JSON_BODY) {
        Ok(bytes) => bytes,
        Err(code) => return body_error(code),
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return Resp::error(400, "invalid_json", "输入不是有效 JSON"),
    };
    let Some(token) = parsed["token"].as_str() else {
        return Resp::error(400, "invalid_input", "请提供引导令牌");
    };
    let stored = match std::fs::read_to_string(&state.bootstrap_file) {
        Ok(content) => content.trim().to_string(),
        Err(_) => {
            return Resp::error(
                403,
                "bootstrap_unavailable",
                "本机衣橱已有客户端授权，或引导令牌已使用；请直接用连接令牌登录",
            )
        }
    };
    if !constant_time_eq(token, &stored) {
        return Resp::error(403, "bootstrap_token_invalid", "引导令牌不正确");
    }
    {
        let app = state.app.lock().unwrap();
        // 双重防线：已有客户端授权时不再引导。
        let has_clients = app
            .execute("client.list", json!({}))
            .ok()
            .and_then(|result| result["items"].as_array().map(|items| !items.is_empty()))
            .unwrap_or(true);
        if has_clients {
            let _ = std::fs::remove_file(&state.bootstrap_file);
            return Resp::error(403, "bootstrap_unavailable", "本机衣橱已有客户端授权，请直接登录");
        }
        let created = app.execute(
            "client.authorize",
            json!({
                "name":"Web 浏览器",
                "mode":"full",
                "operations":[],
                "member_ids":Value::Null,
                "expires_at":Value::Null,
            }),
        );
        match created {
            Ok(doc) => {
                let grant_token = doc["token"].as_str().expect("授权应返回一次性令牌").to_string();
                let session_id = random_hex(32);
                let csrf = random_hex(16);
                state.sessions.lock().unwrap().insert(
                    session_id.clone(),
                    Session {
                        grant_token,
                        csrf: csrf.clone(),
                        last_seen: Instant::now(),
                    },
                );
                let _ = std::fs::remove_file(&state.bootstrap_file);
                Resp::json(
                    200,
                    json!({
                        "schema_version":1,
                        "result":{
                            "token":doc["token"],
                            "client_id":doc["id"],
                            "note":"以上连接令牌只显示这一次：请保存到密码管理器，供其他浏览器或服务重启后登录。"
                        }
                    }),
                )
                .with_cookie(session_cookie_value(&session_id))
            }
            Err(error) => Resp::json(
                409,
                json!({"error":{"code":error.code,"message":error.message}}),
            ),
        }
    }
}

fn auth_login(state: &AppState, request: &mut tiny_http::Request) -> Resp {
    let body = match read_body(request, MAX_JSON_BODY) {
        Ok(bytes) => bytes,
        Err(code) => return body_error(code),
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return Resp::error(400, "invalid_json", "输入不是有效 JSON"),
    };
    let Some(token) = parsed["token"].as_str() else {
        return Resp::error(400, "invalid_input", "请提供连接令牌");
    };
    let app = state.app.lock().unwrap();
    match app.execute_as_client(token, "capabilities", json!({})) {
        Ok(_) => {
            let session_id = random_hex(32);
            let csrf = random_hex(16);
            state.sessions.lock().unwrap().insert(
                session_id.clone(),
                Session {
                    grant_token: token.to_string(),
                    csrf,
                    last_seen: Instant::now(),
                },
            );
            Resp::json(200, json!({"schema_version":1,"result":{"authenticated":true}}))
                .with_cookie(session_cookie_value(&session_id))
        }
        Err(error) => Resp::json(401, json!({"error":{"code":error.code,"message":error.message}})),
    }
}

fn auth_logout(state: &AppState, request: &mut tiny_http::Request) -> Resp {
    let Some(session_id) = session_cookie(request) else {
        return Resp::error(401, "unauthorized", "尚未登录");
    };
    let csrf = {
        let mut sessions = state.sessions.lock().unwrap();
        sessions.remove(&session_id).map(|session| session.csrf)
    };
    match csrf {
        Some(csrf) => {
            let provided = header(request, "X-Wardrobe-CSRF");
            if provided != Some(csrf.as_str()) {
                return Resp::error(403, "csrf_denied", "会话校验失败，请刷新页面重试");
            }
            Resp::json(200, json!({"schema_version":1,"result":{"logged_out":true}}))
                .with_cookie(cleared_cookie())
        }
        None => Resp::error(401, "unauthorized", "尚未登录").with_cookie(cleared_cookie()),
    }
}

fn require_session(
    state: &AppState,
    request: &tiny_http::Request,
) -> Result<(String, String), Resp> {
    let (grant, csrf) = session_of(state, request).ok_or_else(|| {
        Resp::error(401, "unauthorized", "请先登录本机衣橱服务")
    })?;
    let provided = header(request, "X-Wardrobe-CSRF");
    if provided != Some(csrf.as_str()) {
        return Err(Resp::error(403, "csrf_denied", "会话校验失败，请刷新页面重试"));
    }
    Ok((grant, csrf))
}

fn api_bridge(state: &AppState, request: &mut tiny_http::Request) -> Resp {
    let (grant, _csrf) = match require_session(state, request) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let body = match read_body(request, MAX_JSON_BODY) {
        Ok(bytes) => bytes,
        Err(code) => return body_error(code),
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return Resp::error(400, "invalid_json", "输入不是有效 JSON"),
    };
    let Some(operation) = parsed["operation"].as_str() else {
        return Resp::error(400, "invalid_input", "请提供 operation");
    };
    let input = if parsed["input"].is_null() {
        json!({})
    } else {
        parsed["input"].clone()
    };
    let app = state.app.lock().unwrap();
    match app.execute_as_client(&grant, operation, input) {
        Ok(result) => Resp::json(200, json!({"schema_version":1,"result":result})),
        Err(error) => Resp::json(200, json!({"schema_version":1,"error":{"code":error.code,"message":error.message}})),
    }
}

fn image_type_extension(content_type: &str) -> Option<&'static str> {
    IMAGE_TYPES
        .iter()
        .find(|(mime, _)| mime.eq_ignore_ascii_case(content_type))
        .map(|(_, extension)| *extension)
}

fn api_upload(state: &AppState, request: &mut tiny_http::Request, url: &str) -> Resp {
    let (grant, _csrf) = match require_session(state, request) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let query = query_map(url);
    let Some(member) = query.get("member").filter(|value| !value.is_empty()) else {
        return Resp::error(400, "invalid_input", "请指定成员");
    };
    let content_type = header(request, "Content-Type").unwrap_or_default().to_string();
    let Some(extension) = image_type_extension(content_type.split(';').next().unwrap_or_default().trim()) else {
        return Resp::error(400, "unsupported_image_type", "请选择 JPG、PNG、WebP 或 HEIC 图片");
    };
    let bytes = match read_body(request, MAX_UPLOAD_BODY) {
        Ok(bytes) => bytes,
        Err(code) => return body_error(code),
    };
    if bytes.is_empty() {
        return Resp::error(400, "invalid_input", "请选择图片文件");
    }
    let purpose = query.get("purpose").map(String::as_str).unwrap_or("asset");
    if !matches!(purpose, "asset" | "person") {
        return Resp::error(400, "invalid_input", "不支持的上传用途");
    }
    let file_name = format!("upload-{}.{}", random_hex(8), extension);
    let temp_path = state.upload_tmp.join(&file_name);
    if let Err(error) = std::fs::write(&temp_path, &bytes) {
        return Resp::error(
            500,
            "storage_error",
            &format!("上传临时文件写入失败：{error}"),
        );
    }
    let app = state.app.lock().unwrap();
    let outcome = match purpose {
        "asset" => app.execute_as_client(
            &grant,
            "assets.import",
            json!({"member_id":member,"path":temp_path.display().to_string()}),
        ),
        "person" => {
            let kind = query.get("person_kind").map(String::as_str).unwrap_or("");
            let rights = query.get("rights_confirmed").map(String::as_str) == Some("true");
            app.execute_as_client(
                &grant,
                "tryon.person.import",
                json!({
                    "member_id":member,
                    "path":temp_path.display().to_string(),
                    "person_kind":kind,
                    "rights_confirmed":rights,
                    "operation_key":format!("web-person-{}", random_hex(8)),
                }),
            )
        }
        _ => unreachable!(),
    };
    let _ = std::fs::remove_file(&temp_path);
    match outcome {
        Ok(result) => Resp::json(200, json!({"schema_version":1,"result":result})),
        Err(error) => Resp::json(200, json!({"schema_version":1,"error":{"code":error.code,"message":error.message}})),
    }
}

fn sanitize_key(key: &str) -> String {
    let filtered: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    filtered.chars().take(120).collect()
}

fn sanitize_file_name(name: &str, extension: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim();
    if base.is_empty() {
        return format!("photo.{extension}");
    }
    let safe: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "._- ".contains(c) || !c.is_ascii() { c } else { '_' })
        .collect();
    if safe.is_empty() {
        format!("photo.{extension}")
    } else {
        safe
    }
}

fn api_batch_upload(state: &AppState, request: &mut tiny_http::Request) -> Resp {
    let (grant, _csrf) = match require_session(state, request) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let body = match read_body(request, MAX_BATCH_BODY) {
        Ok(bytes) => bytes,
        Err(code) => return body_error(code),
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return Resp::error(400, "invalid_json", "输入不是有效 JSON"),
    };
    let Some(member) = parsed["member_id"].as_str() else {
        return Resp::error(400, "invalid_input", "请指定成员");
    };
    let Some(operation_key) = parsed["operation_key"].as_str() else {
        return Resp::error(400, "invalid_input", "请提供操作标识");
    };
    let Some(images) = parsed["images"].as_array() else {
        return Resp::error(400, "invalid_input", "请选择图片列表");
    };
    if images.is_empty() || images.len() > MAX_BATCH_FILES {
        return Resp::error(400, "invalid_input", "每批须为 1–100 张图片");
    }
    use base64::Engine as _;
    // 临时目录名由 operation_key 派生：同一幂等键重放得到同一组路径，
    // 核心 imports.create 的幂等语义（同 key 同 paths 返回原批次）才能生效。
    let batch_directory = state
        .upload_tmp
        .join(format!("batch-{}", sanitize_key(operation_key)));
    if let Err(error) = std::fs::create_dir_all(&batch_directory) {
        return Resp::error(500, "storage_error", &format!("批量临时目录创建失败：{error}"));
    }
    let mut paths = vec![];
    let mut total = 0usize;
    let mut failure: Option<Resp> = None;
    for (index, image) in images.iter().enumerate() {
        let declared_type = image["type"].as_str().unwrap_or_default();
        let Some(extension) = image_type_extension(declared_type) else {
            failure = Some(Resp::error(400, "unsupported_image_type", "请选择 JPG、PNG、WebP 或 HEIC 图片"));
            break;
        };
        let Some(data) = image["data"].as_str() else {
            failure = Some(Resp::error(400, "invalid_input", "图片数据缺失"));
            break;
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
            failure = Some(Resp::error(400, "invalid_input", "图片数据不是有效 base64"));
            break;
        };
        if bytes.len() > MAX_FILE_BYTES {
            failure = Some(Resp::error(413, "payload_too_large", "每张图片最多 20 MB"));
            break;
        }
        total += bytes.len();
        if total > MAX_BATCH_BODY {
            failure = Some(Resp::error(413, "payload_too_large", "单批图片总量最多 64 MB"));
            break;
        }
        let name = sanitize_file_name(image["name"].as_str().unwrap_or(""), extension);
        let path = batch_directory.join(format!("{index:03}-{name}"));
        if let Err(error) = std::fs::write(&path, &bytes) {
            failure = Some(Resp::error(500, "storage_error", &format!("图片暂存失败：{error}")));
            break;
        }
        paths.push(json!(path.display().to_string()));
    }
    let outcome = if let Some(failure) = failure {
        Err(failure)
    } else {
        let app = state.app.lock().unwrap();
        match app.execute_as_client(
            &grant,
            "imports.create",
            json!({"member_id":member,"operation_key":operation_key,"paths":paths}),
        ) {
            Ok(result) => Ok(Resp::json(200, json!({"schema_version":1,"result":result}))),
            Err(error) => Ok(Resp::json(200, json!({"schema_version":1,"error":{"code":error.code,"message":error.message}}))),
        }
    };
    let _ = std::fs::remove_dir_all(&batch_directory);
    match outcome {
        Ok(response) => response,
        Err(response) => response,
    }
}

fn body_error(code: &str) -> Resp {
    match code {
        "payload_too_large" => Resp::error(413, "payload_too_large", "请求体超过限制"),
        "invalid_body" => Resp::error(400, "invalid_body", "请求体读取失败"),
        _ => Resp::error(400, "invalid_request", "请求无法处理"),
    }
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn notice_page() -> Resp {
    Resp::with_body(Body::Html(format!(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><title>wardrobe-web</title></head><body><h1>衣橱本机服务已启动</h1><p>wardrobe-web 正在运行，但尚未配置浏览器界面目录（web-dist）。</p><p>请以 <code>--web-dist &lt;React 构建产物目录&gt;</code> 或环境变量 <code>WARDROBE_WEB_DIST</code> 重新启动服务；本页只说明状态，不是交付界面。</p><p>API 健康检查：<code>GET /health</code>；登录与会话见 <code>/auth/*</code>。</p></body></html>"
    )))
}

fn serve_static(state: &AppState, path: &str) -> Resp {
    let Some(dist) = state.web_dist.clone() else {
        return notice_page();
    };
    let relative = path.trim_start_matches('/');
    let relative = if relative.is_empty() { "index.html" } else { relative };
    if relative.split('/').any(|component| component == ".." || component.contains('\0')) {
        return Resp::error(403, "forbidden_path", "非法路径");
    }
    let full = dist.join(relative);
    let Ok(dist_canonical) = dist.canonicalize() else {
        return notice_page();
    };
    let canonical = match full.canonicalize() {
        Ok(canonical) => canonical,
        Err(_) => {
            // SPA 回退：无扩展名路径交给前端路由；带扩展名但不存在 → 404
            if !relative.contains('.') {
                match std::fs::read(dist_canonical.join("index.html")) {
                    Ok(bytes) => return Resp::with_body(Body::Bytes(bytes, "text/html; charset=utf-8".into())),
                    Err(_) => return notice_page(),
                }
            }
            return Resp::error(404, "not_found", "文件不存在");
        }
    };
    if !canonical.starts_with(&dist_canonical) {
        return Resp::error(403, "forbidden_path", "非法路径");
    }
    if canonical.is_file() {
        match std::fs::read(&canonical) {
            Ok(bytes) => Resp::with_body(Body::Bytes(bytes, mime_for(&canonical).into())),
            Err(_) => Resp::error(500, "storage_error", "静态文件读取失败"),
        }
    } else if !relative.contains('.') {
        match std::fs::read(dist_canonical.join("index.html")) {
            Ok(bytes) => Resp::with_body(Body::Bytes(bytes, "text/html; charset=utf-8".into())),
            Err(_) => notice_page(),
        }
    } else {
        Resp::error(404, "not_found", "文件不存在")
    }
}

impl Resp {
    fn with_body(body: Body) -> Self {
        Self {
            status: 200,
            body,
            set_cookie: None,
        }
    }
}

fn respond(request: tiny_http::Request, response: Resp) -> Result<(), std::io::Error> {
    let mut tiny_response = match response.body {
        Body::Json(value) => tiny_http::Response::from_string(value.to_string()).with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..])
                .unwrap(),
        ),
        Body::Html(page) => tiny_http::Response::from_string(page).with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                .unwrap(),
        ),
        Body::Bytes(bytes, mime) => tiny_http::Response::from_data(bytes).with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap(),
        ),
    }
    .with_status_code(response.status)
    .with_header(
        tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap(),
    );
    if let Some(cookie) = response.set_cookie {
        tiny_response.add_header(
            tiny_http::Header::from_bytes(&b"Set-Cookie"[..], cookie.as_bytes())
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "无效 Cookie"))?,
        );
    }
    request.respond(tiny_response).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::BrokenPipe, format!("{error}"))
    })
}
