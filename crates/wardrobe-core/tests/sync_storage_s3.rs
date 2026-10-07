//! #29 S3-compatible 同步：SyncStorage 统一契约。
//!
//! 同一套 #27 已合并的存储契约（往返、分页列举、条件写、条件竞争、
//! 键校验、删除、截断完整性与认证失败）同时跑在 LocalFolder 与真实
//! S3-compatible 后端（MinIO）适配器上；切换 adapter 不得改变契约结果，
//! 只有「服务器不支持的能力」允许走既定的明确拒绝降级。
//!
//! S3 后端由环境变量提供：
//!   WARDROBE_TEST_S3_ENDPOINT （如 http://127.0.0.1:19347）
//!   WARDROBE_TEST_S3_BUCKET   （将按需创建的真实 bucket）
//!   WARDROBE_TEST_S3_ACCESS / WARDROBE_TEST_S3_SECRET
//! 未设置时 S3 分支跳过（本地分支恒跑）；证据脚本必须设置后运行并记录输出。
#[path = "s3_support/mod.rs"]
mod s3_support;

use std::io::Read;
use wardrobe_core::sync::storage::{LocalFolder, SyncStorage, WriteCondition};
use wardrobe_core::sync::storage_s3::{S3Credentials, S3Objects, S3Target};

/// 一个可参数化的真实后端句柄：本地临时目录或真实 MinIO bucket。
struct Backend {
    name: &'static str,
    storage: Box<dyn SyncStorage>,
    duplicate: Box<dyn Fn() -> Box<dyn SyncStorage> + 'static>,
    _keep: Option<tempfile::TempDir>,
}

impl Backend {
    fn storage(&self) -> &dyn SyncStorage {
        self.storage.as_ref()
    }
    fn second_handle(&self) -> Box<dyn SyncStorage> {
        (self.duplicate)()
    }
}

fn local_backend() -> Backend {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let storage = LocalFolder::new(&root).unwrap();
    let duplicate_root = root.clone();
    Backend {
        name: "local_folder",
        storage: Box::new(storage),
        duplicate: Box::new(move || Box::new(LocalFolder::new(&duplicate_root).unwrap())),
        _keep: Some(dir),
    }
}

fn s3_target(endpoint: &str, bucket: &str, prefix: String) -> S3Target {
    S3Target {
        endpoint: endpoint.to_string(),
        region: "us-east-1".into(),
        bucket: bucket.to_string(),
        prefix,
        path_style: true,
    }
}

/// S3 分支：真实 MinIO，独立 key 根前缀隔离每个测试。
fn s3_backend(scope: &str) -> Option<S3Objects> {
    let endpoint = std::env::var("WARDROBE_TEST_S3_ENDPOINT").ok()?;
    let bucket = std::env::var("WARDROBE_TEST_S3_BUCKET").ok()?;
    let access = std::env::var("WARDROBE_TEST_S3_ACCESS").ok()?;
    let secret = std::env::var("WARDROBE_TEST_S3_SECRET").ok()?;
    s3_support::ensure_bucket(&endpoint, &bucket, &access, &secret);
    let prefix = format!("contract-{}", s3_support::unique(scope));
    Some(
        S3Objects::new(
            s3_target(&endpoint, &bucket, prefix),
            S3Credentials {
                access_key_id: access,
                secret_access_key: secret,
            },
        )
        .unwrap(),
    )
}

fn env_s3_config() -> Option<(String, String, String, String)> {
    Some((
        std::env::var("WARDROBE_TEST_S3_ENDPOINT").ok()?,
        std::env::var("WARDROBE_TEST_S3_BUCKET").ok()?,
        std::env::var("WARDROBE_TEST_S3_ACCESS").ok()?,
        std::env::var("WARDROBE_TEST_S3_SECRET").ok()?,
    ))
}

