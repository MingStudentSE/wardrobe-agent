//! Existing inventory becomes original journal facts in durable bounded steps.
use super::*;
impl Application {
    pub(super) fn sync_bootstrap(&self, identity: &Value) -> Result<bool, ApiError> {
        let space = string(identity, "space_id")?;
        let tx = self.begin_write()?;
        for table in TABLES {
            tx.execute(
                "INSERT OR IGNORE INTO sync_bootstrap(space_id,table_name) VALUES(?1,?2)",
                params![space, table],
            )?;
        }
        let owner: bool = tx.query_row(
            "SELECT json_extract(document,'$.owner') FROM encryption_state",
            [],
            |r| r.get(0),
        )?;
        let mut budget = 256usize;
        for table in TABLES {
            let progress: Option<(i64, bool)> = tx
                .query_row(
                    "SELECT last_row,done FROM sync_bootstrap WHERE space_id=?1 AND table_name=?2",
                    params![space, table],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((cursor, done)) = progress else {
                continue;
            };
            if done || budget == 0 {
                continue;
            }
            let physical = super::physical_table(table);
            let key = entity_key(table, physical);
            let document=match table {
                "members"=>"json_object('id',id,'name',name)".to_owned(),
                "feedback_receipts"=>"json_object('schema_version',1,'member_id',member_id,'operation_key',operation_key,'request',request,'result',result)".to_owned(),
                "weather_preferences"=>"json_remove(document,'$.config.endpoint','$.cache','$.last_error')".to_owned(),
                "categories"=>super::CATEGORY_DOCUMENT.to_owned(),
                _=>"document".to_owned(),
            };
            let filter = if table == "members" {
                " AND (?3 OR EXISTS(SELECT 1 FROM clothing c WHERE c.member_id=members.id))"
            } else {
                " AND (?3 OR 1)"
            };
            let sql=format!("SELECT rowid,{key},{document} FROM {physical} WHERE rowid>?1{filter} ORDER BY rowid LIMIT ?2");
            let mut statement = tx.prepare(&sql)?;
            let rows = statement
                .query_map(params![cursor, budget, owner], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            let complete = rows.len() < budget;
            let mut last = cursor;
            for (row, id, document) in &rows {
                last = *row;
                if publication::has_original(&tx, table, id, document)? {
                    continue;
                }
                let base: String = tx.query_row(
                    "SELECT COALESCE(json_group_object(device,seq),'{}') FROM sync_clock",
                    [],
                    |r| r.get(0),
                )?;
                let device = string(identity, "device_id")?;
                tx.execute("INSERT INTO sync_clock VALUES(?1,1) ON CONFLICT(device) DO UPDATE SET seq=seq+1",[device])?;
                let clock: String = tx.query_row(
                    "SELECT json_group_object(device,seq) FROM sync_clock",
                    [],
                    |r| r.get(0),
                )?;
                let op_id = uuid::Uuid::now_v7().to_string();
                tx.execute("INSERT INTO sync_journal(op_id,device,entity_type,entity_id,after_doc,base,clock) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![op_id,device,table,id,document,base,clock])?;
                if table == "members" {
                    tx.execute("INSERT INTO sync_bootstrap_ids VALUES(?1)", [op_id])?;
                }
            }
            budget -= rows.len();
            tx.execute(
                "UPDATE sync_bootstrap SET last_row=?3,done=?4 WHERE space_id=?1 AND table_name=?2",
                params![space, table, last, complete],
            )?;
        }
        let more: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_bootstrap WHERE space_id=?1 AND done=0)",
            [space],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(more)
    }
}
