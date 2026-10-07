//! #28 同步冲突、删除与事件一致性：验收切片。
//!
//! 全部测试走公开 `Application` API（真实 SQLite + 真实 OS 钥匙串），两个真实
//! 安装实例共享一个本地文件夹目标；仅用文件改名/复制模拟乱序与重复投递，
//! 不触碰内部表状态。覆盖：迟到更新不复活删除、删改冲突副本与显式恢复、
//! 并发字段与集合合并不丢添加、搭配结构冲突可见、乱序/重复事件正确计数、
//! 离线重连收敛、画像一致与同步不再回声。

use serde_json::{Value, json};
use wardrobe_core::Application;

#[path = "support/learning.rs"]
mod learning_support;
use learning_support::LearningTestSeam;

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
            json!({"password":"conflict space password 唯一短语","device_name":"conflict owner"}),
        )
        .unwrap();
    let offer = owner
        .execute(
            "encryption.pair.offer",
            json!({"device_name":"conflict peer"}),
        )
        .unwrap();
    let receipt = peer
        .execute(
            "encryption.pair.accept",
            json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"conflict space password 唯一短语"}),
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

/// 从同步日志（只读）取某实体最近一次事实的 op_id；仅用于定位传输对象。
fn last_op_id(db: &std::path::Path, table: &str, entity: &str) -> String {
    let read =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    read.query_row(
        "SELECT op_id FROM sync_journal WHERE entity_type=?1 AND entity_id=?2 ORDER BY id DESC LIMIT 1",
        rusqlite::params![table, entity],
        |r| r.get(0),
    )
    .unwrap()
}

fn walk(path: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// 目标上某个 op 的 `.commit` 标记路径。
fn marker(remote: &std::path::Path, op_id: &str) -> std::path::PathBuf {
    let mut files = vec![];
    walk(remote, &mut files);
    files
        .into_iter()
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy() == format!("{op_id}.commit"))
        })
        .unwrap_or_else(|| panic!("marker {op_id}.commit not published yet"))
}

/// 乱序模拟：把 op 的 commit 标记移出目标（对象保留）。
fn park(remote: &std::path::Path, sidecar: &std::path::Path, op_id: &str) {
    std::fs::rename(
        marker(remote, op_id),
        sidecar.join(format!("{op_id}.parked")),
    )
    .unwrap();
}

fn unpark(remote: &std::path::Path, sidecar: &std::path::Path, op_id: &str) {
    std::fs::rename(
        sidecar.join(format!("{op_id}.parked")),
        marker_path(remote, op_id),
    )
    .unwrap();
}

fn marker_path(remote: &std::path::Path, op_id: &str) -> std::path::PathBuf {
    let mut files = vec![];
    walk(remote, &mut files);
    let changes = files
        .into_iter()
        .find(|p| {
            p.extension().is_some_and(|e| e == "commit")
                && p.parent().is_some_and(|d| {
                    d.file_name()
                        .is_some_and(|n| n.to_string_lossy() == "changes")
                })
        })
        .expect("changes 目录应已有 commit 标记")
        .parent()
        .unwrap()
        .to_path_buf();
    changes.join(format!("{op_id}.commit"))
}

fn conflicts_of(app: &Application, member: &str) -> Value {
    app.execute("sync.conflicts", json!({"member_id":member}))
        .unwrap()
}

fn find_conflict<'a>(items: &'a [Value], entity: &str, field: &str) -> Option<&'a Value> {
    items
        .iter()
        .find(|v| v["entity_id"] == json!(entity) && v["field"] == json!(field))
}

