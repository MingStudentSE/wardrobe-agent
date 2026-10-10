use std::io::{self, Read};
use wardrobe_core::sync::storage::{LocalFolder, SyncStorage, WriteCondition};
struct Broken(bool);
impl Read for Broken {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.0 {
            return Err(io::Error::other("interrupted upload"));
        }
        self.0 = true;
        out[..3].copy_from_slice(b"abc");
        Ok(3)
    }
}
#[test]
fn interrupted_local_upload_has_no_object_and_abandoned_lock_does_not_block_retry() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalFolder::new(dir.path()).unwrap();
    assert!(storage
        .put(
            "changes/one",
            &mut Broken(false),
            WriteCondition::CreateOnly
        )
        .is_err());
    assert!(storage.stat("changes/one").is_err());
    std::fs::write(
        dir.path().join("changes/one.lock"),
        b"abandoned previous process",
    )
    .unwrap();
    let meta = storage
        .put(
            "changes/one",
            &mut b"retry succeeds".as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    assert_eq!(meta.bytes, 14);
    let mut bytes = vec![];
    storage
        .get("changes/one")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"retry succeeds");
}
#[test]
fn storage_pages_and_conditional_writes_and_deletes_preserve_versions() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalFolder::new(dir.path()).unwrap();
    assert!(storage.capabilities().conditional_write && storage.capabilities().atomic_publication);
    for i in (0..140).rev() {
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
        let page = storage.list("changes", cursor.as_deref(), 17).unwrap();
        assert!(page.items.len() <= 17);
        all.extend(page.items.into_iter().map(|v| v.key));
        cursor = page.cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(all.len(), 140);
    assert!(all.windows(2).all(|v| v[0] < v[1]));
    let old = storage.stat(&all[0]).unwrap();
    assert_eq!(
        storage
            .put(
                &all[0],
                &mut b"duplicate".as_slice(),
                WriteCondition::CreateOnly
            )
            .unwrap_err()
            .code,
        "storage_precondition"
    );
    let new = storage
        .put(
            &all[0],
            &mut b"new".as_slice(),
            WriteCondition::MatchVersion(old.version.clone()),
        )
        .unwrap();
    assert_ne!(old.version, new.version);
    assert_eq!(
        storage
            .delete(&all[0], WriteCondition::MatchVersion(old.version))
            .unwrap_err()
            .code,
        "storage_precondition"
    );
    storage
        .delete(&all[0], WriteCondition::MatchVersion(new.version))
        .unwrap();
    assert!(storage.stat(&all[0]).is_err());
    assert!(storage.list("changes", None, 0).is_err());
    assert!(storage
        .list("changes", Some("other/not-a-cursor"), 17)
        .is_err());
}
#[cfg(unix)]
#[test]
fn storage_rejects_symlinked_lock_and_bounded_upload_does_not_publish() {
    let dir = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let storage = LocalFolder::new(dir.path()).unwrap();
    std::fs::create_dir(dir.path().join("changes")).unwrap();
    let secret = external.path().join("private");
    std::fs::write(&secret, b"outside").unwrap();
    std::os::unix::fs::symlink(&secret, dir.path().join("changes/one.lock")).unwrap();
    assert_eq!(
        storage
            .put(
                "changes/one.object",
                &mut b"must not publish".as_slice(),
                WriteCondition::CreateOnly
            )
            .unwrap_err()
            .code,
        "storage_corrupt"
    );
    assert_eq!(std::fs::read(&secret).unwrap(), b"outside");
    let oversized = vec![42u8; 2 * 1024 * 1024 + 1];
    assert_eq!(
        storage
            .put(
                "changes/too-large.object",
                &mut oversized.as_slice(),
                WriteCondition::CreateOnly
            )
            .unwrap_err()
            .code,
        "storage_corrupt"
    );
    assert!(storage.stat("changes/too-large.object").is_err());
    assert!(storage.list("changes", None, 20).unwrap().items.is_empty());
}
#[test]
fn portable_object_keys_reject_drives_absolute_and_non_normal_components() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalFolder::new(dir.path()).unwrap();
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
                .put(
                    key,
                    &mut b"must remain bounded".as_slice(),
                    WriteCondition::CreateOnly
                )
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
        assert!(storage.delete(key, WriteCondition::Any).is_err());
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[test]
fn concurrent_conditional_delete_cannot_remove_a_newly_published_version() {
    use std::sync::mpsc;
    use std::time::Duration;
    struct Paused {
        entered: Option<mpsc::Sender<()>>,
        release: mpsc::Receiver<()>,
    }
    impl Read for Paused {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if let Some(entered) = self.entered.take() {
                entered.send(()).unwrap();
                self.release.recv().unwrap();
                out[..3].copy_from_slice(b"new");
                Ok(3)
            } else {
                Ok(0)
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalFolder::new(dir.path()).unwrap();
    let initial = storage
        .put(
            "changes/race.object",
            &mut b"old".as_slice(),
            WriteCondition::CreateOnly,
        )
        .unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let path = dir.path().to_owned();
    let version = initial.version.clone();
    let writer = std::thread::spawn(move || {
        let storage = LocalFolder::new(&path).unwrap();
        storage
            .put(
                "changes/race.object",
                &mut Paused {
                    entered: Some(entered_tx),
                    release: release_rx,
                },
                WriteCondition::MatchVersion(version),
            )
            .unwrap()
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let (delete_tx, delete_rx) = mpsc::channel();
    let path = dir.path().to_owned();
    let deleter = std::thread::spawn(move || {
        let storage = LocalFolder::new(&path).unwrap();
        delete_tx
            .send(storage.delete(
                "changes/race.object",
                WriteCondition::MatchVersion(initial.version),
            ))
            .unwrap();
    });
    // A conditional delete must wait for the in-flight atomic publisher's stable lock inode.
    let premature = delete_rx.recv_timeout(Duration::from_millis(100));
    release_tx.send(()).unwrap();
    writer.join().unwrap();
    deleter.join().unwrap();
    assert!(premature.is_err());
    assert_eq!(
        delete_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap_err()
            .code,
        "storage_precondition"
    );
    let mut bytes = vec![];
    storage
        .get("changes/race.object")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"new");
}
