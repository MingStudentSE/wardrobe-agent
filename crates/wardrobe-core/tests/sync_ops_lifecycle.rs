//! #31 同步运维与密钥生命周期：验收切片。
//!
//! 全部测试走公开 `Application` API（真实 SQLite + 真实 OS 钥匙串），两个真实
//! 安装实例共享真实本地文件夹目标。覆盖：暂停/恢复与队列语义、状态页的
//! 游标/缺图/失败/水位可见、更换目标的复制校验与失败回退（切换原子）、
//! 镜像备份的单向复制与显式提升、按设备水位+保留窗口+备份确认的安全回收、
//! 改密码/撤销/轮换/错误密钥/恢复失败的同步语境。
//! 真实 WebDAV/S3 关键路径见 `sync_ops_real_backends.rs`。

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

/// 两台真实安装经真实配对流程加入同一加密空间。
fn pair(a: &std::path::Path, b: &std::path::Path, password: &str) -> (Application, Application) {
    let owner = Application::open(a).unwrap();
    let peer = Application::open(b).unwrap();
    owner
        .execute(
            "encryption.create",
            json!({"password":password,"device_name":"ops owner"}),
        )
        .unwrap();
    let offer = owner
        .execute("encryption.pair.offer", json!({"device_name":"ops peer"}))
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

fn configure_local(app: &Application, directory: &std::path::Path) {
    app.execute(
        "sync.configure",
        json!({"provider_kind":"local_folder","directory":directory}),
    )
    .unwrap();
}

fn item(app: &Application, member: &str, name: &str) -> Value {
    app.execute(
        "clothing.create",
        json!({"member_id":member,"name":name,"category":"top"}),
    )
    .unwrap()
}

fn names(app: &Application, member: &str) -> Vec<String> {
    let mut v: Vec<String> = app
        .execute("clothing.list", json!({"member_id":member}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_owned())
        .collect();
    v.sort();
    v
}

fn walk_files(path: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            walk_files(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn commit_markers(remote: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = vec![];
    walk_files(remote, &mut files);
    files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "commit"))
        .collect()
}

/// 只统计 changes 目录下的提交标记（不含水位、索引页与 blob 提交）。
fn change_commits(remote: &std::path::Path) -> Vec<std::path::PathBuf> {
    commit_markers(remote)
        .into_iter()
        .filter(|p| {
            p.components()
                .any(|c| c.as_os_str().to_string_lossy() == "changes")
        })
        .collect()
}

fn make_png(path: &std::path::Path) {
    let mut image = image::RgbImage::new(640, 640);
    let mut seed = 7u32;
    for pixel in image.pixels_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *pixel = image::Rgb([(seed >> 24) as u8, (seed >> 16) as u8, (seed >> 8) as u8]);
    }
    image.save(path).unwrap();
}

// ---------------------------------------------------------------------------
// 验收 1a：暂停/恢复。暂停后新的同步批次被拒绝（typed sync_paused），已有
// 队列任务保留、恢复后同一任务继续完成；暂停只影响本机目标，不影响对端。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn pause_blocks_new_batches_and_preserves_queued_job_until_resume() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(
        &a.path().join("db"),
        &b.path().join("db"),
        "ops 暂停空间口令-唯一短语",
    );
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_local(&owner, remote.path());
    configure_local(&peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"暂停家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    item(&owner, &member, "暂停前衬衫");
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(names(&peer, &member), vec!["暂停前衬衫".to_owned()]);

    // 先入队再暂停：队列任务保留，处理被显式拒绝；恢复后同一任务继续。
    let job = owner
        .execute("sync.run", json!({"operation_key":"pause-queued-batch"}))
        .unwrap();
    let paused = owner.execute("sync.pause", json!({})).unwrap();
    assert_eq!(paused["paused"], json!(true));
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(status["paused"], json!(true));
    let err = owner
        .execute("sync.process", json!({"id":job["job_id"]}))
        .unwrap_err();
    assert_eq!(err.code, "sync_paused");
    let err = owner.execute("sync.run", json!({})).unwrap_err();
    assert_eq!(err.code, "sync_paused");

    // 暂停只作用于本机目标：对端继续同步。
    peer.execute(
        "clothing.create",
        json!({"member_id":member,"name":"对端暂停期外套","category":"top"}),
    )
    .unwrap();
    peer.sync_batch().unwrap();

    owner.execute("sync.resume", json!({})).unwrap();
    let processed = owner
        .execute("sync.process", json!({"id":job["job_id"]}))
        .unwrap();
    assert_ne!(processed["state"], json!("failed"), "{}", processed);
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(status["paused"], json!(false));
    owner.sync_batch().unwrap();
    assert!(names(&owner, &member).contains(&"对端暂停期外套".to_owned()));
}