// ---------------------------------------------------------------------------
// #28 验收 2：删除不被迟到更新复活，删改留冲突副本，恢复显式。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn late_edit_arriving_after_delete_keeps_conflict_copy_until_explicit_restore() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_both(&owner, &peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"冲突家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let item = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"迟到编辑衬衫","category":"top"}),
        )
        .unwrap();
    let id = item["id"].as_str().unwrap().to_owned();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    assert_eq!(
        peer.execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap()["name"],
        "迟到编辑衬衫"
    );

    // 对端离线改名并发布；把该 op 的标记移出目标，制造“迟到投递”。
    peer.execute(
        "clothing.update",
        json!({"member_id":member,"id":id,"name":"离线改名不被复活"}),
    )
    .unwrap();
    peer.sync_batch().unwrap();
    let edit_op = last_op_id(&b.path().join("db"), "clothing", &id);
    let parked = a.path().join("parked-late-edit");
    std::fs::create_dir_all(&parked).unwrap();
    park(remote.path(), &parked, &edit_op);

    // 源端删除并完整送达对端：对端本地编辑已存在，删除仍然生效。
    owner
        .execute("clothing.delete", json!({"member_id":member,"id":id}))
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    for app in [&owner, &peer] {
        assert_eq!(
            app.execute("clothing.get", json!({"member_id":member,"id":id}))
                .unwrap()["status"],
            "deleted",
            "删除先落地后，任何一端都不得显示复活"
        );
    }
    // 对端的删改冲突副本在删除送达时即可见。
    let peer_conflicts = conflicts_of(&peer, &member);
    assert!(
        find_conflict(
            peer_conflicts["items"].as_array().unwrap(),
            &id,
            "lifecycle"
        )
        .is_some_and(|v| v["variants"].as_array().unwrap().len() >= 2)
    );

    // 迟到编辑此刻才到达源端：删除不被复活，编辑保留为冲突副本。
    unpark(remote.path(), &parked, &edit_op);
    owner.sync_batch().unwrap();
    assert_eq!(
        owner
            .execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap()["status"],
        "deleted",
        "迟到更新不得复活已删除事实"
    );
    let owner_conflicts = conflicts_of(&owner, &member);
    let lifecycle = find_conflict(
        owner_conflicts["items"].as_array().unwrap(),
        &id,
        "lifecycle",
    )
    .expect("迟到的并发编辑必须保留为可见冲突副本")
    .clone();
    assert!(
        lifecycle["variants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["value"] == json!("离线改名不被复活"))
    );

    // 恢复是显式动作：显式选择后两端收敛为“恢复 + 保留离线编辑”。
    owner.execute("sync.resolve",json!({"member_id":member,"entity_type":"clothing","entity_id":id,"field":"lifecycle","value":"active","expected_token":lifecycle["token"],"operation_key":"ticket28 explicit restore"}))
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    for app in [&owner, &peer] {
        let doc = app
            .execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap();
        assert_eq!(doc["status"], "active");
        assert_eq!(doc["name"], "离线改名不被复活");
        assert!(
            find_conflict(
                conflicts_of(app, &member)["items"].as_array().unwrap(),
                &id,
                "lifecycle"
            )
            .is_none()
        );
    }
    // 收敛后重复同步不再产生同逻辑日志（不回声）。
    for app in [&owner, &peer] {
        let again = app.sync_batch().unwrap();
        assert_eq!(again["sent"], 0, "{again}");
        assert_eq!(again["received"], 0, "{again}");
        assert_eq!(again["pending"], 0, "{again}");
    }
}

