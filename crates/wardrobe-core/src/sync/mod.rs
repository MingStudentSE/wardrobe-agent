//! Incremental workspace synchronization. No provider SDK or credentials here.
mod assets;
mod bootstrap;
mod branches;
mod conflicts;
mod index;
mod pending;
mod publication;
mod schema;
pub mod storage;
mod worker;
use crate::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};
use storage::{LocalFolder, SyncStorage, WriteCondition};
const TABLES: [&str; 11] = [
    "members",
    "clothing",
    "outfits",
    "outfit_versions",
    "wear_events",
    "care_config",
    "care_events",
    "insights_preferences",
    "weather_preferences",
    "feedback_events",
    "feedback_receipts",
];
fn entity_key(table: &str, prefix: &str) -> String {
    match table {
        "outfit_versions" => format!("{prefix}.outfit_id||':'||{prefix}.version"),
        "care_config" => format!("{prefix}.clothing_id"),
        "insights_preferences" | "weather_preferences" => format!("{prefix}.member_id"),
        "feedback_receipts" => format!("{prefix}.member_id||':'||{prefix}.operation_key"),
        _ => format!("{prefix}.id"),
    }
}
/// Derived facts require a unique latest immutable revision; historical branches
/// remain available for explicit review rather than being silently tie-broken.
pub(crate) fn require_resolved_events(
    db: &rusqlite::Connection,
    table: &str,
    member: &str,
) -> Result<(), ApiError> {
    let root = match table {
        "wear_events" | "care_events" => "root_event_id",
        "feedback_events" => "root_id",
        _ => return Err(error("invalid_input")),
    };
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} e WHERE member_id=?1 AND revision=(SELECT max(revision) FROM {table} WHERE member_id=?1 AND {root}=e.{root}) GROUP BY {root} HAVING count(*)>1)");
    let ambiguous: bool = db.query_row(&sql, [member], |row| row.get(0))?;
    if ambiguous {
        return Err(ApiError::new(
            "sync_conflict",
            "历史存在未选择的并发修订，请先查看同步冲突",
        ));
    }
    Ok(())
}