// ---------------------------------------------------------------------------
// 验收 1b：状态页数据。游标（已消费提交数）、设备水位、缺图占位、最近失败
// 与冲突都从公开 sync.status / sync.conflicts 可读。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn status_surfaces_cursor_watermark_missing_images_and_last_failure() {
    use std::os::unix::fs::PermissionsExt;
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(
        &a.path().join("db"),
        &b.path().join("db"),
        "ops 状态空间口令-唯一短语",
    );
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_local(&owner, remote.path());
    configure_local(&peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"状态家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let clean = item(&owner, &member, "状态衬衫");
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();

    // 游标与设备水位：消费提交数、水位 op 位置、水位时间都可见。
    let status: Value = peer.execute("sync.status", json!({})).unwrap();
    assert!(
        status["cursor"]["consumed_commits"].as_u64().unwrap() > 0,
        "{status}"
    );
    assert!(
        status["watermark"]["positions"].is_object(),
        "对端完成首次同步后应已发布设备水位: {status}"
    );
    assert!(status["watermark"]["captured_at"].is_string());

    // 冲突可见（并发编辑同字段）。
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":clean["id"],"name":"状态所有者候选"}),
        )
        .unwrap();
    peer.execute(
        "clothing.update",
        json!({"member_id":member,"id":clean["id"],"name":"状态对端候选"}),
    )
    .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    owner.sync_batch().unwrap();
    let conflicts = owner
        .execute("sync.conflicts", json!({"member_id":member}))
        .unwrap();
    assert!(
        conflicts["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["field"] == json!("name")),
        "{conflicts}"
    );

    // 缺图占位：对端发布采用展示图后移走真实分块，本端状态可见缺图。
    let png = a.path().join("status-original.png");
    make_png(&png);
    let asset = owner
        .execute("assets.import", json!({"member_id":member,"path":png}))
        .unwrap();
    owner
        .execute(
            "assets.adopt",
            json!({"member_id":member,"clothing_id":clean["id"],"asset_id":asset["id"]}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    let mut objects = vec![];
    walk_files(remote.path(), &mut objects);
    let chunk = objects
        .into_iter()
        .find(|p| p.extension().is_some_and(|e| e == "object"))
        .unwrap();
    let parked = a.path().join("status-parked-object");
    std::fs::rename(&chunk, &parked).unwrap();
    peer.sync_batch().unwrap();
    let status: Value = peer.execute("sync.status", json!({})).unwrap();
    assert!(
        status["asset_errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == asset["id"]),
        "缺图占位应出现在状态页: {status}"
    );
    std::fs::rename(&parked, &chunk).unwrap();
    peer.sync_batch().unwrap();
    let status: Value = peer.execute("sync.status", json!({})).unwrap();
    assert!(
        !status["asset_errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == asset["id"]),
        "分块恢复后缺图占位应清除: {status}"
    );

    // 最近失败可见：目标目录不可读时同步批次失败，状态页记录最近失败。
    let readonly = std::fs::Permissions::from_mode(0o000);
    std::fs::set_permissions(remote.path(), readonly).unwrap();
    let queued = owner.execute("sync.run", json!({})).unwrap();
    let processed = owner
        .execute("sync.process", json!({"id":queued["job_id"]}))
        .unwrap();
    std::fs::set_permissions(remote.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(processed["state"], json!("failed"), "{processed}");
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    let failure = status["last_failure"]
        .as_object()
        .expect("最近同步失败应出现在状态页");
    assert!(
        failure.contains_key("code") && failure.contains_key("at"),
        "{status}"
    );
}

// ---------------------------------------------------------------------------
// 验收 1c：更换目标。先完整复制并逐对象校验，全部通过才原子切换主目标；
// 任一步失败保持旧目标（可回退），旧目标数据不被改动。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn target_switch_verifies_copy_first_and_rolls_back_on_failure() {
    use std::os::unix::fs::PermissionsExt;
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote_a = tempfile::tempdir().unwrap();
    let remote_b = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(
        &a.path().join("db"),
        &b.path().join("db"),
        "ops 切换空间口令-唯一短语",
    );
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_local(&owner, remote_a.path());
    configure_local(&peer, remote_a.path());
    let member = owner
        .execute("member.create", json!({"name":"切换家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let shirt = item(&owner, &member, "切换衬衫");
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    let count_a = commit_markers(remote_a.path()).len();

    // 失败回退：新目录不可写，切换必须整体失败且主目标不变。
    let readonly = tempfile::tempdir().unwrap();
    std::fs::set_permissions(readonly.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let err = owner
        .execute(
            "sync.target.switch",
            json!({"provider_kind":"local_folder","directory":readonly.path()}),
        )
        .unwrap_err();
    std::fs::set_permissions(readonly.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        [
            "storage_transient",
            "storage_error",
            "storage_unsupported",
            "storage_not_found",
            "storage_precondition"
        ]
        .contains(&err.code.as_str()),
        "实际错误码 {err:?}"
    );
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(status["target"]["directory"], json!(remote_a.path()));
    owner.sync_batch().unwrap(); // 旧目标继续可用
    assert_eq!(
        commit_markers(remote_a.path()).len(),
        count_a,
        "失败的切换不得改动旧目标"
    );

    // 复制校验 + 原子切换。
    let switched = owner
        .execute(
            "sync.target.switch",
            json!({"provider_kind":"local_folder","directory":remote_b.path()}),
        )
        .unwrap();
    assert!(
        switched["copied"].as_u64().unwrap() > 0,
        "应真实复制已发布对象: {switched}"
    );
    assert!(switched["verified_bytes"].as_u64().unwrap() > 0);
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(status["target"]["directory"], json!(remote_b.path()));
    // 切换后立即同步幂等：复制内容即已发布内容。
    let again = owner.sync_batch().unwrap();
    assert_eq!(again["sent"], json!(0), "{again}");
    // 旧目标文件原样保留（回退可用）。
    assert!(
        commit_markers(remote_a.path()).len() == count_a,
        "旧目标内容不得被清理"
    );

    // 对端仍指旧目标时看不到新事实；对端同样先复制校验再切换后收敛。
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":shirt["id"],"name":"切换后改名"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":shirt["id"]}))
            .unwrap()["name"],
        "切换衬衫",
        "对端未切换前不得从新目标读取"
    );
    let peer_switch = peer
        .execute(
            "sync.target.switch",
            json!({"provider_kind":"local_folder","directory":remote_b.path()}),
        )
        .unwrap();
    assert!(
        peer_switch["copied"].as_u64().unwrap() + peer_switch["skipped"].as_u64().unwrap() > 0,
        "复制校验必须逐对象对账: {peer_switch}"
    );
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":shirt["id"]}))
            .unwrap()["name"],
        "切换后改名"
    );
    assert_eq!(names(&peer, &member), names(&owner, &member));
}

// ---------------------------------------------------------------------------
// 验收 2：一个主目标 + 镜像备份（单向复制）。镜像不参与协调；主目标失败时
// 显式提升，提升前做完整性校验；提升后旧主目标原样保留。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn mirror_backup_syncs_one_way_and_promotes_only_after_verification() {
    let a = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let mirror_dir = tempfile::tempdir().unwrap();
    let (owner, _peer) = pair(
        &a.path().join("db"),
        &a.path().join("unused"),
        "ops 镜像空间口令-唯一短语",
    );
    let _x = Cleanup(a.path().join("db"));
    configure_local(&owner, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"镜像家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    item(&owner, &member, "镜像衬衫");
    owner.sync_batch().unwrap();

    // 未配置镜像时的操作被拒绝。
    let err = owner.execute("sync.mirror.sync", json!({})).unwrap_err();
    assert_eq!(err.code, "sync_mirror_not_configured");

    let mirrored = owner
        .execute(
            "sync.mirror.configure",
            json!({"provider_kind":"local_folder","directory":mirror_dir.path()}),
        )
        .unwrap();
    assert!(mirrored["copied"].as_u64().unwrap() > 0, "{mirrored}");
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(
        status["mirror"]["directory"],
        json!(mirror_dir.path()),
        "{status}"
    );

    // 增量单向复制：新事实发布到主目标后镜像补充；重复执行幂等。
    owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"镜像围巾","category":"top"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    let incremental = owner.execute("sync.mirror.sync", json!({})).unwrap();
    assert!(incremental["copied"].as_u64().unwrap() > 0, "{incremental}");
    let again = owner.execute("sync.mirror.sync", json!({})).unwrap();
    assert_eq!(again["copied"], json!(0), "{again}");

    // 显式提升：校验镜像完整性后交换；旧主目标原样保留。
    let promoted = owner.execute("sync.mirror.promote", json!({})).unwrap();
    assert_eq!(
        promoted["target"]["directory"],
        json!(mirror_dir.path()),
        "{promoted}"
    );
    assert_eq!(promoted["mirror"]["directory"], json!(remote.path()));
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(status["target"]["directory"], json!(mirror_dir.path()));
    assert!(remote.path().exists());
    assert!(commit_markers(remote.path()).len() > 0, "旧主目标数据保留");
    let result = owner.sync_batch().unwrap();
    assert_eq!(result["sent"], json!(0), "{result}");
    assert_eq!(result["received"], json!(0), "{result}");
    assert_eq!(
        names(&owner, &member),
        vec!["镜像围巾".to_owned(), "镜像衬衫".to_owned()]
    );

    // 镜像移除：状态清除；主目标不受影响。
    owner.execute("sync.mirror.remove", json!({})).unwrap();
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert!(status["mirror"].is_null(), "{status}");
    owner.sync_batch().unwrap();
}

// ---------------------------------------------------------------------------
// 验收 3：安全回收。tombstone/已发布日志按「全部当前设备水位 + 保留窗口 +
// 备份确认」回收；缺任一条件不得删除；事实读取与后续投递不受回收影响。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn reclaim_follows_device_watermarks_retention_window_and_backup_acknowledgement() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(
        &a.path().join("db"),
        &b.path().join("db"),
        "ops 回收空间口令-唯一短语",
    );
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_local(&owner, remote.path());
    configure_local(&peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"回收家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    item(&owner, &member, "回收衬衫");
    // 只有所有者同步：对端没有水位，任何条件组合都不允许删除。
    owner.sync_batch().unwrap();
    let count_before = change_commits(remote.path()).len();
    let blocked = owner
        .execute(
            "sync.reclaim",
            json!({"retention_seconds":0,"backup_confirmed":true}),
        )
        .unwrap();
    assert_eq!(blocked["reclaimed_changes"], json!(0), "{blocked}");
    assert_eq!(change_commits(remote.path()).len(), count_before);

    // 对端同步后双方水位齐备；各设备再跑一轮确认同步，页位置才推进到
    // 「已完整消费该页」（消费与确认是两个批次，位置只前进到已应用前缀）。
    peer.sync_batch().unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    let recent = owner
        .execute(
            "sync.reclaim",
            json!({"retention_seconds":3600,"backup_confirmed":true}),
        )
        .unwrap();
    assert_eq!(recent["reclaimed_changes"], json!(0), "{recent}");

    // 备份确认缺失：显式拒绝，不产生任何删除。
    let err = owner
        .execute("sync.reclaim", json!({"retention_seconds":0}))
        .unwrap_err();
    assert_eq!(err.code, "sync_reclaim_baseline_required");
    assert_eq!(change_commits(remote.path()).len(), count_before);

    // 水位 + 窗口 + 备份确认齐备：只回收水位已确认的提交；之后新增的提交保留。
    item(&owner, &member, "回收后新增");
    let reclaimed = owner
        .execute(
            "sync.reclaim",
            json!({"retention_seconds":0,"backup_confirmed":true}),
        )
        .unwrap();
    assert!(
        reclaimed["reclaimed_changes"].as_u64().unwrap() > 0,
        "{reclaimed}"
    );
    let count_after = change_commits(remote.path()).len();
    assert_eq!(
        count_after,
        count_before - reclaimed["reclaimed_changes"].as_u64().unwrap() as usize
    );
    assert_eq!(
        owner.execute("sync.status", json!({})).unwrap()["last_reclaim"]["reclaimed_changes"],
        reclaimed["reclaimed_changes"]
    );

    // 本地事实与对端读取不受回收影响；未确认的新增仍可投递；重复回收幂等。
    assert_eq!(names(&owner, &member).len(), 2);
    let idempotent = owner.sync_batch().unwrap();
    assert_eq!(idempotent["pending"], json!(0), "{idempotent}");
    assert_eq!(names(&owner, &member).len(), 2, "回收不得影响本地事实");
    peer.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(names(&peer, &member).len(), 2, "新增事实照常到达对端");
    let again = owner
        .execute(
            "sync.reclaim",
            json!({"retention_seconds":0,"backup_confirmed":true}),
        )
        .unwrap();
    assert_eq!(
        again["reclaimed_changes"],
        json!(0),
        "对端水位未推进前不得二次回收: {again}"
    );
    peer.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    owner.sync_batch().unwrap();
    let caught_up = owner
        .execute(
            "sync.reclaim",
            json!({"retention_seconds":0,"backup_confirmed":true}),
        )
        .unwrap();
    assert!(
        caught_up["reclaimed_changes"].as_u64().unwrap() > 0,
        "对端确认后新增提交可回收: {caught_up}"
    );
    assert_eq!(names(&peer, &member).len(), 2);
}

// ---------------------------------------------------------------------------
// 验收 3b：已处理 blob commit 键登记（sync_asset_commits）不再永久保留——
// 远端对象消失后其登记行随回收扫描清除；登记不影响发现与展示图完整性。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn processed_blob_commit_registry_rows_are_reclaimed_when_remote_objects_disappear() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(
        &a.path().join("db"),
        &b.path().join("db"),
        "ops 登记空间口令-唯一短语",
    );
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_local(&owner, remote.path());
    configure_local(&peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"登记家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let clean = item(&owner, &member, "登记衬衫");
    let png = a.path().join("registry-original.png");
    make_png(&png);
    let asset = owner
        .execute("assets.import", json!({"member_id":member,"path":png}))
        .unwrap();
    owner
        .execute(
            "assets.adopt",
            json!({"member_id":member,"clothing_id":clean["id"],"asset_id":asset["id"]}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    let adopted = peer
        .execute("assets.get", json!({"member_id":member,"id":asset["id"]}))
        .unwrap();
    assert_ne!(
        adopted["display"]["pending"],
        json!(true),
        "对端应已有展示图"
    );

    // 运维直接从目标移除该资产的 blob 对象（模拟按容量的人工清理）。
    let mut blobs = vec![];
    walk_files(remote.path(), &mut blobs);
    let mut removed = 0;
    for path in blobs {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name.ends_with(".object") || name.ends_with(".commit") {
            if path.to_string_lossy().contains("blobs") {
                std::fs::remove_file(&path).unwrap();
                removed += 1;
            }
        }
    }
    assert!(removed > 0, "应存在 blob 对象");
    // 回收扫描后本地登记行清理（dead rows），状态如实上报；同步保持健康。
    let reclaimed = peer
        .execute(
            "sync.reclaim",
            json!({"retention_seconds":0,"backup_confirmed":true}),
        )
        .unwrap();
    assert!(
        reclaimed["reclaimed_blob_registry"].as_u64().unwrap() > 0,
        "{reclaimed}"
    );
    let healthy = peer.sync_batch().unwrap();
    assert_eq!(healthy["quarantined"], json!(0), "{healthy}");
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":clean["id"]}))
            .unwrap()["name"],
        "登记衬衫"
    );
}

// ---------------------------------------------------------------------------
// 验收 4：密钥生命周期的同步语境。改密码不影响空间密钥（同步继续）；
// 错误密码恢复失败不破坏现有状态；撤销设备后该设备同步被拒；轮换后
// 创建设备按协议以新纪元重新发布，新设备正常加入并取得当前事实。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn key_lifecycle_password_change_revocation_and_rotation_sync_semantics() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let c = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let password = "ops 生命周期口令-唯一短语";
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"), password);
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_local(&owner, remote.path());
    configure_local(&peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"密钥家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let shirt = item(&owner, &member, "生命周期衬衫");
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();

    // 改密码：重包装本机钱包，不影响空间密钥；同步照常双向收敛。
    let state: Value = owner.execute("encryption.status", json!({})).unwrap();
    let bundle = state["state"]["bundle"].clone();
    owner
        .execute(
            "encryption.password.change",
            json!({"old_password":password,"new_password":"ops 新口令-二次短语"}),
        )
        .unwrap();
    item(&owner, &member, "改密后衬衫");
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    assert!(names(&peer, &member).contains(&"改密后衬衫".to_owned()));

    // 错误密码恢复失败：不破坏现有状态，同步继续可用。
    let err = owner
        .execute(
            "encryption.restore",
            json!({"bundle":bundle,"password":"ops 错误口令-绝不相同"}),
        )
        .unwrap_err();
    assert_eq!(err.code, "authentication_failed");
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":shirt["id"],"name":"恢复失败后改名"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":shirt["id"]}))
            .unwrap()["name"],
        "恢复失败后改名"
    );

    // 撤销对端（密钥轮换到纪元 2）。轮换即致盲：所有者新事实只进入
    // 纪元 2 命名空间，旧纪元设备不再取得任何新事实。
    let peer_state: Value = peer.execute("encryption.status", json!({})).unwrap();
    let peer_device = peer_state["state"]["device_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let revoked = owner
        .execute(
            "encryption.device.revoke",
            json!({"device_id":peer_device,"password":"ops 新口令-二次短语"}),
        )
        .unwrap();
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":shirt["id"],"name":"撤销后改名"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    let sealed = peer.sync_batch().unwrap();
    assert_eq!(
        sealed["received"],
        json!(0),
        "被撤销设备不得取得轮换后的新事实: {sealed}"
    );
    assert_ne!(
        peer.execute("clothing.get", json!({"member_id":member,"id":shirt["id"]}))
            .unwrap()["name"],
        "撤销后改名",
        "旧纪元设备的视图停留在撤销前"
    );

    // 被撤销设备经设备状态协议得知撤销后自锁：同步被显式拒绝。
    let err = peer
        .execute(
            "encryption.device.apply",
            json!({"manifest":revoked["manifest"]}),
        )
        .unwrap_err();
    assert_eq!(err.code, "device_revoked", "{err:?}");
    let err = peer.sync_batch().unwrap_err();
    assert_eq!(err.code, "device_revoked", "{err:?}");

    // 所有者以新纪元继续同步：事实按协议重新发布（旧纪元数据原样保留不可读）。
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":shirt["id"],"name":"轮换后改名"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();

    // 新设备在轮换后配对加入：取得当前事实。
    let peer = {
        let fresh = Application::open(&c.path().join("db")).unwrap();
        let offer = owner
            .execute("encryption.pair.offer", json!({"device_name":"ops 新设备"}))
            .unwrap();
        let receipt = fresh
            .execute(
                "encryption.pair.accept",
                json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"ops 新口令-二次短语"}),
            )
            .unwrap();
        let confirmation = owner
            .execute(
                "encryption.pair.confirm",
                json!({"receipt":receipt["receipt"]}),
            )
            .unwrap();
        fresh
            .execute(
                "encryption.device.apply",
                json!({"manifest":confirmation["manifest"]}),
            )
            .unwrap();
        fresh
    };
    let _z = Cleanup(c.path().join("db"));
    configure_local(&peer, remote.path());
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":shirt["id"]}))
            .unwrap()["name"],
        "轮换后改名",
        "新设备应通过新纪元日志取得当前事实"
    );
    assert_eq!(names(&peer, &member), names(&owner, &member));
    // 双向收敛继续成立。
    item(&peer, &member, "新设备衬衫");
    peer.sync_batch().unwrap();
    owner.sync_batch().unwrap();
    assert!(names(&owner, &member).contains(&"新设备衬衫".to_owned()));
}