// ---------------------------------------------------------------------------
// #28 验收 1：不同字段合并；同字段不同值保留双方并可解决；集合并发增删不丢。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn concurrent_field_and_set_edits_merge_without_losing_either_addition() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_both(&owner, &peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"集合家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let item = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"并发合集外套","category":"outerwear","tags":["基础"]}),
        )
        .unwrap();
    let id = item["id"].as_str().unwrap().to_owned();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();

    // 并发：owner 把“基础”换成“羊毛”；peer 同时新增“通勤”。
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":id,"tags":["羊毛"]}),
        )
        .unwrap();
    peer.execute(
        "clothing.update",
        json!({"member_id":member,"id":id,"tags":["基础","通勤"]}),
    )
    .unwrap();
    for app in [&owner, &peer, &owner, &peer] {
        app.sync_batch().unwrap();
    }
    for app in [&owner, &peer] {
        let tags = app
            .execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap()["tags"]
            .clone();
        assert_eq!(
            tags,
            json!(["羊毛", "通勤"]),
            "并发添加互不丢失，独立删除按操作语义保留：{tags}"
        );
    }

    // 并发给集合添加同一个标签：两边各发布一次，结果只保留一个元素。
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":id,"tags":["羊毛","通勤","红色"]}),
        )
        .unwrap();
    peer.execute(
        "clothing.update",
        json!({"member_id":member,"id":id,"tags":["基础","通勤","红色"]}),
    )
    .unwrap();
    for app in [&owner, &peer, &owner, &peer] {
        app.sync_batch().unwrap();
    }
    for app in [&owner, &peer] {
        let tags = app
            .execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap()["tags"]
            .clone();
        let reds = tags
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| **v == json!("红色"))
            .count();
        assert_eq!(reds, 1, "并发重复添加在集合中只计一次：{tags}");
        assert!(tags.as_array().unwrap().contains(&json!("红色")));
    }

    // 不同字段自动合并；同字段（名称）不同值保留双方并可显式解决。
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":id,"name":"甲方名称","color":["blue"]}),
        )
        .unwrap();
    peer.execute(
        "clothing.update",
        json!({"member_id":member,"id":id,"name":"乙方名称","size_label":"XL"}),
    )
    .unwrap();
    for app in [&owner, &peer, &owner, &peer] {
        app.sync_batch().unwrap();
    }
    for app in [&owner, &peer] {
        let doc = app
            .execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap();
        assert_eq!(doc["color"], json!(["blue"]), "不同字段自动合并");
        assert_eq!(doc["size_label"], "XL", "不同字段自动合并");
    }
    let owner_conflicts = conflicts_of(&owner, &member);
    let conflict = find_conflict(owner_conflicts["items"].as_array().unwrap(), &id, "name")
        .expect("同字段不同值必须保留双方并进入待审")
        .clone();
    let mut names: Vec<&str> = conflict["variants"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v["value"].as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["乙方名称", "甲方名称"],
        "双方候选都必须可见（按稳定顺序陈列）；"
    );
    owner
        .execute("sync.resolve", json!({"member_id":member,"entity_type":"clothing","entity_id":id,"field":"name","value":"甲方名称","expected_token":conflict["token"],"operation_key":"ticket28 choose name"}))
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    for app in [&owner, &peer] {
        let doc = app
            .execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap();
        assert_eq!(doc["name"], "甲方名称");
        assert!(
            find_conflict(
                conflicts_of(app, &member)["items"].as_array().unwrap(),
                &id,
                "name"
            )
            .is_none()
        );
    }
    for app in [&owner, &peer] {
        let again = app.sync_batch().unwrap();
        assert_eq!(again["sent"], 0, "{again}");
        assert_eq!(again["received"], 0, "{again}");
    }
}

