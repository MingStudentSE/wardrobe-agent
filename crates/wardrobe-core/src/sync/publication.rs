use super::*;
impl Application {
    /// Relay the original immutable operation, retaining its origin clock/id. The
    /// current authorized publisher transports it without inventing a local fact.
    pub(super) fn sync_publish_operation(
        &self,
        storage: &dyn SyncStorage,
        prefix: &str,
        target: &str,
        op: &Value,
    ) -> Result<(), ApiError> {
        schema::validate_operation(op)?;
        let table = string(op, "entity_type")?;
        let entity = string(op, "entity_id")?;
        schema::validate(table, entity, &op["after"])?;
        if !op["before"].is_null() {
            schema::validate(table, entity, &op["before"])?;
        }
        let id = string(op, "op_id")?;
        let key = format!("{prefix}/{id}.object");
        match storage.stat(&key) {
            Ok(_) => {
                let cipher: Value = serde_json::from_slice(&bounded(storage, &key)?)
                    .map_err(|_| error("storage_corrupt"))?;
                if self.sync_decrypt("changes", &cipher)? != *op {
                    return Err(error("sync_identity_conflict"));
                }
            }
            Err(e) if e.code == "storage_not_found" => {
                let cipher = self.sync_encrypt("changes", op)?;
                storage.put(
                    &key,
                    &mut cipher.to_string().as_bytes(),
                    WriteCondition::CreateOnly,
                )?;
            }
            Err(e) => return Err(e),
        }
        let stored = bounded(storage, &key)?;
        let marker = json!({"key":key,"sha256":digest(&stored),"bytes":stored.len()});
        let marker_key = format!("{prefix}/{id}.commit");
        match storage.stat(&marker_key) {
            Ok(_) => {
                let cipher: Value = serde_json::from_slice(&bounded(storage, &marker_key)?)
                    .map_err(|_| error("storage_corrupt"))?;
                if self.sync_decrypt("changes", &cipher)? != marker {
                    return Err(error("sync_identity_conflict"));
                }
            }
            Err(e) if e.code == "storage_not_found" => {
                let cipher = self.sync_encrypt("changes", &marker)?;
                storage.put(
                    &marker_key,
                    &mut cipher.to_string().as_bytes(),
                    WriteCondition::CreateOnly,
                )?;
            }
            Err(e) => return Err(e),
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO sync_publications(target_id,op_id,locally_published) VALUES(?1,?2,1)",
            params![target, id],
        )?;
        Ok(())
    }
}

pub(super) fn has_original(
    tx: &Transaction<'_>,
    table: &str,
    entity: &str,
    document: &str,
) -> Result<bool, ApiError> {
    let expected: Value = serde_json::from_str(document).map_err(|_| error("sync_invalid"))?;
    let mut statement = tx.prepare("SELECT after_doc,before_doc IS NULL FROM sync_journal WHERE entity_type=?1 AND entity_id=?2 UNION ALL SELECT json_extract(document,'$.after'),json_type(document,'$.before')='null' FROM sync_inbox WHERE json_extract(document,'$.entity_type')=?1 AND json_extract(document,'$.entity_id')=?2")?;
    for row in statement.query_map(params![table, entity], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
    })? {
        let (after, creation) = row?;
        if table == "members" && !creation {
            continue;
        }
        if ![
            "outfit_versions",
            "wear_events",
            "care_events",
            "feedback_events",
            "feedback_receipts",
        ]
        .contains(&table)
        {
            return Ok(true);
        }
        let after: Value = serde_json::from_str(&after).map_err(|_| error("sync_invalid"))?;
        if after == expected {
            return Ok(true);
        }
    }
    Ok(false)
}
