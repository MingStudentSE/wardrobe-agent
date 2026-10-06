use super::{string, ApiError, Application};
use serde_json::{json, Value};
impl Application {
    pub(super) fn default_space(&self) -> Result<Value, ApiError> {
        self.connection.execute(
            "INSERT OR IGNORE INTO members VALUES ('default', '我的衣橱')",
            [],
        )?;
        Ok(json!({"schema_version":1,"space_id":"local","member_id":"default","name":"我的衣橱"}))
    }
    pub(super) fn member<'a>(&self, input: &'a Value) -> Result<&'a str, ApiError> {
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
}
pub(super) fn descriptors() -> Vec<Value> {
    let mut descriptor = json!(
    {"name":"space.default","description":"创建或读取默认本地空间","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}
        );
    descriptor["outputSchema"] = json!({"type":"object","required":["schema_version","space_id","member_id","name"],"properties":{"schema_version":{"type":"integer","const":1},"space_id":{"type":"string"},"member_id":{"type":"string"},"name":{"type":"string"}}});
    vec![descriptor]
}
