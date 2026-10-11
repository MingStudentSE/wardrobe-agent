use crate::{jobs, string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::{io::Read, path::Path};
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS import_batches(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),operation_key TEXT NOT NULL,document TEXT NOT NULL,UNIQUE(member_id,operation_key)); CREATE TABLE IF NOT EXISTS import_drafts(id TEXT PRIMARY KEY,batch_id TEXT NOT NULL REFERENCES import_batches(id),member_id TEXT NOT NULL REFERENCES members(id),document TEXT NOT NULL);")?;
    Ok(())
}
impl Application {
    fn import_source(&self, path: &Path) -> Result<String, ApiError> {
        let meta = std::fs::metadata(path)
            .map_err(|_| ApiError::new("source_unavailable", "无法读取源文件，请恢复文件后重试"))?;
        if !meta.is_file() || meta.len() > 50 * 1024 * 1024 {
            return Err(ApiError::new("image_too_large", "文件须不超过50MiB"));
        }
        let mut bytes = vec![];
        std::fs::File::open(path)
            .map_err(|_| ApiError::new("source_unavailable", "源文件不可读"))?
            .take(50 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ApiError::new("source_unavailable", "源文件读取失败"))?;
        if bytes.len() > 50 * 1024 * 1024 {
            return Err(ApiError::new("image_too_large", "文件超过50MiB"));
        }
        let (_, local) = self.store_blob("originals-local-only", &bytes, "original")?;
        Ok(local)
    }
    pub(super) fn imports_create(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(ApiError::new("invalid_input", "操作标识须为1–200字节"));
        }
        let paths = input["paths"]
            .as_array()
            .ok_or_else(|| ApiError::new("invalid_input", "请选择文件列表"))?;
        if paths.is_empty() || paths.len() > 100 || paths.iter().any(|p| p.as_str().is_none()) {
            return Err(ApiError::new("invalid_input", "每批须为1–100个文件路径"));
        }
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM import_batches WHERE member_id=?1 AND operation_key=?2",
                params![member, key],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(stored) = existing {
            let doc: Value = serde_json::from_str(&stored)
                .map_err(|_| ApiError::new("storage_error", "批次损坏"))?;
            if doc["paths"] != input["paths"] {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "同一操作标识不能替换文件列表",
                ));
            }
            return self.imports_get(json!({"member_id":member,"id":doc["id"]}));
        }
        let mut staged = vec![];
        for path in paths {
            let path = Path::new(path.as_str().unwrap());
            let prepared = self.import_source(path);
            staged.push((
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                prepared,
            ));
        }
        let tx = self.begin_write()?; // Recheck operation identity under cross-process lock before inserting jobs.
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM import_batches WHERE member_id=?1 AND operation_key=?2",
                params![member, key],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            tx.commit()?;
            return self.imports_create(input).map(|doc| {
                debug_assert_eq!(doc["id"], id);
                doc
            });
        }
        let id = uuid::Uuid::now_v7().to_string();
        let mut ids = vec![];
        let mut pending_events: Vec<crate::jobs::PendingJobEvent> = vec![];
        for (index, (name, source)) in staged.into_iter().enumerate() {
            let payload = json!({"batch_id":id,"index":index,"filename":name,"source_path":if Path::new(paths[index].as_str().unwrap()).is_absolute(){Path::new(paths[index].as_str().unwrap()).to_path_buf()}else{std::env::current_dir().map_err(|_|ApiError::new("storage_error","无法定位当前文件目录"))?.join(paths[index].as_str().unwrap())},"local_path":source.as_ref().ok(),"asset_id":uuid::Uuid::now_v7().to_string()});
            let (mut job, enqueue) =
                self.insert_job(&tx, &member, "import", &format!("{id}/{index}"), payload)?;
            if let Some(enqueue) = enqueue {
                pending_events.push(enqueue);
            }
            if let Err(error) = source {
                job["state"] = json!("failed");
                job["error"] = json!({"code":error.code,"message":error.message,"retryable":false});
                if let Some(pending) = self.save_job(&tx, &job)? {
                    pending_events.push(pending);
                }
            }
            ids.push(job["id"].clone());
        }
        let doc = json!({"schema_version":1,"id":id,"job_ids":ids,"member_id":member,"operation_key":key,"paths":paths,"item_ids":ids,"created_at":chrono::Utc::now().to_rfc3339()});
        tx.execute(
            "INSERT INTO import_batches VALUES(?1,?2,?3,?4)",
            params![id, member, key, doc.to_string()],
        )?;
        for item_id in &ids {
            let draft_id = uuid::Uuid::now_v7().to_string();
            let draft = json!({"schema_version":1,"id":draft_id,"member_id":member,"batch_id":id,"item_ids":[item_id],"name":"未命名单品","category":"other","state":"draft","clothing_id":null,"commit_key":null});
            tx.execute(
                "INSERT INTO import_drafts VALUES(?1,?2,?3,?4)",
                params![draft_id, id, member, draft.to_string()],
            )?;
        }
        tx.commit()?;
        for pending in pending_events {
            self.emit_job_event(Some(pending));
        }
        self.imports_get(json!({"member_id":member,"id":id}))
    }
    pub(super) fn imports_get(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM import_batches WHERE member_id=?1 AND id=?2",
                params![member, string(&input, "id")?],
                |r| r.get(0),
            )
            .optional()?;
        let mut doc: Value =
            serde_json::from_str(&stored.ok_or_else(|| ApiError::new("not_found", "批次不存在"))?)
                .map_err(|_| ApiError::new("storage_error", "批次损坏"))?;
        let mut items = vec![];
        for id in doc["item_ids"].as_array().unwrap() {
            let job = self.job_get(json!({"member_id":member,"id":id}))?;
            let mut item = job.clone();
            item["filename"] = job["payload"]["filename"].clone();
            item["asset_id"] = job["result"]["asset_id"].clone();
            items.push(item);
        }
        doc["items"] = json!(items);
        let mut stmt = self.connection.prepare(
            "SELECT document FROM import_drafts WHERE batch_id=?1 AND member_id=?2 ORDER BY id",
        )?;
        let drafts = stmt
            .query_map(params![doc["id"].as_str(), member], |r| {
                r.get::<_, String>(0)
            })?
            .map(|r| serde_json::from_str(&r?).map_err(|_| rusqlite::Error::InvalidQuery))
            .collect::<Result<Vec<Value>, _>>()?;
        doc["drafts"] = json!(drafts);
        Ok(doc)
    }
    pub(super) fn imports_list(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let mut stmt = self
            .connection
            .prepare("SELECT id FROM import_batches WHERE member_id=?1 ORDER BY id DESC")?;
        let ids = stmt
            .query_map([&member], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut items = vec![];
        for id in ids {
            items.push(self.imports_get(json!({"member_id":member,"id":id}))?);
        }
        Ok(json!({"schema_version":1,"items":items}))
    }
    pub(super) fn imports_update(&self, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let batch = self.imports_get(input.clone())?;
        let member = batch["member_id"].as_str().unwrap();
        let draft_id = string(&input, "draft_id")?;
        let mut draft = batch["drafts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == draft_id)
            .cloned()
            .ok_or_else(|| ApiError::new("not_found", "草稿不存在"))?;
        if draft["state"] != "draft" {
            return Err(ApiError::new("invalid_state", "已入库草稿不能修改"));
        }
        let patch = input["facts"]
            .as_object()
            .ok_or_else(|| ApiError::new("invalid_input", "草稿事实须为对象"))?;
        let mut facts = draft.get("facts").cloned().unwrap_or(json!({}));
        facts
            .as_object_mut()
            .ok_or_else(|| ApiError::new("storage_error", "草稿事实损坏"))?
            .extend(patch.clone());
        let fields = facts
            .as_object_mut()
            .ok_or_else(|| ApiError::new("invalid_input", "草稿事实须为对象"))?;
        fields.insert("member_id".into(), json!(member));
        if !fields.contains_key("name") {
            fields.insert("name".into(), draft["name"].clone());
        }
        if !fields.contains_key("category") {
            fields.insert("category".into(), draft["category"].clone());
        }
        let checked = self.clothing_document(facts.clone())?;
        draft["name"] = checked["name"].clone();
        draft["category"] = checked["category"].clone();
        facts.as_object_mut().unwrap().remove("member_id");
        draft["facts"] = facts;
        tx.execute(
            "UPDATE import_drafts SET document=?1 WHERE id=?2",
            params![draft.to_string(), draft_id],
        )?;
        tx.commit()?;
        self.imports_get(input)
    }
    pub(super) fn imports_group(&self, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let batch = self.imports_get(input.clone())?;
        let member = batch["member_id"].as_str().unwrap();
        let ids = input["item_ids"]
            .as_array()
            .ok_or_else(|| ApiError::new("invalid_input", "请选择合并图片"))?;
        let unique: std::collections::HashSet<_> = ids.iter().filter_map(Value::as_str).collect();
        if ids.len() < 2 || ids.len() > 20 || unique.len() != ids.len() {
            return Err(ApiError::new(
                "invalid_input",
                "归并须选择2–20个不同的图片任务",
            ));
        }
        let mut sources = vec![];
        for draft in batch["drafts"].as_array().unwrap() {
            let members = draft["item_ids"].as_array().unwrap();
            if members.iter().any(|id| ids.contains(id)) {
                if draft["state"] != "draft" || members.iter().any(|id| !ids.contains(id)) {
                    return Err(ApiError::new("invalid_state", "请选择完整且尚未入库的草稿"));
                }
                sources.push(draft.clone());
            }
        }
        let mut all = vec![];
        for draft in &sources {
            all.extend(draft["item_ids"].as_array().unwrap().clone());
        }
        if all.len() != ids.len() {
            return Err(ApiError::new("not_found", "图片任务不属于当前批次"));
        }
        let mut target = sources
            .first()
            .cloned()
            .ok_or_else(|| ApiError::new("not_found", "草稿不存在"))?;
        target["item_ids"] = json!(ids);
        for source in &sources {
            tx.execute(
                "DELETE FROM import_drafts WHERE id=?1",
                [source["id"].as_str()],
            )?;
        }
        tx.execute(
            "INSERT INTO import_drafts VALUES(?1,?2,?3,?4)",
            params![
                target["id"].as_str(),
                batch["id"].as_str(),
                member,
                target.to_string()
            ],
        )?;
        tx.commit()?;
        self.imports_get(input)
    }
    pub(super) fn imports_commit(&self, input: Value) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let batch = self.imports_get(input.clone())?;
        let member = batch["member_id"].as_str().unwrap();
        let draft_id = string(&input, "draft_id")?;
        let key = string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 200 {
            return Err(ApiError::new("invalid_input", "入库操作标识须为1–200字节"));
        }
        let mut draft = batch["drafts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == draft_id)
            .cloned()
            .ok_or_else(|| ApiError::new("not_found", "草稿不存在"))?;
        if draft["state"] == "committed" {
            if draft["commit_key"] != key || draft["committed_facts"] != input["facts"] {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "草稿已入库，重试必须使用相同操作与事实",
                ));
            }
            return self.clothing_get(json!({"member_id":member,"id":draft["clothing_id"]}));
        }
        if batch["drafts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["commit_key"] == key)
        {
            return Err(ApiError::new(
                "idempotency_conflict",
                "操作标识已用于其他草稿",
            ));
        }
        let mut facts = input
            .get("facts")
            .cloned()
            .or_else(|| draft.get("facts").cloned())
            .unwrap_or(json!({}));
        let fields = facts
            .as_object_mut()
            .ok_or_else(|| ApiError::new("invalid_input", "单品事实须为对象"))?;
        fields.insert("member_id".into(), json!(member));
        if !fields.contains_key("name") {
            fields.insert("name".into(), draft["name"].clone());
        }
        if !fields.contains_key("category") {
            fields.insert("category".into(), draft["category"].clone());
        }
        let mut clothing = self.clothing_document(facts)?;
        let mut asset_ids = vec![];
        let mut pending_events: Vec<crate::jobs::PendingJobEvent> = vec![];
        for id in draft["item_ids"].as_array().unwrap() {
            let mut job = self.job_get(json!({"member_id":member,"id":id}))?;
            if job["state"] != "waiting_input" {
                return Err(ApiError::new("invalid_state", "全部图片处理成功后才能入库"));
            }
            let asset_id = job["result"]["asset_id"].clone();
            let mut asset = self.asset_get(json!({"member_id":member,"id":asset_id}))?;
            asset["state"] = json!("adopted");
            asset["display"]["state"] = json!("adopted");
            asset["display"]["sync_scope"] = json!("workspace");
            tx.execute(
                "UPDATE assets SET document=?1 WHERE id=?2 AND member_id=?3",
                params![asset.to_string(), asset_id.as_str(), member],
            )?;
            asset_ids.push(asset_id);
            job["state"] = json!("succeeded");
            job["result"]["clothing_id"] = clothing["id"].clone();
            if let Some(pending) = self.save_job(&tx, &job)? {
                pending_events.push(pending);
            }
        }
        clothing["asset_ids"] = json!(asset_ids);
        clothing["display_asset_id"] = asset_ids.first().cloned().unwrap_or(Value::Null);
        let serialized = clothing.to_string();
        let clothing_id = clothing["id"].as_str();
        tx.execute(
            "INSERT INTO clothing VALUES(?1,?2,?3)",
            params![clothing_id, member, serialized],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'imports.commit',?2)",
            params![clothing_id, serialized],
        )?;
        draft["state"] = json!("committed");
        draft["clothing_id"] = clothing["id"].clone();
        draft["commit_key"] = json!(key);
        draft["committed_facts"] = input["facts"].clone();
        tx.execute(
            "UPDATE import_drafts SET document=?1 WHERE id=?2",
            params![draft.to_string(), draft_id],
        )?;
        tx.commit()?;
        for pending in pending_events {
            self.emit_job_event(Some(pending));
        }
        Ok(clothing)
    }
    pub(super) fn imports_process(&self, input: Value) -> Result<Value, ApiError> {
        let batch = self.imports_get(input.clone())?;
        let member = batch["member_id"].as_str().unwrap();
        let requested = input.get("item_id").and_then(Value::as_str);
        if requested.is_some_and(|r| !batch["item_ids"].as_array().unwrap().contains(&json!(r))) {
            return Err(ApiError::new("not_found", "图片任务不属于批次"));
        }
        for item in batch["items"].as_array().unwrap() {
            let id = item["id"].as_str().unwrap();
            if requested.is_some_and(|r| r != id) {
                continue;
            }
            let mut job = match self.job_claim(member, id, 300) {
                Ok(job) => job,
                Err(e) if e.code == "job_unavailable" => continue,
                Err(e) => return Err(e),
            };
            let result = (|| {
                if job["payload"]["local_path"].is_null() {
                    let local = self.import_source(Path::new(
                        job["payload"]["source_path"].as_str().unwrap(),
                    ))?;
                    job["payload"]["local_path"] = json!(local);
                    let tx = self.begin_write()?;
                    let current = self.job_get(json!({"member_id":member,"id":id}))?;
                    if current["lease_token"] != job["lease_token"] {
                        return Err(ApiError::new("lease_lost", "任务已取消"));
                    }
                    let pending = self.save_job(&tx, &job)?;
                    tx.commit()?;
                    self.emit_job_event(pending);
                }
                self.asset_import_with_id(
                    json!({"member_id":member,"path":job["payload"]["local_path"]}),
                    job["payload"]["asset_id"].as_str().unwrap(),
                )
            })();
            let (state, result, error) = match result {
                Ok(asset) => (
                    "waiting_input",
                    json!({"asset_id":asset["id"]}),
                    Value::Null,
                ),
                Err(e) => (
                    "failed",
                    Value::Null,
                    json!({"code":e.code,"message":e.message,"retryable":false}),
                ),
            };
            match self.job_finish(
                member,
                id,
                job["lease_token"].as_str().unwrap(),
                state,
                result,
                error,
            ) {
                Ok(_) => {}
                Err(e) if e.code == "lease_lost" => {}
                Err(e) => return Err(e),
            };
            if requested.is_none() {
                break;
            }
        }
        self.imports_get(input)
    }
}
fn output_schema() -> Value {
    let mut item = jobs::output_schema();
    item["properties"]["filename"] = json!({"type":"string"});
    item["properties"]["asset_id"] = json!({"type":["string","null"]});
    json!({"type":"object","required":["schema_version","id","job_ids","member_id","operation_key","paths","item_ids","items","drafts","created_at"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"job_ids":{"type":"array","items":{"type":"string"}},"member_id":{"type":"string"},"operation_key":{"type":"string"},"paths":{"type":"array","maxItems":100,"items":{"type":"string"}},"created_at":{"type":"string"},"item_ids":{"type":"array","maxItems":100,"items":{"type":"string"}},"items":{"type":"array","maxItems":100,"items":item},"drafts":{"type":"array","items":{"type":"object","required":["id","member_id","batch_id","item_ids","name","category","state","clothing_id","commit_key"],"properties":{"id":{"type":"string"},"member_id":{"type":"string"},"batch_id":{"type":"string"},"item_ids":{"type":"array","maxItems":20,"uniqueItems":true,"items":{"type":"string"}},"name":{"type":"string"},"category":{"type":"string"},"state":{"enum":["draft","committed"]},"clothing_id":{"type":["string","null"]},"commit_key":{"type":["string","null"]},"facts":{"type":"object"}}}}}})
}
pub(super) fn descriptors() -> Vec<Value> {
    ["create","get","list","process","group","commit","update"].iter().map(|action|{let mut properties=json!({"member_id":{"type":"string"}});let fields=match *action{"create"=>vec!["operation_key","paths"],"list"=>vec![],"process"=>vec!["id","item_id"],"group"=>vec!["id","item_ids"],"commit"=>vec!["id","draft_id","operation_key","facts"],"update"=>vec!["id","draft_id","facts"],_=>vec!["id"]};for field in fields{properties[field]=match field{"paths"=>json!({"type":"array","minItems":1,"maxItems":100,"items":{"type":"string"}}),"item_ids"=>json!({"type":"array","minItems":2,"maxItems":20,"uniqueItems":true,"items":{"type":"string"}}),"facts"=>json!({"type":"object"}),_=>json!({"type":"string"})};}let required=match *action{"list"=>vec!["member_id"],"create"=>vec!["member_id","operation_key","paths"],"group"=>vec!["member_id","id","item_ids"],"commit"=>vec!["member_id","id","draft_id","operation_key"],"update"=>vec!["member_id","id","draft_id","facts"],_=>vec!["member_id","id"]};let output=match *action{"commit"=>crate::clothing::output_schema("clothing.get"),"list"=>json!({"type":"object","required":["schema_version","items"],"properties":{"schema_version":{"const":1},"items":{"type":"array","items":output_schema()}}}),_=>output_schema()};let (effect,examples)=match *action {
            "create"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>","paths":["</绝对/本地/图片路径>"]},"prerequisites":["读取提供的本地图片并复制进本机受管存储；原文件不受影响","每文件一个草稿项，可归并；一批最多100个文件","同operation_key幂等返回同一批次"]}])),"get"=>(json!({"kind":"query","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<imports.create返回的批次ID>"},"prerequisites":["读取批次与逐项处理状态"]}])),"list"=>(json!({"kind":"query","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["返回当前成员全部导入批次"]}])),"process"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<批次ID>"},"prerequisites":["本地解码并规范方向/压缩派生图；原图字节不变","单项失败不影响其他项，可重试；处理结果为待采用预览","不发送网络"]}])),"group"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<批次ID>","item_ids":["<批次单项ID一>","<批次单项ID二>"]},"prerequisites":["把2-20张图归并为同一草稿（同一单品的多个视图）","多件衣物照片不会自动分割实体"]}])),"commit"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<批次ID>","draft_id":"<草稿ID>","operation_key":"<唯一操作标识>","facts":{"name":"白衬衫","category":"top"}},"prerequisites":["草稿幂等入库为真实衣物；重复提交返回同一结果","必填名称与类别须合法，不捏造未知字段"]}])),"update"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>","id":"<批次ID>","draft_id":"<草稿ID>","facts":{"category":"top"}},"prerequisites":["更新草稿事实；不写入真实库存","类别须为分类字典合法值"]}])),_=>unreachable!()};json!({"name":format!("imports.{action}"),"description":"持久化批量导入，单项处理、归并草稿及幂等入库","effect":effect,"examples":examples,"inputSchema":{"type":"object","required":required,"additionalProperties":false,"properties":properties},"outputSchema":output})}).collect()
}
