use super::*;
pub(super) const SETS: [&str; 7] = [
    "color", "pattern", "material", "style", "occasion", "tags", "seasons",
];
pub(super) fn register(
    tx: &Transaction<'_>,
    table: &str,
    entity: &str,
    field: &str,
    value: &Value,
    op: &Value,
) -> Result<(), ApiError> {
    let old: Option<String> = tx
        .query_row(
            "SELECT variants FROM sync_fields WHERE entity_type=?1 AND entity_id=?2 AND field=?3",
            params![table, entity, field],
            |r| r.get(0),
        )
        .optional()?;
    let mut variants: Vec<Value> = old
        .map(|s| serde_json::from_str(&s).map_err(|_| error("storage_error")))
        .transpose()?
        .unwrap_or_default();
    if variants
        .iter()
        .any(|v| dominates(&v["clock"], &op["clock"]))
    {
        return Ok(());
    }
    variants.retain(|v| !dominates(&op["clock"], &v["clock"]));
    variants.push(json!({"value":value,"clock":op["clock"],"op_id":op["op_id"]}));
    variants.sort_by(|a, b| a["op_id"].as_str().cmp(&b["op_id"].as_str()));
    tx.execute("INSERT INTO sync_fields VALUES(?1,?2,?3,?4) ON CONFLICT(entity_type,entity_id,field) DO UPDATE SET variants=excluded.variants",params![table,entity,field,serde_json::to_string(&variants).map_err(|_|error("sync_invalid"))?])?;
    Ok(())
}
pub(super) fn set_register(
    tx: &Transaction<'_>,
    table: &str,
    entity: &str,
    field: &str,
    old: &Value,
    new: &Value,
    op: &Value,
) -> Result<(), ApiError> {
    register(tx, table, entity, field, &json!([]), op)?;
    let old = old.as_array().cloned().unwrap_or_default();
    let new = new.as_array().ok_or_else(|| error("sync_invalid"))?;
    for value in old.iter().chain(new) {
        let label = value.as_str().ok_or_else(|| error("sync_invalid"))?;
        let normalized = label.to_lowercase();
        let existed = old
            .iter()
            .any(|v| v.as_str().is_some_and(|s| s.to_lowercase() == normalized));
        let exists = new
            .iter()
            .any(|v| v.as_str().is_some_and(|s| s.to_lowercase() == normalized));
        if existed == exists {
            continue;
        }
        let key = format!(
            "{field}#{}",
            base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                normalized.as_bytes()
            )
        );
        register(
            tx,
            table,
            entity,
            &key,
            &json!({"present":exists,"label":label}),
            op,
        )?;
    }
    Ok(())
}
impl Application {
    pub(super) fn sync_conflicts(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let mut stmt=self.connection.prepare("SELECT entity_type,entity_id,field,variants FROM sync_fields ORDER BY entity_type,entity_id,field")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        let mut items = vec![];
        for (table, id, field, raw) in rows {
            if field.contains('#')
                || SETS.contains(&field.as_str())
                || ["updated_at", "created_at", "revision", "version"].contains(&field.as_str())
            {
                continue;
            }
            let variants: Vec<Value> =
                serde_json::from_str(&raw).map_err(|_| error("storage_error"))?;
            let mut values = vec![];
            for v in &variants {
                if !values.contains(&v["value"]) {
                    values.push(v["value"].clone());
                }
            }
            if values.len() < 2 {
                continue;
            }
            let allowed = if table == "members" {
                id == member
            } else {
                let sql=match table.as_str(){"clothing"|"outfits"=>format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1 AND member_id=?2)"),"care_config"=>"SELECT EXISTS(SELECT 1 FROM care_config WHERE clothing_id=?1 AND member_id=?2)".into(),"insights_preferences"=>"SELECT EXISTS(SELECT 1 FROM insights_preferences WHERE member_id=?1 AND member_id=?2)".into(),_=>continue};
                self.connection
                    .query_row(&sql, params![id, member], |r| r.get::<_, bool>(0))?
            };
            if allowed {
                items.push(json!({"entity_type":table,"entity_id":id,"field":field,"variants":variants,"token":digest(raw.as_bytes())}));
            }
        }
        items.extend(lifecycle(&self.connection, &member)?);
        items.extend(super::branches::conflicts(&self.connection, &member)?);
        Ok(json!({"schema_version":1,"items":items}))
    }
    pub(super) fn sync_resolve(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let table = string(&input, "entity_type")?;
        let id = string(&input, "entity_id")?;
        let field = string(&input, "field")?;
        let key = string(&input, "operation_key")?;
        let tx = self.begin_write()?;
        let previous:Option<(String,String)>=tx.query_row("SELECT request,result FROM sync_resolution_receipts WHERE member_id=?1 AND operation_key=?2",params![member,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((request, result)) = previous {
            if request != input.to_string() {
                return Err(error("operation_key_conflict"));
            }
            return serde_json::from_str(&result).map_err(|_| error("storage_error"));
        }
        if field == "branch" && immutable(table) {
            let result = super::branches::resolve(&tx, &member, &input)?;
            tx.execute(
                "INSERT INTO sync_resolution_receipts VALUES(?1,?2,?3,?4)",
                params![member, key, input.to_string(), result.to_string()],
            )?;
            tx.commit()?;
            return Ok(result);
        }
        if table != "clothing" {
            return Err(error("sync_resolution_unsupported"));
        }
        let field = if field == "lifecycle" {
            let conflict = lifecycle(&tx, &member)?
                .into_iter()
                .find(|v| v["entity_id"] == id)
                .ok_or_else(|| error("conflict_changed"))?;
            if input["expected_token"] != conflict["token"] {
                return Err(error("conflict_changed"));
            }
            "status"
        } else {
            let raw:String=tx.query_row("SELECT variants FROM sync_fields WHERE entity_type=?1 AND entity_id=?2 AND field=?3",params![table,id,field],|r|r.get(0)).map_err(|_|error("conflict_changed"))?;
            if input["expected_token"] != digest(raw.as_bytes()) {
                return Err(error("conflict_changed"));
            }
            field
        };
        let serialized: String = tx
            .query_row(
                "SELECT document FROM clothing WHERE id=?1 AND member_id=?2",
                params![id, member],
                |r| r.get(0),
            )
            .map_err(|_| error("not_found"))?;
        let mut doc: Value =
            serde_json::from_str(&serialized).map_err(|_| error("storage_error"))?;
        if doc["status"] == "deleted" && field != "status" {
            return Err(error("restore_required"));
        }
        let mut facts = serde_json::Map::new();
        facts.insert(
            field.into(),
            input
                .get("value")
                .cloned()
                .ok_or_else(|| error("invalid_input"))?,
        );
        if field == "status" {
            if !["active", "deleted"].iter().any(|v| input["value"] == *v) {
                return Err(error("invalid_input"));
            }
            doc["status"] = input["value"].clone();
            if doc["status"] == "active" {
                doc["deleted_at"] = Value::Null;
            }
        } else {
            crate::clothing::apply_facts(&mut doc, &Value::Object(facts))?;
        }
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE clothing SET document=?1 WHERE id=?2 AND member_id=?3",
            params![doc.to_string(), id, member],
        )?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'sync.resolve',?2)",
            params![id, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![id, doc.to_string()],
        )?;
        let (op_id,clock):(String,String)=tx.query_row("SELECT op_id,clock FROM sync_journal WHERE entity_type='clothing' AND entity_id=?1 ORDER BY id DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        tx.execute(
            "INSERT INTO sync_reassert VALUES(?1,?2)",
            params![op_id, json!([field]).to_string()],
        )?;
        let clock: Value = serde_json::from_str(&clock).map_err(|_| error("storage_error"))?;
        register(
            &tx,
            table,
            id,
            field,
            &doc[field],
            &json!({"op_id":op_id,"clock":clock}),
        )?;
        let result = json!({"schema_version":1,"document":doc});
        tx.execute(
            "INSERT INTO sync_resolution_receipts VALUES(?1,?2,?3,?4)",
            params![member, key, input.to_string(), result.to_string()],
        )?;
        tx.commit()?;
        Ok(result)
    }
}
fn lifecycle(db: &rusqlite::Connection, member: &str) -> Result<Vec<Value>, ApiError> {
    let mut result = vec![];
    let mut st=db.prepare("SELECT c.id,s.variants FROM clothing c JOIN sync_fields s ON s.entity_type='clothing' AND s.entity_id=c.id AND s.field='status' WHERE c.member_id=?1 AND json_extract(c.document,'$.status')='deleted' ORDER BY c.id")?;
    let rows = st
        .query_map([member], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(st);
    for (id, raw) in rows {
        let statuses: Vec<Value> =
            serde_json::from_str(&raw).map_err(|_| error("storage_error"))?;
        let mut variants = vec![];
        let mut st=db.prepare("SELECT field,variants FROM sync_fields WHERE entity_type='clothing' AND entity_id=?1 ORDER BY field")?;
        let fields = st
            .query_map([&id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for deleted in statuses.iter().filter(|v| v["value"] == "deleted") {
            for (field, raw) in &fields {
                if [
                    "status",
                    "updated_at",
                    "created_at",
                    "id",
                    "member_id",
                    "schema_version",
                    "taxonomy_version",
                ]
                .contains(&field.as_str())
                {
                    continue;
                }
                let values: Vec<Value> =
                    serde_json::from_str(raw).map_err(|_| error("storage_error"))?;
                for edit in values {
                    if !dominates(&deleted["clock"], &edit["clock"])
                        && !dominates(&edit["clock"], &deleted["clock"])
                    {
                        if variants.is_empty() {
                            variants.push(json!({"field":"status","value":"deleted","clock":deleted["clock"],"op_id":deleted["op_id"]}));
                        }
                        variants.push(json!({"field":field,"value":edit["value"],"clock":edit["clock"],"op_id":edit["op_id"]}));
                    }
                }
            }
        }
        if variants.len() > 1 {
            result.push(json!({"entity_type":"clothing","entity_id":id,"field":"lifecycle","token":digest(json!(variants).to_string().as_bytes()),"variants":variants}));
        }
    }
    Ok(result)
}