fn materialize(
    tx: &Transaction<'_>,
    table: &str,
    id: &str,
    doc: &Value,
    local: bool,
) -> Result<(), ApiError> {
    if local {
        return Ok(());
    }
    if table == "members" {
        tx.execute(
            "INSERT INTO members VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET name=excluded.name",
            params![id, string(doc, "name")?],
        )?;
        return Ok(());
    }
    let member = doc["member_id"]
        .as_str()
        .ok_or_else(|| error("sync_dependency"))?;
    if !tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM members WHERE id=?1)",
        [member],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(error("sync_dependency"));
    }
    let serialized = doc.to_string();
    match table {
        "clothing" => {
            if doc.get("category").is_none() || doc.get("name").is_none() {
                return Err(error("sync_dependency"));
            }
            tx.execute("INSERT INTO clothing VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET document=excluded.document",params![id,member,serialized])?;
        }
        "outfits" => {
            let items = doc["items"]
                .as_array()
                .ok_or_else(|| error("sync_dependency"))?;
            for item in items {
                let clothing = string(item, "clothing_id")?;
                if !tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM clothing WHERE id=?1 AND member_id=?2)",
                    params![clothing, member],
                    |r| r.get::<_, bool>(0),
                )? {
                    return Err(error("sync_dependency"));
                }
            }
            tx.execute("INSERT INTO outfits VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET document=excluded.document",params![id,member,serialized])?;
            tx.execute("DELETE FROM outfit_items WHERE outfit_id=?1", [id])?;
            for (position, item) in items.iter().enumerate() {
                tx.execute(
                    "INSERT INTO outfit_items VALUES(?1,?2,?3,?4,?5)",
                    params![
                        id,
                        position,
                        string(item, "clothing_id")?,
                        string(item, "role")?,
                        item["layer"]
                            .as_i64()
                            .ok_or_else(|| error("sync_invalid"))?
                    ],
                )?;
            }
        }
        "outfit_versions" => {
            let outfit = string(doc, "id")?;
            if !tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM outfits WHERE id=?1)",
                [outfit],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(error("sync_dependency"));
            }
            tx.execute("INSERT INTO outfit_versions SELECT ?1,?2,?3 WHERE NOT EXISTS(SELECT 1 FROM outfit_versions WHERE outfit_id=?1 AND version=?2 AND document=?3)",params![outfit,doc["version"].as_u64(),serialized])?;
        }
        "wear_events" | "care_events" | "feedback_events" => {
            tx.execute(
                &format!("INSERT OR IGNORE INTO {table} VALUES(?1,?2,?3,?4,?5)"),
                params![
                    id,
                    member,
                    string(
                        doc,
                        if table == "feedback_events" {
                            "root_id"
                        } else {
                            "root_event_id"
                        }
                    )?,
                    doc["revision"].as_u64(),
                    serialized
                ],
            )?;
        }
        "feedback_receipts" => {
            let result: Value =
                serde_json::from_str(string(doc, "result")?).map_err(|_| error("sync_invalid"))?;
            let prior: Option<String> = tx
                .query_row(
                    "SELECT document FROM feedback_events WHERE id=?1 AND member_id=?2",
                    params![string(&result, "id")?, member],
                    |r| r.get(0),
                )
                .optional()?;
            let prior: Value =
                serde_json::from_str(&prior.ok_or_else(|| error("sync_dependency"))?)
                    .map_err(|_| error("sync_invalid"))?;
            if prior != result {
                return Err(error("sync_identity_conflict"));
            }
            // Preserve every distinct receipt as portable business history; ambiguous
            // operation keys cannot replay a single arbitrary branch.
            tx.execute(
                "INSERT OR IGNORE INTO feedback_receipts VALUES(?1,?2,?3,?4)",
                params![
                    member,
                    string(doc, "operation_key")?,
                    string(doc, "request")?,
                    string(doc, "result")?
                ],
            )?;
        }
        "care_config" => {
            tx.execute("INSERT INTO care_config VALUES(?1,?2,?3) ON CONFLICT(member_id,clothing_id) DO UPDATE SET document=excluded.document",params![member,string(doc,"clothing_id")?,serialized])?;
        }
        "weather_preferences" => {
            let mut local = doc.clone();
            if local["config"]["mode"] == "automatic" {
                let endpoint: Option<String> = tx.query_row("SELECT json_extract(document,'$.config.endpoint') FROM weather_preferences WHERE member_id=?1",[member],|r|r.get(0)).optional()?.flatten();
                local["config"]["endpoint"] = json!(
                    endpoint.unwrap_or_else(|| "https://api.open-meteo.com/v1/forecast".into())
                );
            }
            local["cache"] = Value::Null;
            local["last_error"] = Value::Null;
            crate::weather::validate_portable_document(&local)?;
            tx.execute("INSERT INTO weather_preferences VALUES(?1,?2) ON CONFLICT(member_id) DO UPDATE SET document=excluded.document",params![member,local.to_string()])?;
        }
        "insights_preferences" => {
            tx.execute("INSERT INTO insights_preferences VALUES(?1,?2) ON CONFLICT(member_id) DO UPDATE SET document=excluded.document",params![member,serialized])?;
        }
        _ => return Err(error("sync_invalid")),
    }
    assets::ensure_references(tx, table, doc)?;
    Ok(())
}
fn repair_immutable(tx: &Transaction<'_>) -> Result<(), ApiError> {
    for (table, refs) in [
        ("wear_events", Some("wear_asset_refs")),
        ("care_events", Some("care_asset_refs")),
        ("feedback_events", Some("feedback_asset_refs")),
        ("outfit_versions", None),
    ] {
        let sql: String = tx.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |r| r.get(0),
        )?;
        if !sql.contains("UNIQUE(root_event_id,revision)")
            && !sql.contains("PRIMARY KEY(outfit_id,version)")
            && !sql.contains("UNIQUE(root_id,revision)")
        {
            continue;
        }
        for kind in ["INSERT", "UPDATE"] {
            tx.execute_batch(&format!("DROP TRIGGER IF EXISTS sync_{table}_{kind}"))?;
        }
        if let Some(refs) = refs {
            tx.execute_batch(&format!("ALTER TABLE {refs} RENAME TO {refs}_legacy"))?;
        }
        tx.execute_batch(&format!("ALTER TABLE {table} RENAME TO {table}_legacy"))?;
        if table == "outfit_versions" {
            tx.execute_batch("CREATE TABLE outfit_versions(outfit_id TEXT NOT NULL REFERENCES outfits(id),version INTEGER NOT NULL,document TEXT NOT NULL)")?;
        } else {
            let root = if table == "feedback_events" {
                "root_id"
            } else {
                "root_event_id"
            };
            tx.execute_batch(&format!("CREATE TABLE {table}(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),{root} TEXT NOT NULL,revision INTEGER NOT NULL,document TEXT NOT NULL)"))?;
        }
        tx.execute_batch(&format!("INSERT INTO {table} SELECT * FROM {table}_legacy"))?;
        if let Some(refs) = refs {
            tx.execute_batch(&format!("CREATE TABLE {refs}(event_id TEXT NOT NULL REFERENCES {table}(id),asset_id TEXT NOT NULL,member_id TEXT NOT NULL,PRIMARY KEY(event_id,asset_id),FOREIGN KEY(asset_id,member_id) REFERENCES assets(id,member_id)); INSERT INTO {refs} SELECT * FROM {refs}_legacy; DROP TABLE {refs}_legacy"))?;
        }
        tx.execute_batch(&format!("DROP TABLE {table}_legacy"))?;
    }
    tx.execute_batch("CREATE INDEX IF NOT EXISTS sync_wear_root ON wear_events(root_event_id,revision,id);CREATE INDEX IF NOT EXISTS sync_care_root ON care_events(root_event_id,revision,id);CREATE INDEX IF NOT EXISTS sync_outfit_versions ON outfit_versions(outfit_id,version);CREATE INDEX IF NOT EXISTS feedback_member_root ON feedback_events(member_id,root_id,revision)")?;
    Ok(())
}
fn immutable(table: &str) -> bool {
    [
        "wear_events",
        "care_events",
        "feedback_events",
        "feedback_receipts",
        "outfit_versions",
    ]
    .contains(&table)
}
fn error(code: &str) -> ApiError {
    ApiError::new(code, "同步未完成，保留本地事实后重试")
}
/// Portable restore resets local transport progress along with local device trust.
/// Business events and receipts are restored separately; old journals must never
/// republish facts that were deliberately discarded by the restored snapshot.
pub(crate) fn reset_after_restore(tx: &Transaction<'_>) -> Result<(), ApiError> {
    for table in [
        "sync_bootstrap",
        "sync_index_read",
        "sync_index_batches",
        "sync_index_entries",
        "sync_bootstrap_ids",
        "sync_publications",
        "sync_pending",
        "sync_quarantine",
        "sync_reassert",
        "sync_resolution_receipts",
        "sync_asset_queue",
        "sync_immutable",
        "sync_clock",
        "sync_targets",
        "sync_journal",
        "sync_inbox",
        "sync_fields",
        "sync_cursors",
        "sync_index_errors",
        "sync_missing",
    ] {
        tx.execute_batch(&format!("DELETE FROM {table}"))?;
    }
    tx.execute("UPDATE sync_control SET applying=0 WHERE id=1", [])?;
    Ok(())
}