// ---------------------------------------------------------------------------
// #28 验收 1：搭配（outfit）结构冲突可见，显式更新后收敛且历史分支保留。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn concurrent_outfit_structure_edits_stay_visible_until_explicit_update() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_both(&owner, &peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"搭配家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let top = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"白衬衫","category":"top"}),
        )
        .unwrap();
    let bottom = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"牛仔裤","category":"bottom"}),
        )
        .unwrap();
    let outer = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"风衣","category":"outerwear"}),
        )
        .unwrap();
    let look = owner
        .execute(
            "outfit.create",
            json!({"member_id":member,"name":"出差套装","items":[
                {"clothing_id":top["id"],"role":"top","layer":0},
                {"clothing_id":bottom["id"],"role":"bottom","layer":1}
            ]}),
        )
        .unwrap();
    let look_id = look["id"].as_str().unwrap().to_owned();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();

    // 并发结构编辑：owner 换上衣，peer 调整层次结构。
    owner
        .execute(
            "outfit.update",
            json!({"member_id":member,"id":look_id,"expected_version":1,"name":"出差套装","items":[
                {"clothing_id":outer["id"],"role":"outerwear","layer":0},
                {"clothing_id":bottom["id"],"role":"bottom","layer":1}
            ]}),
        )
        .unwrap();
    peer.execute(
        "outfit.update",
        json!({"member_id":member,"id":look_id,"expected_version":1,"name":"出差套装","items":[
            {"clothing_id":top["id"],"role":"top","layer":0},
            {"clothing_id":bottom["id"],"role":"bottom","layer":1},
            {"clothing_id":outer["id"],"role":"outerwear","layer":2}
        ]}),
    )
    .unwrap();
    for app in [&owner, &peer, &owner, &peer] {
        app.sync_batch().unwrap();
    }
    for app in [&owner, &peer] {
        let items = conflicts_of(app, &member)["items"]
            .as_array()
            .unwrap()
            .to_vec();
        let structure = find_conflict(&items, &look_id, "items")
            .expect("搭配结构冲突必须在同步冲突列表可见")
            .clone();
        assert_eq!(structure["entity_type"], "outfits");
        assert_eq!(structure["variants"].as_array().unwrap().len(), 2);
    }
    // 未决期显示一致的暂定结构（两端一致，不脑裂）。
    let owner_items = owner
        .execute("outfit.get", json!({"member_id":member,"id":look_id}))
        .unwrap()["items"]
        .clone();
    let peer_items = peer
        .execute("outfit.get", json!({"member_id":member,"id":look_id}))
        .unwrap()["items"]
        .clone();
    assert_eq!(owner_items, peer_items, "未解决时两端暂定结构一致");

    // 人工处理：显式发布新版本后冲突清除，且并发版本历史都保留可见。
    let current = owner
        .execute("outfit.get", json!({"member_id":member,"id":look_id}))
        .unwrap();
    let version = current["version"].as_u64().unwrap();
    owner
        .execute("outfit.update", json!({"member_id":member,"id":look_id,"expected_version":version,"name":"出差套装","items":[
            {"clothing_id":top["id"],"role":"top","layer":0},
            {"clothing_id":bottom["id"],"role":"bottom","layer":1}
        ]}))
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    for app in [&owner, &peer] {
        assert!(
            find_conflict(
                conflicts_of(app, &member)["items"].as_array().unwrap(),
                &look_id,
                "items"
            )
            .is_none(),
            "显式更新解决结构冲突"
        );
        let doc = app
            .execute("outfit.get", json!({"member_id":member,"id":look_id}))
            .unwrap();
        assert_eq!(doc["items"].as_array().unwrap().len(), 2);
        let versions = app
            .execute("sync.history", json!({"member_id":member,"entity_type":"outfit_versions","entity_id":format!("{look_id}:2")}))
            .unwrap();
        assert_eq!(
            versions["items"].as_array().unwrap().len(),
            2,
            "并发结构版本都保留可查"
        );
    }
    for app in [&owner, &peer] {
        let again = app.sync_batch().unwrap();
        assert_eq!(again["sent"], 0, "{again}");
        assert_eq!(again["received"], 0, "{again}");
    }
}

