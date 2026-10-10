//! #31 同步运维：真实 WebDAV（Apache mod_dav）与 S3-compatible（MinIO）
//! 关键路径验收。全部经公开 `Application` API（真实 SQLite + 真实 OS 钥匙串）。
//!
//! - WebDAV：双设备真实同步 → 更换目标（复制校验+原子切换）→ 纪元轮换后
//!   传输凭据随目标键迁移（所有者无需重输密码）→ 解除配对清理钥匙串。
//! - S3：双设备真实同步 → 镜像备份（单向复制+增量+幂等）→ 显式提升后
//!   以镜像为目标继续同步。
//!
//! WebDAV 夹具自行启动/停止本机 Apache；S3 需要 `WARDROBE_TEST_S3_*`
//! 环境变量（证据脚本自建自停 MinIO，端口避开 29700/29701）。
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
struct Cleanup(Vec<std::path::PathBuf>);
impl Drop for Cleanup {
    fn drop(&mut self) {
        for path in &self.0 {
            if let Ok(app) = Application::open(path) {
                let _ = app.execute("encryption.unpair", json!({}));
            }
        }
    }
}

fn pair(
    a: &std::path::Path,
    b: &std::path::Path,
    password: &str,
    name_a: &str,
    name_b: &str,
) -> (Application, Application) {
    let owner = Application::open(a).unwrap();
    let peer = Application::open(b).unwrap();
    owner
        .execute(
            "encryption.create",
            json!({"password":password,"device_name":name_a}),
        )
        .unwrap();
    let offer = owner
        .execute("encryption.pair.offer", json!({"device_name":name_b}))
        .unwrap();
    let receipt = peer
        .execute(
            "encryption.pair.accept",
            json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":password}),
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
    (owner, peer)
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

fn sync_entry(target_key: &str) -> Option<String> {
    keyring::Entry::new("wardrobe-agent.sync", target_key)
        .ok()
        .and_then(|entry| entry.get_password().ok())
}

#[test]
#[ignore = "requires real Apache WebDAV server and the OS credential store"]
fn webdav_switch_rotation_credential_migration_and_unpair_cleanup() {
    let server = webdav_server::WebDavServer::spawn();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let owner_db = a.path().join("wardrobe.sqlite");
    let peer_db = b.path().join("wardrobe.sqlite");
    let (owner, peer) = pair(
        &owner_db,
        &peer_db,
        "webdav 运维空间口令-唯一短语",
        "webdav ops owner",
        "webdav ops peer",
    );
    let _cleanup = Cleanup(vec![owner_db.clone(), peer_db.clone()]);
    let root1 = format!("wd31-{}", uuid::Uuid::now_v7().simple());
    let root2 = format!("wd31-{}", uuid::Uuid::now_v7().simple());
    configure_webdav(&owner, &server.base_url, &root1);
    configure_webdav(&peer, &server.base_url, &root1);
    let space = owner.execute("space.default", json!({})).unwrap()["space_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let member = owner
        .execute("member.create", json!({"name":"WebDAV 运维成员"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let shirt = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"WebDAV 运维衬衫","category":"top"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":shirt["id"]}))
            .unwrap()["name"],
        "WebDAV 运维衬衫"
    );

    // 更换目标：复制校验通过才切换到第二个真实 WebDAV 根目录。
    let switched = owner
        .execute(
            "sync.target.switch",
            json!({"provider_kind":"webdav","url":server.base_url,"root":root2,"username":webdav_server::USERNAME,"password":webdav_server::PASSWORD}),
        )
        .unwrap();
    assert!(switched["copied"].as_u64().unwrap() > 0, "{switched}");
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(status["target"]["root"], json!(root2));
    let result = owner.sync_batch().unwrap();
    assert_eq!(
        result["sent"],
        json!(0),
        "切换后复制内容即已发布内容: {result}"
    );
    // 旧根目录内容原样保留（回退可用）。
    let old_root_status = peer.sync_batch().unwrap();
    assert_eq!(
        old_root_status["quarantined"],
        json!(0),
        "旧目标继续可用: {old_root_status}"
    );

    // 纪元轮换（撤销对端）：目标键按纪元滚动，传输凭据随之迁移，
    // 所有者无需重输 WebDAV 密码即可继续同步。
    let peer_state: Value = peer.execute("encryption.status", json!({})).unwrap();
    let peer_device = peer_state["state"]["device_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let revoked = owner
        .execute(
            "encryption.device.revoke",
            json!({"device_id":peer_device,"password":"webdav 运维空间口令-唯一短语"}),
        )
        .unwrap();
    assert!(revoked["manifest"].is_object());
    owner.sync_batch().unwrap();
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":shirt["id"],"name":"WebDAV 轮换后改名"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    // 轮换后目标键（含纪元的派生键）下应有凭据；凭据从未进入明文配置。
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    let target_key = status["target"]["target_key"].as_str().unwrap().to_owned();
    assert!(
        sync_entry(&target_key).is_some(),
        "纪元滚动后的目标键应已自动获得传输凭据"
    );
    let doc = status["target"].to_string();
    assert!(
        !doc.contains(webdav_server::PASSWORD),
        "WebDAV 密码不得出现在目标文档中"
    );

    // 解除配对：wardrobe-agent.sync 按目标键累积的条目随 unpair 清理。
    owner.execute("encryption.unpair", json!({})).unwrap();
    assert!(
        sync_entry(&target_key).is_none(),
        "unpair 应清理同步目标凭据"
    );
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert!(status["target"].is_null(), "解除配对后同步目标状态一并清除");
}

fn env_s3() -> Option<(String, String, String, String)> {
    Some((
        std::env::var("WARDROBE_TEST_S3_ENDPOINT").ok()?,
        std::env::var("WARDROBE_TEST_S3_BUCKET").ok()?,
        std::env::var("WARDROBE_TEST_S3_ACCESS").ok()?,
        std::env::var("WARDROBE_TEST_S3_SECRET").ok()?,
    ))
}

fn configure_s3(
    app: &Application,
    endpoint: &str,
    bucket: &str,
    prefix: &str,
    access: &str,
    secret: &str,
) -> Value {
    app.execute(
        "sync.configure",
        json!({
            "provider_kind":"s3",
            "endpoint":endpoint,
            "region":"us-east-1",
            "bucket":bucket,
            "prefix":prefix,
            "path_style":true,
            "access_key_id":access,
            "secret_access_key":secret,
        }),
    )
    .unwrap()
}

#[test]
#[ignore = "requires real MinIO S3 server and the OS credential store"]
fn s3_mirror_backup_and_promotion_key_path() {
    let Some((endpoint, bucket, access, secret)) = env_s3() else {
        panic!("real S3 contract tests require WARDROBE_TEST_S3_* environment");
    };
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let owner_db = a.path().join("wardrobe.sqlite");
    let peer_db = b.path().join("wardrobe.sqlite");
    let (owner, peer) = pair(
        &owner_db,
        &peer_db,
        "s3 运维空间口令-唯一短语",
        "s3 ops owner",
        "s3 ops peer",
    );
    let _cleanup = Cleanup(vec![owner_db, peer_db]);
    let prefix1 = format!("s3ops-{}", uuid::Uuid::now_v7().simple());
    let prefix2 = format!("s3ops-{}", uuid::Uuid::now_v7().simple());
    configure_s3(&owner, &endpoint, &bucket, &prefix1, &access, &secret);
    configure_s3(&peer, &endpoint, &bucket, &prefix1, &access, &secret);
    let member = owner
        .execute("member.create", json!({"name":"S3 运维成员"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let shirt = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"S3 运维衬衫","category":"top"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":shirt["id"]}))
            .unwrap()["name"],
        "S3 运维衬衫"
    );

    // 镜像备份：真实 S3 第二前缀上的单向副本，首次完整复制校验。
    let mirrored = owner
        .execute(
            "sync.mirror.configure",
            json!({"provider_kind":"s3","endpoint":endpoint,"region":"us-east-1","bucket":bucket,"prefix":prefix2,"path_style":true,"access_key_id":access,"secret_access_key":secret}),
        )
        .unwrap();
    assert!(mirrored["copied"].as_u64().unwrap() > 0, "{mirrored}");

    // 增量单向复制 + 幂等。
    owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"S3 运维围巾","category":"top"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    let incremental = owner.execute("sync.mirror.sync", json!({})).unwrap();
    assert!(incremental["copied"].as_u64().unwrap() > 0, "{incremental}");
    let again = owner.execute("sync.mirror.sync", json!({})).unwrap();
    assert_eq!(again["copied"], json!(0), "{again}");

    // 显式提升：校验镜像与主目标一致后切换；提升后同步继续。
    let promoted = owner.execute("sync.mirror.promote", json!({})).unwrap();
    assert_eq!(promoted["target"]["prefix"], json!(prefix2), "{promoted}");
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(status["target"]["prefix"], json!(prefix2));
    assert_eq!(status["mirror"]["prefix"], json!(prefix1));
    let result = owner.sync_batch().unwrap();
    assert_eq!(result["sent"], json!(0), "{result}");
    assert_eq!(result["received"], json!(0), "{result}");
    assert_eq!(
        owner
            .execute("clothing.get", json!({"member_id":member,"id":shirt["id"]}))
            .unwrap()["name"],
        "S3 运维衬衫"
    );
}
