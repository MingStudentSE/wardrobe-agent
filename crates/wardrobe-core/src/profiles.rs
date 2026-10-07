//! 连接 profile：仅保存非秘密连接信息（数据目录），token 仍走 stdin，
//! 授权沿用 Core 随数据目录持久化。配置删除绝不触及衣橱数据。

use crate::ApiError;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const PROFILE_NAME_LIMIT: usize = 64;

fn invalid(s: &str) -> ApiError {
    ApiError::new("invalid_input", s)
}

pub fn validate_name(name: &str) -> Result<(), ApiError> {
    let ok = !name.is_empty()
        && name.chars().count() <= PROFILE_NAME_LIMIT
        && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(invalid(
            "profile名称须为1-64位字母数字，可含_或-，且以字母数字开头",
        ))
    }
}

pub fn profiles_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    let root = home.join("Library/Application Support/wardrobe-agent");
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Roaming"))
        .join("wardrobe-agent");
    #[cfg(all(unix, not(target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("wardrobe-agent");
    root.join("profiles.json")
}

thread_local! {
    static PATH_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// 测试注入隔离配置路径；生产代码不得调用。
pub fn with_config_path<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    PATH_OVERRIDE.with(|cell| *cell.borrow_mut() = Some(path.to_path_buf()));
    let result = f();
    PATH_OVERRIDE.with(|cell| *cell.borrow_mut() = None);
    result
}

fn current_path() -> PathBuf {
    if let Some(p) = std::env::var_os("WARDROBE_PROFILES_PATH") {
        return PathBuf::from(p);
    }
    PATH_OVERRIDE
        .with(|cell| cell.borrow().clone())
        .unwrap_or_else(profiles_path)
}

fn load(path: &Path) -> Result<Value, ApiError> {
    match std::fs::read(path) {
        Err(_) => Ok(json!({"schema_version":1,"profiles":{},"default":null})),
        Ok(bytes) => {
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| ApiError::new("storage_error", "profiles.json 损坏"))?;
            if value["schema_version"] != 1 || !value["profiles"].is_object() {
                return Err(ApiError::new("storage_error", "profiles.json 版本或结构无效"));
            }
            Ok(value)
        }
    }
}

fn save(path: &Path, doc: &Value) -> Result<(), ApiError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::new("storage_error", &format!("无法创建配置目录：{e}")))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, doc.to_string())
        .map_err(|e| ApiError::new("storage_error", &format!("无法写入配置：{e}")))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| ApiError::new("storage_error", &format!("无法原子更新配置：{e}")))?;
    Ok(())
}

pub fn upsert(name: &str, data_dir: &Path, force: bool) -> Result<Value, ApiError> {
    validate_name(name)?;
    if !data_dir.is_absolute() {
        return Err(invalid("profile数据目录须为绝对路径"));
    }
    if !data_dir.is_dir() {
        return Err(ApiError::new("profile_data_dir_missing", "profile数据目录须已存在"));
    }
    let path = current_path();
    let mut doc = load(&path)?;
    let canonical = data_dir
        .canonicalize()
        .map_err(|e| ApiError::new("storage_error", &format!("无法解析数据目录：{e}")))?;
    let profiles = doc["profiles"]
        .as_object_mut()
        .ok_or_else(|| ApiError::new("storage_error", "profiles.json 结构无效"))?;
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(existing) = profiles.get(name) {
        if existing["data_dir"] == json!(canonical.to_string_lossy()) {
            return Ok(json!({"schema_version":1,"name":name,"data_dir":canonical.to_string_lossy(),"updated_at":existing["updated_at"].clone(),"unchanged":true}));
        }
        if !force {
            return Err(ApiError::new("profile_exists", "同名profile已存在；覆盖请使用--force"));
        }
    }
    profiles.insert(
        name.to_string(),
        json!({"data_dir":canonical.to_string_lossy(),"created_at":now,"updated_at":now}),
    );
    if doc["default"].is_null() {
        doc["default"] = json!(name);
    }
    save(&path, &doc)?;
    Ok(json!({"schema_version":1,"name":name,"data_dir":canonical.to_string_lossy(),"updated_at":now}))
}

