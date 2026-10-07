use crate::{string, ApiError, Application};
use rusqlite::params;
use serde_json::{json, Value};
fn invalid(s: &str) -> ApiError {
    ApiError::new("invalid_input", s)
}
fn parse(s: String) -> Result<Value, ApiError> {
    serde_json::from_str(&s).map_err(|_| ApiError::new("storage_error", "推荐意图损坏"))
}
impl Application {
    pub(super) fn pairing_execute(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        if op == "pairing.list" {
            return self.pairing_list(input);
        }
        if op != "pairing.generate" {
            return Err(ApiError::new("unsupported_operation", "配对操作不可用"));
        }
        // Closed public input: anchor plus ordinary shared recommendation controls
        // only; strategy stays a recommendation.generate concern so this slice is
        // rules-only.
        crate::recommendations::fields(
            &input,
            &[
                "member_id",
                "operation_key",
                "anchor_id",
                "timezone",
                "limit",
                "occasion",
                "weather",
                "target",
                "avoid_recent",
                "excluded_ids",
                "inventory_priority_ids",
                "fixed_ids",
            ],
        )?;
        let anchor = string(&input, "anchor_id")?.to_owned();
        if anchor.is_empty() {
            return Err(invalid("配对锚点不能为空"));
        }
        // Translate the anchor into the explicit validated pairing control and keep
        // it in fixed_ids alongside any caller-fixed IDs; the shared
        // recommendation.generate path then owns atomic request/replay/write and all
        // existing fixed-ID/member/care/lifecycle/coverage validation.
        let mut fixed = crate::recommendations::ids(&input, "fixed_ids")?;
        if !fixed.iter().any(|id| id == &anchor) {
            fixed.push(anchor.clone());
        }
        let mut controls = input;
        let object = controls.as_object_mut().unwrap();
        object.remove("anchor_id");
        object.insert("pairing_anchor_id".into(), json!(anchor));
        object.insert("fixed_ids".into(), json!(fixed));
        self.recommendation_execute("recommendation.generate", controls)
    }
    // Bounded durable read of recognized pairing intents. The pairing marker is
    // the document-level object written atomically by the shared generator, so
    // history stays readable even when the anchor no longer exists or is
    // currently unavailable: this query never touches inventory or care state.
    // SQL LIMIT bounds the returned page and its in-memory validation. Database
    // scan/sort work still depends on history size and requires separate scale
    // measurement; keyset pagination alone does not prove bounded scan cost.
    fn pairing_list(&self, input: Value) -> Result<Value, ApiError> {
        crate::recommendations::fields(&input, &["member_id", "anchor_id", "limit", "cursor"])?;
        let member = self.member(&input)?;
        let anchor = input
            .get("anchor_id")
            .map(|v| {
                v.as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| invalid("配对锚点须为非空文字"))
            })
            .transpose()?;
        let limit = input
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .filter(|n| (1..=100).contains(n))
                    .ok_or_else(|| invalid("每页数量须为1–100"))
            })
            .transpose()?
            .unwrap_or(24);
        let cursor = input
            .get("cursor")
            .map(|v| {
                v.as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| invalid("游标须为非空文字"))
            })
            .transpose()?;
        // A cursor is the last returned intent ID; it is only valid when that
        // row still satisfies this exact member/filter/marker query. Mismatched,
        // foreign or invented cursors all fail identically without leaking why.
        if let Some(cursor) = cursor {
            let recognized: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM recommendation_intents WHERE member_id=?1 AND id=?2 AND json_extract(document,'$.pairing.schema_version')=1 AND (?3 IS NULL OR json_extract(document,'$.pairing.anchor_id')=?3))",
                params![member, cursor, anchor],
                |r| r.get(0),
            )?;
            if !recognized {
                return Err(invalid("游标不属于当前配对查询"));
            }
        }
        let mut st = self.connection.prepare(
            "SELECT document FROM recommendation_intents WHERE member_id=?1 AND json_extract(document,'$.pairing.schema_version')=1 AND (?2 IS NULL OR json_extract(document,'$.pairing.anchor_id')=?2) AND (?3 IS NULL OR id<?3) ORDER BY id DESC LIMIT ?4",
        )?;
        let mut items: Vec<Value> = st
            .query_map(params![member, anchor, cursor, limit + 1], |r| {
                r.get::<_, String>(0)
            })?
            .map(|row| parse(row?))
            .collect::<Result<Vec<_>, ApiError>>()?;
        // A listed document is only honest when its durable marker matches the
        // controls that produced it; anything else is storage damage, never an
        // empty page.
        for doc in &items {
            let marker = doc["pairing"]["anchor_id"]
                .as_str()
                .ok_or_else(|| ApiError::new("storage_error", "配对意图标记损坏"))?;
            let controls = &doc["controls"];
            if controls["pairing_anchor_id"] != json!(marker)
                || !controls["fixed_ids"]
                    .as_array()
                    .is_some_and(|fixed| fixed.iter().any(|id| id == marker))
            {
                return Err(ApiError::new("storage_error", "配对意图标记损坏"));
            }
        }
        let more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next = if more {
            items.last().map(|doc| doc["id"].clone())
        } else {
            None
        };
        Ok(json!({"schema_version":1,"member_id":member,"items":items,"next_cursor":next}))
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    vec![
        json!({"name":"pairing.generate","description":"本地离线单品配对：以一件固定锚点生成规则搭配意图并原子持久化；不记录穿着，不产生穿着事实","inputSchema":{"type":"object","required":["member_id","operation_key","anchor_id","timezone"],"properties":{"member_id":{"type":"string"},"operation_key":{"type":"string","minLength":1,"maxLength":200},"anchor_id":{"type":"string","minLength":1},"timezone":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":5},"occasion":{"enum":["daily","work","formal","sport"]},"weather":{"type":"object","required":["mode"],"additionalProperties":false,"properties":{"mode":{"enum":["off","manual","automatic"]},"temperature_c":{"type":"number","minimum":-80,"maximum":60},"location":{"type":"string","maxLength":120},"obtained_at":{"type":"string","format":"date-time"}}},"target":{"enum":["balanced","least_worn","favorites"]},"avoid_recent":{"type":"boolean"},"excluded_ids":{"type":"array","items":{"type":"string"},"maxItems":100,"uniqueItems":true},"inventory_priority_ids":{"type":"array","items":{"type":"string"},"maxItems":100,"uniqueItems":true},"fixed_ids":{"type":"array","items":{"type":"string"},"maxItems":100,"uniqueItems":true}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id","id","proposals","pairing","creates_wear"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"id":{"type":"string"},"created_at":{"type":"string"},"updated_at":{"type":"string"},"controls":{"type":"object"},"requested_count":{"type":"integer"},"actual_count":{"type":"integer"},"proposals":{"type":"array"},"pairing":{"type":"object","required":["schema_version","anchor_id"],"properties":{"schema_version":{"const":1},"anchor_id":{"type":"string"}},"additionalProperties":false},"creates_wear":{"const":false},"shortage_reasons":{"type":"array"}}}}),
        json!({"name":"pairing.list","description":"本地离线分页读取成员配对历史：仅返回带持久配对标记的真实推荐意图完整文档，可按锚点筛选，按意图ID倒序键集分页；不读穿着事实，不产生网络调用","inputSchema":{"type":"object","required":["member_id"],"properties":{"member_id":{"type":"string"},"anchor_id":{"type":"string","minLength":1},"limit":{"type":"integer","minimum":1,"maximum":100,"default":24},"cursor":{"type":"string","minLength":1}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id","items","next_cursor"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"items":{"type":"array","items":{"type":"object","required":["schema_version","id","member_id","proposals","pairing"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"created_at":{"type":"string"},"updated_at":{"type":"string"},"controls":{"type":"object"},"proposals":{"type":"array"},"pairing":{"type":"object","required":["schema_version","anchor_id"],"properties":{"schema_version":{"const":1},"anchor_id":{"type":"string"}},"additionalProperties":false},"creates_wear":{"const":false}}}},"next_cursor":{"type":["string","null"]}}}}),
    ]
}
