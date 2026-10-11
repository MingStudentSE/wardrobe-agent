use super::*;

/// Only the latest numeric revision participates in review; prior branches remain immutable.
pub(super) fn conflicts(db: &rusqlite::Connection, member: &str) -> Result<Vec<Value>, ApiError> {
    let mut result = vec![];
    for table in [
        "wear_events",
        "care_events",
        "feedback_events",
        "outfit_versions",
    ] {
        let sql = if table == "outfit_versions" {
            "SELECT v.outfit_id,v.document FROM outfit_versions v JOIN outfits o ON o.id=v.outfit_id WHERE o.member_id=?1 AND v.version=(SELECT max(version) FROM outfit_versions WHERE outfit_id=v.outfit_id) ORDER BY v.outfit_id,v.document".to_owned()
        } else {
            let root = if table == "feedback_events" {
                "root_id"
            } else {
                "root_event_id"
            };
            format!("SELECT e.{root},e.document FROM {table} e WHERE e.member_id=?1 AND e.revision=(SELECT max(revision) FROM {table} WHERE {root}=e.{root}) ORDER BY e.{root},e.document")
        };
        let mut statement = db.prepare(&sql)?;
        let rows = statement
            .query_map([member], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut grouped: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (id, raw) in rows {
            let doc: Value = serde_json::from_str(&raw).map_err(|_| error("storage_error"))?;
            let variants = grouped.entry(id).or_default();
            if !variants.iter().any(|v| v["document"] == doc) {
                variants
                    .push(json!({"branch_id":digest(doc.to_string().as_bytes()),"document":doc}));
            }
        }
        for (id, variants) in grouped {
            if variants.len() > 1 {
                result.push(json!({"entity_type":table,"entity_id":id,"field":"branch","token":digest(json!(variants).to_string().as_bytes()),"variants":variants}));
            }
        }
    }
    Ok(result)
}

pub(super) fn resolve(
    tx: &Transaction<'_>,
    member: &str,
    input: &Value,
) -> Result<Value, ApiError> {
    let table = string(input, "entity_type")?;
    let entity = string(input, "entity_id")?;
    let conflict = conflicts(tx, member)?
        .into_iter()
        .find(|v| v["entity_type"] == table && v["entity_id"] == entity)
        .ok_or_else(|| error("conflict_changed"))?;
    if conflict["token"] != input["expected_token"] {
        return Err(error("conflict_changed"));
    }
    let selected = conflict["variants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["branch_id"] == input["branch_id"])
        .ok_or_else(|| error("invalid_input"))?;
    let mut doc = selected["document"].clone();
    let now = json!(chrono::Utc::now().to_rfc3339());
    if table == "outfit_versions" {
        let current: String = tx.query_row(
            "SELECT document FROM outfits WHERE id=?1 AND member_id=?2",
            params![entity, member],
            |r| r.get(0),
        )?;
        let current: Value = serde_json::from_str(&current).map_err(|_| error("storage_error"))?;
        if current["status"] == "deleted" {
            return Err(error("restore_required"));
        }
        let max: u64 = tx.query_row(
            "SELECT max(version) FROM outfit_versions WHERE outfit_id=?1",
            [entity],
            |r| r.get(0),
        )?;
        doc["version"] = json!(max + 1);
        doc["updated_at"] = now;
        // Lifecycle and favorite are independent decisions, not part of template-content review.
        doc["status"] = current["status"].clone();
        doc["favorite"] = current["favorite"].clone();
        materialize(tx, "outfits", entity, &doc, false)?;
        materialize(tx, table, &format!("{entity}:{}", max + 1), &doc, false)?;
        let (op_id,clock):(String,String)=tx.query_row("SELECT op_id,clock FROM sync_journal WHERE entity_type='outfits' AND entity_id=?1 ORDER BY id DESC LIMIT 1",[entity],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let fields = json!(["name", "items", "version", "updated_at"]);
        tx.execute(
            "INSERT INTO sync_reassert VALUES(?1,?2)",
            params![op_id, fields.to_string()],
        )?;
        let clock: Value = serde_json::from_str(&clock).map_err(|_| error("storage_error"))?;
        for field in fields.as_array().unwrap() {
            let field = field.as_str().unwrap();
            super::conflicts::register(
                tx,
                "outfits",
                entity,
                field,
                &doc[field],
                &json!({"op_id":op_id,"clock":clock}),
            )?;
        }
    } else {
        let root = if table == "feedback_events" {
            "root_id"
        } else {
            "root_event_id"
        };
        let max: u64 = tx.query_row(
            &format!("SELECT max(revision) FROM {table} WHERE {root}=?1 AND member_id=?2"),
            params![entity, member],
            |r| r.get(0),
        )?;
        doc["supersedes_event_id"] = doc["id"].clone();
        doc["id"] = json!(uuid::Uuid::now_v7().to_string());
        doc["revision"] = json!(max + 1);
        if table != "feedback_events" && doc["kind"] != "revocation" {
            doc["kind"] = json!("correction");
        }
        doc["created_at"] = now;
        materialize(tx, table, string(&doc, "id")?, &doc, false)?;
    }
    tx.execute(
        "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'sync.resolve',?2)",
        params![entity, input.to_string()],
    )?;
    Ok(json!({"schema_version":1,"document":doc}))
}
