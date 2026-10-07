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
pub const SUBCATEGORIES: &[(&str, &str)] = &[
    ("shirt", "top"),
    ("t_shirt", "top"),
    ("knitwear", "top"),
    ("hoodie", "top"),
    ("trousers", "bottom"),
    ("jeans", "bottom"),
    ("skirt", "bottom"),
    ("shorts", "bottom"),
    ("coat", "outerwear"),
    ("jacket", "outerwear"),
    ("blazer", "outerwear"),
    ("dress", "dress"),
    ("jumpsuit", "one_piece"),
    ("sneakers", "shoes"),
    ("boots", "shoes"),
    ("sandals", "shoes"),
    ("formal_shoes", "shoes"),
    ("handbag", "bag"),
    ("backpack", "bag"),
    ("jewelry", "accessory"),
    ("hat", "accessory"),
    ("scarf", "accessory"),
    ("belt", "accessory"),
    ("underwear", "underwear"),
    ("sportswear", "sportswear"),
    ("other", "other"),
];
pub const COLORS: &[&str] = &[
    "white",
    "black",
    "grey",
    "beige",
    "brown",
    "red",
    "orange",
    "yellow",
    "green",
    "blue",
    "purple",
    "pink",
    "silver",
    "gold",
    "multicolor",
    "other",
];
pub(super) fn taxonomy() -> Value {
    json!({"version":1,"sub_categories":SUBCATEGORIES.iter().map(|(id,category)|json!({"id":id,"category":category})).collect::<Vec<_>>(),"colors":COLORS})
}
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
        doc.map(|s| {
            serde_json::from_str(&s).map_err(|_| ApiError::new("storage_error", "单品数据无效"))
        })
        .transpose()?
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
        let status = optional_filter(&input, "status")?.unwrap_or("active");
        if !["active", "archived", "deleted"].contains(&status) {
            return Err(ApiError::new("invalid_input", "生命周期筛选无效"));
        }
        let sort = optional_filter(&input, "sort")?.unwrap_or("created_asc");
        let order=match sort {
            "created_asc"=>"id ASC", "created_desc"=>"id DESC",
            "name_asc"=>"json_extract(document,'$.name') COLLATE NOCASE ASC, id ASC",
            "updated_desc"=>"json_extract(document,'$.updated_at') DESC, id ASC",
            "price_asc"=>"json_extract(document,'$.purchase_price_minor') IS NULL ASC, json_extract(document,'$.currency') ASC, json_extract(document,'$.purchase_price_minor') ASC, id ASC",
            "price_desc"=>"json_extract(document,'$.purchase_price_minor') IS NULL ASC, json_extract(document,'$.currency') ASC, json_extract(document,'$.purchase_price_minor') DESC, id ASC",
            _=>return Err(ApiError::new("invalid_input", "排序无效"))
        };
        let mut conditions = vec![
            "member_id=?1".to_owned(),
            "coalesce(json_extract(document,'$.status'),'active')=?2".to_owned(),
        ];
        let mut bindings = vec![
            rusqlite::types::Value::Text(member.to_string()),
            rusqlite::types::Value::Text(status.to_owned()),
        ];
        for (key, field) in [
            ("category", "category"),
            ("sub_category", "sub_category"),
            ("brand", "brand"),
        ] {
            if let Some(value) = optional_filter(&input, key)? {
                bindings.push(value.to_owned().into());
                conditions.push(format!(
                    "json_extract(document,'$.{field}')=?{}",
                    bindings.len()
                ));
            }
        }
        for (key, field) in [
            ("season", "seasons"),
            ("color", "color"),
            ("pattern", "pattern"),
            ("material", "material"),
            ("style", "style"),
            ("occasion", "occasion"),
            ("tag", "tags"),
        ] {
            if let Some(value) = optional_filter(&input, key)? {
                bindings.push(value.to_owned().into());
                conditions.push(format!(
                    "EXISTS (SELECT 1 FROM json_each(document,'$.{field}') WHERE value=?{})",
                    bindings.len()
                ));
            }
        }
        if let Some(value) = input.get("favorite") {
            let boolean = value
                .as_bool()
                .ok_or_else(|| ApiError::new("invalid_input", "收藏筛选必须为布尔值"))?;
            bindings.push(i64::from(boolean).into());
            conditions.push(format!(
                "json_extract(document,'$.favorite')=?{}",
                bindings.len()
            ));
        }
        if let Some(search) = optional_filter(&input, "search")? {
            if search.chars().count() > 500 {
                return Err(ApiError::new("invalid_input", "搜索最多 500 字符"));
            }
            bindings.push(search.to_owned().into());
            let n = bindings.len();
            conditions.push(format!("(instr(lower(json_extract(document,'$.name')),lower(?{n}))>0 OR EXISTS (SELECT 1 FROM json_each(document) AS f WHERE f.key IN ('brand','description','notes','sub_category','color','pattern','material','style','occasion','tags') AND instr(lower(f.value),lower(?{n}))>0))"));
        }
        bindings.push(cursor.to_owned().into());
        let c = bindings.len();
        bindings.push((limit as i64 + 1).into());
        let l = bindings.len();
        let sql=format!("WITH filtered AS (SELECT id,document,row_number() OVER (ORDER BY {order}) AS position FROM clothing WHERE {}) SELECT document FROM filtered WHERE position>coalesce((SELECT position FROM filtered WHERE id=?{c}),0) ORDER BY position LIMIT ?{l}", conditions.join(" AND "));
        if !cursor.is_empty() {
            let mut lookup = bindings.clone();
            lookup.pop();
            lookup.pop();
            lookup.push(cursor.to_owned().into());
            let exists = self.connection.query_row(
                &format!(
                    "SELECT count(*) FROM clothing WHERE {} AND id=?{c}",
                    conditions.join(" AND ")
                ),
                rusqlite::params_from_iter(lookup),
                |r| r.get::<_, i64>(0),
            )?;
            if exists == 0 {
                return Err(ApiError::new("invalid_input", "分页游标不属于当前筛选结果"));
            }
        }
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement
            .query_map(rusqlite::params_from_iter(bindings), |r| {
                r.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|s| {
                serde_json::from_str::<Value>(&s)
                    .map_err(|_| ApiError::new("storage_error", "单品数据无效"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next = if more {
            rows.last().map(|r| r["id"].clone())
        } else {
            None
        };
        Ok(json!({"schema_version":1,"items":rows,"next_cursor":next}))
    }
    pub(super) fn clothing_history(&self, input: Value) -> Result<Value, ApiError> {
        let doc = self.clothing_get(input.clone())?;
        let limit = input
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .filter(|v| (1..=100).contains(v))
                    .ok_or_else(|| ApiError::new("invalid_input", "每页数量必须为 1–100"))
            })
            .transpose()?
            .unwrap_or(50);
        let cursor = input
            .get("cursor")
            .map(|v| {
                v.as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(|| ApiError::new("invalid_input", "历史游标无效"))
            })
            .transpose()?
            .unwrap_or(0);
        let mut statement=self.connection.prepare("SELECT id,operation,document FROM audit WHERE entity_id=?1 AND id>?2 ORDER BY id LIMIT ?3")?;
        let rows = statement
            .query_map(params![doc["id"].as_str(), cursor, limit + 1], |r| {
                Ok((
                    r.get::<_, u64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let more = rows.len() > limit as usize;
        let mut items = Vec::new();
        for (id, operation, serialized) in rows.into_iter().take(limit as usize) {
            let snapshot: Value = serde_json::from_str(&serialized)
                .map_err(|_| ApiError::new("storage_error", "历史数据无效"))?;
            items.push(json!({"id":id.to_string(),"operation":operation,"snapshot":snapshot}));
        }
        let next = if more {
            items.last().map(|item| item["id"].clone())
        } else {
            None
        };
        Ok(json!({"schema_version":1,"items":items,"next_cursor":next}))
    }
    pub(super) fn clothing_lifecycle(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<Value, ApiError> {
        let mut doc = self.clothing_get(input)?;
        let status = doc["status"].as_str().unwrap_or("active").to_owned();
        match operation {
            "clothing.delete" => {
                if status == "deleted" {
                    return Ok(doc);
                }
                doc["status_before_delete"] = json!(status);
                doc["status"] = json!("deleted");
                doc["deleted_at"] = json!(chrono::Utc::now().to_rfc3339());
            }
            "clothing.restore" => {
                if status != "deleted" {
                    return Err(ApiError::new("conflict", "单品不在回收站"));
                }
                doc["status"] = if doc["status_before_delete"] == "archived" {
                    json!("archived")
                } else {
                    json!("active")
                };
                doc["deleted_at"] = Value::Null;
            }
            "clothing.archive" | "clothing.activate" => {
                if status == "deleted" {
                    return Err(ApiError::new("conflict", "请先从回收站恢复单品"));
                }
                doc["status"] = json!(if operation == "clothing.archive" {
                    "archived"
                } else {
                    "active"
                });
            }
            _ => unreachable!(),
        }
        self.save_clothing(doc, operation)
    }
    pub(super) fn clothing_update(&self, input: Value) -> Result<Value, ApiError> {
        let mut doc = self.clothing_get(input.clone())?;
        if doc["status"] == "deleted" {
            return Err(ApiError::new("conflict", "请先从回收站恢复单品"));
        }
        apply_facts(&mut doc, &input)?;
        self.save_clothing(doc, "clothing.update")
    }
    fn save_clothing(&self, mut doc: Value, operation: &str) -> Result<Value, ApiError> {
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        let tx = self.begin_write()?;
        let serialized = doc.to_string();
        tx.execute(
            "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
            params![serialized, doc["id"].as_str(), doc["member_id"].as_str()],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES (?1,?2,?3)",
            params![doc["id"].as_str(), operation, serialized],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES (?1,?2)",
            params![doc["id"].as_str(), serialized],
        )?;
        tx.commit()?;
        Ok(doc)
    }
    pub(super) fn clothing_document(&self, input: Value) -> Result<Value, ApiError> {
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
        let mut doc = json!({"schema_version":1,"id":id,"member_id":member,"name":name,"category":category,"seasons":checked,"purchase_price_minor":price,"price_exponent":price_exponent,"currency":currency,"size_label":null,"status":"active","favorite":false,"created_at":now,"updated_at":now});
        apply_facts(&mut doc, &input)?;
        Ok(doc)
    }
    pub(super) fn clothing_create(&self, input: Value) -> Result<Value, ApiError> {
        let doc = self.clothing_document(input)?;
        let id = doc["id"].as_str();
        let member = doc["member_id"].as_str();
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
fn optional_filter<'a>(input: &'a Value, key: &str) -> Result<Option<&'a str>, ApiError> {
    input
        .get(key)
        .map(|v| {
            v.as_str()
                .ok_or_else(|| ApiError::new("invalid_input", &format!("{key} 筛选必须为字符串")))
        })
        .transpose()
}
const TEXT_FACTS: &[&str] = &[
    "sub_category",
    "brand",
    "size_label",
    "fit",
    "description",
    "notes",
    "purchase_date",
    "purchase_store",
    "purchase_url",
    "color_description",
];
const ARRAY_FACTS: &[&str] = &["color", "pattern", "material", "style", "occasion", "tags"];
pub(super) fn validate_stored_document(document: &Value) -> Result<(), ApiError> {
    let object = document
        .as_object()
        .ok_or_else(|| ApiError::new("invalid_input", "单品文档须为对象"))?;
    if document["schema_version"] != 1
        || !document["name"].is_string()
        || !document["category"].is_string()
        || !document["status"]
            .as_str()
            .is_some_and(|status| ["active", "archived", "deleted"].contains(&status))
    {
        return Err(ApiError::new("invalid_input", "单品格式或生命周期无效"));
    }
    let facts = Value::Object(
        object
            .iter()
            .filter(|(key, _)| {
                TEXT_FACTS.contains(&key.as_str())
                    || ARRAY_FACTS.contains(&key.as_str())
                    || [
                        "name",
                        "category",
                        "seasons",
                        "favorite",
                        "purchase_price_minor",
                        "currency",
                    ]
                    .contains(&key.as_str())
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    );
    let mut validated = document.clone();
    apply_facts(&mut validated, &facts)?;
    if document.get("price_exponent").is_some()
        && document["price_exponent"] != validated["price_exponent"]
    {
        return Err(ApiError::new("invalid_input", "货币单位与记录不一致"));
    }
    Ok(())
}
pub(super) fn apply_facts(doc: &mut Value, input: &Value) -> Result<(), ApiError> {
    for key in input.as_object().unwrap().keys() {
        if !TEXT_FACTS.contains(&key.as_str())
            && !ARRAY_FACTS.contains(&key.as_str())
            && ![
                "member_id",
                "id",
                "name",
                "category",
                "seasons",
                "favorite",
                "purchase_price_minor",
                "currency",
            ]
            .contains(&key.as_str())
        {
            return Err(ApiError::new(
                "invalid_input",
                &format!("不支持修改字段 {key}"),
            ));
        }
    }

    for key in TEXT_FACTS {
        if let Some(value) = input.get(*key) {
            let max = if *key == "notes" || *key == "description" {
                10000
            } else {
                500
            };
            if !value.is_null() && !value.as_str().is_some_and(|s| s.chars().count() <= max) {
                return Err(ApiError::new(
                    "invalid_input",
                    &format!("{key} 必须为文本且最多 {max} 个字符"),
                ));
            }
            if *key == "purchase_date" && !value.is_null() {
                let date = value.as_str().unwrap();
                if date.len() != 10 || chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err()
                {
                    return Err(ApiError::new(
                        "invalid_input",
                        "购入日期须为有效 YYYY-MM-DD",
                    ));
                }
            }
            doc[*key] = value.clone();
        } else if doc.get(*key).is_none() {
            doc[*key] = Value::Null;
        }
    }
    for key in ARRAY_FACTS {
        if let Some(value) = input.get(*key) {
            let values = value
                .as_array()
                .ok_or_else(|| ApiError::new("invalid_input", "属性必须为数组"))?;
            if values.len() > 100
                || values.iter().any(|v| {
                    !v.as_str()
                        .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 120)
                })
            {
                return Err(ApiError::new(
                    "invalid_input",
                    "属性最多 100 项，每项 1–120 字符",
                ));
            }
            let mut unique = Vec::new();
            let mut keys = Vec::new();
            for v in values {
                let text = v.as_str().unwrap().trim();
                let normalized = text.to_lowercase();
                if !keys.contains(&normalized) {
                    keys.push(normalized);
                    unique.push(json!(text));
                }
            }
            doc[*key] = json!(unique);
        } else if doc.get(*key).is_none() {
            doc[*key] = json!([]);
        }
    }
    if let Some(value) = input.get("name") {
        let name = value.as_str().unwrap_or("").trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(ApiError::new("invalid_input", "名称须为 1–120 个字符"));
        }
        doc["name"] = json!(name);
    }
    if let Some(value) = input.get("category") {
        if !value.as_str().is_some_and(|s| CATEGORIES.contains(&s)) {
            return Err(ApiError::new("invalid_input", "请选择有效类别"));
        }
        doc["category"] = value.clone();
    }
    if !doc["sub_category"].is_null()
        && !SUBCATEGORIES
            .iter()
            .any(|(id, category)| doc["sub_category"] == *id && doc["category"] == *category)
    {
        return Err(ApiError::new("invalid_input", "小类不属于所选类别"));
    }
    if doc["color"].as_array().is_some_and(|values| {
        values
            .iter()
            .any(|v| !v.as_str().is_some_and(|id| COLORS.contains(&id)))
    }) {
        return Err(ApiError::new(
            "invalid_input",
            "颜色须使用稳定颜色 ID；其他颜色可补充描述",
        ));
    }
    doc["taxonomy_version"] = json!(1);
    if let Some(value) = input.get("seasons") {
        let values = value
            .as_array()
            .ok_or_else(|| ApiError::new("invalid_input", "季节必须为数组"))?;
        if values.iter().any(|s| {
            !s.as_str()
                .is_some_and(|v| ["spring", "summer", "autumn", "winter"].contains(&v))
        }) {
            return Err(ApiError::new("invalid_input", "季节无效"));
        }
        doc["seasons"] = json!(["spring", "summer", "autumn", "winter"]
            .into_iter()
            .filter(|s| values.contains(&json!(s)))
            .collect::<Vec<_>>());
    }
    if let Some(value) = input.get("favorite") {
        if !value.is_boolean() {
            return Err(ApiError::new("invalid_input", "收藏必须为布尔值"));
        }
        doc["favorite"] = value.clone();
    }
    for key in ["purchase_price_minor", "currency"] {
        if let Some(value) = input.get(key) {
            doc[key] = value.clone();
        }
    }
    if !doc["purchase_price_minor"].is_null()
        && !doc["purchase_price_minor"]
            .as_u64()
            .is_some_and(|v| v <= 9007199254740991)
    {
        return Err(ApiError::new(
            "invalid_input",
            "价格须为安全范围内非负最小货币单位整数",
        ));
    }
    let currency = doc["currency"]
        .as_str()
        .and_then(iso_currency::Currency::from_code);
    if (!doc["currency"].is_null() && currency.is_none())
        || (!doc["purchase_price_minor"].is_null() && currency.is_none())
    {
        return Err(ApiError::new("invalid_input", "价格需要有效 ISO 币种"));
    }
    let exponent = currency.and_then(|c| c.exponent());
    if !doc["purchase_price_minor"].is_null() && exponent.is_none() {
        return Err(ApiError::new(
            "invalid_input",
            "该币种没有可用的最小货币单位",
        ));
    }
    doc["price_exponent"] = json!(exponent);
    Ok(())
}
pub(super) fn descriptors() -> Vec<Value> {
    let mut descriptors = json!([
        {"name":"clothing.create","description":"保存无图单品","inputSchema":{"type":"object","required":["member_id","name","category"],"properties":{"member_id":{"type":"string"},"name":{"type":"string","minLength":1,"maxLength":120},"category":{"type":"string","enum":CATEGORIES},"seasons":{"type":"array","items":{"type":"string","enum":["spring","summer","autumn","winter"]}},"purchase_price_minor":{"type":["integer","null"],"minimum":0},"currency":{"type":["string","null"]}}}},
        {"name":"clothing.get","description":"读取成员单品","inputSchema":{"type":"object","required":["member_id","id"],"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}}},
        {"name":"clothing.list","description":"分页浏览成员单品","inputSchema":{"type":"object","required":["member_id"],"properties":{"member_id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100},"cursor":{"type":"string"}}}},
    ]);
    let mut history = descriptors[1].clone();
    history["name"] = json!("clothing.history");
    history["description"] = json!("分页读取单品事实变更快照");
    history["inputSchema"]["properties"]["limit"] =
        json!({"type":"integer","minimum":1,"maximum":100});
    history["inputSchema"]["properties"]["cursor"] = json!({"type":"string"});
    descriptors.as_array_mut().unwrap().push(history);
    for name in [
        "clothing.archive",
        "clothing.activate",
        "clothing.delete",
        "clothing.restore",
    ] {
        let mut descriptor = descriptors[1].clone();
        descriptor["name"] = json!(name);
        descriptor["description"] = json!(match name {
            "clothing.archive" => "归档单品",
            "clothing.activate" => "取消归档",
            "clothing.delete" => "移入回收站，保留事实",
            _ => "从回收站恢复",
        });
        descriptors.as_array_mut().unwrap().push(descriptor);
    }
    let mut update = descriptors[0].clone();
    update["name"] = json!("clothing.update");
    update["description"] = json!("局部编辑单品事实，保留未提供字段");
    update["inputSchema"]["required"] = json!(["member_id", "id"]);
    update["inputSchema"]["properties"]["id"] = json!({"type":"string"});
    descriptors.as_array_mut().unwrap().push(update);
    for descriptor in descriptors.as_array_mut().unwrap() {
        if descriptor["name"] == "clothing.list" {
            for key in [
                "status",
                "search",
                "sort",
                "category",
                "sub_category",
                "brand",
                "season",
                "color",
                "pattern",
                "material",
                "style",
                "occasion",
                "tag",
            ] {
                descriptor["inputSchema"]["properties"][key] = json!({"type":"string"});
            }
            descriptor["inputSchema"]["properties"]["status"]["enum"] =
                json!(["active", "archived", "deleted"]);
            descriptor["inputSchema"]["properties"]["sort"]["enum"] = json!([
                "created_asc",
                "created_desc",
                "name_asc",
                "updated_desc",
                "price_asc",
                "price_desc"
            ]);
            descriptor["inputSchema"]["properties"]["favorite"] = json!({"type":"boolean"});
        }
        if ["clothing.create", "clothing.update"].contains(&descriptor["name"].as_str().unwrap()) {
            descriptor["inputSchema"]["additionalProperties"] = json!(false);
            descriptor["inputSchema"]["properties"]["purchase_price_minor"]["maximum"] =
                json!(9007199254740991u64);
            for key in TEXT_FACTS {
                descriptor["inputSchema"]["properties"][*key] = json!({"type":["string","null"],"maxLength":if *key=="notes"||*key=="description"{10000}else{500}});
            }
            for key in ARRAY_FACTS {
                descriptor["inputSchema"]["properties"][*key] = json!({"type":"array","maxItems":100,"items":{"type":"string","minLength":1,"maxLength":120}});
            }
            descriptor["inputSchema"]["properties"]["favorite"] = json!({"type":"boolean"});
        }
        descriptor["outputSchema"] = output_schema(descriptor["name"].as_str().unwrap());
    }
    descriptors.as_array().unwrap().clone()
}
pub(super) fn output_schema(operation: &str) -> Value {
    let mut item = json!({"type":"object","required":["schema_version","id","member_id","name","category","seasons","purchase_price_minor","price_exponent","currency","size_label","status","favorite","created_at","updated_at"],"properties":{"schema_version":{"type":"integer","const":1},"id":{"type":"string"},"member_id":{"type":"string"},"name":{"type":"string"},"category":{"type":"string","enum":CATEGORIES},"seasons":{"type":"array","items":{"type":"string","enum":["spring","summer","autumn","winter"]}},"purchase_price_minor":{"type":["integer","null"],"minimum":0},"price_exponent":{"type":["integer","null"]},"currency":{"type":["string","null"]},"size_label":{"type":["string","null"]},"status":{"type":"string","enum":["active","archived","deleted"]},"favorite":{"type":"boolean"},"created_at":{"type":"string"},"updated_at":{"type":"string"}}});
    for key in TEXT_FACTS {
        item["properties"][*key] = json!({"type":["string","null"]});
    }
    for key in ARRAY_FACTS {
        item["properties"][*key] = json!({"type":"array","items":{"type":"string"}});
    }
    item["properties"]["asset_ids"] =
        json!({"type":"array","maxItems":20,"uniqueItems":true,"items":{"type":"string"}});
    item["properties"]["display_asset_id"] = json!({"type":["string","null"]});
    item["properties"]["deleted_at"] = json!({"type":["string","null"]});
    match operation {
        "clothing.list" => {
            json!({"type":"object","required":["schema_version","items","next_cursor"],"properties":{"schema_version":{"type":"integer","const":1},"items":{"type":"array","items":item},"next_cursor":{"type":["string","null"]}}})
        }
        "clothing.history" => {
            json!({"type":"object","required":["schema_version","items","next_cursor"],"properties":{"schema_version":{"type":"integer","const":1},"items":{"type":"array","items":{"type":"object","required":["id","operation","snapshot"],"properties":{"id":{"type":"string"},"operation":{"type":"string"},"snapshot":item}}},"next_cursor":{"type":["string","null"]}}})
        }
        _ => item,
    }
}
