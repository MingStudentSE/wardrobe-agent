use serde_json::json;
use wardrobe_core::Application;
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(a) = Application::open(&self.0) {
            let _ = a.execute("encryption.unpair", json!({}));
        }
    }
}
#[test]
#[ignore = "requires actual OS credential store"]
fn two_installations_exchange_member_and_clothing_incrementally_without_duplicate_facts() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let ap = a.path().join("wardrobe.sqlite");
    let bp = b.path().join("wardrobe.sqlite");
    let owner = Application::open(&ap).unwrap();
    let peer = Application::open(&bp).unwrap();
    let _a = Cleanup(ap);
    let _b = Cleanup(bp);
    let existing = owner
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"启用同步之前的外套","category":"outerwear"}),
        )
        .unwrap();
    owner
        .execute(
            "encryption.create",
            json!({"password":"temporary sync owner password","device_name":"sync owner"}),
        )
        .unwrap();
    let offer = owner
        .execute("encryption.pair.offer", json!({"device_name":"sync peer"}))
        .unwrap();
    let receipt=peer.execute("encryption.pair.accept",json!({"invitation":offer["invitation"],"pairing_secret":offer["pairing_secret"],"password":"temporary sync peer password"})).unwrap();
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
    let member = owner
        .execute("member.create", json!({"name":"同步家人"}))
        .unwrap();
    let garment = owner
        .execute(
            "clothing.create",
            json!({"member_id":member["member_id"],"name":"同步蓝衬衫","category":"top"}),
        )
        .unwrap();
    for app in [&owner, &peer] {
        app.execute(
            "sync.configure",
            json!({"provider_kind":"local_folder","directory":remote.path()}),
        )
        .unwrap();
    }
    owner.execute("sync.run", json!({})).unwrap();
    peer.execute("sync.run", json!({})).unwrap();
    assert_eq!(
        peer.execute(
            "clothing.get",
            json!({"member_id":existing["member_id"],"id":existing["id"]})
        )
        .unwrap()["name"],
        "启用同步之前的外套"
    );
    let got = peer
        .execute(
            "clothing.get",
            json!({"member_id":member["member_id"],"id":garment["id"]}),
        )
        .unwrap();
    assert_eq!(got["name"], "同步蓝衬衫");
    peer.execute("sync.run", json!({})).unwrap();
    assert_eq!(
        peer.execute("clothing.list", json!({"member_id":member["member_id"]}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    peer.execute(
        "clothing.update",
        json!({"member_id":member["member_id"],"id":garment["id"],"name":"第二设备可编辑"}),
    )
    .unwrap();
    peer.execute("sync.run", json!({})).unwrap();
    owner.execute("sync.run", json!({})).unwrap();
    assert_eq!(
        owner
            .execute(
                "clothing.get",
                json!({"member_id":member["member_id"],"id":garment["id"]})
            )
            .unwrap()["name"],
        "第二设备可编辑"
    );
}
