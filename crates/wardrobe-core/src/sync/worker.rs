use super::*;
impl Application {
    pub(super) fn sync_enqueue(&self, input: Value) -> Result<Value, ApiError> {
        let identity = self.sync_identity()?;
        let target: String = self
            .connection
            .query_row(
                "SELECT document FROM sync_targets WHERE id='primary'",
                [],
                |r| r.get(0),
            )
            .map_err(|_| error("sync_not_configured"))?;
        let mut target: Value =
            serde_json::from_str(&target).map_err(|_| error("storage_error"))?;
        if identity["epoch"].as_u64().unwrap_or(0) > 1 {
            let key=digest(json!({"directory":target["directory"],"space_id":identity["space_id"],"epoch":identity["epoch"]}).to_string().as_bytes());
            if target["target_key"] != key {
                target["target_key"] = json!(key);
                let tx = self.begin_write()?;
                tx.execute(
                    "UPDATE sync_targets SET document=?1 WHERE id='primary'",
                    [target.to_string()],
                )?;
                tx.commit()?;
            }
        }
        let member = self.member(&json!({"member_id":"default"}))?;
        if input.get("operation_key").is_some() && !input["operation_key"].is_string() {
            return Err(error("invalid_input"));
        }
        let key = input["operation_key"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        if key.is_empty() || key.len() > 256 {
            return Err(error("invalid_input"));
        }
        let job = self.job_enqueue(&member,"sync",&key,json!({"target":target,"space_id":identity["space_id"],"device_id":identity["device_id"],"epoch":identity["epoch"]}))?;
        Ok(json!({"schema_version":1,"job_id":job["id"],"member_id":member,"state":job["state"]}))
    }
    /// One explicitly bounded worker step; queued remainder is durable across process exit.
    pub(super) fn sync_process(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&json!({"member_id":"default"}))?;
        let id = string(&input, "id")?;
        let existing = self.job_get(json!({"member_id":member,"id":id}))?;
        if existing["kind"] != "sync" {
            return Err(error("invalid_input"));
        }
        let claimed = self.job_claim(&member, id, 300)?;
        let lease = string(&claimed, "lease_token")?;
        let result = (|| {
            let identity = self.sync_identity()?;
            let target: String = self.connection.query_row(
                "SELECT document FROM sync_targets WHERE id='primary'",
                [],
                |r| r.get(0),
            )?;
            let target: Value =
                serde_json::from_str(&target).map_err(|_| error("storage_error"))?;
            let payload = &claimed["payload"];
            if payload["target"] != target {
                return Err(error("sync_target_changed"));
            }
            if payload["space_id"] != identity["space_id"]
                || payload["device_id"] != identity["device_id"]
                || payload["epoch"] != identity["epoch"]
            {
                return Err(error("sync_state_required"));
            }
            self.sync_step()
        })();
        match result {
            Err(e) => self.job_finish(
                &member,
                id,
                lease,
                "failed",
                Value::Null,
                json!({"code":e.code,"message":e.message,"retryable":false}),
            ),
            Ok(result) if result["has_more"] == true => {
                let tx = self.begin_write()?;
                let mut doc = self.job_get(json!({"member_id":member,"id":id}))?;
                if doc["state"] != "running"
                    || doc["lease_token"] != lease
                    || doc["lease_expires"].as_i64().unwrap_or(0)
                        <= chrono::Utc::now().timestamp_millis()
                {
                    return Err(error("lease_lost"));
                }
                doc["state"] = json!("queued");
                doc["result"] = result;
                doc["lease_token"] = Value::Null;
                doc["lease_expires"] = json!(0);
                doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
                crate::jobs::save(&tx, &doc)?;
                tx.commit()?;
                Ok(doc)
            }
            Ok(result) => self.job_finish(&member, id, lease, "succeeded", result, Value::Null),
        }
    }
}
