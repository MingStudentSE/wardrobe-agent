use super::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

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
impl Application {
    pub(super) fn clothing_get(&self, input: Value) -> Result<Value, ApiError> {
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
    pub(super) fn clothing_list(&self, input: Value) -> Result<Value, ApiError> {
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
        let mut statement = self.connection.prepare(
            "SELECT document FROM clothing WHERE member_id=?1 AND id>?2 ORDER BY id LIMIT ?3",
        )?;
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
    pub(super) fn clothing_create(&self, input: Value) -> Result<Value, ApiError> {
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
        let tx = self.begin_write()?;
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
pub(super) fn descriptors() -> Vec<Value> {
    let mut descriptors = json!([
        {"name":"clothing.create","description":"保存无图单品","inputSchema":{"type":"object","required":["member_id","name","category"],"properties":{"member_id":{"type":"string"},"name":{"type":"string","minLength":1,"maxLength":120},"category":{"type":"string","enum":CATEGORIES},"seasons":{"type":"array","items":{"type":"string","enum":["spring","summer","autumn","winter"]}},"purchase_price_minor":{"type":["integer","null"],"minimum":0},"currency":{"type":["string","null"]}}}},
        {"name":"clothing.get","description":"读取成员单品","inputSchema":{"type":"object","required":["member_id","id"],"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}}},
        {"name":"clothing.list","description":"分页浏览成员单品","inputSchema":{"type":"object","required":["member_id"],"properties":{"member_id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100},"cursor":{"type":"string"}}}},
    ]);
    for descriptor in descriptors.as_array_mut().unwrap() {
        descriptor["outputSchema"] = output_schema(descriptor["name"].as_str().unwrap());
    }
    descriptors.as_array().unwrap().clone()
}
fn output_schema(operation: &str) -> Value {
    let item = json!({"type":"object","required":["schema_version","id","member_id","name","category","seasons","purchase_price_minor","price_exponent","currency","size_label","status","favorite","created_at","updated_at"],"properties":{"schema_version":{"type":"integer","const":1},"id":{"type":"string"},"member_id":{"type":"string"},"name":{"type":"string"},"category":{"type":"string","enum":CATEGORIES},"seasons":{"type":"array","items":{"type":"string","enum":["spring","summer","autumn","winter"]}},"purchase_price_minor":{"type":["integer","null"],"minimum":0},"price_exponent":{"type":["integer","null"]},"currency":{"type":["string","null"]},"size_label":{"type":["string","null"]},"status":{"type":"string","enum":["active","archived","deleted"]},"favorite":{"type":"boolean"},"created_at":{"type":"string"},"updated_at":{"type":"string"}}});
    match operation {
        "clothing.list" => {
            json!({"type":"object","required":["schema_version","items","next_cursor"],"properties":{"schema_version":{"type":"integer","const":1},"items":{"type":"array","items":item},"next_cursor":{"type":["string","null"]}}})
        }
        _ => item,
    }
}
