//! Transport-only contract. Adapters never interpret encrypted objects.
pub use super::webdav::{WebDav, WebDavConfig};
use crate::ApiError;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct ObjectMeta {
    pub key: String,
    pub version: String,
    pub bytes: u64,
}
pub struct Page {
    pub items: Vec<ObjectMeta>,
    pub cursor: Option<String>,
}
#[derive(Clone, Debug)]
pub enum WriteCondition {
    Any,
    CreateOnly,
    MatchVersion(String),
}
pub struct StorageCapabilities {
    pub conditional_write: bool,
    pub atomic_publication: bool,
}
pub trait SyncStorage: Send {
    fn capabilities(&self) -> StorageCapabilities;
    fn list(&self, prefix: &str, cursor: Option<&str>, limit: usize) -> Result<Page, ApiError>;
    fn stat(&self, key: &str) -> Result<ObjectMeta, ApiError>;
    fn get(&self, key: &str) -> Result<Box<dyn Read>, ApiError>;
    fn put(
        &self,
        key: &str,
        data: &mut dyn Read,
        condition: WriteCondition,
    ) -> Result<ObjectMeta, ApiError>;
    fn delete(&self, key: &str, condition: WriteCondition) -> Result<(), ApiError>;
}
fn err(code: &str) -> ApiError {
    ApiError::new(code, "同步存储操作失败，保留本地数据后重试")
}
/// 适配器共享的对象键契约：跨后端（本地文件夹/S3）一致地拒绝驱动器名、
/// 绝对路径、非 Normal 分量与任何无法跨平台移植的分隔符，保证切换
/// adapter 不改变哪些键合法。
pub(super) fn valid_object_key(key: &str) -> bool {
    !key.is_empty()
        && !key.contains(':')
        && !key.chars().any(char::is_control)
        && Path::new(key)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        && key
            .split('/')
            .all(|p| !p.is_empty() && p != ".." && p != "." && !p.contains('\\'))
}
pub struct LocalFolder {
    root: PathBuf,
}
impl LocalFolder {
    pub fn new(root: &Path) -> Result<Self, ApiError> {
        let root = fs::canonicalize(root).map_err(|_| err("storage_not_found"))?;
        if !root.is_dir() {
            return Err(err("storage_not_found"));
        }
        Ok(Self { root })
    }
    fn lock(&self, key: &str) -> Result<fs::File, ApiError> {
        let p = self.path(key)?.with_extension("lock");
        let relative = p
            .strip_prefix(&self.root)
            .map_err(|_| err("invalid_object_key"))?
            .to_str()
            .ok_or_else(|| err("invalid_object_key"))?;
        let p = self.path(relative)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&p)
            .map_err(|_| err("storage_transient"))?;
        let metadata = fs::symlink_metadata(&p).map_err(|_| err("storage_transient"))?;
        if !metadata.is_file() {
            return Err(err("storage_corrupt"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let opened = file.metadata().map_err(|_| err("storage_transient"))?;
            if metadata.dev() != opened.dev() || metadata.ino() != opened.ino() {
                return Err(err("storage_corrupt"));
            }
        }
        file.lock().map_err(|_| err("storage_transient"))?;
        Ok(file)
    }
    fn path(&self, key: &str) -> Result<PathBuf, ApiError> {
        if !valid_object_key(key) {
            return Err(err("invalid_object_key"));
        }
        let mut p = self.root.clone();
        for part in key.split('/') {
            p.push(part);
            if !p.starts_with(&self.root) {
                return Err(err("invalid_object_key"));
            }
            if fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(err("storage_corrupt"));
            }
        }
        Ok(p)
    }
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
impl SyncStorage for LocalFolder {
    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities {
            conditional_write: true,
            atomic_publication: true,
        }
    }
    fn list(&self, prefix: &str, cursor: Option<&str>, limit: usize) -> Result<Page, ApiError> {
        if !(1..=1000).contains(&limit)
            || cursor
                .is_some_and(|c| !c.starts_with(&format!("{prefix}/")) || self.path(c).is_err())
        {
            return Err(err("invalid_cursor"));
        }
        let path = self.path(prefix)?;
        let mut keys = std::collections::BinaryHeap::new();
        if path.exists() {
            for e in fs::read_dir(path).map_err(|_| err("storage_transient"))? {
                let e = e.map_err(|_| err("storage_transient"))?;
                if e.file_name().to_string_lossy().ends_with(".lock")
                    || e.file_name().to_string_lossy().contains(".tmp-")
                {
                    continue;
                }
                if e.file_type()
                    .map_err(|_| err("storage_transient"))?
                    .is_file()
                {
                    let key = format!("{prefix}/{}", e.file_name().to_string_lossy());
                    if cursor.is_none_or(|c| key.as_str() > c) {
                        keys.push(key);
                        if keys.len() > limit + 1 {
                            keys.pop();
                        }
                    }
                }
            }
        }
        let mut keys = keys.into_sorted_vec();
        let more = keys.len() > limit;
        keys.truncate(limit);
        let cursor = if more { keys.last().cloned() } else { None };
        Ok(Page {
            items: keys
                .iter()
                .map(|k| self.stat(k))
                .collect::<Result<_, _>>()?,
            cursor,
        })
    }
    fn stat(&self, key: &str) -> Result<ObjectMeta, ApiError> {
        let mut file = self.get(key)?;
        let mut bytes = vec![];
        file.by_ref()
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| err("storage_transient"))?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(err("storage_corrupt"));
        }
        Ok(ObjectMeta {
            key: key.into(),
            version: digest(&bytes),
            bytes: bytes.len() as u64,
        })
    }
    fn get(&self, key: &str) -> Result<Box<dyn Read>, ApiError> {
        let p = self.path(key)?;
        let meta = fs::symlink_metadata(&p).map_err(|_| err("storage_not_found"))?;
        if !meta.is_file() {
            return Err(err("storage_corrupt"));
        }
        let f = fs::File::open(&p).map_err(|_| err("storage_transient"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let opened = f.metadata().map_err(|_| err("storage_transient"))?;
            if meta.dev() != opened.dev() || meta.ino() != opened.ino() {
                return Err(err("storage_corrupt"));
            }
        }
        Ok(Box::new(f))
    }
    fn put(
        &self,
        key: &str,
        data: &mut dyn Read,
        condition: WriteCondition,
    ) -> Result<ObjectMeta, ApiError> {
        let p = self.path(key)?;
        fs::create_dir_all(p.parent().ok_or_else(|| err("invalid_object_key"))?)
            .map_err(|_| err("storage_transient"))?;
        let _guard = self.lock(key)?;
        let result = (|| {
            match condition {
                WriteCondition::CreateOnly if p.exists() => {
                    return Err(err("storage_precondition"))
                }
                WriteCondition::MatchVersion(ref expected)
                    if self.stat(key)?.version != *expected =>
                {
                    return Err(err("storage_precondition"))
                }
                _ => {}
            }
            let staging = p.with_extension(format!("tmp-{}", uuid::Uuid::now_v7()));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging)
                .map_err(|_| err("storage_transient"))?;
            let written = (|| {
                let bytes = std::io::copy(&mut data.take(2 * 1024 * 1024 + 1), &mut file)
                    .map_err(|_| err("storage_transient"))?;
                if bytes > 2 * 1024 * 1024 {
                    return Err(err("storage_corrupt"));
                }
                file.flush().map_err(|_| err("storage_transient"))?;
                file.sync_all().map_err(|_| err("storage_transient"))?;
                fs::rename(&staging, &p).map_err(|_| err("storage_transient"))?;
                #[cfg(unix)]
                fs::File::open(p.parent().ok_or_else(|| err("invalid_object_key"))?)
                    .and_then(|dir| dir.sync_all())
                    .map_err(|_| err("storage_transient"))?;
                self.stat(key)
            })();
            if written.is_err() {
                let _ = fs::remove_file(staging);
            }
            written
        })();
        drop(_guard);
        // Keep the lock inode stable; OS releases advisory locks on process exit.
        // Unlinking permits two concurrent writers to lock different inodes.
        result
    }
    fn delete(&self, key: &str, condition: WriteCondition) -> Result<(), ApiError> {
        let _guard = self.lock(key)?;
        if matches!(condition, WriteCondition::CreateOnly) {
            return Err(err("storage_unsupported"));
        }
        if let WriteCondition::MatchVersion(v) = condition {
            if self.stat(key)?.version != v {
                return Err(err("storage_precondition"));
            }
        }
        let path = self.path(key)?;
        fs::remove_file(&path).map_err(|_| err("storage_not_found"))?;
        #[cfg(unix)]
        fs::File::open(path.parent().ok_or_else(|| err("invalid_object_key"))?)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| err("storage_transient"))?;
        Ok(())
    }
}
