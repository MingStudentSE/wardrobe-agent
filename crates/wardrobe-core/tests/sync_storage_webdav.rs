//! #30 WebDAV 适配器外部契约：与 `sync_storage.rs`（LocalFolder）同一套
//! 传输契约，跑在真实 Apache mod_dav 服务器上（条件 PUT、Basic 认证、
//! LOCK/423）。全部走公开 `SyncStorage` trait，不断言内部实现。
//!
//! 服务器不支持条件写入的能力缺失路径（dufs）单独成测，明确拒绝而非静默。
#![cfg(unix)]
#[path = "support/webdav_server.rs"]
mod webdav_server;

use std::io::Read;
use wardrobe_core::sync::storage::{SyncStorage, WebDav, WebDavConfig, WriteCondition};
use webdav_server::{PASSWORD, USERNAME};

fn connect(server: &webdav_server::WebDavServer) -> WebDav {
    WebDav::connect(WebDavConfig {
        url: server.base_url.clone(),
        root: "wardrobe-sync".into(),
        username: USERNAME.into(),
        password: PASSWORD.into(),
        timeout: std::time::Duration::from_secs(20),
    })
    .unwrap()
}

#[test]
#[ignore = "requires local Apache httpd WebDAV server"]
fn webdav_pages_and_conditional_writes_and_deletes_preserve_etag_versions() {
    let server = webdav_server::WebDavServer::spawn();
    let storage = connect(&server);
    let capabilities = storage.capabilities();
    assert!(
        capabilities.conditional_write,
        "mod_dav enforces conditional writes; absence must be rejected at connect"
    );
    assert!(
        !capabilities.atomic_publication,
        "WebDAV must not claim atomic rename; publication uses markers"
    );
    for i in (0..40).rev() {
        storage
            .put(
                &format!("changes/{i:04}.object"),
                &mut b"one".as_slice(),
                WriteCondition::CreateOnly,
            )
            .unwrap();
    }
    let mut cursor = None;
    let mut all = vec![];
    loop {
        let page = storage.list("changes", cursor.as_deref(), 7).unwrap();
        assert!(page.items.len() <= 7);
        all.extend(page.items.into_iter().map(|v| v.key));
        cursor = page.cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(all.len(), 40);
    assert!(
        all.windows(2).all(|v| v[0] < v[1]),
        "deterministic key order"
    );
    let old = storage.stat(&all[0]).unwrap();
    // ETag is only an opaque conditional-write token — never a content hash.
    use sha2::{Digest, Sha256};
    let content_hash = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    assert_ne!(
        old.version,
        content_hash(b"one"),
        "ETag must not be a content hash"
    );
    assert_eq!(
        storage
            .put(
                &all[0],
                &mut b"duplicate".as_slice(),
                WriteCondition::CreateOnly
            )
            .unwrap_err()
            .code,
        "storage_precondition",
        "server-enforced create-only"
    );
    let new = storage
        .put(
            &all[0],
            // 长度必须不同：mod_dav 的弱 ETag 是 (size, mtime) 派生，同秒同长度的
            // 覆盖可能得到相同 ETag。ETag 只是条件写令牌，引擎正确性不依赖
            // "覆盖后版本必变"。
            &mut b"much longer new body".as_slice(),
            WriteCondition::MatchVersion(old.version.clone()),
        )
        .unwrap();
    assert_ne!(old.version, new.version);
    assert_eq!(
        storage
            .delete(&all[0], WriteCondition::MatchVersion(old.version))
            .unwrap_err()
            .code,
        "storage_precondition",
        "stale conditional delete cannot remove a freshly published version"
    );
    storage
        .delete(&all[0], WriteCondition::MatchVersion(new.version))
        .unwrap();
    assert!(storage.stat(&all[0]).is_err());
    assert_eq!(
        storage
            .delete(&all[0], WriteCondition::Any)
            .unwrap_err()
            .code,
        "storage_not_found"
    );
    assert!(storage.list("changes", None, 0).is_err());
    assert!(storage
        .list("changes", Some("other/not-a-cursor"), 7)
        .is_err());
}

#[test]
#[ignore = "requires local Apache httpd WebDAV server"]
fn webdav_round_trip_streams_bytes_and_missing_keys_are_typed() {
    let server = webdav_server::WebDavServer::spawn();
    let storage = connect(&server);
    let payload: Vec<u8> = (0..400_000u32).map(|i| (i % 251) as u8).collect();
    let written = storage
        .put(
            "changes/streamed.object",
            &mut payload.as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    assert_eq!(written.bytes as usize, payload.len());
    let mut bytes = vec![];
    storage
        .get("changes/streamed.object")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, payload);
    let absent = match storage.get("changes/absent.object") {
        Ok(_) => panic!("missing objects must not be readable"),
        Err(e) => e,
    };
    assert_eq!(absent.code, "storage_not_found");
    assert_eq!(
        storage.stat("changes/absent.object").unwrap_err().code,
        "storage_not_found"
    );
    for key in [
        "/absolute",
        "a\\b",
        "a/../outside",
        "./object",
        "a//object",
        "a/./object",
        "a/\0object",
        "",
    ] {
        assert_eq!(
            storage
                .put(key, &mut b"bounded".as_slice(), WriteCondition::CreateOnly)
                .unwrap_err()
                .code,
            "invalid_object_key",
            "{key:?}"
        );
        assert_eq!(
            storage.stat(key).unwrap_err().code,
            "invalid_object_key",
            "{key:?}"
        );
    }
    assert!(
        server
            .data_dir()
            .read_dir()
            .unwrap()
            .all(|e| e.unwrap().file_name() == "wardrobe-sync"),
        "keys never escape the configured root"
    );
}

#[test]
#[ignore = "requires local Apache httpd WebDAV server"]
fn webdav_concurrent_create_only_has_a_single_winner_and_locked_resource_maps_to_storage_locked() {
    let server = webdav_server::WebDavServer::spawn();
    let storage = connect(&server);
    let first = storage
        .put(
            "changes/race.object",
            &mut b"first".as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    let storage_path = server.base_url.clone();
    let racer = {
        let storage_path = storage_path.clone();
        std::thread::spawn(move || {
            let storage = WebDav::connect(WebDavConfig {
                url: storage_path,
                root: "wardrobe-sync".into(),
                username: USERNAME.into(),
                password: PASSWORD.into(),
                timeout: std::time::Duration::from_secs(20),
            })
            .unwrap();
            storage.put(
                "changes/race.object",
                &mut b"second".as_slice(),
                WriteCondition::CreateOnly,
            )
        })
    };
    assert_eq!(
        racer.join().unwrap().unwrap_err().code,
        "storage_precondition",
        "the loser of a create-only race is a typed precondition failure"
    );
    let mut bytes = vec![];
    storage
        .get("changes/race.object")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"first");
    // A server-side DAV write lock (mod_dav LOCK) blocks unconditional writes:
    // the adapter must expose 423 as its own typed error, not transient noise.
    let lock_xml = r#"<?xml version="1.0"?><D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype><D:owner>fixture</D:owner></D:lockinfo>"#;
    let output = std::process::Command::new("/usr/bin/curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-u",
            &format!("{USERNAME}:{PASSWORD}"),
            "-X",
            "LOCK",
            "--data-binary",
            lock_xml,
            "-H",
            "Timeout: Second-30",
            &format!("{}/wardrobe-sync/changes/race.object", server.base_url),
        ])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "200");
    assert_eq!(
        storage
            .put(
                "changes/race.object",
                &mut b"blocked".as_slice(),
                WriteCondition::Any
            )
            .unwrap_err()
            .code,
        "storage_locked"
    );
    assert_eq!(
        storage
            .delete(
                "changes/race.object",
                WriteCondition::MatchVersion(first.version)
            )
            .unwrap_err()
            .code,
        "storage_locked"
    );
    let _ = first;
}

