//! #29 S3-compatible 同步：Application API 级验收。
//!
//! - `sync.configure` 的 S3 目标（endpoint/region/bucket/prefix/path-style）
//!   可操作：配置期做能力探测与认证校验；凭据只进 OS 安全存储，
//!   `sync.status`/配置输出与远端对象字节中不出现任何 secret。
//! - 适配器等价：同一 fixture 分别经 LocalFolder 与真实 S3（MinIO）
//!   完成真实双设备同步，两端的领域结果（事实列表、同步计数）一致。
//!
//! 需要 `WARDROBE_TEST_S3_*` 环境变量与真实 OS 凭据存储；
//! 证据脚本显式设置并以 `-- --ignored` 运行（与 #27 双设备回归一致）。
#[path = "s3_support/mod.rs"]
mod s3_support;

use serde_json::{json, Value};
use wardrobe_core::Application;

fn env_s3_config() -> Option<(String, String, String, String)> {
    Some((
        std::env::var("WARDROBE_TEST_S3_ENDPOINT").ok()?,
        std::env::var("WARDROBE_TEST_S3_BUCKET").ok()?,
        std::env::var("WARDROBE_TEST_S3_ACCESS").ok()?,
        std::env::var("WARDROBE_TEST_S3_SECRET").ok()?,
    ))
}

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
                // 移除该安装登记过的 S3 目标凭据（target_key 从 status 读取）。
                if let Ok(status) = app.execute("sync.status", json!({})) {
                    if let Some(key) = status["target"]["target_key"].as_str() {
                        if let Ok(entry) =
                            keyring::Entry::new("wardrobe-agent.sync", key)
                        {
                            let _ = entry.delete_credential();
                        }
                    }
                }
                let _ = app.execute("encryption.unpair", json!({}));
            }
        }
    }
}

fn pair(a: &std::path::Path, b: &std::path::Path, password: &str, name_a: &str, name_b: &str) -> (Application, Application) {
    let owner = Application::open(a).unwrap();
    let peer = Application::open(b).unwrap();
    owner
        .execute("encryption.create", json!({"password":password,"device_name":name_a}))
        .unwrap();
    let offer = owner.execute("encryption.pair.offer", json!({"device_name":name_b})).unwrap();
    let receipt = peer
        .execute(
            "encryption.pair.accept",
            json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":password}),
        )
        .unwrap();
    let confirmed = owner
        .execute("encryption.pair.confirm", json!({"receipt":receipt["receipt"]}))
        .unwrap();
    peer.execute("encryption.device.apply", json!({"manifest":confirmed["manifest"]})).unwrap();
    (owner, peer)
}

/// 递归剥离身份与时间戳字段，保留可跨安装比较的领域事实。
fn domain_facts(value: &Value) -> Value {
    // 身份/引用/时间戳/本机路径在两轮独立安装之间必然不同，不属于领域结果。
    const VOLATILE: [&str; 9] = [
        "id",
        "member_id",
        "created_at",
        "updated_at",
        "local_path",
        "clothing_id",
        "root_event_id",
        "asset_ids",
        "display_asset_id",
    ];
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !VOLATILE.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), domain_facts(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(domain_facts).collect()),
        other => other.clone(),
    }
}

fn unique_png(path: &std::path::Path, size: u32, seed: u32) -> Vec<u8> {
    let mut image = image::RgbImage::new(size, size);
    let mut seed = seed;
    for pixel in image.pixels_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *pixel = image::Rgb([(seed >> 24) as u8, (seed >> 16) as u8, (seed >> 8) as u8]);
    }
    image.save(path).unwrap();
    std::fs::read(path).unwrap()
}

