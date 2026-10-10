//! #27 本地文件夹双设备同步：剩余验收切片。
//!
//! 全部测试走公开 `Application` API（真实 SQLite + 临时资产目录 + 真实 OS
//! 钥匙串），两个真实安装实例共享一个本地文件夹目标。覆盖前人局部闭环之外
//! 的缺口：删除 tombstone 与受保护分类删除的跨设备往返、跨成员隔离、重复
//! 同步幂等与包内容白名单（无原图/无凭据/无原始文件名）、损坏对象不误应用、
//! 资产发现的预算与公平收敛，以及重开数据库（重跑迁移）后同步继续工作。

use serde_json::{json, Value};
use wardrobe_core::Application;

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
fn pair(a: &std::path::Path, b: &std::path::Path) -> (Application, Application) {
    let owner = Application::open(a).unwrap();
    let peer = Application::open(b).unwrap();
    owner
        .execute(
            "encryption.create",
            json!({"password":"dual device space password 唯一短语","device_name":"dual owner"}),
        )
        .unwrap();
    let offer = owner
        .execute("encryption.pair.offer", json!({"device_name":"dual peer"}))
        .unwrap();
    let receipt = peer
        .execute(
            "encryption.pair.accept",
            json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"dual device space password 唯一短语"}),
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

fn configure_both(owner: &Application, peer: &Application, remote: &std::path::Path) {
    for app in [owner, peer] {
        app.execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote}),
        )
        .unwrap();
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

/// 递归收集目标文件夹内每个文件的（相对路径, 字节）。
fn collect_files(root: &std::path::Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, &relative, out);
        } else {
            out.push((relative, std::fs::read(&path).unwrap()));
        }
    }
}

fn select(files: &[(String, Vec<u8>)], folder: &str, suffix: &str) -> Vec<(String, Vec<u8>)> {
    files
        .iter()
        .filter(|(p, _)| {
            let marker = format!("{folder}/");
            (p.starts_with(&marker) || p.contains(&format!("/{marker}")))
                && p.ends_with(suffix)
                && !p.ends_with(".lock")
        })
        .cloned()
        .collect()
}

fn assert_not_in_package(remote: &std::path::Path, needles: &[&[u8]]) {
    let mut files = vec![];
    collect_files(remote, "", &mut files);
    assert!(
        files.len() > 3,
        "同步目标应包含同步对象，实际文件数 {}",
        files.len()
    );
    for (relative, bytes) in &files {
        for needle in needles {
            assert!(
                !bytes.windows(needle.len()).any(|v| v == *needle),
                "同步包文件 {relative} 泄露了禁止内容（{} 字节序列）",
                needle.len()
            );
        }
    }
}