// ---------------------------------------------------------------------------
// #28 验收 2/3：到达顺序与时间序号不裁决事实；解决只由显式选择或因果支配产生。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn arrival_order_and_op_ordering_never_decide_facts_resolution_does() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_both(&owner, &peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"乱序家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let item = owner
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"乱序衬衫","category":"top"}),
        )
        .unwrap();
    let id = item["id"].as_str().unwrap().to_owned();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();

    // 两端各自离线编辑同一字段；owner 的 op 时间序在前，却让对端的先到达。
    owner
        .execute(
            "clothing.update",
            json!({"member_id":member,"id":id,"name":"甲编辑"}),
        )
        .unwrap();
    owner.sync_batch().unwrap();
    let owner_edit_op = last_op_id(&a.path().join("db"), "clothing", &id);
    let parked = a.path().join("parked-owner-edit");
    std::fs::create_dir_all(&parked).unwrap();
    park(remote.path(), &parked, &owner_edit_op);
    peer.execute(
        "clothing.update",
        json!({"member_id":member,"id":id,"name":"乙编辑"}),
    )
    .unwrap();
    peer.sync_batch().unwrap();

    // 先到者（乙）先落地：与本地未发布的甲并发，双方都保留。
    owner.sync_batch().unwrap();
    {
        let doc = owner
            .execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap();
        let provisional = doc["name"].as_str().unwrap();
        assert!(
            provisional == "甲编辑" || provisional == "乙编辑",
            "未解决时只能显示保留候选之一，不得发明第三种结果：{doc}"
        );
        let conflict = find_conflict(
            conflicts_of(&owner, &member)["items"].as_array().unwrap(),
            &id,
            "name",
        )
        .expect("并发编辑必须保留双方")
        .clone();
        let mut names: Vec<String> = conflict["variants"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["value"].as_str().map(str::to_owned))
            .collect();
        names.sort_unstable();
        assert_eq!(names, vec!["乙编辑", "甲编辑"]);
    }
    // 后到者（甲）此刻才到达：依旧保留双方，不因后到覆盖既有候选。
    unpark(remote.path(), &parked, &owner_edit_op);
    peer.sync_batch().unwrap();
    {
        let conflict = find_conflict(
            conflicts_of(&peer, &member)["items"].as_array().unwrap(),
            &id,
            "name",
        )
        .expect("对端同样保留双方")
        .clone();
        let names: Vec<String> = conflict["variants"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["value"].as_str().map(str::to_owned))
            .collect();
        assert!(names.contains(&"甲编辑".to_string()) && names.contains(&"乙编辑".to_string()));
        let doc = peer
            .execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap();
        assert_eq!(
            doc["name"],
            owner
                .execute("clothing.get", json!({"member_id":member,"id":id}))
                .unwrap()["name"],
            "两端暂定结果一致，不脑裂"
        );
        assert!(
            doc["name"] == json!("甲编辑") || doc["name"] == json!("乙编辑"),
            "暂定结果必须是保留候选之一：{doc}"
        );
    }
    // 显式选择解决冲突并向对端收敛。
    let conflict = find_conflict(
        conflicts_of(&owner, &member)["items"].as_array().unwrap(),
        &id,
        "name",
    )
    .unwrap()
    .clone();
    owner
        .execute("sync.resolve", json!({"member_id":member,"entity_type":"clothing","entity_id":id,"field":"name","value":"甲编辑","expected_token":conflict["token"],"operation_key":"ticket28 pick after reorder"}))
        .unwrap();
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    for app in [&owner, &peer] {
        assert_eq!(
            app.execute("clothing.get", json!({"member_id":member,"id":id}))
                .unwrap()["name"],
            "甲编辑"
        );
        assert!(
            find_conflict(
                conflicts_of(app, &member)["items"].as_array().unwrap(),
                &id,
                "name"
            )
            .is_none()
        );
    }
    // 看到对方选择后的新编辑按因果支配直接收敛，无需再次人工裁决。
    peer.execute(
        "clothing.update",
        json!({"member_id":member,"id":id,"name":"乙后继编辑"}),
    )
    .unwrap();
    for app in [&peer, &owner, &peer, &owner] {
        app.sync_batch().unwrap();
    }
    for app in [&owner, &peer] {
        let doc = app
            .execute("clothing.get", json!({"member_id":member,"id":id}))
            .unwrap();
        assert_eq!(doc["name"], "乙后继编辑", "因果更新直接收敛：{doc}");
        assert!(
            conflicts_of(app, &member)["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v["entity_id"] != json!(id))
        );
    }
    for app in [&owner, &peer] {
        let again = app.sync_batch().unwrap();
        assert_eq!(again["sent"], 0, "{again}");
        assert_eq!(again["received"], 0, "{again}");
    }
}