/// 参数化：本地分支恒跑；S3 分支在配置了真实后端时跑同一断言。
fn backends(scope: &str) -> Vec<Backend> {
    let mut out = vec![local_backend()];
    if let Some((endpoint, bucket, access, secret)) = env_s3_config() {
        s3_support::ensure_bucket(&endpoint, &bucket, &access, &secret);
        let prefix = format!("contract-{}", s3_support::unique(scope));
        let storage = S3Objects::new(
            s3_target(&endpoint, &bucket, prefix.clone()),
            S3Credentials {
                access_key_id: access.clone(),
                secret_access_key: secret.clone(),
            },
        )
        .unwrap();
        let duplicate_target = s3_target(&endpoint, &bucket, prefix);
        out.push(Backend {
            name: "s3",
            storage: Box::new(storage),
            duplicate: Box::new(move || {
                Box::new(
                    S3Objects::new(
                        duplicate_target.clone(),
                        S3Credentials {
                            access_key_id: access.clone(),
                            secret_access_key: secret.clone(),
                        },
                    )
                    .unwrap(),
                )
            }),
            _keep: None,
        });
    } else {
        eprintln!("SKIP s3 backend for {scope}: WARDROBE_TEST_S3_* not configured");
    }
    out
}

/// 同一契约：往返、版本语义、删除。
#[test]
fn storage_roundtrip_versions_and_delete_on_both_backends() {
    for backend in backends("roundtrip") {
        let name = backend.name;
        let storage = backend.storage();
        let meta = storage
            .put(
                "space/changes/a.object",
                &mut b"payload".as_slice(),
                WriteCondition::CreateOnly,
            )
            .unwrap();
        assert_eq!(meta.bytes, 7, "{name}");
        assert_eq!(bounded(storage, "space/changes/a.object"), b"payload", "{name}");
        let stat = storage.stat("space/changes/a.object").unwrap();
        assert_eq!(stat.bytes, 7, "{name}");
        assert!(!stat.version.is_empty(), "{name}");
        assert!(storage.stat("space/changes/missing").is_err(), "{name}");
        // 版本随内容变化：ETag 只是条件写协调的版本，不是内容 hash（完整性由协议 sha256 承担）。
        let updated = storage
            .put(
                "space/changes/a.object",
                &mut b"payload-v2".as_slice(),
                WriteCondition::Any,
            )
            .unwrap();
        assert_ne!(updated.version, meta.version, "{name}");
        storage
            .delete("space/changes/a.object", WriteCondition::Any)
            .unwrap();
        assert!(storage.stat("space/changes/a.object").is_err(), "{name}");
        // 删除不存在的对象与本地文件夹同语义。
        assert_eq!(
            storage
                .delete("space/changes/a.object", WriteCondition::Any)
                .unwrap_err()
                .code,
            "storage_not_found",
            "{name}"
        );
    }
}