#[test]
#[ignore = "requires actual OS credential store"]
fn deleted_facts_and_protected_category_moves_reach_peers_exactly() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _c1 = Cleanup(a.path().join("db"));
    let _c2 = Cleanup(b.path().join("db"));
    let member = owner
        .execute("member.create", json!({"name":"删除切片成员"}))
        .unwrap();
    let member = member["member_id"].as_str().unwrap().to_owned();
    let category = owner
        .execute(
            "category.create",
            json!({"member_id":member,"name":"舞台服装"}),
        )
        .unwrap();
    let category_id = category["id"].as_str().unwrap().to_owned();
    let garment = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"演出外套","category":category_id}),
        )
        .unwrap();
    let garment_id = garment["id"].as_str().unwrap().to_owned();
    configure_both(&owner, &peer, remote.path());
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();

    // 受保护删除：默认分类 other 不可删除。
    let protected = peer.execute("category.delete", json!({"member_id":member,"id":"other"}));
    assert_eq!(protected.unwrap_err().code, "conflict");

    // tombstone 往返：删除到达对端后状态一致，列表默认不再显示。
    owner
        .execute(
            "clothing.delete",
            json!({"member_id":member,"id":garment_id}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    let deleted_on_peer = peer
        .execute("clothing.get", json!({"member_id":member,"id":garment_id}))
        .unwrap();
    assert_eq!(deleted_on_peer["status"], "deleted");
    assert!(!deleted_on_peer["deleted_at"].as_str().unwrap().is_empty());
    let live = peer
        .execute("clothing.list", json!({"member_id":member}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(live, 0, "回收站单品不应出现在默认列表");

    // 恢复往返。
    owner
        .execute(
            "clothing.restore",
            json!({"member_id":member,"id":garment_id}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":garment_id}))
            .unwrap()["status"],
        "active"
    );

    // 受保护分类删除：A 删除分类时衣物自动转入 other；对端收到迁移与 tombstone。
    let moved = owner
        .execute(
            "category.delete",
            json!({"member_id":member,"id":category_id}),
        )
        .unwrap();
    assert_eq!(moved["moved"], 1);
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    let listing = peer
        .execute("category.list", json!({"member_id":member}))
        .unwrap();
    let names: Vec<&str> = listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v["id"].as_str())
        .collect();
    assert!(
        !names.contains(&category_id.as_str()),
        "已删分类不应再出现在对端分类列表：{names:?}"
    );
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":garment_id}))
            .unwrap()["category"],
        "other",
        "对端衣物应与源端一致地转入 other"
    );

    // 全新第三台设备只收到当前事实（删除后的历史不复活，衣物已归入 other）。
    let c = tempfile::tempdir().unwrap();
    let late = Application::open(&c.path().join("db")).unwrap();
    let _c3 = Cleanup(c.path().join("db"));
    let offer = owner
        .execute(
            "encryption.pair.offer",
            json!({"device_name":"late third device"}),
        )
        .unwrap();
    let receipt = late
        .execute(
            "encryption.pair.accept",
            json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"dual device space password 唯一短语"}),
        )
        .unwrap();
    let confirmation = owner
        .execute(
            "encryption.pair.confirm",
            json!({"receipt":receipt["receipt"]}),
        )
        .unwrap();
    late.execute(
        "encryption.device.apply",
        json!({"manifest":confirmation["manifest"]}),
    )
    .unwrap();
    late.execute(
        "sync.configure",
        json!({"provider_kind":"local_folder","directory":remote.path()}),
    )
    .unwrap();
    let mut rounds = 0;
    loop {
        let step = late.sync_batch().unwrap();
        rounds += 1;
        if step["has_more"] == false && step["missing_assets"] == 0 {
            break;
        }
        assert!(rounds <= 8, "第三设备 bootstrap 未在有限批内收敛：{step}");
    }
    assert_eq!(
        late.execute("clothing.get", json!({"member_id":member,"id":garment_id}))
            .unwrap()["category"],
        "other"
    );
    assert_eq!(
        late.execute("clothing.list", json!({"member_id":member}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let late_listing = late
        .execute("category.list", json!({"member_id":member}))
        .unwrap();
    let late_names: Vec<&str> = late_listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v["id"].as_str())
        .collect();
    assert!(!late_names.contains(&category_id.as_str()));
}

#[test]
#[ignore = "requires actual OS credential store"]
fn two_members_stay_isolated_across_devices_with_history_branches() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _c1 = Cleanup(a.path().join("db"));
    let _c2 = Cleanup(b.path().join("db"));
    let second = owner
        .execute("member.create", json!({"name":"第二位成员"}))
        .unwrap();
    let second = second["member_id"].as_str().unwrap().to_owned();
    let top = owner
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"默认成员衬衫","category":"top"}),
        )
        .unwrap();
    // 每台安装的 default 是本地别名，实际成员身份是 UUID；跨设备一律用源端 ID。
    let owner_default = top["member_id"].as_str().unwrap().to_owned();
    let pants = owner
        .execute(
            "clothing.create",
            json!({"member_id":second,"name":"第二成员长裤","category":"bottom"}),
        )
        .unwrap();
    configure_both(&owner, &peer, remote.path());
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();

    // 两个成员的衣物各自到达，且成员边界严格隔离。
    assert_eq!(
        peer.execute("clothing.list", json!({"member_id":owner_default}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        peer.execute("clothing.list", json!({"member_id":second}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let cross = peer.execute("clothing.get", json!({"member_id":second,"id":top["id"]}));
    assert_eq!(cross.unwrap_err().code, "not_found", "成员边界不允许跨读");
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":second,"id":pants["id"]}))
            .unwrap()["name"],
        "第二成员长裤"
    );

    // 事件顺序与历史：同一根穿着的修订按序到达，历史与分支完整一致。
    let outfit = owner
        .execute(
            "outfit.create",
            json!({"member_id":"default","name":"隔离切片搭配","items":[{"clothing_id":top["id"],"role":"top","layer":0}]}),
        )
        .unwrap();
    let wear = owner
        .execute(
            "wear.record",
            json!({"member_id":"default","source_outfit_id":outfit["id"],"source_version":1,"local_date":"2026-10-04","timezone":"Asia/Shanghai","operation_key":"isolation wear"}),
        )
        .unwrap();
    owner
        .execute(
            "wear.correct",
            json!({"member_id":"default","root_event_id":wear["root_event_id"],"expected_revision":1,"clothing_ids":[top["id"]],"local_date":"2026-10-05","timezone":"Asia/Shanghai","operation_key":"isolation correction"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    let owner_history = owner
        .execute(
            "wear.history",
            json!({"member_id":owner_default,"root_event_id":wear["root_event_id"]}),
        )
        .unwrap()["items"]
        .clone();
    let peer_history = peer
        .execute(
            "wear.history",
            json!({"member_id":owner_default,"root_event_id":wear["root_event_id"]}),
        )
        .unwrap()["items"]
        .clone();
    assert_eq!(owner_history.as_array().unwrap().len(), 2);
    assert_eq!(owner_history, peer_history, "顺序同步后历史逐字节一致");
    let revision_two = owner_history.as_array().unwrap().last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let owner_branches = owner
        .execute(
            "sync.history",
            json!({"member_id":owner_default,"entity_type":"wear_events","entity_id":revision_two}),
        )
        .unwrap()["items"]
        .clone();
    let peer_branches = peer
        .execute(
            "sync.history",
            json!({"member_id":owner_default,"entity_type":"wear_events","entity_id":revision_two}),
        )
        .unwrap()["items"]
        .clone();
    assert_eq!(owner_branches, peer_branches);
    assert!(
        !owner_branches.as_array().unwrap().is_empty(),
        "已应用的不可变事件应保留可审查分支"
    );
    assert_eq!(
        peer.execute("sync.conflicts", json!({"member_id":owner_default}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "顺序同步不应产生冲突"
    );
}

#[test]
#[ignore = "requires actual OS credential store"]
fn repeat_sync_is_idempotent_and_package_carries_only_adopted_displays() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _c1 = Cleanup(a.path().join("db"));
    let _c2 = Cleanup(b.path().join("db"));
    let member = owner
        .execute("member.create", json!({"name":"白名单成员"}))
        .unwrap();
    let member = member["member_id"].as_str().unwrap().to_owned();
    let garment = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"白名单衬衫","category":"top"}),
        )
        .unwrap();

    // 原图（含独特字节）、真实 OS 钥匙串凭据、空间口令全部留在本机。
    let original = a.path().join("私密原图-请勿同步-9f2b1c.png");
    let original_bytes = unique_png(&original, 600, 7);
    owner
        .execute(
            "provider.save",
            json!({"name":"白名单端点","endpoint":"http://127.0.0.1:9/v1","model":"test-model","capabilities":["text"],"authentication":"bearer"}),
        )
        .unwrap();
    let providers = owner.execute("provider.list", json!({})).unwrap();
    let provider_id = providers["items"].as_array().unwrap()[0]["id"].clone();
    owner
        .execute(
            "provider.credential.set",
            json!({"id":provider_id,"api_key":"sk-绝密凭据-8c1a55e7"}),
        )
        .unwrap();

    let imported = owner
        .execute("assets.import", json!({"member_id":member,"path":original}))
        .unwrap();
    owner
        .execute(
            "assets.adopt",
            json!({"member_id":member,"clothing_id":garment["id"],"asset_id":imported["id"]}),
        )
        .unwrap();
    configure_both(&owner, &peer, remote.path());

    let first = owner.sync_batch().unwrap();
    assert!(first["sent"].as_u64().unwrap() > 0);
    // 重复同步幂等：没有新事实时不再发送任何对象。
    let second = owner.sync_batch().unwrap();
    assert_eq!(second["sent"], 0, "重复同步不应重复发送：{second}");
    assert_eq!(second["received"], 0);
    assert_eq!(second["pending"], 0);
    peer.sync_batch().unwrap();

    // 包内容白名单：无原图字节、无源文件名、无凭据、无空间口令。
    assert_not_in_package(
        remote.path(),
        &[
            original_bytes.as_slice(),
            "私密原图-请勿同步".as_bytes(),
            "sk-绝密凭据-8c1a55e7".as_bytes(),
            "dual device space password".as_bytes(),
        ],
    );
    let mut files = vec![];
    collect_files(remote.path(), "", &mut files);
    let mut space_dirs: Vec<String> = std::fs::read_dir(remote.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(space_dirs.len(), 1, "目标顶层应只有一个空间目录");
    let space_dir = space_dirs.pop().unwrap();
    let mut blob_commits = 0;
    for (relative, _) in &files {
        if relative.ends_with(".lock") {
            continue; // 传输本地的锁边车文件，不属于数据包
        }
        let rest = relative
            .strip_prefix(&format!("{space_dir}/"))
            .unwrap_or_else(|| panic!("意外顶层文件 {relative}"));
        let parts: Vec<&str> = rest.split('/').collect();
        let allowed = match parts.as_slice() {
            [first, ..] if *first == "changes" || *first == "blobs" => {
                relative.ends_with(".commit") || relative.ends_with(".object")
            }
            ["epochs", n, folder, ..] if n.chars().all(|c| c.is_ascii_digit()) => match *folder {
                "changes" | "blobs" => {
                    relative.ends_with(".commit") || relative.ends_with(".object")
                }
                "index" => {
                    relative.ends_with(".page")
                        || relative.ends_with("head.index")
                        || relative.ends_with("mode.index")
                }
                // #31：设备消费水位记录（加密信封，按设备一台一个）。
                "watermarks" => relative.ends_with(".commit"),
                _ => false,
            },
            _ => false,
        };
        assert!(allowed, "同步包出现未知文件 {relative}");
        assert!(
            !relative.contains("/media/"),
            "同步包不得包含本机 media 资产目录"
        );
        if rest.contains("blobs/") && relative.ends_with(".commit") {
            blob_commits += 1;
        }
    }
    assert_eq!(blob_commits, 1, "只应发布 1 张最终采用的压缩商品展示图");

    // 新设备无原图：展示图可用、缩略图本机重建、可继续编辑与记录穿着。
    let remote_asset = peer
        .execute(
            "assets.get",
            json!({"member_id":member,"id":imported["id"]}),
        )
        .unwrap();
    assert!(remote_asset["original"].is_null(), "原图不跨设备");
    assert!(
        !remote_asset["display"]["local_path"]
            .as_str()
            .unwrap_or("")
            .is_empty(),
        "展示图应已本机落地：{}",
        remote_asset
    );
    let preview = peer
        .execute(
            "assets.preview",
            json!({"member_id":member,"id":imported["id"],"variant":"thumbnail"}),
        )
        .unwrap();
    assert!(preview["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/webp;base64,"));
    peer.execute(
        "clothing.update",
        json!({"member_id":member,"id":garment["id"],"notes":"无原图设备仍可编辑"}),
    )
    .unwrap();
    peer.execute(
        "wear.record",
        json!({"member_id":member,"clothing_ids":[garment["id"]],"local_date":"2026-10-06","timezone":"Asia/Shanghai","operation_key":"whitelist wear on peer"}),
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
            .unwrap()["notes"],
        "无原图设备仍可编辑",
        "对端编辑按序回流"
    );

    // 重开数据库：迁移重跑后同步状态与后续同步继续工作。
    drop(owner);
    drop(peer);
    let owner = Application::open(&a.path().join("db")).unwrap();
    let peer = Application::open(&b.path().join("db")).unwrap();
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":garment["id"],"name":"重开后仍同步"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member,"id":garment["id"]})
        )
        .unwrap()["name"],
        "重开后仍同步"
    );
}

#[test]
#[ignore = "requires actual OS credential store"]
fn interrupted_or_corrupt_uploads_never_misapply_partial_data() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _c1 = Cleanup(a.path().join("db"));
    let _c2 = Cleanup(b.path().join("db"));
    let garment = owner
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"中断切片外套","category":"outerwear"}),
        )
        .unwrap();
    // default 是本地别名；跨设备校验一律用源端成员 UUID。
    let member = garment["member_id"].as_str().unwrap().to_owned();
    configure_both(&owner, &peer, remote.path());
    owner.sync_batch().unwrap();

    let mut files = vec![];
    collect_files(remote.path(), "", &mut files);
    let objects = select(&files, "changes", ".object");
    let markers = select(&files, "changes", ".commit");
    assert!(objects.len() >= 2, "应存在成员与衣物两个操作对象");
    assert_eq!(objects.len(), markers.len());

    // 中断发生在对象与发布标记之间：对端看不到任何半成品，本地事实为零。
    // （源端成员尚未同步到对端时，list 返回 not_found 而不是空列表。）
    let peer_items = |app: &Application| -> usize {
        match app.execute("clothing.list", json!({"member_id":member})) {
            Ok(v) => v["items"].as_array().unwrap().len(),
            Err(e) if e.code == "not_found" => 0,
            Err(e) => panic!("意外错误：{}", e.code),
        }
    };
    for (path, _) in &markers {
        let backup = a
            .path()
            .join(std::path::Path::new(path).file_name().unwrap());
        std::fs::rename(remote.path().join(path), &backup).unwrap();
    }
    let hidden = peer.sync_batch().unwrap();
    assert_eq!(hidden["received"], 0, "缺少发布标记的对象不得被发现");
    assert_eq!(peer_items(&peer), 0);
    for (path, _) in &markers {
        let backup = a
            .path()
            .join(std::path::Path::new(path).file_name().unwrap());
        std::fs::rename(&backup, remote.path().join(path)).unwrap();
    }

    // 截断/损坏的已发布对象必须整体失败，不得把部分数据误应用到对端。
    let mut originals = vec![];
    for (path, bytes) in &objects {
        originals.push((path.clone(), bytes.clone()));
        std::fs::write(remote.path().join(path), &bytes[..bytes.len() / 2]).unwrap();
    }
    let queued = peer.execute("sync.run", json!({})).unwrap();
    let processed = peer
        .execute("sync.process", json!({"id":queued["job_id"]}))
        .unwrap();
    assert_eq!(processed["state"], "failed", "{processed}");
    assert_eq!(processed["error"]["code"], "storage_corrupt");
    assert_eq!(peer_items(&peer), 0, "损坏对象不得产生任何部分应用");

    // 修复字节后同一日志继续同步，无需重新发布。
    for (path, bytes) in &originals {
        std::fs::write(remote.path().join(path), bytes).unwrap();
    }
    let repaired = peer.sync_batch().unwrap();
    assert_eq!(
        repaired["received"].as_u64().unwrap(),
        originals.len() as u64,
        "修复后全部原始操作应完整应用：{repaired}"
    );
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":member,"id":garment["id"]})
        )
        .unwrap()["name"],
        "中断切片外套"
    );
}

