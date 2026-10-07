use super::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
const DEFAULTS: &[(&str, &str)] = &[
    ("top", "上装"),
    ("bottom", "下装"),
    ("outerwear", "外套"),
    ("dress", "连衣裙"),
    ("one_piece", "连体"),
    ("shoes", "鞋"),
    ("bag", "包"),
    ("accessory", "配饰"),
    ("underwear", "内衣"),
    ("sportswear", "运动服"),
    ("other", "其他"),
];
impl Application {
    pub(super) fn category_exists(&self, member: &str, id: &str) -> Result<bool, ApiError> {
        let row: Option<bool> = self
            .connection
            .query_row(
                "SELECT deleted FROM wardrobe_categories WHERE member_id=?1 AND id=?2",
                params![member, id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(row
            .map(|deleted| !deleted)
            .unwrap_or(DEFAULTS.iter().any(|(key, _)| *key == id)))
    }
    pub(super) fn category_operation(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        if operation == "category.list" {
            let mut rows = Vec::<Value>::new();
            for (id, name) in DEFAULTS {
                if self.category_exists(&member, id)? {
                    rows.push(json!({"schema_version":1,"id":id,"name":name}));
                }
            }
            let mut stmt=self.connection.prepare("SELECT id,name FROM wardrobe_categories WHERE member_id=?1 AND deleted=0 ORDER BY rowid")?;
            for row in stmt.query_map([&member], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })? {
                let (id, name) = row?;
                if !DEFAULTS.iter().any(|(key, _)| *key == id) {
                    rows.push(json!({"schema_version":1,"id":id,"name":name}));
                }
            }
            for row in &mut rows {
                row["count"]=json!(self.connection.query_row("SELECT count(*) FROM clothing WHERE member_id=?1 AND json_extract(document,'$.category')=?2",params![member,row["id"].as_str()],|r|r.get::<_,i64>(0))?);
            }
            return Ok(json!({"schema_version":1,"items":rows}));
        }
        if operation == "category.create" {
            let name = string(&input, "name")?.trim();
            if name.is_empty() || name.chars().count() > 40 {
                return Err(ApiError::new("invalid_input", "分类名称须为 1–40 个字符"));
            }
            let tx = self.begin_write()?;
            let duplicate:i64=tx.query_row("SELECT count(*) FROM wardrobe_categories WHERE member_id=?1 AND name=?2 AND deleted=0",params![member,name],|r|r.get(0))?;
            let default_duplicate = DEFAULTS
                .iter()
                .any(|(id, n)| *n == name && self.category_exists(&member, id).unwrap_or(true));
            if duplicate > 0 || default_duplicate {
                return Err(ApiError::new("conflict", "这个分类名称已存在"));
            }
            let id = format!("custom_{}", uuid::Uuid::now_v7());
            tx.execute(
                "INSERT INTO wardrobe_categories(member_id,id,name,deleted) VALUES(?1,?2,?3,0)",
                params![member, id, name],
            )?;
            tx.commit()?;
            return Ok(json!({"schema_version":1,"id":id,"name":name,"count":0}));
        }
        let id = string(&input, "id")?;
        if id == "other" {
            return Err(ApiError::new(
                "conflict",
                "其他分类用于收纳未分类衣物，不能删除",
            ));
        }
        let tx = self.begin_write()?;
        if !self.category_exists(&member, id)? {
            return Err(ApiError::new("not_found", "分类不存在"));
        }
        let docs = {
            let mut stmt=tx.prepare("SELECT document FROM clothing WHERE member_id=?1 AND json_extract(document,'$.category')=?2")?;
            let rows = stmt
                .query_map(params![member, id], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let moved = docs.len();
        for raw in docs {
            let mut doc: Value = serde_json::from_str(&raw)
                .map_err(|_| ApiError::new("storage_error", "单品数据无效"))?;
            doc["category"] = json!("other");
            doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
            let serialized = doc.to_string();
            tx.execute(
                "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
                params![serialized, doc["id"].as_str(), member],
            )?;
            tx.execute(
                "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'category.delete',?2)",
                params![doc["id"].as_str(), serialized],
            )?;
            tx.execute(
                "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
                params![doc["id"].as_str(), serialized],
            )?;
        }
        tx.execute("INSERT INTO wardrobe_categories(member_id,id,name,deleted) VALUES(?1,?2,?2,1) ON CONFLICT(member_id,id) DO UPDATE SET deleted=1",params![member,id])?;
        tx.commit()?;
        Ok(json!({"schema_version":1,"deleted":id,"moved":moved}))
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    vec![
        json!({"name":"category.list","description":"列出衣橱分类与单品数量","inputSchema":{"type":"object","required":["member_id"],"properties":{"member_id":{"type":"string"}}},"effect":{"kind":"query","network":"none","local_files":false,"async":null},"examples":[{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["先以 space.default 初始化工作空间并取得成员ID"]}],"outputSchema":{"type":"object","required":["schema_version","items"],"properties":{"schema_version":{"type":"integer","const":1},"items":{"type":"array","items":{"type":"object","required":["schema_version","id","name","count"],"properties":{"schema_version":{"type":"integer","const":1},"id":{"type":"string"},"name":{"type":"string"},"count":{"type":"integer"}}}}}}}),
        json!({"name":"category.create","description":"新增自定义衣物分类","inputSchema":{"type":"object","required":["member_id","name"],"properties":{"member_id":{"type":"string"},"name":{"type":"string","minLength":1,"maxLength":40}}},"effect":{"kind":"mutation","network":"none","local_files":false,"async":null},"examples":[{"request":{"member_id":"<已授权成员ID>","name":"<自定义分类名称>"},"prerequisites":["space.default 初始化；名称为1-40字符且不与现有分类重名"]}],"outputSchema":{"type":"object","required":["schema_version","id","name","count"],"properties":{"schema_version":{"type":"integer","const":1},"id":{"type":"string"},"name":{"type":"string"},"count":{"type":"integer"}}}}),
        json!({"name":"category.delete","description":"删除分类，已有单品转入其他；保留穿搭快照","inputSchema":{"type":"object","required":["member_id","id"],"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}},"effect":{"kind":"mutation","network":"none","local_files":false,"async":null},"examples":[{"request":{"member_id":"<已授权成员ID>","id":"<category.create 返回的 custom_ 分类ID>"},"prerequisites":["其他分类不可删除；已有衣物转入其他"]}],"outputSchema":{"type":"object","required":["schema_version","deleted","moved"],"properties":{"schema_version":{"type":"integer","const":1},"deleted":{"type":"string"},"moved":{"type":"integer"}}}}),
    ]
}
