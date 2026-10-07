//! Deterministic two-connection contract for the shared publisher index
//! cursor. Both workers drive the unchanged public Application API against the
//! same installation with real SQLite, real transport files and real OS
//! credentials; a thread-local scheduling gate only decides when the overlapping
//! worker observes the remote frontier, and transport file fixtures perform the
//! genuine remote rollback and the consumed-page hiding. No private SQL state is
//! mutated or asserted.
use crate::Application;
use serde_json::json;

struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(app) = Application::open(&self.0) {
            let _ = app.execute("encryption.unpair", json!({}));
        }
    }
}

/// Releases the scheduling gate even when an assertion fails between park and
/// release, so the public worker can never stay suspended into the next test.
struct ReleaseOnDrop(crate::sync::schedule::Gate);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn sync_once(app: &Application) -> serde_json::Value {
    let queued = app.execute("sync.run", json!({})).unwrap();
    app.execute("sync.process", json!({"id":queued["job_id"]}))
        .unwrap()
}

#[test]
#[ignore = "requires actual OS credential store"]
fn overlapping_local_jobs_never_corrupt_the_index_cursor_and_reject_fresh_remote_rollback() {
    let owner_home = tempfile::tempdir().unwrap();
    let peer_home = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let owner_db = owner_home.path().join("wardrobe.sqlite");
    let peer_db = peer_home.path().join("wardrobe.sqlite");
    let owner = Application::open(&owner_db).unwrap();
    let peer = Application::open(&peer_db).unwrap();
    let _guards = [Cleanup(owner_db.clone()), Cleanup(peer_db.clone())];
    owner
        .execute(
            "encryption.create",
            json!({"password":"generated cursor fixture","device_name":"cursor owner"}),
        )
        .unwrap();
    let member = owner.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let offer = owner
        .execute(
            "encryption.pair.offer",
            json!({"device_name":"cursor peer"}),
        )
        .unwrap();
    let receipt = peer
        .execute(
            "encryption.pair.accept",
            json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"generated peer fixture"}),
        )
        .unwrap();
    let confirmed = owner
        .execute(
            "encryption.pair.confirm",
            json!({"receipt":receipt["receipt"]}),
        )
        .unwrap();
    peer.execute(
        "encryption.device.apply",
        json!({"manifest":confirmed["manifest"]}),
    )
    .unwrap();
    for app in [&owner, &peer] {
        app.execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote.path()}),
        )
        .unwrap();
    }
    for n in 1..=2 {
        owner
            .execute(
                "clothing.create",
                json!({"member_id":member,"name":format!("cursor garment {n}"),"category":"top"}),
            )
            .unwrap();
        sync_once(&owner);
    }
    // The peer consumes both pages and then persists their consumed prefix.
    sync_once(&peer);
    sync_once(&peer);
    owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"cursor garment 3","category":"top"}),
        )
        .unwrap();
    sync_once(&owner);
    let owner_state = owner.execute("encryption.status", json!({})).unwrap()["state"].clone();
    let space = owner_state["space_id"].as_str().unwrap().to_owned();
    let publisher = owner_state["device_id"].as_str().unwrap().to_owned();
    let head_path = remote
        .path()
        .join(format!("{space}/epochs/1/index/{publisher}/head.index"));
    let head_page3 = std::fs::read(&head_path).unwrap();
    let job1 = peer.execute("sync.run", json!({})).unwrap()["job_id"].clone();
    let job2 = peer.execute("sync.run", json!({})).unwrap()["job_id"].clone();
    assert_ne!(
        job1, job2,
        "overlapping workers need two distinct queued public sync jobs"
    );
    let gate = crate::sync::schedule::Gate::new();
    let _release = ReleaseOnDrop(gate.clone());
    let worker = {
        let gate = gate.clone();
        let peer_db = peer_db.clone();
        std::thread::spawn(move || {
            crate::sync::schedule::hold_after_prior_read(gate);
            let worker = Application::open(&peer_db).unwrap();
            worker.execute("sync.process", json!({"id":job1})).unwrap()
        })
    };
    assert!(
        gate.wait_parked(),
        "the overlapping worker never reached its scheduling point"
    );
    // The publisher advances the remote frontier while the overlapping worker
    // holds its accepted snapshot, then the second connection on the same
    // installation processes the other queued job.
    owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"cursor garment 4","category":"top"}),
        )
        .unwrap();
    sync_once(&owner);
    let advanced = peer.execute("sync.process", json!({"id":job2})).unwrap();
    assert_ne!(advanced["state"], "failed", "{}", advanced);
    // B 先继续消化第 3/4 页，使 cursor 推进到 next_page=5 且 previous 摘要与
    // 第 4 页配对，然后才释放持有过期快照的 A。
    let consumed = sync_once(&peer);
    assert_ne!(consumed["state"], "failed", "{}", consumed);
    gate.release();
    let held = worker.join().unwrap();
    assert_ne!(held["state"], "failed", "{}", held);
    // 合法本地重叠：被接管的过期快照必须是安全 no-op，不得谎报 storage_corrupt。
    let status = peer.execute("sync.status", json!({})).unwrap();
    assert_eq!(
        status["transport_errors"].as_array().unwrap().len(),
        0,
        "benign local overlap must not raise a backend-corruption alarm: {status}"
    );
    assert_eq!(status["pending"].as_u64().unwrap(), 0, "{status}");
    let listed = peer
        .execute("clothing.list", json!({"member_id":member}))
        .unwrap();
    assert_eq!(
        listed["items"].as_array().unwrap().len(),
        4,
        "every published page must reach the peer across overlapping jobs"
    );
    // 隐藏已消费的第 3/4 页文件：配对正确的 cursor+digest 不得再触碰它们。
    let hidden = remote.path().join("hidden-pages");
    std::fs::create_dir_all(&hidden).unwrap();
    for page in [3u64, 4] {
        std::fs::rename(
            remote.path().join(format!(
                "{space}/epochs/1/index/{publisher}/{page:020}.page"
            )),
            hidden.join(format!("{page:020}.page")),
        )
        .unwrap();
    }
    owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"cursor garment 5","category":"top"}),
        )
        .unwrap();
    sync_once(&owner);
    let head_page5 = std::fs::read(&head_path).unwrap();
    let witnessed = sync_once(&peer);
    assert_ne!(witnessed["state"], "failed", "{}", witnessed);
    let status = peer.execute("sync.status", json!({})).unwrap();
    assert_eq!(
        status["transport_errors"].as_array().unwrap().len(),
        0,
        "continued sync must follow the paired cursor to the new page without transport errors: {status}"
    );
    assert_eq!(status["pending"].as_u64().unwrap(), 0, "{status}");
    let listed = peer
        .execute("clothing.list", json!({"member_id":member}))
        .unwrap();
    assert_eq!(
        listed["items"].as_array().unwrap().len(),
        5,
        "the new page must arrive without rereading the hidden consumed pages"
    );
    // 真正的远端回退：发布者把 head 回写到旧版本，随后全新同步必须拒绝。
    std::fs::write(&head_path, &head_page3).unwrap();
    let rejected = sync_once(&peer);
    assert_ne!(rejected["state"], "failed", "{}", rejected);
    let status = peer.execute("sync.status", json!({})).unwrap();
    let alarms = status["transport_errors"].as_array().unwrap();
    assert!(
        alarms
            .iter()
            .any(|e| e["publisher"] == publisher && e["code"] == "storage_corrupt"),
        "a freshly read rolled-back remote head must be rejected: {status}"
    );
    // 恢复发布者 frontier 后继续同步必须收敛并清除错误状态。
    std::fs::write(&head_path, &head_page5).unwrap();
    let resumed = sync_once(&peer);
    assert_ne!(resumed["state"], "failed", "{}", resumed);
    let status = peer.execute("sync.status", json!({})).unwrap();
    assert_eq!(
        status["transport_errors"].as_array().unwrap().len(),
        0,
        "continued healthy sync must clear the publisher alarm: {status}"
    );
    assert_eq!(status["pending"].as_u64().unwrap(), 0, "{status}");
}
