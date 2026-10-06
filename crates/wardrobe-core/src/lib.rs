use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const DATABASE_SCHEMA_VERSION: u32 = 2;
pub const CATEGORIES: &[&str] = &[
    "top",
    "bottom",
    "outerwear",
    "dress",
    "one_piece",
    "shoes",
    "bag",
    "accessory",
    "underwear",
    "sportswear",
    "other",
];
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
        tx.commit()?;
        Ok(Self { connection })
    }
    pub fn execute(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        if !input.is_object() {
            return Err(ApiError::new("invalid_input", "输入必须是 JSON 对象"));
        }
        match operation {
            "capabilities" => {
                Ok(json!({"schema_version":1,"operations":operations(),"categories":CATEGORIES}))
            }
            "space.default" => {
                self.connection.execute(
                    "INSERT OR IGNORE INTO members VALUES ('default', '我的衣橱')",
                    [],
                )?;
                Ok(
                    json!({"schema_version":1,"space_id":"local","member_id":"default","name":"我的衣橱"}),
                )
            }
            "clothing.create" => self.create(input),
            "clothing.get" => {
                let member = self.member(&input)?;
                let id = string(&input, "id")?;
                let doc: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT document FROM clothing WHERE id=?1 AND member_id=?2",
                        params![id, member],
                        |r| r.get(0),
                    )
                    .optional()?;
                doc.map(|s| serde_json::from_str(&s).unwrap())
                    .ok_or_else(|| ApiError::new("not_found", "找不到该成员的单品"))
            }
            "clothing.list" => {
                let member = self.member(&input)?;
                let limit = match input.get("limit") {
                    None => 50,
                    Some(value) => value
                        .as_u64()
                        .ok_or_else(|| ApiError::new("invalid_input", "每页数量必须为整数"))?,
                };
                if !(1..=100).contains(&limit) {
                    return Err(ApiError::new("invalid_input", "每页数量必须为 1–100"));
                }
                let cursor = match input.get("cursor") {
                    None => "",
                    Some(value) => value
                        .as_str()
                        .ok_or_else(|| ApiError::new("invalid_input", "分页游标必须为字符串"))?,
                };
                let mut statement = self.connection.prepare("SELECT document FROM clothing WHERE member_id=?1 AND id>?2 ORDER BY id LIMIT ?3")?;
                let mut rows = statement
                    .query_map(params![member, cursor, limit + 1], |r| {
                        r.get::<_, String>(0)
                    })?
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|s| serde_json::from_str::<Value>(&s).unwrap())
                    .collect::<Vec<_>>();
                let more = rows.len() > limit as usize;
                rows.truncate(limit as usize);
                let next = if more {
                    rows.last().map(|r| r["id"].clone())
                } else {
                    None
                };
                Ok(json!({"schema_version":1,"items":rows,"next_cursor":next}))
            }
            _ => Err(ApiError::new("unsupported_operation", "此操作尚不可用")),
        }
    }
    fn member<'a>(&self, input: &'a Value) -> Result<&'a str, ApiError> {
        let member = string(input, "member_id")?;
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM members WHERE id=?1)",
            [member],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(ApiError::new("not_found", "成员不存在"));
        }
        Ok(member)
    }
    fn create(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let name = string(&input, "name")?.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(ApiError::new("invalid_input", "名称须为 1–120 个字符"));
        }
        let category = string(&input, "category")?;
        if !CATEGORIES.contains(&category) {
            return Err(ApiError::new("invalid_input", "请选择有效类别"));
        }
        let seasons = input.get("seasons").cloned().unwrap_or(json!([]));
        let seasons = seasons
            .as_array()
            .ok_or_else(|| ApiError::new("invalid_input", "季节必须为数组"))?;
        let mut checked = Vec::<&str>::new();
        for value in seasons {
            let s = value
                .as_str()
                .ok_or_else(|| ApiError::new("invalid_input", "季节无效"))?;
            if !["spring", "summer", "autumn", "winter"].contains(&s) {
                return Err(ApiError::new("invalid_input", "季节无效"));
            }
            if !checked.contains(&s) {
                checked.push(s);
            }
        }
        let price = input
            .get("purchase_price_minor")
            .cloned()
            .unwrap_or(Value::Null);
        let currency = input.get("currency").cloned().unwrap_or(Value::Null);
        if !price.is_null() && price.as_u64().is_none() {
            return Err(ApiError::new(
                "invalid_input",
                "价格须为非负最小货币单位整数",
            ));
        }
        if !price.is_null() && currency.is_null() {
            return Err(ApiError::new("invalid_input", "价格需要币种"));
        }
        if !currency.is_null()
            && !currency
                .as_str()
                .is_some_and(|s| iso_currency::Currency::from_code(s).is_some())
        {
            return Err(ApiError::new(
                "invalid_input",
                "币种须为 ISO 三字母大写代码",
            ));
        }
        let price_exponent = currency
            .as_str()
            .and_then(iso_currency::Currency::from_code)
            .and_then(|c| c.exponent());
        let id = uuid::Uuid::now_v7().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let doc = json!({"schema_version":1,"id":id,"member_id":member,"name":name,"category":category,"seasons":checked,"purchase_price_minor":price,"price_exponent":price_exponent,"currency":currency,"size_label":null,"status":"active","favorite":false,"created_at":now,"updated_at":now});
        let serialized = doc.to_string();
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO clothing VALUES (?1,?2,?3)",
            params![id, member, serialized],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES (?1,'clothing.create',?2)",
            params![id, serialized],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES (?1,?2)",
            params![id, serialized],
        )?;
        tx.commit()?;
        Ok(doc)
    }
}
fn string<'a>(input: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::new("invalid_input", &format!("缺少字符串字段 {key}")))
}
pub fn operations() -> Value {
    let mut descriptors = json!([
        {"name":"space.default","description":"创建或读取默认本地空间","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"clothing.create","description":"保存无图单品","inputSchema":{"type":"object","required":["member_id","name","category"],"properties":{"member_id":{"type":"string"},"name":{"type":"string","minLength":1,"maxLength":120},"category":{"type":"string","enum":CATEGORIES},"seasons":{"type":"array","items":{"type":"string","enum":["spring","summer","autumn","winter"]}},"purchase_price_minor":{"type":["integer","null"],"minimum":0},"currency":{"type":["string","null"]}}}},
        {"name":"clothing.get","description":"读取成员单品","inputSchema":{"type":"object","required":["member_id","id"],"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}}},
        {"name":"clothing.list","description":"分页浏览成员单品","inputSchema":{"type":"object","required":["member_id"],"properties":{"member_id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100},"cursor":{"type":"string"}}}},
        {"name":"capabilities","description":"发现当前可用能力","inputSchema":{"type":"object","properties":{}}}
    ]);
    for descriptor in descriptors.as_array_mut().unwrap() {
        descriptor["outputSchema"] = output_schema(descriptor["name"].as_str().unwrap());
    }
    descriptors
}
fn output_schema(operation: &str) -> Value {
    let item = json!({"type":"object","required":["schema_version","id","member_id","name","category","seasons","purchase_price_minor","price_exponent","currency","size_label","status","favorite","created_at","updated_at"],"properties":{"schema_version":{"type":"integer","const":1},"id":{"type":"string"},"member_id":{"type":"string"},"name":{"type":"string"},"category":{"type":"string","enum":CATEGORIES},"seasons":{"type":"array","items":{"type":"string","enum":["spring","summer","autumn","winter"]}},"purchase_price_minor":{"type":["integer","null"],"minimum":0},"price_exponent":{"type":["integer","null"]},"currency":{"type":["string","null"]},"size_label":{"type":["string","null"]},"status":{"type":"string","enum":["active","archived","deleted"]},"favorite":{"type":"boolean"},"created_at":{"type":"string"},"updated_at":{"type":"string"}}});
    match operation {
        "clothing.create" | "clothing.get" => item,
        "space.default" => {
            json!({"type":"object","required":["schema_version","space_id","member_id","name"],"properties":{"schema_version":{"type":"integer","const":1},"space_id":{"type":"string"},"member_id":{"type":"string"},"name":{"type":"string"}}})
        }
        "clothing.list" => {
            json!({"type":"object","required":["schema_version","items","next_cursor"],"properties":{"schema_version":{"type":"integer","const":1},"items":{"type":"array","items":item},"next_cursor":{"type":["string","null"]}}})
        }
        _ => {
            json!({"type":"object","required":["schema_version","operations","categories"],"properties":{"schema_version":{"type":"integer","const":1},"operations":{"type":"array","items":{"type":"object","required":["name","description","inputSchema","outputSchema"]}},"categories":{"type":"array","items":{"type":"string"}}}})
        }
    }
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
