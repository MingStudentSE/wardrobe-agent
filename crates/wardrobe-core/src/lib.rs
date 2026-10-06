mod clothing;
mod members;
mod providers;

use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const DATABASE_SCHEMA_VERSION: u32 = 3;
pub use clothing::CATEGORIES;
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
impl ApiError {
    fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl From<rusqlite::Error> for ApiError {
    fn from(_: rusqlite::Error) -> Self {
        Self::new("storage_error", "本地数据库操作失败")
    }
}
pub struct Application {
    connection: Connection,
}
impl Application {
    pub fn open(path: &Path) -> Result<Self, ApiError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .map_err(|_| ApiError::new("storage_error", "无法创建数据目录"))?;
        }
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let version: u32 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > DATABASE_SCHEMA_VERSION {
            return Err(ApiError::new(
                "unsupported_schema",
                "数据库由更新版本创建，请升级应用后再打开",
            ));
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS members (id TEXT PRIMARY KEY, name TEXT NOT NULL); CREATE TABLE IF NOT EXISTS clothing (id TEXT PRIMARY KEY, member_id TEXT NOT NULL REFERENCES members(id), document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS audit (id INTEGER PRIMARY KEY, entity_id TEXT NOT NULL, operation TEXT NOT NULL, document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS outbox (id INTEGER PRIMARY KEY, entity_id TEXT NOT NULL, document TEXT NOT NULL);")?;
        if version < 2 {
            for table in ["clothing", "audit", "outbox"] {
                let entries = {
                    let mut statement =
                        tx.prepare(&format!("SELECT rowid, document FROM {table}"))?;
                    let entries = statement
                        .query_map([], |row| {
                            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    entries
                };
                for (rowid, serialized) in entries {
                    let mut document: Value = serde_json::from_str(&serialized).map_err(|_| {
                        ApiError::new("storage_error", "迁移遇到无效单品数据，未修改数据库")
                    })?;
                    let object = document.as_object_mut().ok_or_else(|| {
                        ApiError::new("storage_error", "迁移遇到无效单品数据，未修改数据库")
                    })?;
                    if !object.contains_key("purchase_price_minor") {
                        let price = object.remove("price_minor").unwrap_or(Value::Null);
                        object.insert("purchase_price_minor".into(), price);
                    }
                    if object.get("category") == Some(&json!("jumpsuit")) {
                        object.insert("category".into(), json!("one_piece"));
                    }
                    if !object.contains_key("price_exponent") {
                        let exponent = object
                            .get("currency")
                            .and_then(Value::as_str)
                            .and_then(iso_currency::Currency::from_code)
                            .and_then(|currency| currency.exponent());
                        object.insert("price_exponent".into(), json!(exponent));
                    }
                    tx.execute(
                        &format!("UPDATE {table} SET document=?1 WHERE rowid=?2"),
                        params![document.to_string(), rowid],
                    )?;
                }
            }
            tx.execute_batch("PRAGMA user_version=2;")?;
        }
        tx.execute_batch("CREATE INDEX IF NOT EXISTS clothing_member_status_id ON clothing(member_id,json_extract(document,'$.status'),id);")?;
        members::migrate(&tx)?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS providers(id TEXT PRIMARY KEY, document TEXT NOT NULL);",
        )?;
        tx.execute_batch("PRAGMA user_version=3;")?;
        tx.commit()?;
        Ok(Self { connection })
    }
    fn check_schema(&self) -> Result<(), ApiError> {
        let version: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > DATABASE_SCHEMA_VERSION {
            return Err(ApiError::new(
                "unsupported_schema",
                "数据库由更新版本创建，请升级应用后再打开",
            ));
        }
        Ok(())
    }
    fn begin_write(&self) -> Result<rusqlite::Transaction<'_>, ApiError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        self.check_schema()?;
        Ok(tx)
    }
    pub fn execute(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        self.check_schema()?;
        if !input.is_object() {
            return Err(ApiError::new("invalid_input", "输入必须是 JSON 对象"));
        }
        let allowed: Option<&[&str]> = match operation {
            "space.default" | "member.list" => Some(&[]),
            "member.create" => Some(&["name"]),
            "member.update" => Some(&["member_id", "name"]),
            "member.select" => Some(&["member_id"]),
            _ => None,
        };
        if let Some(allowed) = allowed {
            if input
                .as_object()
                .unwrap()
                .keys()
                .any(|key| !allowed.contains(&key.as_str()))
            {
                return Err(ApiError::new("invalid_input", "成员操作包含不支持的字段"));
            }
        }
        match operation {
            "capabilities" => Ok(
                json!({"schema_version":1,"operations":operations(),"categories":CATEGORIES,"taxonomy":clothing::taxonomy()}),
            ),
            "space.default" => self.default_space(),
            "member.list" => self.member_list(),
            "member.create" => self.member_create(input),
            "member.update" => self.member_update(input),
            "member.select" => self.member_select(input),
            "provider.credential.set"
            | "provider.credential.remove"
            | "provider.credential.status" => self.provider_credential(operation, input),
            "provider.validate" => self.provider_validate(input),
            "provider.save" => self.provider_save(input),
            "provider.list" => self.provider_list(input),
            "clothing.create" => self.clothing_create(input),
            "clothing.history" => self.clothing_history(input),
            "clothing.update" => self.clothing_update(input),
            "clothing.archive" | "clothing.activate" | "clothing.delete" | "clothing.restore" => {
                self.clothing_lifecycle(operation, input)
            }
            "clothing.get" => self.clothing_get(input),
            "clothing.list" => self.clothing_list(input),
            _ => Err(ApiError::new("unsupported_operation", "此操作尚不可用")),
        }
    }
}
fn string<'a>(input: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::new("invalid_input", &format!("缺少字符串字段 {key}")))
}
pub fn operations() -> Value {
    let mut descriptors = members::descriptors();
    descriptors.extend(clothing::descriptors());
    descriptors.extend(providers::descriptors());
    let mut capabilities = json!(
    {"name":"capabilities","description":"发现当前可用能力","inputSchema":{"type":"object","properties":{}}}
        );
    capabilities["outputSchema"] = json!({"type":"object","required":["schema_version","operations","categories"],"properties":{"schema_version":{"type":"integer","const":1},"operations":{"type":"array","items":{"type":"object","required":["name","description","inputSchema","outputSchema"]}},"categories":{"type":"array","items":{"type":"string"}}}});
    descriptors.push(capabilities);
    Value::Array(descriptors)
}

pub fn default_database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("WARDROBE_DATA_DIR") {
        return PathBuf::from(path).join("wardrobe.sqlite");
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    let root = home.join("Library/Application Support/wardrobe-agent");
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or(home)
        .join("wardrobe-agent");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("wardrobe-agent");
    root.join("wardrobe.sqlite")
}
