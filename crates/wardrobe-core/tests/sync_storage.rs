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
