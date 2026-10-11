use super::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS workspace (id TEXT PRIMARY KEY, default_member_id TEXT NOT NULL REFERENCES members(id), selected_member_id TEXT NOT NULL REFERENCES members(id));")?;
    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM workspace)", [], |r| r.get(0))?;
    if exists {
        return Ok(());
    }
    let member_id = uuid::Uuid::now_v7().to_string();
    let space_id = uuid::Uuid::now_v7().to_string();
    let legacy_name: Option<String> = tx
        .query_row("SELECT name FROM members WHERE id='default'", [], |r| {
            r.get(0)
        })
        .optional()?;
    let default_name = legacy_name
        .filter(|name| name != "我的衣橱")
        .unwrap_or_else(|| "我".into());
    tx.execute(
        "INSERT INTO members(id,name) VALUES(?1,?2)",
        params![member_id, default_name],
    )?;
    tx.execute(
        "UPDATE clothing SET member_id=?1 WHERE member_id='default'",
        [&member_id],
    )?;
    for table in ["clothing", "audit", "outbox"] {
        let entries = {
            let mut statement = tx.prepare(&format!("SELECT rowid,document FROM {table}"))?;
            let result = statement
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            result
        };
        for (rowid, serialized) in entries {
            let mut document: Value = serde_json::from_str(&serialized)
                .map_err(|_| ApiError::new("storage_error", "迁移遇到无效数据，未修改数据库"))?;
            if document.get("member_id") == Some(&json!("default")) {
                document["member_id"] = json!(member_id);
                tx.execute(
                    &format!("UPDATE {table} SET document=?1 WHERE rowid=?2"),
                    params![document.to_string(), rowid],
                )?;
            }
        }
    }
    tx.execute("DELETE FROM members WHERE id='default'", [])?;
    tx.execute(
        "INSERT INTO workspace VALUES(?1,?2,?2)",
        params![space_id, member_id],
    )?;
    Ok(())
}
fn name(input: &Value) -> Result<&str, ApiError> {
    let name = string(input, "name")?.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(ApiError::new("invalid_input", "成员名称须为 1–80 个字符"));
    }
    Ok(name)
}
impl Application {
    pub(super) fn default_space(&self) -> Result<Value, ApiError> {
        Ok(self.connection.query_row("SELECT w.id,m.id,m.name,w.selected_member_id FROM workspace w JOIN members m ON m.id=w.default_member_id",[],|r|Ok(json!({"schema_version":1,"space_id":r.get::<_,String>(0)?,"member_id":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,"selected_member_id":r.get::<_,String>(3)?})))?)
    }
    pub(super) fn member(&self, input: &Value) -> Result<String, ApiError> {
        let requested = string(input, "member_id")?;
        let member = if requested == "default" {
            self.connection
                .query_row("SELECT default_member_id FROM workspace", [], |r| {
                    r.get::<_, String>(0)
                })?
        } else {
            requested.to_owned()
        };
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM members WHERE id=?1)",
            [&member],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(ApiError::new("not_found", "成员不存在"));
        }
        Ok(member)
    }
    pub(super) fn member_list(&self) -> Result<Value, ApiError> {
        let mut stmt = self
            .connection
            .prepare("SELECT id,name FROM members ORDER BY rowid")?;
        let items=stmt.query_map([],|r|Ok(json!({"schema_version":1,"member_id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?})))?.collect::<Result<Vec<_>,_>>()?;
        let selected: String =
            self.connection
                .query_row("SELECT selected_member_id FROM workspace", [], |r| r.get(0))?;
        Ok(json!({"schema_version":1,"items":items,"selected_member_id":selected}))
    }
    pub(super) fn member_create(&self, input: Value) -> Result<Value, ApiError> {
        let name = name(&input)?;
        let id = uuid::Uuid::now_v7().to_string();
        let tx = self.begin_write()?;
        tx.execute(
            "INSERT INTO members(id,name) VALUES(?1,?2)",
            params![id, name],
        )?;
        tx.commit()?;
        Ok(json!({"schema_version":1,"member_id":id,"name":name}))
    }
    pub(super) fn member_update(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let name = name(&input)?;
        let tx = self.begin_write()?;
        tx.execute(
            "UPDATE members SET name=?1 WHERE id=?2",
            params![name, member],
        )?;
        tx.commit()?;
        Ok(json!({"schema_version":1,"member_id":member,"name":name}))
    }
    pub(super) fn member_select(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let tx = self.begin_write()?;
        tx.execute("UPDATE workspace SET selected_member_id=?1", [&member])?;
        tx.commit()?;
        Ok(json!({"schema_version":1,"member_id":member}))
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    let member = json!({"type":"object","required":["schema_version","member_id","name"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"name":{"type":"string"}}});
    let examples = json!({
        "space.default": [{"request":{},"prerequisites":["首次调用将初始化工作空间与默认成员"]}],
        "member.list": [{"request":{},"prerequisites":["space.default 之后调用；受限授权仅返回授权成员"]}],
        "member.create": [{"request":{"name":"<成员名称>"},"prerequisites":["名称为1-80字符"]}],
        "member.update": [{"request":{"member_id":"<成员ID>","name":"<新名称>"},"prerequisites":["成员ID来自 member.list 或 space.default"]}],
        "member.select": [{"request":{"member_id":"<成员ID>"},"prerequisites":["切换工作空间选中成员"]}],
    });
    let mut result = vec![];
    for (operation, description, required, properties, output) in [
        (
            "space.default",
            "读取默认空间",
            json!([]),
            json!({}),
            json!({"type":"object","required":["schema_version","space_id","member_id","name","selected_member_id"],"properties":{"schema_version":{"const":1},"space_id":{"type":"string"},"member_id":{"type":"string"},"name":{"type":"string"},"selected_member_id":{"type":"string"}}}),
        ),
        (
            "member.list",
            "列出独立成员档案",
            json!([]),
            json!({}),
            json!({"type":"object","required":["schema_version","items","selected_member_id"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":member},"selected_member_id":{"type":"string"}}}),
        ),
        (
            "member.create",
            "创建独立成员档案",
            json!(["name"]),
            json!({"name":{"type":"string","minLength":1,"maxLength":80}}),
            member.clone(),
        ),
        (
            "member.update",
            "编辑成员名称",
            json!(["member_id", "name"]),
            json!({"member_id":{"type":"string"},"name":{"type":"string","minLength":1,"maxLength":80}}),
            member,
        ),
        (
            "member.select",
            "切换当前成员",
            json!(["member_id"]),
            json!({"member_id":{"type":"string"}}),
            json!({"type":"object","required":["schema_version","member_id"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"}}}),
        ),
    ] {
        result.push(json!({"name":operation,"description":description,"inputSchema":{"type":"object","required":required,"properties":properties,"additionalProperties":false},"outputSchema":output}));
    }
    let mut collected = result;
    for descriptor in collected.iter_mut() {
        descriptor["examples"] = examples[descriptor["name"].as_str().unwrap()].clone();
    }
    collected
}
