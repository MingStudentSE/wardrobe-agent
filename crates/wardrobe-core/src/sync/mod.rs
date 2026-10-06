//! Incremental workspace synchronization. No provider SDK or credentials here.
pub mod storage;
use crate::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};
use storage::{LocalFolder, SyncStorage, WriteCondition};
fn error(code: &str) -> ApiError {
    ApiError::new(code, "同步未完成，保留本地事实后重试")
}
pub(crate) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS sync_control(id INTEGER PRIMARY KEY CHECK(id=1),applying INTEGER NOT NULL); INSERT OR IGNORE INTO sync_control VALUES(1,0); CREATE TABLE IF NOT EXISTS sync_clock(device TEXT PRIMARY KEY,seq INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS sync_targets(id TEXT PRIMARY KEY,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sync_journal(id INTEGER PRIMARY KEY,op_id TEXT NOT NULL UNIQUE,device TEXT NOT NULL,entity_type TEXT NOT NULL,entity_id TEXT NOT NULL,before_doc TEXT,after_doc TEXT NOT NULL,base TEXT NOT NULL,clock TEXT NOT NULL DEFAULT '{}',published INTEGER NOT NULL DEFAULT 0,registered INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS sync_inbox(op_id TEXT PRIMARY KEY,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sync_fields(entity_type TEXT NOT NULL,entity_id TEXT NOT NULL,field TEXT NOT NULL,variants TEXT NOT NULL,PRIMARY KEY(entity_type,entity_id,field)); CREATE TABLE IF NOT EXISTS sync_cursors(target_id TEXT NOT NULL,key TEXT NOT NULL,PRIMARY KEY(target_id,key));")?;
    for table in ["members", "clothing"] {
        for kind in ["INSERT", "UPDATE"] {
            let after = if table == "members" {
                "json_object('id',NEW.id,'name',NEW.name)"
            } else {
                "NEW.document"
            };
            let before = if kind == "INSERT" {
                "NULL"
            } else if table == "members" {
                "json_object('id',OLD.id,'name',OLD.name)"
            } else {
                "OLD.document"
            };
            tx.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS sync_{table}_{kind} AFTER {kind} ON {table} WHEN (SELECT applying FROM sync_control WHERE id=1)=0 AND EXISTS(SELECT 1 FROM encryption_state) BEGIN INSERT INTO sync_journal(op_id,device,entity_type,entity_id,before_doc,after_doc,base) VALUES(lower(hex(randomblob(16))),(SELECT json_extract(document,'$.device_id') FROM encryption_state),'{table}',NEW.id,{before},{after},COALESCE((SELECT json_group_object(device,seq) FROM sync_clock),'{{}}')); INSERT INTO sync_clock(device,seq) VALUES((SELECT json_extract(document,'$.device_id') FROM encryption_state),1) ON CONFLICT(device) DO UPDATE SET seq=seq+1; UPDATE sync_journal SET clock=(SELECT json_group_object(device,seq) FROM sync_clock) WHERE id=(SELECT max(id) FROM sync_journal); END;"))?;
        }
    }
    Ok(())
}
fn vector(v: &Value) -> BTreeMap<String, u64> {
    v.as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_u64().map(|n| (k.clone(), n)))
                .collect()
        })
        .unwrap_or_default()
}
fn dominates(a: &Value, b: &Value) -> bool {
    let a = vector(a);
    vector(b)
        .iter()
        .all(|(k, n)| a.get(k).copied().unwrap_or(0) >= *n)
}
fn digest(v: &[u8]) -> String {
    Sha256::digest(v)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn bounded(storage: &dyn SyncStorage, key: &str) -> Result<Vec<u8>, ApiError> {
    let mut bytes = vec![];
    storage
        .get(key)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("storage_transient"))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(error("storage_corrupt"));
    }
    Ok(bytes)
}
impl Application {
    fn sync_apply(&self, op: &Value, local: bool) -> Result<(), ApiError> {
        let id = string(op, "op_id")?;
        if self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_inbox WHERE op_id=?1)",
            [id],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok(());
        }
        let table = string(op, "entity_type")?;
        if !["members", "clothing"].contains(&table) {
            return Err(error("sync_invalid"));
        }
        let entity = string(op, "entity_id")?;
        let after = op["after"]
            .as_object()
            .ok_or_else(|| error("sync_invalid"))?;
        let before = op["before"].as_object();
        let tx = self.begin_write()?;
        tx.execute("UPDATE sync_control SET applying=1 WHERE id=1", [])?;
        for (field, value) in after {
            if before.is_some_and(|b| b.get(field) == Some(value)) {
                continue;
            }
            let old:Option<String>=tx.query_row("SELECT variants FROM sync_fields WHERE entity_type=?1 AND entity_id=?2 AND field=?3",params![table,entity,field],|r|r.get(0)).optional()?;
            let mut variants: Vec<Value> = old
                .map(|s| serde_json::from_str(&s).map_err(|_| error("storage_error")))
                .transpose()?
                .unwrap_or_default();
            if variants
                .iter()
                .any(|v| dominates(&v["clock"], &op["clock"]))
            {
                continue;
            }
            variants.retain(|v| !dominates(&op["clock"], &v["clock"]));
            variants.push(json!({"value":value,"clock":op["clock"],"op_id":id}));
            tx.execute("INSERT INTO sync_fields VALUES(?1,?2,?3,?4) ON CONFLICT(entity_type,entity_id,field) DO UPDATE SET variants=excluded.variants",params![table,entity,field,serde_json::to_string(&variants).map_err(|_|error("sync_invalid"))?])?;
        }
        let mut doc = serde_json::Map::new();
        let mut stmt = tx.prepare(
            "SELECT field,variants FROM sync_fields WHERE entity_type=?1 AND entity_id=?2",
        )?;
        let rows = stmt
            .query_map(params![table, entity], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (field, variants) in rows {
            let mut variants: Vec<Value> =
                serde_json::from_str(&variants).map_err(|_| error("sync_invalid"))?;
            variants.sort_by(|a, b| a["op_id"].as_str().cmp(&b["op_id"].as_str()));
            let winner = if field == "status" {
                variants
                    .iter()
                    .find(|v| v["value"] == "deleted")
                    .or_else(|| variants.last())
            } else {
                variants.last()
            };
            if let Some(w) = winner {
                doc.insert(field, w["value"].clone());
            }
        }
        if table == "members" {
            tx.execute("INSERT INTO members(id,name) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET name=excluded.name",params![entity,doc.get("name").and_then(Value::as_str).ok_or_else(||error("sync_invalid"))?])?;
        } else {
            let member = doc
                .get("member_id")
                .and_then(Value::as_str)
                .ok_or_else(|| error("sync_invalid"))?;
            let known: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM members WHERE id=?1)",
                [member],
                |r| r.get(0),
            )?;
            if !known {
                return Err(error("sync_dependency"));
            }
            if !local {
                tx.execute("INSERT INTO clothing(id,member_id,document) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET member_id=excluded.member_id,document=excluded.document",params![entity,member,Value::Object(doc.clone()).to_string()])?;
            }
        }
        for (device, seq) in vector(&op["clock"]) {
            tx.execute("INSERT INTO sync_clock VALUES(?1,?2) ON CONFLICT(device) DO UPDATE SET seq=max(seq,excluded.seq)",params![device,seq])?;
        }
        tx.execute(
            "INSERT INTO sync_inbox VALUES(?1,?2)",
            params![id, op.to_string()],
        )?;
        tx.execute("UPDATE sync_control SET applying=0 WHERE id=1", [])?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn sync_operation(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        match op {
            "sync.configure" => {
                if input["provider_kind"] != "local_folder" {
                    return Err(error("storage_unsupported"));
                }
                let directory = std::fs::canonicalize(string(&input, "directory")?)
                    .map_err(|_| error("storage_not_found"))?;
                let local = std::fs::canonicalize(
                    self.asset_root
                        .parent()
                        .ok_or_else(|| error("storage_error"))?,
                )
                .map_err(|_| error("storage_error"))?;
                if directory.starts_with(local) {
                    return Err(error("invalid_input"));
                }
                let identity = self.sync_identity()?;
                LocalFolder::new(&directory)?;
                let tx = self.begin_write()?;
                let owner: bool = tx.query_row(
                    "SELECT json_extract(document,'$.owner') FROM encryption_state",
                    [],
                    |r| r.get(0),
                )?;
                for table in ["members", "clothing"] {
                    let sql = if table == "members" {
                        "SELECT id,json_object('id',id,'name',name) FROM members WHERE ?1 OR EXISTS(SELECT 1 FROM clothing c WHERE c.member_id=members.id)"
                    } else {
                        "SELECT id,document FROM clothing WHERE ?1 OR 1"
                    };
                    let mut stmt = tx.prepare(sql)?;
                    let rows = stmt
                        .query_map([owner], |r| {
                            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    drop(stmt);
                    for (id, document) in rows {
                        let seen:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sync_journal WHERE entity_type=?1 AND entity_id=?2)",params![table,id],|r|r.get(0))?;
                        if seen {
                            continue;
                        }
                        let base: String = tx.query_row(
                            "SELECT COALESCE(json_group_object(device,seq),'{}') FROM sync_clock",
                            [],
                            |r| r.get(0),
                        )?;
                        let device = string(&identity, "device_id")?;
                        tx.execute("INSERT INTO sync_clock VALUES(?1,1) ON CONFLICT(device) DO UPDATE SET seq=seq+1",[device])?;
                        let clock: String = tx.query_row(
                            "SELECT json_group_object(device,seq) FROM sync_clock",
                            [],
                            |r| r.get(0),
                        )?;
                        tx.execute("INSERT INTO sync_journal(op_id,device,entity_type,entity_id,after_doc,base,clock) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![uuid::Uuid::now_v7().to_string(),device,table,id,document,base,clock])?;
                    }
                }

                let target =
                    json!({"id":"primary","provider_kind":"local_folder","directory":directory});
                tx.execute("INSERT INTO sync_targets VALUES('primary',?1) ON CONFLICT(id) DO UPDATE SET document=excluded.document",[target.to_string()])?;
                tx.commit()?;
                Ok(json!({"schema_version":1,"target":target}))
            }
            "sync.status" => {
                let target: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT document FROM sync_targets WHERE id='primary'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok(
                    json!({"schema_version":1,"target":target.map(|s|serde_json::from_str::<Value>(&s)).transpose().map_err(|_|error("storage_error"))?}),
                )
            }
            "sync.run" => {
                let identity = self.sync_identity()?;
                let target: String = self
                    .connection
                    .query_row(
                        "SELECT document FROM sync_targets WHERE id='primary'",
                        [],
                        |r| r.get(0),
                    )
                    .map_err(|_| error("sync_not_configured"))?;
                let target: Value =
                    serde_json::from_str(&target).map_err(|_| error("storage_error"))?;
                let storage = LocalFolder::new(Path::new(string(&target, "directory")?))?;
                let prefix = format!("{}/changes", string(&identity, "space_id")?);
                let mut stmt=self.connection.prepare("SELECT id,op_id,device,entity_type,entity_id,before_doc,after_doc,base,clock,published FROM sync_journal ORDER BY id")?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, String>(6)?,
                            r.get::<_, String>(7)?,
                            r.get::<_, String>(8)?,
                            r.get::<_, bool>(9)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                drop(stmt);
                let mut sent = 0;
                for (row, id, device, table, entity, before, after, base, clock, published) in rows
                {
                    let parse = |s: &str| {
                        serde_json::from_str::<Value>(s).map_err(|_| error("storage_error"))
                    };
                    let op = json!({"protocol_version":1,"op_id":id,"device_id":device,"entity_type":table,"entity_id":entity,"before":before.as_deref().map(parse).transpose()?,"after":parse(&after)?,"base":parse(&base)?,"clock":parse(&clock)?});
                    self.sync_apply(&op, true)?;
                    if !published {
                        let cipher = self.sync_encrypt("changes", &op)?;
                        let bytes =
                            serde_json::to_vec(&cipher).map_err(|_| error("sync_invalid"))?;
                        let key = format!("{prefix}/{id}.object");
                        if !storage.stat(&key).is_ok() {
                            storage.put(&key, &mut bytes.as_slice(), WriteCondition::CreateOnly)?;
                        }
                        let stored = bounded(&storage, &key)?;
                        let marker =
                            json!({"key":key,"sha256":digest(&stored),"bytes":stored.len()});
                        let marker = self.sync_encrypt("changes", &marker)?;
                        let marker_bytes =
                            serde_json::to_vec(&marker).map_err(|_| error("sync_invalid"))?;
                        let marker_key = format!("{prefix}/{id}.commit");
                        if storage.stat(&marker_key).is_err() {
                            storage.put(
                                &marker_key,
                                &mut marker_bytes.as_slice(),
                                WriteCondition::CreateOnly,
                            )?;
                        }
                        self.connection
                            .execute("UPDATE sync_journal SET published=1 WHERE id=?1", [row])?;
                        sent += 1;
                    }
                }
                let mut received = 0;
                loop {
                    let initial = received;
                    let mut cursor = None;
                    loop {
                        let page = storage.list(&prefix, cursor.as_deref(), 64)?;
                        for object in page.items {
                            if !object.key.ends_with(".commit") {
                                continue;
                            }
                            if self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_cursors WHERE target_id='primary' AND key=?1)",[&object.key],|r|r.get::<_,bool>(0))?{continue;}
                            let marker: Value =
                                serde_json::from_slice(&bounded(&storage, &object.key)?)
                                    .map_err(|_| error("storage_corrupt"))?;
                            let marker = self.sync_decrypt("changes", &marker)?;
                            let bytes = bounded(&storage, string(&marker, "key")?)?;
                            if marker["sha256"] != digest(&bytes) || marker["bytes"] != bytes.len()
                            {
                                return Err(error("storage_corrupt"));
                            }
                            let cipher: Value = serde_json::from_slice(&bytes)
                                .map_err(|_| error("storage_corrupt"))?;
                            let op = self.sync_decrypt("changes", &cipher)?;
                            match self.sync_apply(&op, false) {
                                Ok(()) => {}
                                Err(e) if e.code == "sync_dependency" => continue,
                                Err(e) => return Err(e),
                            }
                            self.connection.execute(
                                "INSERT OR IGNORE INTO sync_cursors VALUES('primary',?1)",
                                [&object.key],
                            )?;
                            received += 1;
                        }
                        cursor = page.cursor;
                        if cursor.is_none() {
                            break;
                        }
                    }
                    if received == initial {
                        break;
                    }
                }
                Ok(json!({"schema_version":1,"sent":sent,"received":received}))
            }
            _ => Err(error("unsupported_operation")),
        }
    }
}
pub(crate) fn descriptors() -> Vec<Value> {
    ["configure","run","status"].into_iter().map(|op|json!({"name":format!("sync.{op}"),"description":"同步已配对加密空间","inputSchema":{"type":"object","properties":{"provider_kind":{"type":"string"},"directory":{"type":"string"}}},"outputSchema":{"type":"object","required":["schema_version"],"properties":{"schema_version":{"type":"integer","const":1}}}})).collect()
}