// ---------------------------------------------------------------------------
// #28 验收 3：真实两空间 API 场景——离线重连、乱序、重复投递、事件计数、
// 画像收敛，且收敛后不再生成同逻辑 outbox（同步不回声）。
// ---------------------------------------------------------------------------
#[test]
#[ignore = "requires actual OS credential store"]
fn offline_reconnect_converges_events_and_learning_without_echo_outbox() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let (owner, peer) = pair(&a.path().join("db"), &b.path().join("db"));
    let _x = Cleanup(a.path().join("db"));
    let _y = Cleanup(b.path().join("db"));
    configure_both(&owner, &peer, remote.path());
    let member = owner
        .execute("member.create", json!({"name":"离线家人"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut ids = Vec::new();
    for (name, category) in [("上衣", "top"), ("长裤", "bottom"), ("鞋", "shoes")] {
        ids.push(
            owner
                .execute(
                    "clothing.create",
                    json!({"member_id":member,"name":name,"category":category}),
                )
                .unwrap()["id"]
                .clone(),
        );
    }
    owner.sync_batch().unwrap();
    peer.sync_batch().unwrap();
    let shirt = ids[0].as_str().unwrap().to_owned();

    // 两台设备各自离线产生穿着、洗护与反馈事实。
    let owner_intent = owner
        .execute("recommendation.generate", json!({"member_id":member,"operation_key":"ticket28 owner intent","timezone":"Asia/Shanghai"}))
        .unwrap();
    owner
        .execute("feedback.record", json!({"member_id":member,"operation_key":"ticket28 owner like","intent_id":owner_intent["id"],"proposal_index":0,"reaction":"like"}))
        .unwrap();
    let owner_wear = owner
        .execute("wear.record", json!({"member_id":member,"clothing_ids":[shirt],"local_date":"2026-10-04","timezone":"Asia/Shanghai","operation_key":"ticket28 owner wear"}))
        .unwrap();
    owner
        .execute("care.record", json!({"member_id":member,"clothing_id":shirt,"action":"wash","local_date":"2026-10-04","timezone":"Asia/Shanghai","operation_key":"ticket28 owner wash"}))
        .unwrap();
    owner.sync_batch().unwrap();

    let peer_intent = peer
        .execute("recommendation.generate", json!({"member_id":member,"operation_key":"ticket28 peer intent","timezone":"Asia/Shanghai"}))
        .unwrap();
    peer
        .execute("feedback.record", json!({"member_id":member,"operation_key":"ticket28 peer dislike","intent_id":peer_intent["id"],"proposal_index":0,"reaction":"dislike"}))
        .unwrap();
    peer
        .execute("wear.record", json!({"member_id":member,"clothing_ids":[shirt],"local_date":"2026-10-05","timezone":"Asia/Shanghai","operation_key":"ticket28 peer wear"}))
        .unwrap();
    peer
        .execute("care.record", json!({"member_id":member,"clothing_id":shirt,"action":"wash","local_date":"2026-10-05","timezone":"Asia/Shanghai","operation_key":"ticket28 peer wash"}))
        .unwrap();

    // 离线重连 + 乱序：在对端重连前把 owner 穿着事件的标记移走，同设备的洗护
    // 因因果依赖挂起；反馈事件额外复制一份标记制造重复投递。
    let wear_op = last_op_id(
        &a.path().join("db"),
        "wear_events",
        owner_wear["id"].as_str().unwrap(),
    );
    let parked = a.path().join("parked-owner-wear");
    std::fs::create_dir_all(&parked).unwrap();
    park(remote.path(), &parked, &wear_op);
    {
        let read = rusqlite::Connection::open_with_flags(
            a.path().join("db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let op: String = read
            .query_row(
                "SELECT op_id FROM sync_journal WHERE entity_type='feedback_events' ORDER BY id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        drop(read);
        let source = marker(remote.path(), &op);
        let duplicate = source
            .parent()
            .unwrap()
            .join(format!("{op}-duplicate.commit"));
        std::fs::copy(&source, &duplicate).unwrap();
    }
    let reconnect = peer.sync_batch().unwrap();
    assert!(
        reconnect["pending"].as_u64().unwrap() > 0,
        "因果前序缺失时挂起等待而不是静默丢弃：{reconnect}"
    );
    unpark(remote.path(), &parked, &wear_op);
    let healed = peer.sync_batch().unwrap();
    assert_eq!(healed["pending"], 0, "前序到达后挂起事件补齐：{healed}");
    assert!(
        healed["received"].as_u64().unwrap() >= 1,
        "重复投递按已见 op 幂等处理：{healed}"
    );
    // owner 侧重连收敛对端离线事实。
    owner.sync_batch().unwrap();
    for app in [&owner, &peer] {
        let step = app.sync_batch().unwrap();
        assert_eq!(step["sent"], 0, "{step}");
        assert_eq!(step["received"], 0, "{step}");
    }

    // 事件计数既有口径：次数按有效事件、天数按当地日期去重；两端一致。
    for app in [&owner, &peer] {
        let summary = app
            .execute("wear.summary", json!({"member_id":member}))
            .unwrap();
        let entry = summary["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["clothing_id"] == json!(shirt))
            .expect("穿着计数包含两台设备的事件");
        assert_eq!(entry["wearCount"], 2, "乱序与重复投递不改变计数：{summary}");
        assert_eq!(entry["wearDays"], 2, "不同当地日期分别去重计数：{summary}");
        let history = app
            .execute("care.history", json!({"member_id":member,"root_event_id":app.execute("care.list", json!({"member_id":member})).unwrap()["items"][0]["root_event_id"]}))
            .unwrap();
        assert!(!history["items"].as_array().unwrap().is_empty());
    }

    // 同一当地日期的再次穿着只增次数不增天数：天数按当地日期去重。
    owner
        .execute("wear.record", json!({"member_id":member,"clothing_ids":[shirt],"local_date":"2026-10-05","timezone":"Asia/Shanghai","operation_key":"ticket28 owner wear same day"}))
        .unwrap();
    for app in [&peer, &owner, &peer, &owner] {
        app.sync_batch().unwrap();
    }
    for app in [&owner, &peer] {
        let summary = app
            .execute("wear.summary", json!({"member_id":member}))
            .unwrap();
        let entry = summary["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["clothing_id"] == json!(shirt))
            .unwrap();
        assert_eq!(entry["wearCount"], 3, "次数按有效事件累计：{summary}");
        assert_eq!(entry["wearDays"], 2, "同一当地日期去重：{summary}");
    }

    // 画像在收敛后重算一致：两空间从同一事件集推出同一画像内容。
    let owner_profile = owner
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":member,"operation_key":"ticket28 owner profile"}),
        )
        .unwrap();
    let peer_profile = peer
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":member,"operation_key":"ticket28 peer profile"}),
        )
        .unwrap();
    assert_eq!(
        owner_profile["inferred"], peer_profile["inferred"],
        "画像特征权重一致"
    );
    assert_eq!(
        owner_profile["pair_projection"], peer_profile["pair_projection"],
        "搭配投影一致"
    );
    assert_eq!(
        owner_profile["algorithm_version"],
        peer_profile["algorithm_version"]
    );

    // 收敛后不再生成同逻辑 outbox：重复同步零发送、零接收，画像重算也不引发回声。
    for app in [&owner, &peer] {
        for _ in 0..2 {
            let step = app.sync_batch().unwrap();
            assert_eq!(step["sent"], 0, "{step}");
            assert_eq!(step["received"], 0, "{step}");
            assert_eq!(step["pending"], 0, "{step}");
            assert_eq!(step["has_more"], false, "{step}");
        }
    }
    owner
        .execute_and_wait(
            "learning.recompute",
            json!({"member_id":member,"operation_key":"ticket28 owner profile again"}),
        )
        .unwrap();
    let after_learning = owner.sync_batch().unwrap();
    assert_eq!(after_learning["sent"], 0, "{after_learning}");
    assert_eq!(after_learning["received"], 0, "{after_learning}");
}