#[test]
#[ignore = "requires actual OS credential store"]
fn asset_discovery_converges_fairly_across_members_and_survives_late_damage() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _c1 = Cleanup(a.path().join("db"));
    let _c2 = Cleanup(b.path().join("db"));
    let members: Vec<String> = (0..2)
        .map(|i| {
            let m = owner
                .execute("member.create", json!({"name":format!("公平成员{i}")}))
                .unwrap();
            m["member_id"].as_str().unwrap().to_owned()
        })
        .collect();
    let mut assets = vec![];
    for (index, member) in members.iter().enumerate() {
        for item in 0..3 {
            let garment = owner
                .execute(
                    "clothing.create",
                    json!({"member_id":member,"name":format!("公平衣物{index}-{item}"),"category":"top"}),
                )
                .unwrap();
            let original = a.path().join(format!("fair-{index}-{item}.png"));
            unique_png(&original, 240, 100 + index as u32 * 10 + item as u32);
            let imported = owner
                .execute("assets.import", json!({"member_id":member,"path":original}))
                .unwrap();
            owner
                .execute(
                    "assets.adopt",
                    json!({"member_id":member,"clothing_id":garment["id"],"asset_id":imported["id"]}),
                )
                .unwrap();
            assets.push((member.clone(), imported["id"].as_str().unwrap().to_owned()));
        }
    }
    configure_both(&owner, &peer, remote.path());
    owner.sync_batch().unwrap();

    // 有界发现允许跨多次批次收敛，但两位成员的展示图都不得被饿死。
    let mut rounds = 0;
    loop {
        let step = peer.sync_batch().unwrap();
        if step["missing_assets"].as_u64().unwrap() == 0 {
            break;
        }
        rounds += 1;
        assert!(rounds <= 10, "展示图发现未在有限批内收敛：{step}");
    }
    for (member, id) in &assets {
        let got = peer
            .execute("assets.get", json!({"member_id":member,"id":id}))
            .unwrap();
        assert!(
            !got["display"]["local_path"]
                .as_str()
                .unwrap_or("")
                .is_empty(),
            "资源 {id} 未到达：{got}"
        );
        assert!(!got["thumbnail"]["local_path"].as_str().unwrap().is_empty());
    }
    let step = peer.sync_batch().unwrap();
    assert_eq!(step["missing_assets"], 0);
    assert_eq!(step["sent"], 0, "全部就绪后重复同步保持幂等");

    // 已下载展示图后，历史 blob 发布提交损坏不再阻塞后续同步。
    let mut files = vec![];
    collect_files(remote.path(), "", &mut files);
    let victim = select(&files, "blobs", ".commit")
        .into_iter()
        .next()
        .expect("应有 blob 提交标记")
        .0;
    let saved = std::fs::read(remote.path().join(&victim)).unwrap();
    std::fs::write(remote.path().join(&victim), &saved[..saved.len() / 2]).unwrap();
    let damaged = peer.sync_batch().unwrap();
    assert_eq!(damaged["missing_assets"], 0, "{damaged}");
    assert_eq!(damaged["sent"], 0);
    let still = peer
        .execute(
            "assets.get",
            json!({"member_id":members[0],"id":assets[0].1}),
        )
        .unwrap();
    assert!(
        !still["display"]["local_path"]
            .as_str()
            .unwrap_or("")
            .is_empty(),
        "已下载的展示图不受历史提交损坏影响"
    );
}