pub(crate) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    repair_immutable(tx)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS sync_bootstrap(space_id TEXT NOT NULL,table_name TEXT NOT NULL,last_row INTEGER NOT NULL DEFAULT 0,done INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(space_id,table_name))")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS sync_index_read(target_id TEXT NOT NULL,epoch INTEGER NOT NULL,publisher TEXT NOT NULL,next_page INTEGER NOT NULL,observed_head INTEGER NOT NULL,previous_digest TEXT,PRIMARY KEY(target_id,epoch,publisher)); CREATE TABLE IF NOT EXISTS sync_index_batches(target_id TEXT NOT NULL,epoch INTEGER NOT NULL,page INTEGER NOT NULL,document TEXT NOT NULL,published INTEGER NOT NULL,PRIMARY KEY(target_id,epoch,page)); CREATE TABLE IF NOT EXISTS sync_index_entries(target_id TEXT NOT NULL,epoch INTEGER NOT NULL,op_id TEXT NOT NULL,page INTEGER NOT NULL,PRIMARY KEY(target_id,epoch,op_id)); CREATE TABLE IF NOT EXISTS sync_bootstrap_ids(op_id TEXT PRIMARY KEY); CREATE TABLE IF NOT EXISTS sync_publications(target_id TEXT NOT NULL,op_id TEXT NOT NULL,PRIMARY KEY(target_id,op_id)); CREATE TABLE IF NOT EXISTS sync_pending(target_id TEXT NOT NULL,marker_key TEXT NOT NULL,op_id TEXT NOT NULL,document TEXT NOT NULL,reason TEXT NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(target_id,marker_key)); CREATE TABLE IF NOT EXISTS sync_quarantine(target_id TEXT NOT NULL,marker_key TEXT NOT NULL,op_id TEXT NOT NULL,code TEXT NOT NULL,document TEXT NOT NULL,PRIMARY KEY(target_id,marker_key)); CREATE TABLE IF NOT EXISTS sync_reassert(op_id TEXT PRIMARY KEY,fields TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sync_resolution_receipts(member_id TEXT NOT NULL,operation_key TEXT NOT NULL,request TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(member_id,operation_key)); CREATE TABLE IF NOT EXISTS sync_asset_queue(id TEXT PRIMARY KEY,document TEXT NOT NULL,state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sync_immutable(entity_type TEXT NOT NULL,entity_id TEXT NOT NULL,branch_id TEXT NOT NULL,document TEXT NOT NULL,clock TEXT NOT NULL,PRIMARY KEY(entity_type,entity_id,branch_id)); CREATE TABLE IF NOT EXISTS sync_control(id INTEGER PRIMARY KEY CHECK(id=1),applying INTEGER NOT NULL); INSERT OR IGNORE INTO sync_control VALUES(1,0); CREATE TABLE IF NOT EXISTS sync_clock(device TEXT PRIMARY KEY,seq INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS sync_targets(id TEXT PRIMARY KEY,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sync_journal(id INTEGER PRIMARY KEY,op_id TEXT NOT NULL UNIQUE,device TEXT NOT NULL,entity_type TEXT NOT NULL,entity_id TEXT NOT NULL,before_doc TEXT,after_doc TEXT NOT NULL,base TEXT NOT NULL,clock TEXT NOT NULL DEFAULT '{}',published INTEGER NOT NULL DEFAULT 0,registered INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS sync_inbox(op_id TEXT PRIMARY KEY,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sync_fields(entity_type TEXT NOT NULL,entity_id TEXT NOT NULL,field TEXT NOT NULL,variants TEXT NOT NULL,PRIMARY KEY(entity_type,entity_id,field)); CREATE TABLE IF NOT EXISTS sync_cursors(target_id TEXT NOT NULL,key TEXT NOT NULL,PRIMARY KEY(target_id,key));")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS sync_index_errors(target_id TEXT NOT NULL,epoch INTEGER NOT NULL,publisher TEXT NOT NULL,code TEXT NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(target_id,epoch,publisher))")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS sync_missing(target_id TEXT NOT NULL,epoch INTEGER NOT NULL,publisher TEXT NOT NULL,key TEXT NOT NULL,document TEXT NOT NULL,code TEXT NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(target_id,epoch,key));CREATE INDEX IF NOT EXISTS sync_missing_retry ON sync_missing(target_id,epoch,attempts,key)")?;
    // Recover every previously validated receipt branch into portable business facts.
    tx.execute_batch("INSERT OR IGNORE INTO feedback_receipts(member_id,operation_key,request,result) SELECT json_extract(document,'$.member_id'),json_extract(document,'$.operation_key'),json_extract(document,'$.request'),json_extract(document,'$.result') FROM sync_immutable WHERE entity_type='feedback_receipts';")?;
    let has_local_receipt_flag: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('sync_publications') WHERE name='locally_published')",[],|r|r.get(0))?;
    if !has_local_receipt_flag {
        tx.execute_batch("ALTER TABLE sync_publications ADD COLUMN locally_published INTEGER NOT NULL DEFAULT 1 CHECK(locally_published IN (0,1))")?;
    }
    for table in TABLES {
        for kind in ["INSERT", "UPDATE"] {
            let key = entity_key(table, "NEW");
            let after = if table == "weather_preferences" {
                "json_remove(NEW.document,'$.config.endpoint','$.cache','$.last_error')"
            } else if table == "feedback_receipts" {
                "json_object('schema_version',1,'member_id',NEW.member_id,'operation_key',NEW.operation_key,'request',NEW.request,'result',NEW.result)"
            } else if table == "members" {
                "json_object('id',NEW.id,'name',NEW.name)"
            } else {
                "NEW.document"
            };
            let before = if kind == "INSERT" {
                "NULL"
            } else if table == "weather_preferences" {
                "json_remove(OLD.document,'$.config.endpoint','$.cache','$.last_error')"
            } else if table == "feedback_receipts" {
                "json_object('schema_version',1,'member_id',OLD.member_id,'operation_key',OLD.operation_key,'request',OLD.request,'result',OLD.result)"
            } else if table == "members" {
                "json_object('id',OLD.id,'name',OLD.name)"
            } else {
                "OLD.document"
            };
            let changed = if table == "weather_preferences" && kind == "UPDATE" {
                format!(" AND {before}!={after}")
            } else {
                String::new()
            };
            tx.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS sync_{table}_{kind} AFTER {kind} ON {table} WHEN (SELECT applying FROM sync_control WHERE id=1)=0 AND EXISTS(SELECT 1 FROM encryption_state){changed} BEGIN INSERT INTO sync_journal(op_id,device,entity_type,entity_id,before_doc,after_doc,base) VALUES(lower(hex(randomblob(16))),(SELECT json_extract(document,'$.device_id') FROM encryption_state),'{table}',{key},{before},{after},COALESCE((SELECT json_group_object(device,seq) FROM sync_clock),'{{}}')); INSERT INTO sync_clock(device,seq) VALUES((SELECT json_extract(document,'$.device_id') FROM encryption_state),1) ON CONFLICT(device) DO UPDATE SET seq=seq+1; UPDATE sync_journal SET clock=(SELECT json_group_object(device,seq) FROM sync_clock) WHERE id=(SELECT max(id) FROM sync_journal); END;"))?;
        }
    }
    tx.execute_batch("CREATE INDEX IF NOT EXISTS sync_journal_entity ON sync_journal(entity_type,entity_id); CREATE INDEX IF NOT EXISTS sync_inbox_entity ON sync_inbox(json_extract(document,'$.entity_type'),json_extract(document,'$.entity_id'));")?;
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
        let prior: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM sync_inbox WHERE op_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(prior) = prior {
            let serialized_op = op.to_string();
            if prior != serialized_op {
                return Err(error("sync_identity_conflict"));
            }
            return Ok(());
        }
        schema::validate_operation(op)?;
        let table = string(op, "entity_type")?;
        if !TABLES.contains(&table) {
            return Err(error("sync_invalid"));
        }
        let entity = string(op, "entity_id")?;
        schema::validate(table, entity, &op["after"])?;
        if !op["before"].is_null() {
            schema::validate(table, entity, &op["before"])?;
        }
        let after = op["after"]
            .as_object()
            .ok_or_else(|| error("sync_invalid"))?;
        let before = op["before"].as_object();
        let tx = self.begin_write()?;
        schema::validate_identity(&tx, table, entity, &op["after"])?;
        tx.execute("UPDATE sync_control SET applying=1 WHERE id=1", [])?;
        if immutable(table) {
            if !op["before"].is_null() {
                return Err(error("sync_identity_conflict"));
            }
            let canonical = op["after"].to_string();
            let branch = digest(canonical.as_bytes());
            if !["outfit_versions", "feedback_receipts"].contains(&table) {
                if op["after"]["id"] != entity {
                    return Err(error("sync_invalid"));
                }
                let prior:Option<String>=tx.query_row("SELECT document FROM sync_immutable WHERE entity_type=?1 AND entity_id=?2 LIMIT 1",params![table,entity],|r|r.get(0)).optional()?;
                if prior.is_some_and(|s| s != canonical) {
                    return Err(error("sync_identity_conflict"));
                }
            }
            tx.execute(
                "INSERT OR IGNORE INTO sync_immutable VALUES(?1,?2,?3,?4,?5)",
                params![table, entity, branch, canonical, op["clock"].to_string()],
            )?;
            materialize(&tx, table, entity, &op["after"], local)?;
        } else {
            for (field, value) in after {
                if before.is_some_and(|b| b.get(field) == Some(value))
                    && !op["reassert"]
                        .as_array()
                        .is_some_and(|fields| fields.iter().any(|v| v.as_str() == Some(field)))
                {
                    continue;
                }
                if conflicts::SETS.contains(&field.as_str()) {
                    conflicts::set_register(
                        &tx,
                        table,
                        entity,
                        field,
                        before.and_then(|b| b.get(field)).unwrap_or(&Value::Null),
                        value,
                        op,
                    )?;
                } else {
                    conflicts::register(&tx, table, entity, field, value, op)?;
                }
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
            let mut sets: BTreeMap<String, Vec<Value>> = BTreeMap::new();
            for (field, variants) in rows {
                let mut variants: Vec<Value> =
                    serde_json::from_str(&variants).map_err(|_| error("sync_invalid"))?;
                if let Some((set, _)) = field.split_once('#') {
                    let labels = sets.entry(set.into()).or_default();
                    let mut present: Vec<_> = variants
                        .iter()
                        .filter(|v| v["value"]["present"] == true)
                        .filter_map(|v| v["value"]["label"].as_str())
                        .collect();
                    present.sort();
                    if let Some(label) = present.first() {
                        labels.push(json!(label));
                    }
                    continue;
                }
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
            for (field, mut values) in sets {
                values.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                doc.insert(field, json!(values));
            }
            materialize(&tx, table, entity, &Value::Object(doc), local)?;
        }
        if !local {
            for (device, required) in vector(&op["base"]) {
                let seen: Option<u64> = tx
                    .query_row(
                        "SELECT seq FROM sync_clock WHERE device=?1",
                        [device],
                        |r| r.get(0),
                    )
                    .optional()?;
                if seen.unwrap_or(0) < required {
                    return Err(error("sync_causal_dependency"));
                }
            }
            tx.execute(
                "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'sync.apply',?2)",
                params![entity, op.to_string()],
            )?;
        }
        let device = string(op, "device_id")?;
        let seq = op["clock"][device]
            .as_u64()
            .ok_or_else(|| error("sync_invalid"))?;
        tx.execute("INSERT INTO sync_clock VALUES(?1,?2) ON CONFLICT(device) DO UPDATE SET seq=max(seq,excluded.seq)",params![device,seq])?;
        tx.execute(
            "INSERT INTO sync_inbox VALUES(?1,?2)",
            params![id, op.to_string()],
        )?;
        tx.execute("UPDATE sync_control SET applying=0 WHERE id=1", [])?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn sync_operation(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        let allowed: &[&str] = match op {
            "sync.configure" => &["provider_kind", "directory"],
            "sync.run" => &["operation_key"],
            "sync.process" => &["id"],
            "sync.status" => &[],
            "sync.conflicts" => &["member_id"],
            "sync.history" => &["member_id", "entity_type", "entity_id"],
            "sync.resolve" => &[
                "member_id",
                "entity_type",
                "entity_id",
                "field",
                "value",
                "branch_id",
                "expected_token",
                "operation_key",
            ],
            _ => return Err(error("unsupported_operation")),
        };
        if !input
            .as_object()
            .is_some_and(|o| o.keys().all(|k| allowed.contains(&k.as_str())))
        {
            return Err(error("invalid_input"));
        }

        match op {
            "sync.conflicts" => self.sync_conflicts(input),
            "sync.resolve" => self.sync_resolve(input),
            "sync.history" => {
                let member = self.member(&input)?;
                let table = string(&input, "entity_type")?;
                let id = string(&input, "entity_id")?;
                let mut st=self.connection.prepare("SELECT branch_id,document FROM sync_immutable WHERE entity_type=?1 AND entity_id=?2 ORDER BY branch_id")?;
                let rows = st
                    .query_map(params![table, id], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                let mut items = vec![];
                for (branch, doc) in rows {
                    let doc: Value =
                        serde_json::from_str(&doc).map_err(|_| error("storage_error"))?;
                    if doc["member_id"] == member {
                        items.push(json!({"branch_id":branch,"document":doc}));
                    }
                }
                Ok(json!({"schema_version":1,"items":items}))
            }
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
                for table in TABLES {
                    tx.execute(
                        "INSERT OR IGNORE INTO sync_bootstrap(space_id,table_name) VALUES(?1,?2)",
                        params![string(&identity, "space_id")?, table],
                    )?;
                }

                let target = json!({"id":"primary","target_key":digest(json!({"directory":directory,"space_id":identity["space_id"]}).to_string().as_bytes()),"provider_kind":"local_folder","directory":directory});
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
                let target_value = target
                    .as_deref()
                    .map(serde_json::from_str::<Value>)
                    .transpose()
                    .map_err(|_| error("storage_error"))?;
                let target_key = target_value
                    .as_ref()
                    .and_then(|v| v["target_key"].as_str())
                    .unwrap_or("primary");
                let pending = self.connection.query_row(
                    "SELECT (SELECT count(*) FROM sync_pending WHERE target_id=?1)+(SELECT count(*) FROM sync_missing WHERE target_id=?1)+(SELECT count(*) FROM sync_index_errors WHERE target_id=?1)",
                    [target_key],
                    |r| r.get::<_, u64>(0),
                )?;
                let mut state = self
                    .connection
                    .prepare("SELECT id,state FROM sync_asset_queue WHERE state!='ready'")?;
                let asset_errors = state
                    .query_map([], |r| {
                        Ok(json!({"id":r.get::<_,String>(0)?,"code":r.get::<_,String>(1)?}))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;

                let mut quarantine_statement = self.connection.prepare(
                    "SELECT op_id,code FROM sync_quarantine WHERE target_id=?1 ORDER BY marker_key",
                )?;
                let quarantine = quarantine_statement
                    .query_map([target_key], |r| {
                        Ok(json!({"op_id":r.get::<_,String>(0)?,"code":r.get::<_,String>(1)?}))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                let mut transport_statement=self.connection.prepare("SELECT publisher,code,attempts FROM sync_index_errors WHERE target_id=?1 ORDER BY epoch,publisher LIMIT 32")?;
                let transport_errors=transport_statement.query_map([target_key],|r|Ok(json!({"publisher":r.get::<_,String>(0)?,"code":r.get::<_,String>(1)?,"attempts":r.get::<_,u64>(2)?})))?.collect::<Result<Vec<_>,_>>()?;
                let active_job: Option<String> = self.connection.query_row("SELECT document FROM jobs WHERE kind='sync' AND json_extract(document,'$.payload.target.target_key')=?1 AND json_extract(document,'$.state') IN ('queued','running') ORDER BY id DESC LIMIT 1",[target_key],|r|r.get(0)).optional()?;
                let active_job=active_job.as_deref().map(serde_json::from_str::<Value>).transpose().map_err(|_|error("storage_error"))?.map(|job|json!({"id":job["id"],"member_id":job["member_id"],"state":job["state"]}));
                Ok(
                    json!({"schema_version":1,"pending":pending,"asset_errors":asset_errors,"transport_errors":transport_errors,"quarantine":quarantine,"target":target_value,"active_job":active_job}),
                )
            }
            "sync.run" => self.sync_enqueue(input),
            "sync.process" => self.sync_process(input),
            _ => Err(error("unsupported_operation")),
        }
    }
}
impl Application {
    fn sync_step(&self) -> Result<Value, ApiError> {
        let identity = self.sync_identity()?;
        let target: String = self
            .connection
            .query_row(
                "SELECT document FROM sync_targets WHERE id='primary'",
                [],
                |r| r.get(0),
            )
            .map_err(|_| error("sync_not_configured"))?;
        let target: Value = serde_json::from_str(&target).map_err(|_| error("storage_error"))?;
        let storage = LocalFolder::new(Path::new(string(&target, "directory")?))?;
        let target_key = target["target_key"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                digest(
                    json!({"directory":target["directory"],"space_id":identity["space_id"]})
                        .to_string()
                        .as_bytes(),
                )
            });
        let prefix = if identity["epoch"].as_u64().unwrap_or(0) == 1 {
            format!("{}/changes", string(&identity, "space_id")?)
        } else {
            format!(
                "{}/epochs/{}/changes",
                string(&identity, "space_id")?,
                identity["epoch"]
                    .as_u64()
                    .ok_or_else(|| error("sync_invalid"))?
            )
        };
        let bootstrap_more = self.sync_bootstrap(&identity)?;
        // Only local outstanding facts can require a new namespace. Received
        // members must never generate a synthetic local echo or observed clock.
        let tx = self.begin_write()?;
        let mut namespaces=tx.prepare("SELECT DISTINCT m.id,json_object('id',m.id,'name',m.name) FROM members m JOIN (SELECT json_extract(after_doc,'$.member_id') AS member FROM sync_journal j WHERE entity_type!='members' AND NOT EXISTS(SELECT 1 FROM sync_publications p WHERE p.target_id=?1 AND p.op_id=j.op_id) ORDER BY id LIMIT 256) needed ON needed.member=m.id WHERE NOT EXISTS(SELECT 1 FROM sync_journal WHERE entity_type='members' AND entity_id=m.id) AND NOT EXISTS(SELECT 1 FROM sync_inbox WHERE json_extract(document,'$.entity_type')='members' AND json_extract(document,'$.entity_id')=m.id) LIMIT 32")?;
        let roots = namespaces
            .query_map([&target_key], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(namespaces);
        for (id, document) in roots {
            let base: String = tx.query_row(
                "SELECT COALESCE(json_group_object(device,seq),'{}') FROM sync_clock",
                [],
                |r| r.get(0),
            )?;
            let device = string(&identity, "device_id")?;
            tx.execute(
                "INSERT INTO sync_clock VALUES(?1,1) ON CONFLICT(device) DO UPDATE SET seq=seq+1",
                [device],
            )?;
            let clock: String = tx.query_row(
                "SELECT json_group_object(device,seq) FROM sync_clock",
                [],
                |r| r.get(0),
            )?;
            let op_id = uuid::Uuid::now_v7().to_string();
            tx.execute("INSERT INTO sync_journal(op_id,device,entity_type,entity_id,after_doc,base,clock) VALUES(?1,?2,'members',?3,?4,?5,?6)",params![op_id,device,id,document,base,clock])?;
            tx.execute("INSERT INTO sync_bootstrap_ids VALUES(?1)", [op_id])?;
        }
        tx.commit()?;
        self.sync_publish_assets(&storage, &identity)?;
        self.sync_discover_assets(&storage, &identity)?;
        let mut stmt=self.connection.prepare("SELECT id,op_id,device,entity_type,entity_id,before_doc,after_doc,base,clock,EXISTS(SELECT 1 FROM sync_publications p WHERE p.target_id=?1 AND p.op_id=j.op_id) FROM sync_journal j WHERE NOT EXISTS(SELECT 1 FROM sync_publications p WHERE p.target_id=?1 AND p.op_id=j.op_id) OR registered=0 ORDER BY id LIMIT 256")?;
        let rows = stmt
            .query_map([&target_key], |r| {
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
        for (row, id, device, table, entity, before, after, base, clock, published) in rows {
            let parse =
                |s: &str| serde_json::from_str::<Value>(s).map_err(|_| error("storage_error"));
            let reassert: Option<String> = self
                .connection
                .query_row(
                    "SELECT fields FROM sync_reassert WHERE op_id=?1",
                    [&id],
                    |r| r.get(0),
                )
                .optional()?;
            let reassert = reassert
                .as_deref()
                .map(parse)
                .transpose()?
                .unwrap_or_else(|| json!([]));
            let bootstrap: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_bootstrap_ids WHERE op_id=?1)",
                [&id],
                |r| r.get(0),
            )?;
            let mut op = json!({"reassert":reassert,"protocol_version":1,"op_id":id,"device_id":device,"entity_type":table,"entity_id":entity,"before":before.as_deref().map(parse).transpose()?,"after":parse(&after)?,"base":parse(&base)?,"clock":parse(&clock)?});
            if bootstrap {
                op["bootstrap_identity"] = json!(true);
            }
            self.sync_apply(&op, true)?;
            self.connection
                .execute("UPDATE sync_journal SET registered=1 WHERE id=?1", [row])?;
            if !published {
                self.sync_publish_operation(&storage, &prefix, &target_key, &op)?;
                sent += 1;
            }
        }
        let mut forwarded = self.connection.prepare("SELECT op_id,document FROM sync_inbox i WHERE NOT EXISTS(SELECT 1 FROM sync_publications p WHERE p.target_id=?1 AND p.op_id=i.op_id) ORDER BY rowid LIMIT ?2")?;
        let original_ops = forwarded
            .query_map(params![target_key, 256 - sent], |r| r.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(forwarded);
        for document in original_ops {
            let op: Value = serde_json::from_str(&document).map_err(|_| error("sync_invalid"))?;
            self.sync_publish_operation(&storage, &prefix, &target_key, &op)?;
            sent += 1;
        }
        self.sync_publish_index(&storage, &identity, &target_key, &prefix)?;
        let (indexed_objects, indexed_more) =
            self.sync_index_objects(&storage, &identity, &target_key, &prefix)?;
        let mut received = 0;
        let mut quarantined = 0;
        let mut incoming_count = 0usize;
        let mut incoming_more = indexed_more;
        'incoming: loop {
            let initial = received;
            let mut cursor = None;
            loop {
                let page = if let Some(items) = &indexed_objects {
                    storage::Page {
                        items: items.clone(),
                        cursor: None,
                    }
                } else {
                    storage.list(&prefix, cursor.as_deref(), 64)?
                };
                for object in page.items {
                    if !object.key.ends_with(".commit") {
                        continue;
                    }
                    if self.connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM sync_cursors WHERE target_id=?1 AND key=?2)",
                        params![target_key, object.key],
                        |r| r.get::<_, bool>(0),
                    )? {
                        continue;
                    }
                    if incoming_count == 256 {
                        incoming_more = true;
                        break 'incoming;
                    }
                    incoming_count += 1;
                    let marker: Value = serde_json::from_slice(&bounded(&storage, &object.key)?)
                        .map_err(|_| error("storage_corrupt"))?;
                    let marker = self.sync_decrypt("changes", &marker)?;
                    let bytes = bounded(&storage, string(&marker, "key")?)?;
                    if marker["sha256"] != digest(&bytes) || marker["bytes"] != bytes.len() {
                        return Err(error("storage_corrupt"));
                    }
                    let cipher: Value =
                        serde_json::from_slice(&bytes).map_err(|_| error("storage_corrupt"))?;
                    let op = self.sync_decrypt("changes", &cipher)?;
                    match self.sync_apply(&op, false) {
                        Ok(()) => {}
                        Err(e)
                            if ["sync_dependency", "sync_causal_dependency"]
                                .contains(&e.code.as_str()) =>
                        {
                            self.sync_stage_pending(&target_key, &object.key, &op, &e.code)?;
                            continue;
                        }
                        Err(e)
                            if ["sync_identity_conflict", "sync_invalid", "invalid_input"]
                                .contains(&e.code.as_str()) =>
                        {
                            let tx = self.begin_write()?;
                            tx.execute(
                                "INSERT INTO sync_quarantine VALUES(?1,?2,?3,?4,?5)",
                                params![
                                    target_key,
                                    object.key,
                                    op["op_id"].as_str().unwrap_or("invalid"),
                                    e.code,
                                    op.to_string()
                                ],
                            )?;
                            tx.execute("INSERT INTO audit(entity_id,operation,document) VALUES(?1,'sync.quarantine',?2)",params![op["entity_id"].as_str().unwrap_or("invalid"),json!({"op_id":op["op_id"],"code":e.code}).to_string()])?;
                            tx.execute(
                                "INSERT OR IGNORE INTO sync_cursors VALUES(?1,?2)",
                                params![target_key, object.key],
                            )?;
                            tx.commit()?;
                            quarantined += 1;
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                    let tx = self.begin_write()?;
                    tx.execute("INSERT OR IGNORE INTO sync_publications(target_id,op_id,locally_published) VALUES(?1,?2,0)",params![target_key,string(&op,"op_id")?])?;
                    tx.execute(
                        "INSERT OR IGNORE INTO sync_cursors VALUES(?1,?2)",
                        params![target_key, object.key],
                    )?;
                    tx.commit()?;
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
        let (replayed, replay_quarantined, pending) =
            self.sync_replay_pending(&target_key, 256 - incoming_count)?;
        self.connection.execute("DELETE FROM sync_missing WHERE target_id=?1 AND epoch=?2 AND EXISTS(SELECT 1 FROM sync_cursors c WHERE c.target_id=sync_missing.target_id AND c.key=sync_missing.key)", params![target_key, identity["epoch"].as_u64()])?;
        let missing_markers: u64 = self.connection.query_row(
            "SELECT (SELECT count(*) FROM sync_missing WHERE target_id=?1 AND epoch=?2)+(SELECT count(*) FROM sync_index_errors WHERE target_id=?1 AND epoch=?2)",
            params![target_key, identity["epoch"].as_u64()],
            |row| row.get(0),
        )?;
        received += replayed;
        quarantined += replay_quarantined;
        let missing_assets = self.sync_fetch_assets(&storage)?;
        let has_more: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_journal j WHERE NOT EXISTS(SELECT 1 FROM sync_publications p WHERE p.target_id=?1 AND p.op_id=j.op_id) OR registered=0) OR EXISTS(SELECT 1 FROM sync_inbox i WHERE NOT EXISTS(SELECT 1 FROM sync_publications p WHERE p.target_id=?1 AND p.op_id=i.op_id)) OR EXISTS(SELECT 1 FROM sync_publications p JOIN sync_inbox i ON i.op_id=p.op_id WHERE p.target_id=?1 AND p.locally_published=1 AND NOT EXISTS(SELECT 1 FROM sync_index_entries e WHERE e.target_id=?1 AND e.epoch=?2 AND e.op_id=p.op_id))", params![target_key,identity["epoch"].as_u64().ok_or_else(||error("sync_invalid"))?],|r|r.get(0))?;
        Ok(
            json!({"schema_version":1,"sent":sent,"received":received,"pending":pending + missing_markers,"quarantined":quarantined,"missing_assets":missing_assets,"has_more":bootstrap_more || has_more || incoming_more || (pending > 0 && (incoming_count == 256 || replayed + replay_quarantined > 0))}),
        )
    }
}

pub(crate) fn descriptors() -> Vec<Value> {
    ["configure","run","process","status","history","conflicts","resolve"].into_iter().map(|op|{
        let mut properties=json!({});let mut required=vec![];
        if op=="run" {properties=json!({"operation_key":{"type":"string","minLength":1}});}
        if op=="process" {properties=json!({"id":{"type":"string","minLength":1}});required.push("id");}
        if op=="configure" {properties=json!({"provider_kind":{"const":"local_folder"},"directory":{"type":"string","minLength":1}});required.extend(["provider_kind","directory"]);}
        if ["history","conflicts","resolve"].contains(&op) {properties["member_id"]=json!({"type":"string"});required.push("member_id");}
        if ["history","resolve"].contains(&op) {for key in ["entity_type","entity_id"] {properties[key]=json!({"type":"string"});required.push(key);}}
        if op=="resolve" {for key in ["field","expected_token","operation_key"] {properties[key]=json!({"type":"string","minLength":1});required.push(key);}properties["value"]=json!({});properties["branch_id"]=json!({"type":"string"});}
        let mut schema=json!({"type":"object","additionalProperties":false,"required":required,"properties":properties});
        if op=="resolve" {schema["oneOf"]=json!([{"required":["value"],"not":{"required":["branch_id"]}},{"required":["branch_id"],"not":{"required":["value"]}}]);}
        let output=match op {"run"=>json!({"type":"object","required":["schema_version","job_id","member_id","state"],"properties":{"schema_version":{"const":1},"job_id":{"type":"string"},"member_id":{"type":"string"},"state":{"type":"string"}}}),"process"=>crate::jobs::output_schema(),_=>json!({"type":"object","required":["schema_version"],"properties":{"schema_version":{"type":"integer","const":1}}})};
        json!({"name":format!("sync.{op}"),"description":"同步已配对加密空间：run立即返回持久job_id，process执行有限worker批次；空间级操作需要全部成员授权，冲突选择保留旧历史","inputSchema":schema,"outputSchema":output})
    }).collect()
}