/// 同一业务场景：成员、两件单品、一张采用展示图、一件搭配、一次穿着。
/// 返回对端的领域事实快照（用于跨后端比较）。
fn run_scenario(owner: &Application, peer: &Application, fixture_png: &std::path::Path, seed: u32) -> Value {
    let member = owner.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let shirt = owner
        .execute("clothing.create", json!({"member_id":member,"name":format!("等价衬衫 {seed}"),"category":"top","color":["black"]}))
        .unwrap()
        .clone();
    let pants = owner
        .execute("clothing.create", json!({"member_id":member,"name":format!("等价长裤 {seed}"),"category":"bottom"}))
        .unwrap()
        .clone();
    let png = unique_png(fixture_png, 48, 1000 + seed);
    std::fs::write(fixture_png, &png).unwrap();
    let imported = owner
        .execute("assets.import", json!({"member_id":member,"path":fixture_png}))
        .unwrap()
        .clone();
    owner
        .execute("assets.adopt", json!({"member_id":member,"clothing_id":shirt["id"],"asset_id":imported["id"]}))
        .unwrap();
    let outfit = owner
        .execute(
            "outfit.create",
            json!({"member_id":member,"name":format!("等价搭配 {seed}"),"items":[
                {"clothing_id":shirt["id"],"role":"base","layer":0},
                {"clothing_id":pants["id"],"role":"base","layer":1}
            ]}),
        )
        .unwrap()
        .clone();
    // 双向多轮同步直至收敛。
    for round in 0..3 {
        let owner_batch = owner.sync_batch().unwrap();
        assert_eq!(owner_batch["pending"].as_u64().unwrap_or(0), 0, "{owner_batch}");
        let peer_batch = peer.sync_batch().unwrap();
        assert_eq!(peer_batch["pending"].as_u64().unwrap_or(0), 0, "{peer_batch}");
        if round == 0 {
            assert!(
                owner_batch["sent"].as_u64().unwrap_or(0) > 0 || peer_batch["received"].as_u64().unwrap_or(0) > 0,
                "first round must transport facts: owner={owner_batch} peer={peer_batch}"
            );
        }
    }
    let shirt_on_peer = peer
        .execute("clothing.list", json!({"member_id":member}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == shirt["id"])
        .cloned()
        .expect("shirt must reach the peer");
    let png_on_peer = peer
        .execute("assets.get", json!({"member_id":member,"id":imported["id"]}))
        .unwrap()
        .clone();
    peer
        .execute("wear.record", json!({
            "member_id":member,
            "clothing_ids":[shirt_on_peer["id"]],
            "local_date":"2026-10-06",
            "timezone":"Asia/Shanghai",
            "operation_key":format!("equivalence wear {seed}")
        }))
        .unwrap();
    for _ in 0..2 {
        peer.sync_batch().unwrap();
        owner.sync_batch().unwrap();
    }
    let wear_on_owner = owner
        .execute("wear.list", json!({"member_id":member}))
        .unwrap()
        .clone();
    let _ = outfit;
    json!({
        "member_id":member,
        "shirt":shirt,
        "pants":pants,
        "peer_clothing_names":peer.execute("clothing.list", json!({"member_id":member})).unwrap()["items"]
            .as_array().unwrap().iter().map(|v| v["name"].clone()).collect::<Vec<_>>(),
        "owner_clothing_names":owner.execute("clothing.list", json!({"member_id":member})).unwrap()["items"]
            .as_array().unwrap().iter().map(|v| v["name"].clone()).collect::<Vec<_>>(),
        "peer_display_bytes":png_on_peer["display"]["bytes"].clone(),
        "peer_display_pending":png_on_peer["display"]["pending"].clone(),
        "owner_wear_count":wear_on_owner["items"].as_array().map(|a| a.len()).unwrap_or(0),
        "owner_wear_facts":wear_on_owner["items"].as_array().and_then(|a| a.first()).map(|v| domain_facts(v)).unwrap_or(Value::Null),
    })
}

/// S3 配置：可操作、真实往返、secret 不出现在任何输出。
#[test]
#[ignore = "requires actual OS credential store and real S3 backend"]
fn s3_configure_roundtrip_dual_device_and_secret_hygiene() {
    let Some((endpoint, bucket, access, secret)) = env_s3_config() else {
        eprintln!("SKIP s3 configure test: WARDROBE_TEST_S3_* not configured");
        return;
    };
    s3_support::ensure_bucket(&endpoint, &bucket, &access, &secret);
    let prefix = format!("app-{}", s3_support::unique("configure"));
    let home_a = tempfile::tempdir().unwrap();
    let home_b = tempfile::tempdir().unwrap();
    let db_a = home_a.path().join("wardrobe.sqlite");
    let db_b = home_b.path().join("wardrobe.sqlite");
    let _cleanup = Cleanup(vec![db_a.clone(), db_b.clone()]);
    let (owner, peer) = pair(
        &db_a,
        &db_b,
        "s3 双设备口令短语",
        "s3 owner",
        "s3 peer",
    );
    let configure_payload = json!({
        "provider_kind":"s3",
        "endpoint":endpoint,
        "region":"us-east-1",
        "bucket":bucket,
        "prefix":prefix,
        "path_style":true,
        "access_key_id":access,
        "secret_access_key":secret,
    });
    let configured = owner.execute("sync.configure", configure_payload.clone()).unwrap();
    assert_eq!(configured["target"]["provider_kind"], "s3", "{configured}");
    assert_eq!(configured["target"]["endpoint"], endpoint.as_str(), "{configured}");
    assert_eq!(configured["target"]["bucket"], bucket.as_str(), "{configured}");
    assert_eq!(configured["target"]["prefix"], prefix.as_str(), "{configured}");
    // secret 与 access key 不出现在配置输出。
    assert!(
        !configured.to_string().contains(secret.as_str()) && !configured.to_string().contains(access.as_str()),
        "credentials must never appear in configure output: {configured}"
    );
    let status = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(status["target"]["provider_kind"], "s3", "{status}");
    assert!(
        !status.to_string().contains(secret.as_str()),
        "credentials must never appear in sync.status: {status}"
    );
    // 反向组合被拒绝：local_folder 不收 S3 字段；s3 不收 directory。
    let mixed = owner
        .execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":home_a.path(),"bucket":"x"}),
        )
        .unwrap_err();
    assert_eq!(mixed.code, "invalid_input", "{mixed:?}");
    let mixed = owner
        .execute(
            "sync.configure",
            json!({"provider_kind":"s3","endpoint":endpoint,"bucket":bucket,"directory":"/tmp","access_key_id":access,"secret_access_key":secret}),
        )
        .unwrap_err();
    assert_eq!(mixed.code, "invalid_input", "{mixed:?}");
    // 错误凭据在配置期即失败（真实认证校验）。
    let bad = owner
        .execute(
            "sync.configure",
            json!({
                "provider_kind":"s3",
                "endpoint":endpoint,
                "region":"us-east-1",
                "bucket":bucket,
                "prefix":format!("{prefix}-bad"),
                "path_style":true,
                "access_key_id":access,
                "secret_access_key":"wrong-secret-rejected",
            }),
        )
        .unwrap_err();
    assert_eq!(bad.code, "storage_auth", "{bad:?}");
    assert!(!bad.message.contains("wrong-secret-rejected"), "{bad:?}");
    // 对端配置同一目标后，真实双设备经 MinIO 往返。
    peer.execute("sync.configure", configure_payload).unwrap();
    let member = owner.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    owner
        .execute("clothing.create", json!({"member_id":member,"name":"S3 往返衬衫","category":"top"}))
        .unwrap();
    let first = owner.sync_batch().unwrap();
    assert!(first["sent"].as_u64().unwrap() > 0, "{first}");
    let received = peer.sync_batch().unwrap();
    assert!(received["received"].as_u64().unwrap() > 0, "{received}");
    let listed = peer
        .execute("clothing.list", json!({"member_id":member}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["name"], "S3 往返衬衫", "{listed:?}");
    // 重复同步幂等。
    let again_owner = owner.sync_batch().unwrap();
    let again_peer = peer.sync_batch().unwrap();
    assert_eq!(again_owner["sent"].as_u64().unwrap() + again_peer["received"].as_u64().unwrap(), 0, "{again_owner} {again_peer}");
    assert_eq!(again_owner["pending"].as_u64().unwrap(), 0, "{again_owner}");
    // 远端对象字节扫描：无 secret、无 access key 明文。
    let objects = s3_support::list_objects(&endpoint, &bucket, &access, &secret, &prefix);
    for (key, bytes) in &objects {
        let text = bytes.iter().map(|b| *b as char).collect::<String>();
        assert!(!text.contains(secret.as_str()), "secret leaked in {key}");
        assert!(!text.contains(access.as_str()), "access key leaked in {key}");
    }
}

/// 适配器等价：同一 fixture 分别经 LocalFolder 与 S3（MinIO）同步，
/// 领域结果一致（切换 adapter 不改领域结果）。
#[test]
#[ignore = "requires actual OS credential store and real S3 backend"]
fn same_fixture_converges_identically_via_local_and_s3() {
    let Some((endpoint, bucket, access, secret)) = env_s3_config() else {
        eprintln!("SKIP s3 equivalence test: WARDROBE_TEST_S3_* not configured");
        return;
    };
    s3_support::ensure_bucket(&endpoint, &bucket, &access, &secret);
    let fixture = tempfile::tempdir().unwrap();
    let fixture_png = fixture.path().join("fixture.png");

    // 第一轮：本地文件夹目标。
    let local_remote = tempfile::tempdir().unwrap();
    let home_a = tempfile::tempdir().unwrap();
    let home_b = tempfile::tempdir().unwrap();
    let db_a = home_a.path().join("wardrobe.sqlite");
    let db_b = home_b.path().join("wardrobe.sqlite");
    {
        let _cleanup = Cleanup(vec![db_a.clone(), db_b.clone()]);
        let (owner, peer) = pair(&db_a, &db_b, "等价口令 本地", "eq owner local", "eq peer local");
        for app in [&owner, &peer] {
            app.execute("sync.configure", json!({"provider_kind":"local_folder","directory":local_remote.path()}))
                .unwrap();
        }
        let local_result = run_scenario(&owner, &peer, &fixture_png, 7);
        std::fs::write(
            fixture.path().join("local-result.json"),
            local_result.to_string(),
        )
        .unwrap();
    }

    // 第二轮：同一 fixture、S3 目标（全新安装）。
    let home_a = tempfile::tempdir().unwrap();
    let home_b = tempfile::tempdir().unwrap();
    let db_a = home_a.path().join("wardrobe.sqlite");
    let db_b = home_b.path().join("wardrobe.sqlite");
    {
        let _cleanup = Cleanup(vec![db_a.clone(), db_b.clone()]);
        let (owner, peer) = pair(&db_a, &db_b, "等价口令 S3", "eq owner s3", "eq peer s3");
        let s3_prefix = format!("eq-{}", s3_support::unique("equivalence"));
        let configure_payload = json!({
            "provider_kind":"s3",
            "endpoint":endpoint,
            "region":"us-east-1",
            "bucket":bucket,
            "prefix":s3_prefix,
            "path_style":true,
            "access_key_id":access,
            "secret_access_key":secret,
        });
        for app in [&owner, &peer] {
            app.execute("sync.configure", configure_payload.clone()).unwrap();
        }
        let s3_result = run_scenario(&owner, &peer, &fixture_png, 7);
        let local_result: Value = serde_json::from_str(
            &std::fs::read_to_string(fixture.path().join("local-result.json")).unwrap(),
        )
        .unwrap();
        // 领域事实逐一相等；仅身份（uuid/成员 id）与时间戳允许不同——
        // 两轮是彼此独立的全新安装。展示图路径（本机文件位置）同理，
        // 因此比较展示图字节数与 pending 状态而不是本机路径。
        assert_eq!(s3_result["member_id"].is_null(), local_result["member_id"].is_null());
        assert_eq!(
            domain_facts(&s3_result["shirt"]),
            domain_facts(&local_result["shirt"]),
            "shirt facts must match across adapters"
        );
        assert_eq!(
            domain_facts(&s3_result["pants"]),
            domain_facts(&local_result["pants"]),
            "pants facts must match across adapters"
        );
        assert_eq!(
            s3_result["peer_clothing_names"], local_result["peer_clothing_names"],
            "peer clothing must match across adapters"
        );
        assert_eq!(
            s3_result["owner_clothing_names"], local_result["owner_clothing_names"],
            "owner clothing must match across adapters"
        );
        assert_eq!(
            s3_result["peer_display_bytes"], local_result["peer_display_bytes"],
            "adopted display bytes must match across adapters"
        );
        assert_eq!(
            s3_result["peer_display_pending"], local_result["peer_display_pending"],
            "display readiness must match across adapters"
        );
        assert_eq!(
            s3_result["owner_wear_count"], local_result["owner_wear_count"],
            "wear events must match across adapters"
        );
        assert_eq!(
            s3_result["owner_wear_facts"], local_result["owner_wear_facts"],
            "wear event facts must match across adapters"
        );
    }
}
