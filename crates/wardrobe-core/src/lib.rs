mod ai_recommendations;
mod assets;
mod authorization;
mod backup;
mod care;
mod clothing;
mod categories;
mod cutout;
mod data_directory;
mod encryption;
mod feedback;
mod flatlay;
mod image_editing;
mod imports;
mod insights;
mod jobs;
mod learning;
mod maintenance;
mod members;
mod outfits;
mod pairing;
mod product;
mod providers;
mod recognition;
mod recommendations;
mod pair_search;
mod tryon;
mod wears;
mod weather;

use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const DATABASE_SCHEMA_VERSION: u32 = 21;
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
    fn from(error: rusqlite::Error) -> Self {
        #[cfg(all(debug_assertions, feature = "backup-test-diagnostics"))]
        if let rusqlite::Error::SqliteFailure(code, _) = &error {
            eprintln!(
                "backup_test_sqlite_code={:?} extended_code={}",
                code.code, code.extended_code
            );
        }
        let _ = error;
        Self::new("storage_error", "本地数据库操作失败")
    }
}
pub struct Application {
    connection: Connection,
    asset_root: PathBuf,
    installation_generation: i64,
}
impl Application {
    pub fn open(path: &Path) -> Result<Self, ApiError> {
        let resolved = data_directory::resolve(path)?;
        let path = resolved.as_path();
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
        tx.execute_batch("CREATE TABLE IF NOT EXISTS wardrobe_categories(member_id TEXT NOT NULL REFERENCES members(id),id TEXT NOT NULL,name TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(member_id,id));")?;
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
        assets::migrate(&tx)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS clients(id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE, document TEXT NOT NULL);")?;
        outfits::migrate(&tx)?;
        wears::migrate(&tx)?;
        insights::migrate(&tx)?;
        care::migrate(&tx)?;
        recommendations::migrate(&tx)?;
        weather::migrate(&tx)?;
        feedback::migrate(&tx)?;
        learning::migrate(&tx)?;
        jobs::migrate(&tx)?;
        imports::migrate(&tx)?;
        cutout::migrate(&tx)?;
        recognition::migrate(&tx)?;
        encryption::migrate(&tx)?;
        backup::migrate(&tx)?;
        maintenance::migrate(&tx)?;
        tx.execute_batch(&format!(
            "PRAGMA user_version={};",
            version.max(DATABASE_SCHEMA_VERSION)
        ))?;
        tx.commit()?;
        let installation_generation = connection.query_row(
            "SELECT generation FROM local_installation_state WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        Ok(Self {
            installation_generation,
            connection,
            asset_root: path.parent().unwrap_or(Path::new(".")).join("assets"),
        })
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
    fn installation_changed(&self) -> Result<bool, ApiError> {
        let current: i64 = self.connection.query_row(
            "SELECT generation FROM local_installation_state WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        Ok(current != self.installation_generation)
    }
    fn begin_write(&self) -> Result<rusqlite::Transaction<'_>, ApiError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        self.check_schema()?;
        if self.installation_changed()? {
            return Err(ApiError::new(
                "data_restored",
                "数据已恢复，请重新读取并重试操作",
            ));
        }
        if self.redirected()?.is_some() {
            return Err(ApiError::new(
                "data_directory_moved",
                "数据目录已切换，请重试操作",
            ));
        }
        Ok(tx)
    }
    pub fn execute(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        if self.installation_changed()? {
            return Application::open(Path::new(self.connection.path().unwrap()))?
                .execute(operation, input);
        }
        if let Some(path) = self.redirected()? {
            return Application::open(&path)?.execute(operation, input);
        }
        self.execute_current(operation, input)
    }
    pub(crate) fn execute_current(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        if self.installation_changed()? {
            return Err(ApiError::new(
                "data_restored",
                "数据已恢复，请重新验证授权后重试",
            ));
        }
        if self.redirected()?.is_some() {
            return Err(ApiError::new(
                "data_directory_moved",
                "数据目录已切换，请重新验证授权后重试",
            ));
        }
        self.check_schema()?;
        if !input.is_object() {
            return Err(ApiError::new("invalid_input", "输入必须是 JSON 对象"));
        }
        if operation.starts_with("images.") {
            image_editing::validate_input(operation, &input)?;
        }
        let allowed: Option<&[&str]> = match operation {
            "imports.create" => Some(&["member_id", "operation_key", "paths"]),
            "imports.list" | "jobs.list" => Some(&["member_id"]),
            "imports.get" | "jobs.get" | "jobs.cancel" | "jobs.retry" => Some(&["member_id", "id"]),
            "imports.process" => Some(&["member_id", "id", "item_id"]),
            "imports.group" => Some(&["member_id", "id", "item_ids"]),
            "imports.update" => Some(&["member_id", "id", "draft_id", "facts"]),
            "imports.commit" => Some(&["member_id", "id", "draft_id", "operation_key", "facts"]),
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
                return Err(ApiError::new("invalid_input", "操作包含不支持的字段"));
            }
        }
        match operation {
            "category.list" | "category.create" | "category.delete" => self.category_operation(operation,input),
            name if matches!(name,"backup.create"|"backup.preview"|"backup.restore"|"data_directory.move") => self.maintenance_enqueue(name,input),
            name if name.starts_with("maintenance.") => self.maintenance_operation(name,input),
            name if name.starts_with("data_directory.") => self.directory_operation(name, input),
            name if name.starts_with("backup.") => self.backup_operation(name, input),
            name if name.starts_with("encryption.") => self.encryption_operation(name, input),
            "capabilities" => Ok(
                json!({"schema_version":1,"operations":operations(),"categories":CATEGORIES,"taxonomy":clothing::taxonomy()}),
            ),
            "recognition.create" => self.recognition_create(input),
            "recognition.process" => self.recognition_process(input),
            "recognition.get" => self.recognition_get(input),
            "recognition.list" => self.recognition_list(input),
            "recognition.review" => self.recognition_review(input, false),
            "recognition.dismiss" => self.recognition_review(input, true),
            "client.list" => self.client_list(input),
            "client.revoke" => self.client_revoke(input),
            "client.authorize" => self.client_authorize(input),
            "space.default" => self.default_space(),
            "member.list" => self.member_list(),
            "member.create" => self.member_create(input),
            "member.update" => self.member_update(input),
            "member.select" => self.member_select(input),
            "provider.credential.set"
            | "provider.credential.remove"
            | "provider.credential.status" => self.provider_credential(operation, input),
            "tryon.person.import" => self.tryon_person_import(input),
            "tryon.process" => self.tryon_process(input),
            "tryon.save" => self.tryon_save(input),
            "tryon.preview" => self.tryon_preview(input),
            "tryon.person.preview" => self.tryon_person_preview(input),
            "tryon.compare" => self.tryon_compare(input),
            "tryon.generate" => self.tryon_generate(input),
            "flatlay.generate" => self.flatlay_generate(input),
            "flatlay.process" => self.flatlay_process(input),
            "flatlay.save" => self.flatlay_save(input),
            "flatlay.preview" => self.flatlay_preview(input),
            "product.generate" => self.product_generate(input),
            "product.process" => self.product_process(input),
            "product.adopt" => self.product_adopt(input),
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
            op if op.starts_with("outfit.") => self.outfit_execute(op, input),
            op if op.starts_with("wear.") => self.wear_execute(op, input),
            op if op.starts_with("learning.") => self.learning_execute(op, input),
            op if op.starts_with("feedback.") => self.feedback_execute(op, input),
            op if op.starts_with("weather.") => self.weather_execute(op, input),
            op if op.starts_with("recommendation.") => self.recommendation_execute(op, input),
            op if op.starts_with("pairing.") => self.pairing_execute(op, input),
            "insights.summary" => self.insights_summary(input),
            "insights.configure" => self.insights_configure(input),
            op if op.starts_with("care.") => self.care_execute(op, input),
            "imports.update" => self.imports_update(input),
            "imports.group" => self.imports_group(input),
            "imports.commit" => self.imports_commit(input),
            "imports.create" => self.imports_create(input),
            "imports.get" => self.imports_get(input),
            "imports.list" => self.imports_list(input),
            "imports.process" => self.imports_process(input),
            "jobs.get" => self.job_get(input),
            "jobs.list" => self.job_list(input),
            "jobs.cancel" | "jobs.retry" => self.job_action(operation, input),
            "images.merge" | "images.replace" | "images.main" => self.image_links(operation, input),
            "images.model.download" | "images.cutout" => self.cutout_enqueue(operation, input),
            "images.worker.run" => self.cutout_run(input),
            "images.model.status" => self.cutout_model_status(input),
            "images.rotate" | "images.restore" => self.image_transform(operation, input),
            "assets.import" => self.asset_import(input),
            "assets.get" => self.asset_get(input),
            "assets.preview" => self.asset_preview(input),
            "assets.rebuild_thumbnail" => self.asset_rebuild_thumbnail(input),
            "assets.adopt" => self.asset_adopt(input),
            "assets.reorder" => self.asset_reorder(input),
            "assets.detach" => self.asset_detach(input),
            "assets.sync_manifest" => self.asset_sync_manifest(input),
            _ => Err(ApiError::new("unsupported_operation", "此操作尚不可用")),
        }.map(|mut result| {
            if operation.starts_with("backup.") || operation.starts_with("data_directory.") {
                result["schema_version"] = json!(1);
            }
            result
        })
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
    descriptors.extend(backup::descriptors());
    descriptors.extend(maintenance::descriptors());
    descriptors.extend(data_directory::descriptors());
    descriptors.extend(encryption::descriptors());
    descriptors.extend(clothing::descriptors());
    descriptors.extend(categories::descriptors());
    descriptors.extend(providers::descriptors());
    descriptors.extend(recognition::descriptors());
    descriptors.extend(product::descriptors());
    descriptors.extend(flatlay::descriptors());
    descriptors.extend(tryon::descriptors());
    descriptors.extend(authorization::descriptors());
    descriptors.extend(assets::descriptors());
    descriptors.extend(outfits::descriptors());
    descriptors.extend(wears::descriptors());
    descriptors.extend(insights::descriptors());
    descriptors.extend(care::descriptors());
    descriptors.extend(recommendations::descriptors());
    descriptors.extend(pairing::descriptors());
    descriptors.extend(weather::descriptors());
    descriptors.extend(feedback::descriptors());
    descriptors.extend(learning::descriptors());
    descriptors.extend(jobs::descriptors());
    descriptors.extend(imports::descriptors());
    descriptors.extend(image_editing::descriptors());
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
