use super::*;
impl Application {
    pub(super) fn sync_stage_pending(
        &self,
        target: &str,
        key: &str,
        op: &Value,
        reason: &str,
    ) -> Result<(), ApiError> {
        let tx = self.begin_write()?;
        tx.execute("INSERT INTO sync_pending(target_id,marker_key,op_id,document,reason,attempts) VALUES(?1,?2,?3,?4,?5,0) ON CONFLICT(target_id,marker_key) DO NOTHING",params![target,key,string(op,"op_id")?,op.to_string(),reason])?;
        tx.execute(
            "INSERT OR IGNORE INTO sync_cursors VALUES(?1,?2)",
            params![target, key],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// Bounded and fair: permanent historical dependencies cannot monopolize later arrivals.
    pub(super) fn sync_replay_pending(&self, target: &str) -> Result<(u64, u64, u64), ApiError> {
        let mut applied = 0;
        let mut quarantined = 0;
        for _ in 0..8 {
            let mut statement=self.connection.prepare("SELECT marker_key,op_id,document FROM sync_pending WHERE target_id=?1 ORDER BY attempts,op_id LIMIT 256")?;
            let rows = statement
                .query_map([target], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            let mut progress = false;
            for (key, id, raw) in rows {
                let op: Value = serde_json::from_str(&raw).map_err(|_| error("storage_error"))?;
                match self.sync_apply(&op, false) {
                    Ok(()) => {
                        self.connection.execute(
                            "DELETE FROM sync_pending WHERE target_id=?1 AND marker_key=?2",
                            params![target, key],
                        )?;
                        applied += 1;
                        progress = true;
                    }
                    Err(e) if e.code == "sync_dependency" => {
                        self.connection.execute("UPDATE sync_pending SET attempts=attempts+1,reason=?3 WHERE target_id=?1 AND marker_key=?2",params![target,key,e.code])?;
                    }
                    Err(e)
                        if ["sync_identity_conflict", "sync_invalid", "invalid_input"]
                            .contains(&e.code.as_str()) =>
                    {
                        let tx = self.begin_write()?;
                        tx.execute(
                            "INSERT OR IGNORE INTO sync_quarantine VALUES(?1,?2,?3,?4,?5)",
                            params![target, key, id, e.code, raw],
                        )?;
                        tx.execute("INSERT INTO audit(entity_id,operation,document) VALUES(?1,'sync.quarantine',?2)",params![op["entity_id"].as_str().unwrap_or("invalid"),json!({"op_id":id,"code":e.code}).to_string()])?;
                        tx.execute(
                            "DELETE FROM sync_pending WHERE target_id=?1 AND marker_key=?2",
                            params![target, key],
                        )?;
                        tx.commit()?;
                        quarantined += 1;
                        progress = true;
                    }
                    Err(e) => return Err(e),
                }
            }
            if !progress {
                break;
            }
        }
        let pending = self.connection.query_row(
            "SELECT count(*) FROM sync_pending WHERE target_id=?1",
            [target],
            |r| r.get::<_, u64>(0),
        )?;
        Ok((applied, quarantined, pending))
    }
}
