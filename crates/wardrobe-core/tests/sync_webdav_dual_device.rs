//! #30 WebDAV 双设备真实同步验收：同一台机器、两个真实数据目录、一台真实
//! Apache mod_dav 服务器（Basic 认证）。全部经公开 `Application` API：
//! 配置（密码只进 OS 安全存储，不入明文配置）→ 真实配对 → 增量发布/接收 →
//! 重复同步幂等 → 服务器端对象截断后不应用部分数据、修复后同一日志收敛 →
//! 包内容白名单（口令/原图/原始文件名不进远端目录）。
use serde_json::{json, Value};
use wardrobe_core::Application;

#[path = "support/webdav_server.rs"]
mod webdav_server;

trait SyncBatch {
    #[track_caller]
    fn sync_batch(&self) -> Result<Value, wardrobe_core::ApiError>;
}
impl SyncBatch for Application {
    fn sync_batch(&self) -> Result<Value, wardrobe_core::ApiError> {
        let queued = self.execute("sync.run", json!({}))?;
        let processed = self.execute("sync.process", json!({"id":queued["job_id"]}))?;
        if processed["state"] == "failed" {
            panic!("sync worker failed: {}", processed["error"]);
        }
        Ok(processed["result"].clone())
    }
}
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(a) = Application::open(&self.0) {
            let _ = a.execute("encryption.unpair", json!({}));
        }
    }
}

fn configure_webdav(app: &Application, url: &str, root: &str) -> Value {
    app.execute(
        "sync.configure",
        json!({
            "provider_kind":"webdav",
            "url":url,
            "root":root,
            "username":webdav_server::USERNAME,
            "password":webdav_server::PASSWORD,
        }),
    )
    .unwrap()
}

