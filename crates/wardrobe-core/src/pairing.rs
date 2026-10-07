use crate::{string, ApiError, Application};
use serde_json::{json, Value};
fn invalid(s: &str) -> ApiError {
    ApiError::new("invalid_input", s)
}
impl Application {
    pub(super) fn pairing_execute(&self, op: &str, input: Value) -> Result<Value, ApiError> {
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
}
pub(super) fn descriptors() -> Vec<Value> {
    vec![
        json!({"name":"pairing.generate","description":"本地离线单品配对：以一件固定锚点生成规则搭配意图并原子持久化；不记录穿着，不产生穿着事实","inputSchema":{"type":"object","required":["member_id","operation_key","anchor_id","timezone"],"properties":{"member_id":{"type":"string"},"operation_key":{"type":"string","minLength":1,"maxLength":200},"anchor_id":{"type":"string","minLength":1},"timezone":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":5},"occasion":{"enum":["daily","work","formal","sport"]},"weather":{"type":"object","required":["mode"],"additionalProperties":false,"properties":{"mode":{"enum":["off","manual","automatic"]},"temperature_c":{"type":"number","minimum":-80,"maximum":60},"location":{"type":"string","maxLength":120},"obtained_at":{"type":"string","format":"date-time"}}},"target":{"enum":["balanced","least_worn","favorites"]},"avoid_recent":{"type":"boolean"},"excluded_ids":{"type":"array","items":{"type":"string"},"maxItems":100,"uniqueItems":true},"inventory_priority_ids":{"type":"array","items":{"type":"string"},"maxItems":100,"uniqueItems":true},"fixed_ids":{"type":"array","items":{"type":"string"},"maxItems":100,"uniqueItems":true}},"additionalProperties":false},"outputSchema":{"type":"object","required":["schema_version","member_id","id","proposals","pairing","creates_wear"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"id":{"type":"string"},"created_at":{"type":"string"},"updated_at":{"type":"string"},"controls":{"type":"object"},"requested_count":{"type":"integer"},"actual_count":{"type":"integer"},"proposals":{"type":"array"},"pairing":{"type":"object","required":["schema_version","anchor_id"],"properties":{"schema_version":{"const":1},"anchor_id":{"type":"string"}},"additionalProperties":false},"creates_wear":{"const":false},"shortage_reasons":{"type":"array"}}}}),
    ]
}