pub fn remove(name: &str) -> Result<Value, ApiError> {
    validate_name(name)?;
    let path = current_path();
    let mut doc = load(&path)?;
    let profiles = doc["profiles"]
        .as_object_mut()
        .ok_or_else(|| ApiError::new("storage_error", "profiles.json 结构无效"))?;
    if profiles.remove(name).is_none() {
        return Err(ApiError::new("profile_not_found", "profile不存在"));
    }
    if doc["default"] == json!(name) {
        doc["default"] = Value::Null;
    }
    save(&path, &doc)?;
    Ok(json!({"schema_version":1,"removed":name}))
}

pub fn set_default(name: &str) -> Result<Value, ApiError> {
    validate_name(name)?;
    let path = current_path();
    let mut doc = load(&path)?;
    if doc["profiles"][name].is_null() {
        return Err(ApiError::new("profile_not_found", "profile不存在"));
    }
    doc["default"] = json!(name);
    save(&path, &doc)?;
    Ok(json!({"schema_version":1,"default":name}))
}

pub fn list() -> Result<Value, ApiError> {
    let doc = load(&current_path())?;
    Ok(json!({
        "schema_version":1,
        "profiles":doc["profiles"].clone(),
        "default":doc["default"].clone(),
    }))
}

/// 连接解析：Some(名称) 时该 profile 必须存在；None 时若设置了默认
/// profile 则用之，否则返回 None 由调用方回落默认数据库路径。
pub fn resolve(profile: Option<&str>) -> Result<Option<PathBuf>, ApiError> {
    if let Some(name) = profile {
        validate_name(name)?;
        let doc = load(&current_path())?;
        let entry = &doc["profiles"][name];
        if entry.is_null() {
            return Err(ApiError::new("profile_not_found", "profile不存在"));
        }
        let dir = entry["data_dir"]
            .as_str()
            .ok_or_else(|| ApiError::new("storage_error", "profile数据目录损坏"))?;
        return Ok(Some(Path::new(dir).join("wardrobe.sqlite")));
    }
    let doc = load(&current_path())?;
    if let Some(name) = doc["default"].as_str() {
        return resolve(Some(name));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_crud_is_atomic_and_never_touches_data() {
        let dir = tempfile::tempdir().unwrap();
        let data_a = dir.path().join("data-a");
        let data_b = dir.path().join("data-b");
        std::fs::create_dir_all(&data_a).unwrap();
        std::fs::create_dir_all(&data_b).unwrap();
        std::fs::write(data_a.join("wardrobe.sqlite"), b"keep-a").unwrap();
        let config = dir.path().join("config").join("profiles.json");
        with_config_path(&config, || {
            assert_eq!(
                upsert("work", &data_a, false).unwrap()["name"],
                json!("work")
            );
            assert!(upsert("work", &data_b, false).is_err(), "重复添加须拒绝");
            upsert("work", &data_b, true).unwrap();
            assert_eq!(resolve(Some("work")).unwrap().unwrap(), data_b.join("wardrobe.sqlite"));
            assert_eq!(
                resolve(None).unwrap().unwrap(),
                data_b.join("wardrobe.sqlite"),
                "首个profile成为默认"
            );
            set_default("work").unwrap();
            remove("work").unwrap();
            assert_eq!(resolve(None).unwrap(), None, "删除默认后回落");
            assert!(data_b.join("wardrobe.sqlite").exists() || std::fs::read_dir(&data_b).is_ok(),
                "配置删除不触及数据目录");
            // 相对路径与非法名称
            assert!(upsert("rel", Path::new("relative/dir"), false).is_err());
            assert!(validate_name("1bad-").is_ok());
            assert!(validate_name("-bad").is_err());
            assert!(validate_name("含中文").is_err());
        });
    }

    #[test]
    fn corrupt_or_invalid_config_is_storage_error_not_silent_reset() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("profiles.json");
        std::fs::write(&config, b"{not json").unwrap();
        with_config_path(&config, || {
            let err = list().unwrap_err();
            assert_eq!(err.code, "storage_error");
            assert_eq!(resolve(Some("any")).unwrap_err().code, "storage_error");
            assert_eq!(resolve(None).unwrap_err().code, "storage_error");
        });
    }
}
