use crate::{string, ApiError, Application};
use base64::Engine;
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::HashSet;

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS outfits(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS outfit_items(outfit_id TEXT NOT NULL REFERENCES outfits(id),position INTEGER NOT NULL,clothing_id TEXT NOT NULL REFERENCES clothing(id),role TEXT NOT NULL,layer INTEGER NOT NULL,PRIMARY KEY(outfit_id,position),UNIQUE(outfit_id,clothing_id)); CREATE TABLE IF NOT EXISTS outfit_versions(outfit_id TEXT NOT NULL REFERENCES outfits(id),version INTEGER NOT NULL,document TEXT NOT NULL,PRIMARY KEY(outfit_id,version)); CREATE INDEX IF NOT EXISTS outfits_member ON outfits(member_id,id);")?;
    Ok(())
}
fn invalid(message: &str) -> ApiError {
    ApiError::new("invalid_input", message)
}
fn fields(input: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if input
        .as_object()
        .unwrap()
        .keys()
        .any(|key| !allowed.contains(&key.as_str()))
    {
        return Err(invalid("搭配包含不支持的字段"));
    }
    Ok(())
}
fn name(input: &Value) -> Result<String, ApiError> {
    let n = string(input, "name")?.trim();
    if n.is_empty() || n.chars().count() > 120 {
        return Err(invalid("搭配名称须为1–120个字符"));
    }
    Ok(n.into())
}
impl Application {
    pub(super) fn outfit_execute(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        let allowed = match operation {
            "outfit.create" => vec!["member_id", "name", "items"],
            "outfit.update" => vec!["member_id", "id", "name", "items", "expected_version"],
            "outfit.list" => vec!["member_id", "status", "favorite", "limit", "cursor"],
            "outfit.version" => vec!["member_id", "id", "version"],
            "outfit.favorite" => vec!["member_id", "id", "favorite"],
            "outfit.clone" => vec!["member_id", "id", "name"],
            _ => vec!["member_id", "id"],
        };
        fields(&input, &allowed)?;
        let member = self.member(&input)?;
        if operation == "outfit.list" {
            return self.outfit_list(&member, &input);
        }
        if operation == "outfit.get" {
            return self.outfit_get(&member, string(&input, "id")?);
        }
        if operation == "outfit.version" {
            let id = string(&input, "id")?;
            self.outfit_get(&member, id)?;
            let version = input["version"]
                .as_u64()
                .filter(|v| *v > 0)
                .ok_or_else(|| invalid("版本须为正整数"))?;
            let serialized: Option<String> = self
                .connection
                .query_row(
                    "SELECT document FROM outfit_versions WHERE outfit_id=?1 AND version=?2",
                    params![id, version],
                    |r| r.get(0),
                )
                .optional()?;
            return parse(serialized);
        }
        if operation == "outfit.render_local" {
            return self.outfit_render(&member, string(&input, "id")?);
        }
        // Hold an immediate write transaction across validating current clothing and committing snapshots.
        if self.connection.is_autocommit() {
            let tx = self.begin_write()?;
            let result = self.outfit_execute(operation, input)?;
            tx.commit()?;
            return Ok(result);
        }
        let mut doc = if operation == "outfit.create" {
            let items = self.outfit_items(&member, &input["items"])?;
            let now = chrono::Utc::now().to_rfc3339();
            json!({"schema_version":1,"id":uuid::Uuid::now_v7().to_string(),"member_id":member,"name":name(&input)?,"version":1,"status":"active","favorite":false,"items":items,"created_at":now,"updated_at":now})
        } else {
            self.outfit_get(&member, string(&input, "id")?)?
        };
        if operation != "outfit.create" {
            if operation == "outfit.restore" {
                if doc["status"] != "deleted" {
                    return Err(ApiError::new("invalid_state", "搭配未在回收站"));
                }
                doc["status"] = json!("active");
            } else {
                if doc["status"] == "deleted" {
                    return Err(ApiError::new("invalid_state", "请先恢复回收站搭配"));
                }
                match operation {
                    "outfit.update" => {
                        let expected = input["expected_version"]
                            .as_u64()
                            .filter(|v| *v > 0)
                            .ok_or_else(|| invalid("编辑搭配需要明确expected_version"))?;
                        if expected != doc["version"].as_u64().unwrap() {
                            return Err(ApiError::new(
                                "version_conflict",
                                "搭配已被修改，请重新读取",
                            ));
                        }
                        doc["name"] = json!(name(&input)?);
                        doc["items"] = json!(self.outfit_items(&member, &input["items"])?);
                    }
                    "outfit.favorite" => {
                        doc["favorite"] = json!(input["favorite"]
                            .as_bool()
                            .ok_or_else(|| invalid("收藏须为布尔值"))?);
                    }
                    "outfit.delete" => doc["status"] = json!("deleted"),
                    "outfit.clone" => {
                        doc["id"] = json!(uuid::Uuid::now_v7().to_string());
                        doc["name"] = json!(name(&input)?);
                        doc["version"] = json!(0);
                        doc["favorite"] = json!(false);
                        doc["created_at"] = json!(chrono::Utc::now().to_rfc3339());
                    }
                    _ => return Err(ApiError::new("unsupported_operation", "搭配操作不可用")),
                }
            }
            doc["version"] = json!(doc["version"].as_u64().unwrap() + 1);
            doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        }
        let id = doc["id"].as_str().unwrap();
        let serialized = doc.to_string();
        self.connection.execute("INSERT INTO outfits(id,member_id,document) VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET document=excluded.document",params![id,member,serialized])?;
        self.connection
            .execute("DELETE FROM outfit_items WHERE outfit_id=?1", [id])?;
        for (position, item) in doc["items"].as_array().unwrap().iter().enumerate() {
            self.connection.execute(
                "INSERT INTO outfit_items VALUES (?1,?2,?3,?4,?5)",
                params![
                    id,
                    position,
                    item["clothing_id"].as_str(),
                    item["role"].as_str(),
                    item["layer"].as_i64()
                ],
            )?;
        }
        self.connection.execute(
            "INSERT INTO outfit_versions VALUES (?1,?2,?3)",
            params![id, doc["version"].as_u64(), serialized],
        )?;
        self.connection.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES (?1,?2,?3)",
            params![id, operation, serialized],
        )?;
        self.connection.execute(
            "INSERT INTO outbox(entity_id,document) VALUES (?1,?2)",
            params![id, serialized],
        )?;
        Ok(doc)
    }
    fn outfit_get(&self, member: &str, id: &str) -> Result<Value, ApiError> {
        let serialized = self
            .connection
            .query_row(
                "SELECT document FROM outfits WHERE id=?1 AND member_id=?2",
                params![id, member],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        parse(serialized)
    }
    fn outfit_items(&self, member: &str, value: &Value) -> Result<Vec<Value>, ApiError> {
        let items = value
            .as_array()
            .filter(|items| !items.is_empty() && items.len() <= 100)
            .ok_or_else(|| invalid("搭配须有1–100件单品"))?;
        let mut ids = HashSet::new();
        let mut result = vec![];
        for item in items {
            if !item.is_object() {
                return Err(invalid("搭配单品须为对象"));
            }
            fields(item, &["clothing_id", "role", "layer"])?;
            let id = string(item, "clothing_id")?;
            if !ids.insert(id) {
                return Err(invalid("搭配不能重复单品"));
            }
            let role = string(item, "role")?.trim();
            if role.is_empty() || role.chars().count() > 40 {
                return Err(invalid("角色须为1–40个字符"));
            }
            let layer = item["layer"]
                .as_i64()
                .filter(|v| (-100..=100).contains(v))
                .ok_or_else(|| invalid("层次须为-100到100的整数"))?;
            let garment = self.clothing_get(json!({"member_id":member,"id":id}))?;
            if garment["status"] == "deleted" {
                return Err(ApiError::new("invalid_state", "回收站单品不可加入新搭配"));
            }
            result.push(json!({"clothing_id":id,"role":role,"layer":layer,"snapshot":garment}));
        }
        Ok(result)
    }
    fn outfit_list(&self, member: &str, input: &Value) -> Result<Value, ApiError> {
        let status = input
            .get("status")
            .map(|v| v.as_str().ok_or_else(|| invalid("搭配状态须为字符串")))
            .transpose()?
            .unwrap_or("active");
        if !["active", "deleted", "all"].contains(&status) {
            return Err(invalid("请选择有效搭配状态"));
        }
        let favorite = match input.get("favorite") {
            Some(v) => Some(v.as_bool().ok_or_else(|| invalid("收藏须为布尔值"))?),
            None => None,
        };
        let limit = input
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .filter(|v| (1..=100).contains(v))
                    .ok_or_else(|| invalid("每页数量须为1–100"))
            })
            .transpose()?
            .unwrap_or(24);
        let cursor = input
            .get("cursor")
            .map(|v| v.as_str().ok_or_else(|| invalid("游标须为字符串")))
            .transpose()?;
        let mut statement = self
            .connection
            .prepare("SELECT document FROM outfits WHERE member_id=?1 ORDER BY id DESC")?;
        let mut filtered = vec![];
        for row in statement.query_map([member], |r| r.get::<_, String>(0))? {
            let doc: Value = serde_json::from_str(&row?)
                .map_err(|_| ApiError::new("storage_error", "搭配记录损坏"))?;
            if (status == "all" || doc["status"] == status)
                && favorite.is_none_or(|f| doc["favorite"] == f)
            {
                filtered.push(doc);
            }
        }
        let start = match cursor {
            Some(cursor) => filtered
                .iter()
                .position(|d| d["id"] == cursor)
                .map(|i| i + 1)
                .ok_or_else(|| invalid("搭配游标不属于当前筛选"))?,
            None => 0,
        };
        let has_more = filtered.len() > start + limit as usize;
        let items: Vec<_> = filtered
            .into_iter()
            .skip(start)
            .take(limit as usize)
            .collect();
        let next = if has_more {
            items.last().map(|d| d["id"].clone())
        } else {
            None
        };
        Ok(json!({"schema_version":1,"items":items,"next_cursor":next}))
    }
    fn outfit_render(&self, member: &str, id: &str) -> Result<Value, ApiError> {
        let doc = self.outfit_get(member, id)?;
        let items = doc["items"].as_array().unwrap();
        let columns = if items.len() == 1 { 1 } else { 2 };
        let rows = items.len().div_ceil(columns);
        let mut canvas = image::RgbaImage::from_pixel(
            800,
            (rows * 480) as u32,
            image::Rgba([246, 243, 235, 255]),
        );
        let width = 800 / columns as u32;
        let mut order: Vec<_> = items.iter().enumerate().collect();
        order.sort_by_key(|(i, item)| (item["layer"].as_i64().unwrap(), *i));
        let mut missing = 0;
        for (position, (_, item)) in order.iter().enumerate() {
            let x = (position % columns) as u32 * width;
            let y = (position / columns) as u32 * 480;
            let asset = item["snapshot"]["display_asset_id"]
                .as_str()
                .and_then(|id| self.asset_get(json!({"member_id":member,"id":id})).ok());
            let image = asset.and_then(|a| {
                a["display"]["local_path"]
                    .as_str()
                    .and_then(|path| image::open(path).ok())
            });
            if let Some(image) = image {
                let resized = image.thumbnail(width - 48, 432).to_rgba8();
                image::imageops::overlay(
                    &mut canvas,
                    &resized,
                    (x + (width - resized.width()) / 2) as i64,
                    (y + (480 - resized.height()) / 2) as i64,
                );
            } else {
                missing += 1;
                for yy in y + 24..y + 456 {
                    for xx in x + 24..x + width - 24 {
                        if (xx - x == 24)
                            || (xx - x == width - 25)
                            || (yy - y == 24)
                            || (yy - y == 455)
                        {
                            canvas.put_pixel(xx, yy, image::Rgba([164, 161, 151, 255]));
                        }
                    }
                }
            }
        }
        let bytes = webp::Encoder::from_rgba(canvas.as_raw(), canvas.width(), canvas.height())
            .encode(82.0)
            .to_vec();
        Ok(
            json!({"schema_version":1,"outfit_id":id,"version":doc["version"],"role":"outfit","sync_scope":"local_only","width":canvas.width(),"height":canvas.height(),"missing_images":missing,"data_url":format!("data:image/webp;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes))}),
        )
    }
}
fn parse(serialized: Option<String>) -> Result<Value, ApiError> {
    serde_json::from_str(&serialized.ok_or_else(|| ApiError::new("not_found", "搭配或版本不存在"))?)
        .map_err(|_| ApiError::new("storage_error", "搭配记录损坏"))
}
pub(super) fn descriptors() -> Vec<Value> {
    let item = json!({"type":"object","additionalProperties":false,"required":["clothing_id","role","layer"],"properties":{"clothing_id":{"type":"string"},"role":{"type":"string","minLength":1,"maxLength":40},"layer":{"type":"integer","minimum":-100,"maximum":100}}});
    ["create","get","list","update","version","favorite","clone","delete","restore","render_local"].iter().map(|operation|{let mut properties=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];if *operation!="create"&&*operation!="list"{properties["id"]=json!({"type":"string"});required.push("id");}
if ["create","update","clone"].contains(operation){properties["name"]=json!({"type":"string","minLength":1,"maxLength":120});required.push("name");}
if ["create","update"].contains(operation){properties["items"]=json!({"type":"array","minItems":1,"maxItems":100,"items":item});required.push("items");}
if *operation=="update"{properties["expected_version"]=json!({"type":"integer","minimum":1});required.push("expected_version");}
if *operation=="version"{properties["version"]=json!({"type":"integer","minimum":1});required.push("version");}
if *operation=="favorite"{properties["favorite"]=json!({"type":"boolean"});required.push("favorite");}
if *operation=="list"{properties["status"]=json!({"enum":["active","deleted","all"]});properties["favorite"]=json!({"type":"boolean"});properties["limit"]=json!({"type":"integer","minimum":1,"maximum":100});properties["cursor"]=json!({"type":"string"});}let output=match *operation{ "list"=>json!({"type":"object","required":["schema_version","items","next_cursor"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":{"type":"object","required":["id","member_id","name","items","version","status","favorite"]}},"next_cursor":{"type":["string","null"]}}}), "render_local"=>json!({"type":"object","required":["schema_version","outfit_id","version","role","sync_scope","width","height","missing_images","data_url"],"properties":{"role":{"const":"outfit"},"sync_scope":{"const":"local_only"},"data_url":{"type":"string"}}}),_=>json!({"type":"object","required":["schema_version","id","member_id","name","items","version","status","favorite","created_at","updated_at"],"properties":{"schema_version":{"const":1},"version":{"type":"integer","minimum":1},"items":{"type":"array","minItems":1,"items":{"type":"object","required":["clothing_id","role","layer","snapshot"]}}}})};
json!({"name":format!("outfit.{operation}"),"description":format!("本地Lookbook {operation}"),"inputSchema":{"type":"object","additionalProperties":false,"required":required,"properties":properties},"outputSchema":output})}).collect()
}
