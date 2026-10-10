use super::*;
/// 纪元推进滚动目标键后，把 OS 安全存储中的传输凭据迁移到新键
/// （WebDAV 密码 / S3 访问键）。旧键凭据保留不删：回退到旧目标时仍可用。
fn migrate_sync_credential(
    app: &Application,
    old_key: &str,
    new_key: &str,
    target: &Value,
) -> Result<(), ApiError> {
    if old_key.is_empty() || old_key == new_key {
        return Ok(());
    }
    match target["provider_kind"].as_str() {
        Some("webdav") => {
            let password = match sync_webdav_password(old_key) {
                Ok(password) => password,
                Err(e) if e.code == "storage_unauthorized" => return Ok(()),
                Err(e) => return Err(e),
            };
            // 无条件覆盖：old_key 是当前目标文档正在使用的键，其凭据即权威。
            // 新键上可能残留更早轮换周期留下的上一远端凭据——若「已存在即
            // 跳过」，切换远端后的首个批次会拿旧凭据连新远端。
            sync_store_webdav_password(new_key, &password)?;
        }
        Some("s3") => {
            let credentials = match sync_s3_credentials(old_key) {
                Ok(credentials) => credentials,
                Err(e) if e.code == "storage_auth" => return Ok(()),
                Err(e) => return Err(e),
            };
            sync_store_s3_credentials(
                new_key,
                &credentials.access_key_id,
                &credentials.secret_access_key,
            )?;
        }
        // local_folder 无凭据；其余类型不带凭据。
        _ => {
            let _: &Application = app;
        }
    }
    Ok(())
}

/// 纪元键滚动时搬迁按 (target_key, epoch) 索引的发布者索引状态（页/条目/
/// 游标/错误/缺失）。只搬 epoch 维度表：纯纪元轮换下这些行只属于旧纪元
/// （新纪元尚无行），搬运无害；而「切换远端后的下一次入队滚动」必须让
/// 已逐字节复制到新远端的索引页状态跟随到纪元派生键，否则索引页会从
/// 第 1 页重开并与远端已有页冲突（sync_identity_conflict）。非 epoch 表
/// （publications/cursors/pending/quarantine）不搬：纯轮换必须按新纪元
/// 强制重发布 changes，搬运会错误抑制。
fn carry_epoch_index_state(
    tx: &rusqlite::Transaction<'_>,
    old_key: &str,
    new_key: &str,
) -> Result<(), ApiError> {
    for table in [
        "sync_index_errors",
        "sync_missing",
        "sync_index_read",
        "sync_index_batches",
        "sync_index_entries",
    ] {
        tx.execute(
            &format!("UPDATE {table} SET target_id=?1 WHERE target_id=?2"),
            params![new_key, old_key],
        )
        .map_err(|_| error("storage_error"))?;
    }
    Ok(())
}
impl Application {
    pub(super) fn sync_enqueue(&self, input: Value) -> Result<Value, ApiError> {
        // 暂停是持久开关：暂停期间拒绝新批次，队列任务保留。
        self.sync_pause_gate()?;
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
                // 纪元推进后目标键滚动（强制按新纪元重新发布日志）。传输凭据
                // 按 target_key 隔离在 OS 安全存储：随键迁移，避免轮换后
                // 需要重新输入 WebDAV/S3 凭据。
                let old_key = target["target_key"].as_str().unwrap_or_default().to_owned();
                migrate_sync_credential(self, &old_key, &key, &target)?;
                target["target_key"] = json!(key);
                let tx = self.begin_write()?;
                tx.execute(
                    "UPDATE sync_targets SET document=?1 WHERE id='primary'",
                    [target.to_string()],
                )?;
                carry_epoch_index_state(&tx, &old_key, &key)?;
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
        // 暂停期间排队任务不得继续：显式拒绝且任务保留，恢复后同一任务继续。
        self.sync_pause_gate()?;
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
