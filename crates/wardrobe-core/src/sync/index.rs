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
        // Advertise the indexed protocol even when this publisher has no facts.
        // Peers can distinguish a genuinely empty upgraded publisher from legacy data.
        let mode_key = format!("{namespace}/mode.index");
        let mode = json!({"protocol_version":1,"space_id":space,"epoch":epoch,"publisher":publisher,"mode":"indexed"});
        match storage.stat(&mode_key) {
            Ok(_) => {
                let envelope: Value = serde_json::from_slice(&bounded(storage, &mode_key)?)
                    .map_err(|_| error("storage_corrupt"))?;
                if envelope["identity"]["device_id"] != publisher
                    || self.sync_decrypt("changes", &envelope)? != mode
                {
                    return Err(error("storage_corrupt"));
                }
            }
            Err(e) if e.code == "storage_not_found" => {
                let envelope = self.sync_encrypt("changes", &mode)?;
                storage.put(
                    &mode_key,
                    &mut envelope.to_string().as_bytes(),
                    WriteCondition::CreateOnly,
                )?;
            }
            Err(e) => return Err(e),
        }
        let prepared: Option<String> = self.connection.query_row(
            "SELECT document FROM sync_index_batches WHERE target_id=?1 AND epoch=?2 AND published=0 ORDER BY page LIMIT 1",
            params![target,epoch], |r|r.get(0),
        ).optional()?;
        let page: Value = if let Some(prepared) = prepared {
            serde_json::from_str(&prepared).map_err(|_| error("storage_corrupt"))?
        } else {
            let mut statement = self.connection.prepare("SELECT i.op_id FROM sync_inbox i JOIN sync_publications p ON p.op_id=i.op_id AND p.target_id=?1 AND p.locally_published=1 WHERE NOT EXISTS(SELECT 1 FROM sync_index_entries e WHERE e.target_id=?1 AND e.epoch=?2 AND e.op_id=i.op_id) ORDER BY i.rowid LIMIT 256")?;
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
    /// Durable publisher-index cursor persistence. Overlapping local workers may
    /// hold superseded snapshots, so the live row is advanced inside the write
    /// statement itself: observed heads never move backwards, and the previous
    /// page digest is only replaced together with a forward cursor move so the
    /// authenticated page chain stays verifiable. A genuinely rolled-back remote
    /// head is rejected earlier by the freshly-read frontier check.
    fn sync_persist_index_read(
        &self,
        target: &str,
        epoch: u64,
        publisher: &str,
        next_page: u64,
        observed_head: u64,
        previous_digest: Option<&str>,
    ) -> Result<(), ApiError> {
        self.connection.execute(
            "INSERT INTO sync_index_read(target_id,epoch,publisher,next_page,observed_head,previous_digest) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(target_id,epoch,publisher) DO UPDATE SET observed_head=max(observed_head,excluded.observed_head), next_page=CASE WHEN excluded.next_page>next_page THEN excluded.next_page ELSE next_page END, previous_digest=CASE WHEN excluded.next_page>next_page THEN excluded.previous_digest ELSE previous_digest END",
            params![target, epoch, publisher, next_page, observed_head, previous_digest],
        )?;
        Ok(())
    }

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
        // Discovery of an absent marker is durable work, independent of an
        // applied fact/causal clock. Retrying bounded known keys survives restart
        // and lets the authenticated page move past an unavailable object.
        self.connection.execute("DELETE FROM sync_missing WHERE target_id=?1 AND epoch=?2 AND EXISTS(SELECT 1 FROM sync_cursors c WHERE c.target_id=sync_missing.target_id AND c.key=sync_missing.key)", params![target, epoch])?;
        let mut retry = self.connection.prepare("SELECT key,document FROM sync_missing WHERE target_id=?1 AND epoch=?2 ORDER BY attempts,key LIMIT 32")?;
        let retries = retry
            .query_map(params![target, epoch], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(retry);
        for (key, raw) in retries {
            let entry: Value = serde_json::from_str(&raw).map_err(|_| error("storage_corrupt"))?;
            self.connection.execute("UPDATE sync_missing SET attempts=attempts+1 WHERE target_id=?1 AND epoch=?2 AND key=?3", params![target,epoch,key])?;
            match bounded(storage, &key) {
                Ok(marker)
                    if entry["digest"] == digest(&marker) && entry["bytes"] == marker.len() =>
                {
                    objects.push(storage::ObjectMeta {
                        key,
                        version: digest(&marker),
                        bytes: marker.len() as u64,
                    });
                }
                Ok(_) => {
                    self.connection.execute("UPDATE sync_missing SET code='storage_corrupt' WHERE target_id=?1 AND epoch=?2 AND key=?3",params![target,epoch,key])?;
                }
                Err(e) if ["storage_not_found", "storage_transient"].contains(&e.code.as_str()) => {
                }
                Err(e) => {
                    self.connection.execute("UPDATE sync_missing SET code=?4 WHERE target_id=?1 AND epoch=?2 AND key=?3",params![target,epoch,key,e.code])?;
                }
            }
        }
        let mut supported = false;
        let mut more = false;
        let mut legacy = Vec::new();
        for device in manifest["devices"]
            .as_array()
            .ok_or_else(|| error("sync_invalid"))?
        {
            let publisher = string(device, "device_id")?;
            let namespace = format!("{space}/epochs/{epoch}/index/{publisher}");
            let result = (|| -> Result<(), ApiError> {
                let head_key = format!("{namespace}/head.index");
                let bytes = match bounded(storage, &head_key) {
                    Ok(bytes) => bytes,
                    Err(e) if e.code == "storage_not_found" => {
                        let observed: u64 = self.connection.query_row("SELECT COALESCE(MAX(observed_head),0) FROM sync_index_read WHERE target_id=?1 AND epoch=?2 AND publisher=?3",params![target,epoch,publisher],|r|r.get(0))?;
                        if observed > 0 {
                            return Err(e);
                        }
                        let mode_key = format!("{namespace}/mode.index");
                        match bounded(storage, &mode_key) {
                            Ok(bytes) => {
                                let envelope: Value = serde_json::from_slice(&bytes)
                                    .map_err(|_| error("storage_corrupt"))?;
                                let expected = json!({"protocol_version":1,"space_id":space,"epoch":epoch,"publisher":publisher,"mode":"indexed"});
                                if envelope["identity"]["device_id"] != publisher
                                    || self.sync_decrypt("changes", &envelope)? != expected
                                {
                                    return Err(error("storage_corrupt"));
                                }
                                supported = true;
                            }
                            Err(e) if e.code == "storage_not_found" => {
                                legacy.push(publisher.to_owned())
                            }
                            Err(e) => return Err(e),
                        }
                        return Ok(());
                    }
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
                #[cfg(test)]
                crate::sync::schedule::after_prior_read_point();
                if pages < observed {
                    return Err(error("storage_corrupt"));
                }
                // Head observation is durable even while a page still has pending facts.
                // Consumed page position remains independent from the advertised frontier.
                self.sync_persist_index_read(
                    target,
                    epoch,
                    publisher,
                    number,
                    pages,
                    previous.as_deref(),
                )?;
                let mut read_pages = 0;
                let mut consumed_prefix = true;
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
                        "SELECT EXISTS(SELECT 1 FROM sync_cursors WHERE target_id=?1 AND key=?2) OR EXISTS(SELECT 1 FROM sync_missing WHERE target_id=?1 AND epoch=?3 AND key=?2)",
                        params![target, key, epoch],
                        |r| r.get(0),
                    )?;
                        if consumed {
                            continue;
                        }
                        if objects.len() == 256 {
                            complete = false;
                            more = true;
                            break;
                        }
                        let marker = match bounded(storage, key) {
                            Ok(marker) => marker,
                            Err(e) => {
                                let tx = self.begin_write()?;
                                tx.execute("INSERT OR IGNORE INTO sync_missing(target_id,epoch,publisher,key,document,code) VALUES(?1,?2,?3,?4,?5,?6)",params![target,epoch,publisher,key,entry.to_string(),e.code])?;
                                tx.execute("INSERT INTO audit(entity_id,operation,document) VALUES(?1,'sync.transport_pending',?2)",params![id,json!({"publisher":publisher,"epoch":epoch,"key":key,"code":e.code}).to_string()])?;
                                tx.commit()?;
                                continue;
                            }
                        };
                        complete = false;
                        if entry["digest"] != digest(&marker) || entry["bytes"] != marker.len() {
                            let tx = self.begin_write()?;
                            tx.execute("INSERT OR IGNORE INTO sync_missing(target_id,epoch,publisher,key,document,code) VALUES(?1,?2,?3,?4,?5,'storage_corrupt')",params![target,epoch,publisher,key,entry.to_string()])?;
                            tx.execute("INSERT INTO audit(entity_id,operation,document) VALUES(?1,'sync.transport_pending',?2)",params![id,json!({"publisher":publisher,"epoch":epoch,"key":key,"code":"storage_corrupt"}).to_string()])?;
                            tx.commit()?;
                            continue;
                        }
                        objects.push(storage::ObjectMeta {
                            key: key.to_owned(),
                            version: digest(&marker),
                            bytes: marker.len() as u64,
                        });
                    }
                    read_pages += 1;
                    consumed_prefix &= complete;
                    // Read the next authenticated chained page within the same bounded
                    // budget even when this page has just queued facts for application.
                    // Persist only the contiguous already-consumed prefix, never the
                    // optimistic read position of queued or pending facts.
                    previous = Some(digest(page.to_string().as_bytes()));
                    number += 1;
                    if consumed_prefix {
                        self.sync_persist_index_read(
                            target,
                            epoch,
                            publisher,
                            number,
                            pages,
                            previous.as_deref(),
                        )?;
                    }
                }
                Ok(())
            })();
            match result {
                Ok(()) => {
                    self.connection.execute("DELETE FROM sync_index_errors WHERE target_id=?1 AND epoch=?2 AND publisher=?3",params![target,epoch,publisher])?;
                }
                Err(e) => {
                    supported = true;
                    let tx = self.begin_write()?;
                    tx.execute("INSERT INTO sync_index_errors(target_id,epoch,publisher,code,attempts) VALUES(?1,?2,?3,?4,1) ON CONFLICT(target_id,epoch,publisher) DO UPDATE SET code=excluded.code,attempts=sync_index_errors.attempts+1",params![target,epoch,publisher,e.code])?;
                    tx.execute("INSERT INTO audit(entity_id,operation,document) VALUES(?1,'sync.publisher_pending',?2)",params![publisher,json!({"epoch":epoch,"code":e.code}).to_string()])?;
                    tx.commit()?;
                }
            }
        }
        // Only publishers without an authenticated indexed-protocol declaration
        // use compatibility enumeration. Indexed publishers keep their direct pages.
        if !legacy.is_empty() {
            let mut cursor = None;
            'legacy: loop {
                let page = storage.list(changes, cursor.as_deref(), 64)?;
                for object in page.items {
                    if !object.key.ends_with(".commit") {
                        continue;
                    }
                    let consumed: bool = self.connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM sync_cursors WHERE target_id=?1 AND key=?2)",
                        params![target, object.key],
                        |r| r.get(0),
                    )?;
                    if consumed {
                        continue;
                    }
                    let envelope: Value = serde_json::from_slice(&bounded(storage, &object.key)?)
                        .map_err(|_| error("storage_corrupt"))?;
                    if !legacy
                        .iter()
                        .any(|publisher| envelope["identity"]["device_id"] == publisher.as_str())
                    {
                        continue;
                    }
                    self.sync_decrypt("changes", &envelope)?;
                    if objects.len() == 256 {
                        more = true;
                        break 'legacy;
                    }
                    objects.push(object);
                }
                cursor = page.cursor;
                if cursor.is_none() {
                    break;
                }
            }
            supported = true;
        }
        Ok((supported.then_some(objects), more))
    }
}
