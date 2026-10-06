//! Durable immutable publisher pages. A head is published only after its page and
//! referenced commit markers exist. Random operation names never act as a cursor.
use super::*;
impl Application {
    pub(super) fn sync_publish_index(
        &self,
        storage: &dyn SyncStorage,
        identity: &Value,
        target: &str,
        changes: &str,
    ) -> Result<(), ApiError> {
        let publisher = string(identity, "device_id")?;
        let epoch = identity["epoch"]
            .as_u64()
            .ok_or_else(|| error("sync_invalid"))?;
        let space = string(identity, "space_id")?;
        let namespace = format!("{space}/epochs/{epoch}/index/{publisher}");
        let prepared: Option<String> = self.connection.query_row(
            "SELECT document FROM sync_index_batches WHERE target_id=?1 AND epoch=?2 AND published=0 ORDER BY page LIMIT 1",
            params![target,epoch], |r|r.get(0),
        ).optional()?;
        let page: Value = if let Some(prepared) = prepared {
            serde_json::from_str(&prepared).map_err(|_| error("storage_corrupt"))?
        } else {
            let mut statement = self.connection.prepare("SELECT j.op_id FROM sync_journal j JOIN sync_publications p ON p.op_id=j.op_id AND p.target_id=?1 WHERE NOT EXISTS(SELECT 1 FROM sync_index_entries e WHERE e.target_id=?1 AND e.epoch=?2 AND e.op_id=j.op_id) ORDER BY j.id LIMIT 256")?;
            let ids = statement
                .query_map(params![target, epoch], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            if ids.is_empty() {
                return Ok(());
            }
            let mut entries = Vec::with_capacity(ids.len());
            for id in ids {
                let key = format!("{changes}/{id}.commit");
                let bytes = bounded(storage, &key)?;
                entries.push(
                    json!({"op_id":id,"key":key,"digest":digest(&bytes),"bytes":bytes.len()}),
                );
            }
            let tx = self.begin_write()?;
            let previous: Option<(u64,String)> = tx.query_row("SELECT page,document FROM sync_index_batches WHERE target_id=?1 AND epoch=?2 ORDER BY page DESC LIMIT 1",params![target,epoch],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let (number, previous_digest) = previous
                .map(|(number, doc)| (number + 1, Value::String(digest(doc.as_bytes()))))
                .unwrap_or((1, Value::Null));
            let page = json!({"protocol_version":1,"space_id":space,"epoch":epoch,"publisher":publisher,"page":number,"previous_digest":previous_digest,"entries":entries});
            tx.execute("INSERT INTO sync_index_batches(target_id,epoch,page,document,published) VALUES(?1,?2,?3,?4,0)",params![target,epoch,number,page.to_string()])?;
            tx.commit()?;
            page
        };
        let number = page["page"]
            .as_u64()
            .ok_or_else(|| error("storage_corrupt"))?;
        let page_key = format!("{namespace}/{number:020}.page");
        let encrypted = self.sync_encrypt("changes", &page)?;
        match storage.stat(&page_key) {
            Ok(_) => {
                let value: Value = serde_json::from_slice(&bounded(storage, &page_key)?)
                    .map_err(|_| error("storage_corrupt"))?;
                if self.sync_decrypt("changes", &value)? != page {
                    return Err(error("sync_identity_conflict"));
                }
            }
            Err(e) if e.code == "storage_not_found" => {
                storage.put(
                    &page_key,
                    &mut encrypted.to_string().as_bytes(),
                    WriteCondition::CreateOnly,
                )?;
            }
            Err(e) => return Err(e),
        }
        let page_bytes = bounded(storage, &page_key)?;
        let head = json!({"protocol_version":1,"space_id":space,"epoch":epoch,"publisher":publisher,"pages":number,"last_page_digest":digest(&page_bytes)});
        let head_key = format!("{namespace}/head.index");
        let condition = match storage.stat(&head_key) {
            Ok(meta) => {
                let old: Value = serde_json::from_slice(&bounded(storage, &head_key)?)
                    .map_err(|_| error("storage_corrupt"))?;
                let old = self.sync_decrypt("changes", &old)?;
                if old == head {
                    None
                } else {
                    if old["space_id"] != space
                        || old["epoch"] != epoch
                        || old["publisher"] != publisher
                        || old["protocol_version"] != 1
                        || old["pages"].as_u64() != number.checked_sub(1)
                    {
                        return Err(error("sync_identity_conflict"));
                    }
                    Some(WriteCondition::MatchVersion(meta.version))
                }
            }
            Err(e) if e.code == "storage_not_found" => {
                if number != 1 {
                    return Err(error("storage_corrupt"));
                }
                Some(WriteCondition::CreateOnly)
            }
            Err(e) => return Err(e),
        };
        if let Some(condition) = condition {
            let encrypted = self.sync_encrypt("changes", &head)?;
            storage.put(&head_key, &mut encrypted.to_string().as_bytes(), condition)?;
        }
        let tx = self.begin_write()?;
        for entry in page["entries"]
            .as_array()
            .ok_or_else(|| error("storage_corrupt"))?
        {
            tx.execute("INSERT OR IGNORE INTO sync_index_entries(target_id,epoch,op_id,page) VALUES(?1,?2,?3,?4)",params![target,epoch,string(entry,"op_id")?,number])?;
        }
        tx.execute(
            "UPDATE sync_index_batches SET published=1 WHERE target_id=?1 AND epoch=?2 AND page=?3",
            params![target, epoch, number],
        )?;
        tx.commit()?;
        Ok(())
    }
}

impl Application {
    /// Read authenticated publisher pages by ordinal, retaining only one bounded
    /// batch. Missing index support uses the explicitly supported legacy flat path.
    pub(super) fn sync_index_objects(
        &self,
        storage: &dyn SyncStorage,
        identity: &Value,
        target: &str,
        changes: &str,
    ) -> Result<(Option<Vec<storage::ObjectMeta>>, bool), ApiError> {
        use base64::Engine;
        let manifest = base64::engine::general_purpose::STANDARD
            .decode(string(&identity["manifest"], "payload_base64")?)
            .map_err(|_| error("sync_invalid"))?;
        let manifest: Value =
            serde_json::from_slice(&manifest).map_err(|_| error("sync_invalid"))?;
        let space = string(identity, "space_id")?;
        let epoch = identity["epoch"]
            .as_u64()
            .ok_or_else(|| error("sync_invalid"))?;
        let mut objects = Vec::new();
        let mut supported = false;
        let mut more = false;
        for device in manifest["devices"]
            .as_array()
            .ok_or_else(|| error("sync_invalid"))?
        {
            let publisher = string(device, "device_id")?;
            let namespace = format!("{space}/epochs/{epoch}/index/{publisher}");
            let head_key = format!("{namespace}/head.index");
            let bytes = match bounded(storage, &head_key) {
                Ok(bytes) => bytes,
                Err(e) if e.code == "storage_not_found" => continue,
                Err(e) => return Err(e),
            };
            supported = true;
            let envelope: Value =
                serde_json::from_slice(&bytes).map_err(|_| error("storage_corrupt"))?;
            if envelope["identity"]["device_id"] != publisher {
                return Err(error("storage_corrupt"));
            }
            let head = self.sync_decrypt("changes", &envelope)?;
            let pages = head["pages"]
                .as_u64()
                .filter(|n| *n > 0 && *n < i64::MAX as u64)
                .ok_or_else(|| error("storage_corrupt"))?;
            if head.as_object().map(|o| o.len()) != Some(6)
                || head["protocol_version"] != 1
                || head["space_id"] != space
                || head["epoch"] != epoch
                || head["publisher"] != publisher
            {
                return Err(error("storage_corrupt"));
            }
            let prior: Option<(u64,u64,Option<String>)> = self.connection.query_row("SELECT next_page,observed_head,previous_digest FROM sync_index_read WHERE target_id=?1 AND epoch=?2 AND publisher=?3",params![target,epoch,publisher],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            let (mut number, observed, mut previous) = prior.unwrap_or((1, 0, None));
            if pages < observed {
                return Err(error("storage_corrupt"));
            }
            let mut read_pages = 0;
            while number <= pages {
                if objects.len() == 256 || read_pages == 2 {
                    more = true;
                    break;
                }
                let page_key = format!("{namespace}/{number:020}.page");
                let bytes = bounded(storage, &page_key)?;
                if number == pages && head["last_page_digest"] != digest(&bytes) {
                    return Err(error("storage_corrupt"));
                }
                let envelope: Value =
                    serde_json::from_slice(&bytes).map_err(|_| error("storage_corrupt"))?;
                if envelope["identity"]["device_id"] != publisher {
                    return Err(error("storage_corrupt"));
                }
                let page = self.sync_decrypt("changes", &envelope)?;
                if page.as_object().map(|o| o.len()) != Some(7)
                    || page["protocol_version"] != 1
                    || page["space_id"] != space
                    || page["epoch"] != epoch
                    || page["publisher"] != publisher
                    || page["page"] != number
                    || page["previous_digest"] != json!(previous)
                {
                    return Err(error("storage_corrupt"));
                }
                let entries = page["entries"]
                    .as_array()
                    .filter(|v| !v.is_empty() && v.len() <= 256)
                    .ok_or_else(|| error("storage_corrupt"))?;
                let mut complete = true;
                for entry in entries {
                    let id = string(entry, "op_id")?;
                    let key = string(entry, "key")?;
                    if entry.as_object().map(|o| o.len()) != Some(4)
                        || uuid::Uuid::parse_str(id).is_err()
                        || key != format!("{changes}/{id}.commit")
                    {
                        return Err(error("storage_corrupt"));
                    }
                    let consumed: bool = self.connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM sync_cursors WHERE target_id=?1 AND key=?2)",
                        params![target, key],
                        |r| r.get(0),
                    )?;
                    if consumed {
                        continue;
                    }
                    complete = false;
                    if objects.len() == 256 {
                        more = true;
                        break;
                    }
                    let marker = bounded(storage, key)?;
                    if entry["digest"] != digest(&marker) || entry["bytes"] != marker.len() {
                        return Err(error("storage_corrupt"));
                    }
                    objects.push(storage::ObjectMeta {
                        key: key.to_owned(),
                        version: digest(&marker),
                        bytes: marker.len() as u64,
                    });
                }
                read_pages += 1;
                if !complete {
                    if number < pages {
                        more = true;
                    }
                    break;
                }
                previous = Some(digest(page.to_string().as_bytes()));
                number += 1;
                self.connection.execute("INSERT INTO sync_index_read(target_id,epoch,publisher,next_page,observed_head,previous_digest) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(target_id,epoch,publisher) DO UPDATE SET next_page=excluded.next_page,observed_head=excluded.observed_head,previous_digest=excluded.previous_digest",params![target,epoch,publisher,number,pages,previous])?;
            }
        }
        Ok((supported.then_some(objects), more))
    }
}