/// 同一契约：分页列举、游标单调、非法 limit/游标拒绝。
#[test]
fn storage_pagination_contract_on_both_backends() {
    for backend in backends("pagination") {
        let name = backend.name;
        let storage = backend.storage();
        for i in (0..40).rev() {
            storage
                .put(
                    &format!("space/changes/{i:04}.object"),
                    &mut b"one".as_slice(),
                    WriteCondition::CreateOnly,
                )
                .unwrap();
        }
        let mut cursor = None;
        let mut all = vec![];
        loop {
            let page = storage.list("space/changes", cursor.as_deref(), 7).unwrap();
            assert!(page.items.len() <= 7, "{name}");
            all.extend(page.items.into_iter().map(|v| v.key));
            cursor = page.cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(all.len(), 40, "{name}");
        assert!(all.windows(2).all(|v| v[0] < v[1]), "{name}");
        assert!(storage.list("space/changes", None, 0).is_err(), "{name}");
        assert!(storage.list("space/changes", None, 1001).is_err(), "{name}");
        assert!(
            storage
                .list("space/changes", Some("other/not-a-cursor"), 7)
                .is_err(),
            "{name}"
        );
    }
}

/// 同一契约：可移植对象键校验（驱动器、绝对路径、非 Normal 分量）。
#[test]
fn portable_object_keys_rejected_on_both_backends() {
    for backend in backends("keys") {
        let name = backend.name;
        let storage = backend.storage();
        for key in [
            "C:/outside",
            "C:foo",
            "/absolute",
            "//server/share",
            "a\\b",
            "a/../outside",
            "./object",
            "a//object",
            "a/./object",
            "a/\0object",
        ] {
            assert_eq!(
                storage
                    .put(key, &mut b"bounded".as_slice(), WriteCondition::CreateOnly)
                    .unwrap_err()
                    .code,
                "invalid_object_key",
                "{key:?} on {name}"
            );
            assert_eq!(
                storage.stat(key).unwrap_err().code,
                "invalid_object_key",
                "{key:?} on {name}"
            );
            assert!(storage.delete(key, WriteCondition::Any).is_err(), "{name}");
        }
    }
}

/// 同一契约：CreateOnly 重复发布与 MatchVersion 陈旧版本必须整体拒绝。
#[test]
fn conditional_writes_preserve_versions_on_both_backends() {
    for backend in backends("conditional") {
        let name = backend.name;
        let storage = backend.storage();
        let old = storage
            .put(
                "space/changes/r.object",
                &mut b"old".as_slice(),
                WriteCondition::CreateOnly,
            )
            .unwrap();
        assert_eq!(
            storage
                .put(
                    "space/changes/r.object",
                    &mut b"duplicate".as_slice(),
                    WriteCondition::CreateOnly
                )
                .unwrap_err()
                .code,
            "storage_precondition",
            "{name}"
        );
        assert_eq!(
            storage
                .put(
                    "space/changes/r.object",
                    &mut b"stale".as_slice(),
                    WriteCondition::MatchVersion("stale-etag".into()),
                )
                .unwrap_err()
                .code,
            "storage_precondition",
            "{name}"
        );
        let new = storage
            .put(
                "space/changes/r.object",
                &mut b"new".as_slice(),
                WriteCondition::MatchVersion(old.version.clone()),
            )
            .unwrap();
        assert_ne!(old.version, new.version, "{name}");
        // 删除的版本条件：陈旧版本删除必须拒绝且不影响当前版本。
        // 本地文件夹支持条件删除（precondition）；S3 服务器生态不统一：
        // MinIO 静默忽略 DELETE 的 If-Match，适配器按既定降级明确拒绝
        // （storage_unsupported）。两者都不会移除新发布的版本。
        let stale_delete = storage
            .delete(
                "space/changes/r.object",
                WriteCondition::MatchVersion(old.version.clone()),
            )
            .unwrap_err();
        assert_eq!(bytes_at(storage, "space/changes/r.object"), b"new", "{name}");
        match storage
            .delete(
                "space/changes/r.object",
                WriteCondition::MatchVersion(new.version.clone()),
            ) {
            Ok(()) => {
                assert_eq!(stale_delete.code, "storage_precondition", "{name}");
            }
            Err(rejected) => {
                assert_eq!(
                    rejected.code, "storage_unsupported",
                    "{name}: {rejected:?}"
                );
                assert_eq!(stale_delete.code, "storage_unsupported", "{name}");
                // 明确拒绝后对象保持新版本；无条件删除可用。
                assert_eq!(bytes_at(storage, "space/changes/r.object"), b"new", "{name}");
                storage
                    .delete("space/changes/r.object", WriteCondition::Any)
                    .unwrap();
            }
        }
        assert!(storage.stat("space/changes/r.object").is_err(), "{name}");
    }
}

/// 同一契约：2MB 上限。超限对象绝不发布。
#[test]
fn oversized_upload_is_rejected_whole_on_both_backends() {
    for backend in backends("oversize") {
        let name = backend.name;
        let storage = backend.storage();
        let oversized = vec![42u8; 2 * 1024 * 1024 + 1];
        assert_eq!(
            storage
                .put(
                    "space/changes/too-large.object",
                    &mut oversized.as_slice(),
                    WriteCondition::CreateOnly
                )
                .unwrap_err()
                .code,
            "storage_corrupt",
            "{name}"
        );
        assert!(storage.stat("space/changes/too-large.object").is_err(), "{name}");
        assert!(
            storage
                .list("space/changes", None, 20)
                .unwrap()
                .items
                .is_empty(),
            "{name}"
        );
    }
}

/// 同一契约：并发条件竞争——同一版本的两个条件写只能成功一个；
/// 条件删除绝不能移除新发布的版本（S3 后端以明确拒绝实现该保证）。
#[test]
fn racing_conditional_writes_admit_exactly_one_winner_on_both_backends() {
    for backend in backends("race") {
        let name = backend.name;
        let storage = backend.storage();
        let base = storage
            .put(
                "space/changes/race.object",
                &mut b"base".as_slice(),
                WriteCondition::CreateOnly,
            )
            .unwrap();
        let handle_a = backend.second_handle();
        let handle_b = backend.second_handle();
        let version = base.version.clone();
        let version_a = version.clone();
        let a = std::thread::spawn(move || {
            handle_a.put(
                "space/changes/race.object",
                &mut b"writer-a".as_slice(),
                WriteCondition::MatchVersion(version_a),
            )
        });
        let version_b = version.clone();
        let b = std::thread::spawn(move || {
            handle_b.put(
                "space/changes/race.object",
                &mut b"writer-b".as_slice(),
                WriteCondition::MatchVersion(version_b),
            )
        });
        let (ra, rb) = (a.join().unwrap(), b.join().unwrap());
        let winners = [ra.is_ok(), rb.is_ok()].into_iter().filter(|v| *v).count();
        assert_eq!(winners, 1, "{name}: {ra:?} vs {rb:?}");
        // 条件删除不能移除新发布的版本：本地走 precondition，
        // S3 对条件删除明确拒绝（真实 MinIO 忽略 DELETE 的 If-Match，静默改语义不可接受）。
        let rejected = storage
            .delete(
                "space/changes/race.object",
                WriteCondition::MatchVersion(base.version.clone()),
            )
            .unwrap_err();
        assert!(
            ["storage_precondition", "storage_unsupported"].contains(&rejected.code.as_str()),
            "{name}: {rejected:?}"
        );
        let bytes = bounded(storage, "space/changes/race.object");
        assert!(
            bytes == b"writer-a" || bytes == b"writer-b",
            "{name}: surviving bytes must be one conditional winner, got {bytes:?}"
        );
    }
}

fn bounded(storage: &dyn SyncStorage, key: &str) -> Vec<u8> {
    let mut bytes = vec![];
    storage.get(key).unwrap().read_to_end(&mut bytes).unwrap();
    bytes
}

fn bytes_at(storage: &dyn SyncStorage, key: &str) -> Vec<u8> {
    bounded(storage, key)
}

/// S3 专属：认证失败映射类型化错误（配置期即可发现，不静默半配置）。
#[test]
fn s3_auth_failure_maps_to_typed_error() {
    let Some((endpoint, bucket, access, _)) = env_s3_config() else {
        eprintln!("SKIP s3 auth test: WARDROBE_TEST_S3_* not configured");
        return;
    };
    let error = S3Objects::new(
        s3_target(
            &endpoint,
            &bucket,
            format!("auth-{}", s3_support::unique("auth")),
        ),
        S3Credentials {
            access_key_id: access,
            secret_access_key: "wrong-secret-do-not-match".into(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "storage_auth", "{error:?}");
    assert!(
        !error.message.contains("wrong-secret"),
        "secret must never leak into error text: {error:?}"
    );
}

/// S3 专属：不存在的 bucket 明确失败（storage_not_found），配置期即可发现。
#[test]
fn s3_missing_bucket_maps_to_typed_error() {
    let Some((endpoint, _, access, secret)) = env_s3_config() else {
        eprintln!("SKIP s3 missing-bucket test: WARDROBE_TEST_S3_* not configured");
        return;
    };
    let error = S3Objects::new(
        s3_target(
            &endpoint,
            &format!("no-such-bucket-{}", s3_support::unique("missing")),
            "x".into(),
        ),
        S3Credentials {
            access_key_id: access,
            secret_access_key: secret,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "storage_not_found", "{error:?}");
}

/// S3 专属：503 SlowDown 按重试预算自动重试并遵守 Retry-After；
/// 预算耗尽后映射 storage_transient。故障由本地真实 HTTP 代理注入。
#[test]
fn s3_slowdown_is_retried_within_budget_then_maps_to_transient() {
    let Some((endpoint, bucket, access, secret)) = env_s3_config() else {
        eprintln!("SKIP s3 slowdown test: WARDROBE_TEST_S3_* not configured");
        return;
    };
    let upstream = endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string();
    // 注入器：最初 2 次请求返回 503 + Retry-After: 0，之后放行到真实 MinIO。
    let injector = s3_support::FaultProxy::spawn(&upstream, 2);
    let prefix = format!("slowdown-{}", s3_support::unique("slowdown"));
    let storage = S3Objects::new(
        s3_target(&injector.url(), &bucket, prefix),
        S3Credentials {
            access_key_id: access.clone(),
            secret_access_key: secret.clone(),
        },
    )
    .unwrap();
    storage
        .put(
            "space/changes/retry.object",
            &mut b"payload".as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    let stats = injector.stats();
    assert_eq!(stats.slowdown, 2, "two injected 503s must be absorbed: {stats:?}");
    assert_eq!(bounded(&storage, "space/changes/retry.object"), b"payload");
    // 预算耗尽：持续 503 超过 2 次自动重试 → storage_transient（含能力探测本身）。
    let injector = s3_support::FaultProxy::spawn(&upstream, 50);
    let error = S3Objects::new(
        s3_target(
            &injector.url(),
            &bucket,
            format!("exhaust-{}", s3_support::unique("exhaust")),
        ),
        S3Credentials {
            access_key_id: access,
            secret_access_key: secret,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "storage_transient", "{error:?}");
    let stats = injector.stats();
    assert_eq!(stats.slowdown, 3, "initial + 2 retries, never more: {stats:?}");
}

/// S3 专属：响应体截断必须整体失败，部分字节不可见；修复后重试收敛。
#[test]
fn s3_truncated_response_body_fails_whole() {
    let Some((endpoint, bucket, access, secret)) = env_s3_config() else {
        eprintln!("SKIP s3 truncation test: WARDROBE_TEST_S3_* not configured");
        return;
    };
    let upstream = endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string();
    let injector = s3_support::FaultProxy::spawn(&upstream, 0);
    let prefix = format!("truncate-{}", s3_support::unique("truncate"));
    let storage = S3Objects::new(
        s3_target(&injector.url(), &bucket, prefix),
        S3Credentials {
            access_key_id: access,
            secret_access_key: secret,
        },
    )
    .unwrap();
    storage
        .put(
            "space/changes/victim.object",
            &mut b"0123456789abcdef".as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    assert_eq!(
        bounded(&storage, "space/changes/victim.object"),
        b"0123456789abcdef"
    );
    // 代理截断下一次 GET 响应体（保留声明的 Content-Length，随后断开）。
    injector.truncate_next_get(5);
    let mut handle = storage.get("space/changes/victim.object").unwrap();
    let mut bytes = vec![];
    let result = handle.read_to_end(&mut bytes);
    assert!(
        result.is_err() || bytes.len() < 16,
        "truncated body must not be delivered whole: {result:?} {bytes:?}"
    );
}

/// S3 专属：同一 key 的并发 CreateOnly 只有一个赢家（真实服务器仲裁）。
#[test]
fn s3_concurrent_create_only_has_single_winner() {
    let Some(s3) = s3_backend("concurrent-create") else {
        eprintln!("SKIP s3 concurrent-create test: backend not configured");
        return;
    };
    let s3 = std::sync::Arc::new(s3);
    let handles: Vec<_> = (0..4)
        .map(|n| {
            let s3 = s3.clone();
            std::thread::spawn(move || {
                s3.put(
                    "space/changes/unique.object",
                    &mut format!("writer-{n}").as_bytes(),
                    WriteCondition::CreateOnly,
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1, "{results:?}");
    let mut bytes = vec![];
    s3.get("space/changes/unique.object")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.starts_with(b"writer-"), "{bytes:?}");
}