#[test]
#[ignore = "requires local Apache httpd WebDAV server and the dufs binary"]
fn webdav_connect_rejects_servers_that_ignore_conditional_writes() {
    let dufs = std::env::var("WARDROBE_DUFFS_BIN")
        .unwrap_or_else(|_| "/Users/links/.codex/wardrobe-execution/tools/bin/dufs".to_string());
    if !std::path::Path::new(&dufs).is_file() {
        panic!("dufs binary required at {dufs}");
    }
    let root =
        tempfile::tempdir_in("/Users/links/.codex/external-workspace-volume/codex/build-temp")
            .unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut child = std::process::Command::new(&dufs)
        .arg(root.path())
        .args(["-b", "127.0.0.1", "-p"])
        .arg(port.to_string())
        .args(["--auth", "dufsuser:dufspass@/:rw", "-A"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now();
    let mut ready = false;
    while deadline.elapsed() < std::time::Duration::from_secs(10) {
        let probe = std::process::Command::new("/usr/bin/curl")
            .args([
                "-s",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "-u",
                "dufsuser:dufspass",
                "-X",
                "PROPFIND",
                "-H",
                "Depth: 0",
                &format!("http://127.0.0.1:{port}/"),
            ])
            .output();
        if probe
            .map(|out| out.stdout == b"207".to_vec())
            .unwrap_or(false)
        {
            ready = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    assert!(ready, "dufs fixture never became ready");
    let result = WebDav::connect(WebDavConfig {
        url: format!("http://127.0.0.1:{port}"),
        root: "wardrobe-sync".into(),
        username: "dufsuser".into(),
        password: "dufspass".into(),
        timeout: std::time::Duration::from_secs(20),
    });
    let _ = child.kill();
    let _ = child.wait();
    let err = match result {
        Ok(_) => panic!("a server that ignores conditional writes must be rejected at connect"),
        Err(e) => e,
    };
    assert_eq!(
        err.code, "storage_unsupported",
        "expected typed storage_unsupported, got {}: {}",
        err.code, err.message
    );
}

#[test]
#[ignore = "requires local Apache httpd WebDAV server"]
fn webdav_auth_failures_are_typed_and_never_retried() {
    let server = webdav_server::WebDavServer::spawn();
    let started = std::time::Instant::now();
    let err = match WebDav::connect(WebDavConfig {
        url: server.base_url.clone(),
        root: "wardrobe-sync".into(),
        username: USERNAME.into(),
        password: "wrong password".into(),
        timeout: std::time::Duration::from_secs(20),
    }) {
        Ok(_) => panic!("wrong credentials must be rejected"),
        Err(e) => e,
    };
    assert_eq!(err.code, "storage_unauthorized");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "authentication failures must not burn the automatic retry budget"
    );
}

// ---- 故障注入契约：限流/超时/截断/断网经本地代理注入真实故障 ----

struct FaultProxy {
    port: u16,
    counter: std::path::PathBuf,
    child: std::process::Child,
}
impl FaultProxy {
    fn spawn(target_port: u16, mode: &str, param: usize) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let counter =
            tempfile::tempdir_in("/Users/links/.codex/external-workspace-volume/codex/build-temp")
                .unwrap()
                .keep()
                .join("counter.json");
        let script = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/webdav_fault_proxy.py"
        );
        let child = std::process::Command::new("python3")
            .arg(script)
            .arg(port.to_string())
            .arg(target_port.to_string())
            .arg(mode)
            .arg(param.to_string())
            .arg(&counter)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn fault proxy");
        let deadline = std::time::Instant::now();
        loop {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            assert!(
                deadline.elapsed() < std::time::Duration::from_secs(10),
                "fault proxy never became ready"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Self {
            port,
            counter,
            child,
        }
    }
    fn requests(&self) -> u64 {
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&self.counter).unwrap())
            .unwrap()["requests"]
            .as_u64()
            .unwrap()
    }
}
impl Drop for FaultProxy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn connect_via(port: u16, timeout: std::time::Duration) -> Result<WebDav, wardrobe_core::ApiError> {
    WebDav::connect(WebDavConfig {
        url: format!("http://127.0.0.1:{port}"),
        root: "wardrobe-sync".into(),
        username: USERNAME.into(),
        password: PASSWORD.into(),
        timeout,
    })
}

#[test]
#[ignore = "requires local Apache httpd WebDAV server"]
fn webdav_rate_limit_retries_within_budget_then_maps_to_storage_rate_limited() {
    let server = webdav_server::WebDavServer::spawn();
    // 单次 503：恰好触发一次自动重试（探针共 6 个请求 + 1 次重试 = 7）。
    let proxy = FaultProxy::spawn(
        server.base_url.rsplit(':').next().unwrap().parse().unwrap(),
        "rate-limit",
        1,
    );
    let port = proxy.port;
    connect_via(port, std::time::Duration::from_secs(20)).expect("one 503 must be retried away");
    assert_eq!(
        proxy.requests(),
        7,
        "exactly one automatic retry for one 503"
    );
    drop(proxy);
    // 持续限流：1 次原始 + 最多 2 次自动重试（父规格预算），之后类型化失败。
    let proxy = FaultProxy::spawn(
        server.base_url.rsplit(':').next().unwrap().parse().unwrap(),
        "rate-limit",
        100,
    );
    let port = proxy.port;
    let err = match connect_via(port, std::time::Duration::from_secs(20)) {
        Ok(_) => panic!("persistent rate limiting must fail"),
        Err(e) => e,
    };
    assert_eq!(err.code, "storage_rate_limited");
    assert_eq!(
        proxy.requests(),
        3,
        "retry budget: original request + at most 2 automatic retries"
    );
}

#[test]
#[ignore = "requires local Apache httpd WebDAV server"]
fn webdav_timeout_maps_to_transient_within_budget_and_dead_server_fails_fast() {
    let server = webdav_server::WebDavServer::spawn();
    // 首请求挂起超过客户端超时：自动重试后成功。
    let proxy = FaultProxy::spawn(
        server.base_url.rsplit(':').next().unwrap().parse().unwrap(),
        "stall",
        1,
    );
    let port = proxy.port;
    let started = std::time::Instant::now();
    connect_via(port, std::time::Duration::from_secs(1)).expect("stalled first attempt is retried");
    assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    drop(proxy);
    // 每个请求都超时：预算用尽后 storage_transient。
    let proxy = FaultProxy::spawn(
        server.base_url.rsplit(':').next().unwrap().parse().unwrap(),
        "stall",
        100,
    );
    let port = proxy.port;
    let err = match connect_via(port, std::time::Duration::from_secs(1)) {
        Ok(_) => panic!("persistent timeouts must fail"),
        Err(e) => e,
    };
    assert_eq!(err.code, "storage_transient");
    assert_eq!(proxy.requests(), 3, "retry budget holds for timeouts");
    drop(proxy);
    // 断网（连接拒绝）：类型化 transient，无长时间挂起。
    let started = std::time::Instant::now();
    let err = match connect_via(1, std::time::Duration::from_secs(5)) {
        Ok(_) => panic!("a closed port must fail"),
        Err(e) => e,
    };
    assert_eq!(err.code, "storage_transient");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "connection refusal must not stall the worker"
    );
}