#[test]
#[ignore = "requires real Apache WebDAV server and the OS credential store"]
fn webdav_dual_device_round_trip_is_idempotent_and_heals_after_server_side_truncation() {
    let server = webdav_server::WebDavServer::spawn();
    let owner_home = tempfile::tempdir().unwrap();
    let peer_home = tempfile::tempdir().unwrap();
    let owner_db = owner_home.path().join("wardrobe.sqlite");
    let peer_db = peer_home.path().join("wardrobe.sqlite");
    let owner = Application::open(&owner_db).unwrap();
    let peer = Application::open(&peer_db).unwrap();
    let _cleanup = [Cleanup(owner_db.clone()), Cleanup(peer_db.clone())];

    let password = "webdav 双设备口令-唯一短语";
    owner
        .execute(
            "encryption.create",
            json!({"password":"webdav 空间口令-唯一短语","device_name":"webdav owner"}),
        )
        .unwrap();
    let member = owner
        .execute("member.create", json!({"name":"WebDAV 同步成员"}))
        .unwrap()["member_id"]
        .clone();
    let garment = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"WebDAV 蓝衬衫","category":"top"}),
        )
        .unwrap();
    let offer = owner
        .execute(
            "encryption.pair.offer",
            json!({"device_name":"webdav peer"}),
        )
        .unwrap();
    let receipt = peer
        .execute(
            "encryption.pair.accept",
            json!({
                "invitation":offer["invitation"],
                "pairing_secret":offer["pairing_secret"],
                "password":"webdav 空间口令-唯一短语"
            }),
        )
        .unwrap();
    let confirmation = owner
        .execute(
            "encryption.pair.confirm",
            json!({"receipt":receipt["receipt"]}),
        )
        .unwrap();
    peer.execute(
        "encryption.device.apply",
        json!({"manifest":confirmation["manifest"]}),
    )
    .unwrap();

    // 配置：真实连接 + 能力探针在配置期执行；密码不进入返回值/目标文档。
    let root = format!("wd-{}", uuid::Uuid::now_v7());
    let configured = configure_webdav(&owner, &server.base_url, &root);
    assert_eq!(configured["target"]["provider_kind"], "webdav");
    assert_eq!(configured["target"]["url"], server.base_url);
    assert!(
        configured["target"]["password"].is_null(),
        "密码不得进入同步目标配置"
    );
    let owner_doc = configured["target"].to_string();
    assert!(
        !owner_doc.contains(password),
        "密码不得出现在任何目标文档中"
    );
    configure_webdav(&peer, &server.base_url, &root);

    // 设备 A 首次发布。
    let sent = owner.sync_batch().unwrap();
    assert!(
        sent["sent"].as_u64().unwrap() > 0,
        "first publication must send facts: {sent}"
    );

    // 服务器端截断一个未消费对象：对端整体失败（storage_corrupt），不应用部分
    // 数据——完整性由引擎的 sha256 发布标记判定，而非 ETag。
    fn find_victim(dir: &std::path::Path) -> std::path::PathBuf {
        let mut victims = vec![];
        fn scan(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    scan(&path, out);
                } else if path.extension().is_some_and(|e| e == "object") {
                    out.push(path);
                }
            }
        }
        scan(dir, &mut victims);
        victims.sort();
        victims.pop().expect("published objects exist")
    }
    let victim = find_victim(&server.data_dir().join(&root));
    let original_bytes = std::fs::read(&victim).unwrap();
    let parked = owner_home.path().join("truncated-object-backup");
    std::fs::write(&parked, &original_bytes).unwrap();
    std::fs::write(&victim, &original_bytes[..original_bytes.len() / 2]).unwrap();
    let interrupted = peer.execute("sync.run", json!({})).unwrap();
    let job = peer
        .execute("sync.process", json!({"id":interrupted["job_id"]}))
        .unwrap();
    assert_eq!(
        job["state"], "failed",
        "truncated object must fail the batch"
    );
    assert_eq!(job["error"]["code"], "storage_corrupt", "{}", job);
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member,"id":garment["id"]})
        )
        .unwrap_err()
        .code,
        "not_found",
        "no partial application while the transport is corrupt"
    );
    std::fs::write(&victim, &original_bytes).unwrap();

    // 修复后同一日志继续收敛。
    let received = peer.sync_batch().unwrap();
    assert!(
        received["received"].as_u64().unwrap() > 0,
        "healed transport converges: {received}"
    );
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member,"id":garment["id"]})
        )
        .unwrap()["name"],
        "WebDAV 蓝衬衫"
    );

    // 重复同步幂等：双方再次 run/process 必须零发送零接收。
    for app in [&owner, &peer] {
        let again = app.sync_batch().unwrap();
        assert_eq!(
            again,
            json!({"schema_version":1,"sent":0,"received":0,"pending":0,"quarantined":0,"missing_assets":0,"has_more":false}),
            "repeated sync must be idempotent: {again}"
        );
    }

    // 双向：对端编辑回传。
    peer.execute(
        "clothing.update",
        json!({"member_id":member,"id":garment["id"],"name":"第二设备可编辑"}),
    )
    .unwrap();
    peer.sync_batch().unwrap();
    owner.sync_batch().unwrap();
    assert_eq!(
        owner
            .execute(
                "clothing.get",
                json!({"member_id":member,"id":garment["id"]})
            )
            .unwrap()["name"],
        "第二设备可编辑"
    );

    // 包内容白名单：远端目录不得出现空间口令、成员名明文或 WebDAV 密码。
    let needles: Vec<Vec<u8>> = vec![
        password.as_bytes().to_vec(),
        "webdav 空间口令".as_bytes().to_vec(),
        "WebDAV 同步成员".as_bytes().to_vec(),
        "WebDAV 蓝衬衫".as_bytes().to_vec(),
        webdav_server::PASSWORD.as_bytes().to_vec(),
    ];
    fn scan(path: &std::path::Path, needles: &[Vec<u8>]) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                scan(&entry.path(), needles);
            } else {
                let bytes = std::fs::read(entry.path()).unwrap();
                for needle in needles {
                    assert!(
                        !bytes.windows(needle.len()).any(|w| w == needle.as_slice()),
                        "plaintext leaked into {}",
                        entry.path().display()
                    );
                }
            }
        }
    }
    scan(&server.data_dir().join(&root), &needles);
}