// ---------------------------------------------------------------------------
// 纪元轮换后切换远端（review P1-1）：目标键滚动到纪元派生键后，切换远端的
// 首个批次必须落在新远端。凭据语义由真实 WebDAV 回归钉住；本地文件夹
// 覆盖同一生命周期的键滚动与发布落点。
// ---------------------------------------------------------------------------

fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn epoch_rotation_then_remote_switch_publishes_to_new_remote() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote1 = tempfile::tempdir().unwrap();
    let remote2 = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(
        &a.path().join("db"),
        &b.path().join("db"),
        "纪元切换空间口令-唯一短语",
    );
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_local(&owner, remote1.path());
    let member = owner
        .execute("member.create", json!({"name":"纪元切换家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    item(&owner, &member, "轮换前衬衫");
    owner.sync_batch().unwrap();

    // 纪元推进（撤销对端）：下一次入队把目标键滚动到纪元派生键。
    let peer_state: Value = peer.execute("encryption.status", json!({})).unwrap();
    let peer_device = peer_state["state"]["device_id"].as_str().unwrap().to_owned();
    owner
        .execute(
            "encryption.device.revoke",
            json!({"device_id":peer_device,"password":"纪元切换空间口令-唯一短语"}),
        )
        .unwrap();
    let space = owner
        .execute("space.default", json!({}))
        .unwrap()["space_id"]
        .as_str()
        .unwrap()
        .to_owned();
    owner.sync_batch().unwrap();
    let directory1 = std::fs::canonicalize(remote1.path()).unwrap();
    let rolled_key1 = sha256_hex(
        &json!({"directory":directory1.to_string_lossy(),"space_id":space,"epoch":2}).to_string(),
    );
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(
        status["target"]["target_key"],
        json!(rolled_key1),
        "纪元推进后目标键滚动到纪元派生键: {status}"
    );

    // 切换到另一个真实本地文件夹远端：完整复制校验通过才切换，旧远端原样保留。
    let remote1_commits = commit_markers(remote1.path()).len();
    let switched = owner
        .execute(
            "sync.target.switch",
            json!({"provider_kind":"local_folder","directory":remote2.path()}),
        )
        .unwrap();
    assert!(switched["copied"].as_u64().unwrap() > 0, "{switched}");
    assert_eq!(
        commit_markers(remote1.path()).len(),
        remote1_commits,
        "切换复制不得改动旧远端"
    );

    // 切换后首个批次：新事实发布到新远端，旧远端不再有新写入。
    item(&owner, &member, "切换后衬衫");
    owner.sync_batch().unwrap();
    let directory2 = std::fs::canonicalize(remote2.path()).unwrap();
    let rolled_key2 = sha256_hex(
        &json!({"directory":directory2.to_string_lossy(),"space_id":space,"epoch":2}).to_string(),
    );
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(
        status["target"]["target_key"],
        json!(rolled_key2),
        "切换远端后批次以新远端的纪元派生键发布: {status}"
    );
    assert_eq!(
        commit_markers(remote1.path()).len(),
        remote1_commits,
        "切换后不得再写旧远端"
    );
    assert!(
        commit_markers(remote2.path()).len() > remote1_commits,
        "切换后首个批次落在新远端"
    );
}

/// 回归（review P1-1）：纪元轮换滚动目标键时，派生键上可能残留更早周期
/// 留下的凭据。迁移必须以「当前文档正在使用的键」的凭据为权威无条件覆盖；
/// 否则「已存在即跳过」会让切换远端后的首个批次拿旧凭据连新远端。
#[test]
#[ignore = "requires real Apache WebDAV server and the OS credential store"]
fn epoch_rotation_migration_overwrites_stale_credential_and_switch_lands_on_new_remote() {
    let server = webdav_server::WebDavServer::spawn();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let owner_db = a.path().join("wardrobe.sqlite");
    let peer_db = b.path().join("wardrobe.sqlite");
    let (owner, peer) = pair(&owner_db, &peer_db, "epoch 凭据空间口令-唯一短语");
    let _x = Cleanup(owner_db);
    let _y = Cleanup(peer_db);
    let root1 = format!("wd-review-a-{}", uuid::Uuid::now_v7().simple());
    let configured = owner
        .execute(
            "sync.configure",
            json!({
                "provider_kind":"webdav","url":server.base_url,"root":root1,
                "username":webdav_server::USERNAME,"password":webdav_server::PASSWORD,
            }),
        )
        .unwrap();
    // 明文 http 目标在配置回执与状态页如实标注（review P2-9）。
    assert_eq!(
        configured["target"]["transport"],
        json!("plain-http"),
        "{configured}"
    );
    let member = owner
        .execute("member.create", json!({"name":"凭据纪元家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    item(&owner, &member, "轮换前衬衫");
    owner.sync_batch().unwrap();

    // 纪元推进（撤销对端）。
    let peer_state: Value = peer.execute("encryption.status", json!({})).unwrap();
    let peer_device = peer_state["state"]["device_id"].as_str().unwrap().to_owned();
    owner
        .execute(
            "encryption.device.revoke",
            json!({"device_id":peer_device,"password":"epoch 凭据空间口令-唯一短语"}),
        )
        .unwrap();

    // 在纪元派生键上预置「上一周期残留的旧凭据」（真实场景：上次轮换迁移
    // 留下的上一远端凭据；与当前文档权威凭据不同）。
    let space = owner
        .execute("space.default", json!({}))
        .unwrap()["space_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let rolled_key = sha256_hex(&json!({"directory":Value::Null,"space_id":space,"epoch":2}).to_string());
    keyring::Entry::new("wardrobe-agent.sync", &rolled_key)
        .unwrap()
        .set_password("stale-credential-from-previous-cycle")
        .unwrap();

    // 入队滚动目标键：迁移必须用当前权威凭据覆盖残留，首个批次成功。
    owner.sync_batch().unwrap();
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(
        status["target"]["target_key"],
        json!(rolled_key),
        "纪元推进后目标键滚动到纪元派生键: {status}"
    );
    assert_eq!(status["target"]["transport"], json!("plain-http"));
    let stored = keyring::Entry::new("wardrobe-agent.sync", &rolled_key)
        .unwrap()
        .get_password()
        .unwrap();
    assert_eq!(
        stored,
        webdav_server::PASSWORD,
        "滚动键上的残留凭据必须被当前权威凭据无条件覆盖"
    );

    // 切换远端（同一服务器的第二个根目录）：复制校验后原子切换；同纪元下
    // 目标键再次滚动到同一纪元派生键，迁移仍以切换后的权威凭据覆盖，
    // 首个批次成功发布到新远端。
    let root2 = format!("wd-review-b-{}", uuid::Uuid::now_v7().simple());
    let switched = owner
        .execute(
            "sync.target.switch",
            json!({
                "provider_kind":"webdav","url":server.base_url,"root":root2,
                "username":webdav_server::USERNAME,"password":webdav_server::PASSWORD,
            }),
        )
        .unwrap();
    assert!(switched["copied"].as_u64().unwrap() > 0, "{switched}");
    assert_eq!(switched["target"]["transport"], json!("plain-http"));
    item(&owner, &member, "切换远端后衬衫");
    owner.sync_batch().unwrap();
    let status: Value = owner.execute("sync.status", json!({})).unwrap();
    assert_eq!(status["target"]["root"], json!(root2), "{status}");
    assert_eq!(status["target"]["target_key"], json!(rolled_key));
    assert_eq!(
        keyring::Entry::new("wardrobe-agent.sync", &rolled_key)
            .unwrap()
            .get_password()
            .unwrap(),
        webdav_server::PASSWORD,
        "切换远端后滚动键仍持有新远端权威凭据"
    );

    // 解除配对清理滚动键凭据。
    owner.execute("encryption.unpair", json!({})).unwrap();
    assert!(
        keyring::Entry::new("wardrobe-agent.sync", &rolled_key)
            .unwrap()
            .get_password()
            .is_err(),
        "unpair 应清理纪元派生键凭据"
    );
}