#[test]
#[ignore = "requires local Apache httpd WebDAV server"]
fn webdav_get_delivers_what_the_wire_sends_and_never_validates_content_with_etag() {
    let server = webdav_server::WebDavServer::spawn();
    let proxy = FaultProxy::spawn(
        server.base_url.rsplit(':').next().unwrap().parse().unwrap(),
        "truncate",
        1,
    );
    let port = proxy.port;
    let storage = connect_via(port, std::time::Duration::from_secs(20)).unwrap();
    let payload: Vec<u8> = (0..2000u32).map(|i| (i % 251) as u8).collect();
    storage
        .put(
            "changes/truncate-me.object",
            &mut payload.as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    let before = storage.stat("changes/truncate-me.object").unwrap();
    // 第一个 GET 被代理截断一半：适配器如实交付线上字节，不做任何
    // "ETag vs 内容"校验（内容完整性由引擎的 sha256 发布标记负责）。
    let mut bytes = vec![];
    storage
        .get("changes/truncate-me.object")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes.len(), 1000);
    assert_eq!(bytes, payload[..1000]);
    assert_eq!(
        proxy.requests(),
        12,
        "6 probe + 2 MKCOL parents + 1 PUT + 1 put-tail stat + 1 pre-get stat + 1 truncated GET + 1 post-get stat"
    );
    // 服务器端对象未被截断：stat 仍返回原始大小与同一 ETag 令牌。
    let after = storage.stat("changes/truncate-me.object").unwrap();
    assert_eq!(after.bytes, 2000);
    assert_eq!(
        after.version, before.version,
        "ETag is only a conditional-write token"
    );
}
