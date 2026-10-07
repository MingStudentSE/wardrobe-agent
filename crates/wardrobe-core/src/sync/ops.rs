//! #31 同步运维：暂停/恢复、更换目标（复制校验+原子切换+失败回退）、
//! 镜像备份（单向复制+显式提升）、按设备水位的安全回收。
//!
//! 设计要点（ADR 0033）：
//! - 暂停是本机主目标文档内的持久开关：`sync.run`/`sync.process` 以
//!   `sync_paused` 显式拒绝，队列任务保留，恢复后同一任务继续。
//! - 更换目标先做完整复制与逐对象 sha256 回读校验，任一步失败保持旧
//!   主目标（回退 = 旧目标原样可用）；全部通过才切换主目标文档。
//! - 镜像备份是主目标的单向副本，不参与多主协调；提升前校验镜像与主
//!   目标逐对象一致且发布标记可解密自洽，旧主目标原样保留。
//! - 安全回收绝不按固定时间盲删：改动日志（tombstone/已发布提交）只有
//!   在「全部当前配对设备已发布水位且水位覆盖该提交 + 保留窗口已过 +
//!   调用方显式确认存在可恢复备份基线」时才删除；已处理 blob 键登记在
//!   远端对象消失后随回收扫描清除，避免登记永久膨胀。
use super::*;
use base64::Engine;
use rusqlite::Transaction;

const RECLAIM_PAGE_BUDGET: usize = 64;
const RECLAIM_REGISTRY_BUDGET: usize = 512;
/// 保留窗口默认 30 天：覆盖离线设备回连与备份恢复窗口。
const RECLAIM_RETENTION_DEFAULT: u64 = 30 * 24 * 3600;

fn merge(mut base: Value, extra: Value) -> Value {
    if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    base
}

// ---------------------------------------------------------------------------
// 目标文档与凭据
// ---------------------------------------------------------------------------

/// 读取主目标文档；未配置返回 sync_not_configured。
pub(super) fn primary_target(app: &Application) -> Result<Value, ApiError> {
    let raw: String = app
        .connection
        .query_row(
            "SELECT document FROM sync_targets WHERE id='primary'",
            [],
            |r| r.get(0),
        )
        .map_err(|_| error("sync_not_configured"))?;
    serde_json::from_str(&raw).map_err(|_| error("storage_error"))
}

fn write_primary_target(app: &Application, target: &Value) -> Result<(), ApiError> {
    let tx = app.begin_write()?;
    tx.execute(
        "UPDATE sync_targets SET document=?1 WHERE id='primary'",
        [target.to_string()],
    )?;
    tx.commit().map_err(|_| error("storage_error"))
}

/// 主目标是否暂停。
pub(super) fn target_paused(target: &Value) -> bool {
    target["paused"] == json!(true)
}

/// 运维与配置共用的目标文档构造：真实连接 + 能力探针，凭据只进 OS 安全存储。
/// `sync.configure` / `sync.target.switch` / `sync.mirror.configure` 共用。
pub(super) fn build_target(app: &Application, input: &Value) -> Result<Value, ApiError> {
    let identity = app.sync_identity()?;
    let space = string(&identity, "space_id")?;
    let target = match input["provider_kind"].as_str() {
        Some("local_folder") => {
            if [
                "endpoint",
                "region",
                "bucket",
                "prefix",
                "path_style",
                "access_key_id",
                "secret_access_key",
            ]
            .iter()
            .any(|k| input.get(*k).is_some())
            {
                return Err(ApiError::new("invalid_input", "输入包含不支持的字段"));
            }
            let directory = std::fs::canonicalize(string(input, "directory")?)
                .map_err(|_| ApiError::new("storage_not_found", "同步目录不存在"))?;
            let local = std::fs::canonicalize(
                app.asset_root
                    .parent()
                    .ok_or_else(|| ApiError::new("storage_error", "本地数据目录无效"))?,
            )
            .map_err(|_| ApiError::new("storage_error", "本地数据目录无效"))?;
            if directory.starts_with(local) {
                return Err(ApiError::new(
                    "invalid_input",
                    "同步目录不得位于本机数据目录内",
                ));
            }
            LocalFolder::new(&directory)?;
            json!({"id":"primary","target_key":digest(json!({"directory":directory,"space_id":space}).to_string().as_bytes()),"provider_kind":"local_folder","directory":directory})
        }
        Some("webdav") => {
            let url = string(input, "url")?.trim_end_matches('/').to_string();
            let root = input["root"]
                .as_str()
                .map(str::trim)
                .map(|r| r.trim_matches('/').to_string())
                .unwrap_or_default();
            let username = string(input, "username")?;
            if url.is_empty() || root.is_empty() || username.is_empty() {
                return Err(ApiError::new("invalid_input", "输入包含不支持的字段"));
            }
            let target_key = digest(
                json!({"provider":"webdav","url":url,"root":root,"username":username,"space_id":space})
                    .to_string()
                    .as_bytes(),
            );
            let password = match input["password"].as_str() {
                Some(password) if !password.is_empty() => password.to_string(),
                _ => sync_webdav_password(&target_key)?,
            };
            storage::WebDav::connect(storage::WebDavConfig {
                url: url.clone(),
                root: root.clone(),
                username: username.to_string(),
                password: password.clone(),
                timeout: std::time::Duration::from_secs(30),
            })?;
            sync_store_webdav_password(&target_key, &password)?;
            json!({"id":"primary","target_key":target_key,"provider_kind":"webdav","url":url,"root":root,"username":username})
        }
        Some("s3") => {
            if input.get("directory").is_some() {
                return Err(ApiError::new("invalid_input", "输入包含不支持的字段"));
            }
            let access_key_id = string(input, "access_key_id")?.to_string();
            let secret_access_key = string(input, "secret_access_key")?.to_string();
            if access_key_id.is_empty() || secret_access_key.is_empty() {
                return Err(ApiError::new("invalid_input", "输入包含不支持的字段"));
            }
            let connection = json!({
                "endpoint": string(input, "endpoint")?,
                "region": input["region"].as_str().unwrap_or("us-east-1"),
                "bucket": string(input, "bucket")?,
                "prefix": input["prefix"].as_str().unwrap_or_default(),
                "path_style": input["path_style"].as_bool().unwrap_or(true),
            });
            let target_key = digest(
                json!({"connection":connection,"space_id":space})
                    .to_string()
                    .as_bytes(),
            );
            let adapter = S3Objects::new(
                S3Target {
                    endpoint: string(&connection, "endpoint")?.to_string(),
                    region: string(&connection, "region")?.to_string(),
                    bucket: string(&connection, "bucket")?.to_string(),
                    prefix: string(&connection, "prefix")?.to_string(),
                    path_style: connection["path_style"].as_bool().unwrap_or(true),
                },
                S3Credentials {
                    access_key_id: access_key_id.clone(),
                    secret_access_key: secret_access_key.clone(),
                },
            )?;
            drop(adapter);
            sync_store_s3_credentials(&target_key, &access_key_id, &secret_access_key)?;
            json!({
                "id":"primary",
                "target_key":target_key,
                "provider_kind":"s3",
                "endpoint":connection["endpoint"],
                "region":connection["region"],
                "bucket":connection["bucket"],
                "prefix":connection["prefix"],
                "path_style":connection["path_style"],
            })
        }
        _ => return Err(ApiError::new("storage_unsupported", "不支持的同步目标类型")),
    };
    Ok(target)
}

/// 删除一个目标的传输凭据（忽略不存在的条目）。
fn delete_sync_credential(target_key: &str) {
    if let Ok(entry) = keyring::Entry::new(SYNC_CREDENTIAL_SERVICE, target_key) {
        let _ = entry.delete_credential();
    }
}

/// 解除配对/恢复备份后：按已登记目标（含镜像）清理 OS 安全存储中的传输
/// 凭据，并清空全部按目标键索引的传输状态，避免凭据孤儿与新空间误写旧目标。
pub(crate) fn drop_target_state(app: &Application) -> Result<(), ApiError> {
    let docs: Vec<String> = {
        let mut statement = app
            .connection
            .prepare("SELECT document FROM sync_targets")?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for doc in docs {
        let Ok(target) = serde_json::from_str::<Value>(&doc) else {
            continue;
        };
        if let Some(key) = target["target_key"].as_str() {
            delete_sync_credential(key);
        }
        if let Some(key) = target["mirror"]["target_key"].as_str() {
            delete_sync_credential(key);
        }
    }
    let tx = app.begin_write()?;
    for table in [
        "sync_targets",
        "sync_cursors",
        "sync_publications",
        "sync_pending",
        "sync_quarantine",
        "sync_index_errors",
        "sync_missing",
        "sync_index_read",
        "sync_index_batches",
        "sync_index_entries",
        "sync_asset_commits",
        "sync_blob_scan",
        "sync_watermark",
        "sync_maintenance",
        "sync_bootstrap",
    ] {
        tx.execute_batch(&format!("DELETE FROM {table}"))?;
    }
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 复制与校验
// ---------------------------------------------------------------------------

/// 目标键滚动后搬迁按目标键索引的传输状态（对象逐字节一致的前提成立）。
fn carry_target_state(tx: &Transaction<'_>, old_key: &str, new_key: &str) -> Result<(), ApiError> {
    for table in [
        "sync_cursors",
        "sync_publications",
        "sync_pending",
        "sync_quarantine",
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

/// 同步空间在目标上的已知平铺目录（changes/blobs 为文件层，index 按当前
/// 已配对发布者展开，watermarks 平铺）。
/// 从已签名清单信封解码当前配对设备列表（payload_base64 内才是设备表）。
fn manifest_devices(identity: &Value) -> Result<Vec<String>, ApiError> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(string(&identity["manifest"], "payload_base64")?)
        .map_err(|_| error("sync_invalid"))?;
    let manifest: Value = serde_json::from_slice(&raw).map_err(|_| error("sync_invalid"))?;
    Ok(manifest["devices"]
        .as_array()
        .ok_or_else(|| error("sync_invalid"))?
        .iter()
        .filter_map(|d| d["device_id"].as_str().map(str::to_owned))
        .collect())
}

fn target_folders(identity: &Value) -> Result<Vec<String>, ApiError> {
    let space = string(identity, "space_id")?;
    let epoch = identity["epoch"]
        .as_u64()
        .ok_or_else(|| error("sync_invalid"))?;
    let publishers: Vec<String> = manifest_devices(identity)?;
    let mut folders = vec![format!("{space}/changes"), format!("{space}/blobs")];
    for e in 1..=epoch {
        folders.push(format!("{space}/epochs/{e}/changes"));
        folders.push(format!("{space}/epochs/{e}/blobs"));
        folders.push(format!("{space}/epochs/{e}/watermarks"));
        for publisher in &publishers {
            folders.push(format!("{space}/epochs/{e}/index/{publisher}"));
        }
    }
    Ok(folders)
}

/// 当前纪元 changes 目录（纪元 1 为旧扁平目录）。
fn changes_folders(identity: &Value) -> Result<Vec<String>, ApiError> {
    let space = string(identity, "space_id")?;
    let epoch = identity["epoch"]
        .as_u64()
        .ok_or_else(|| error("sync_invalid"))?;
    if epoch == 1 {
        Ok(vec![format!("{space}/changes")])
    } else {
        Ok(vec![format!("{space}/epochs/{epoch}/changes")])
    }
}

/// 读取一个对象的全部字节并计算摘要；所有同步对象均小于 bounded 上限。
fn read_object(storage: &dyn SyncStorage, key: &str) -> Result<(Vec<u8>, String), ApiError> {
    let bytes = bounded(storage, key)?;
    let check = digest(&bytes);
    Ok((bytes, check))
}

/// 把 src 的全部对象镜像复制到 dst：已存在且逐字节一致的对象跳过；新写入
/// 对象回读校验；存在但字节不一致的对象整体失败。任何失败由调用方决定
/// 是否保持旧状态（切换/提升均保持，即失败可回退）。
/// 单调对象的向前合并：head.index（发布者页游标）与设备水位只会前进；
/// 副本落后于目标时以更前进者为准，同页码不同摘要仍视为损坏冲突。
fn resolve_forward(
    app: &Application,
    key: &str,
    src_bytes: &[u8],
    dst_bytes: &[u8],
) -> Result<std::cmp::Ordering, ApiError> {
    let decode_head = |bytes: &[u8]| -> Result<u64, ApiError> {
        let envelope: Value = serde_json::from_slice(bytes)
            .map_err(|_| ApiError::new("storage_corrupt", "发布对象不是有效信封"))?;
        let head = app.sync_decrypt("changes", &envelope)?;
        head["pages"].as_u64().ok_or_else(|| error("sync_invalid"))
    };
    let decode_watermark = |bytes: &[u8]| -> Result<String, ApiError> {
        let envelope: Value = serde_json::from_slice(bytes)
            .map_err(|_| ApiError::new("storage_corrupt", "发布对象不是有效信封"))?;
        let record = app.sync_decrypt("changes", &envelope)?;
        Ok(record["captured_at"]
            .as_str()
            .ok_or_else(|| error("sync_invalid"))?
            .to_owned())
    };
    if key.ends_with("/head.index") {
        let src_pages = decode_head(src_bytes)?;
        let dst_pages = decode_head(dst_bytes)?;
        if src_pages == dst_pages {
            // 同页码必须同字节（页文件不可变）；不同即冲突。
            return Err(ApiError::new(
                "sync_target_verify_failed",
                &format!("发布者页游标同页不同摘要，操作已中止：{key}"),
            ));
        }
        return Ok(src_pages.cmp(&dst_pages));
    }
    if key.contains("/watermarks/") && key.ends_with(".commit") {
        let src_through = decode_watermark(src_bytes)?;
        let dst_through = decode_watermark(dst_bytes)?;
        if src_through == dst_through {
            return Err(ApiError::new(
                "sync_target_verify_failed",
                &format!("设备水位同位置不同字节，操作已中止：{key}"),
            ));
        }
        return Ok(src_through.as_str().cmp(dst_through.as_str()));
    }
    Err(ApiError::new(
        "sync_target_verify_failed",
        &format!("新目标对象与源字节不一致，操作已中止：{key}"),
    ))
}

fn copy_objects(
    app: &Application,
    identity: &Value,
    src: &dyn SyncStorage,
    dst: &dyn SyncStorage,
) -> Result<Value, ApiError> {
    let mut copied = 0u64;
    let mut skipped = 0u64;
    let mut verified_bytes = 0u64;
    for folder in target_folders(identity)? {
        let mut cursor = None;
        loop {
            let page = src.list(&folder, cursor.as_deref(), 64)?;
            for object in page.items {
                let (bytes, check) = read_object(src, &object.key)?;
                match dst.stat(&object.key) {
                    Ok(meta) if meta.bytes == bytes.len() as u64 => {
                        let (existing, existing_check) = read_object(dst, &object.key)?;
                        if existing_check != check {
                            // 单调对象（发布者页游标/设备水位）允许向前合并；
                            // 其余对象字节不一致仍整体失败。
                            if resolve_forward(app, &object.key, &bytes, &existing)?
                                != std::cmp::Ordering::Greater
                            {
                                skipped += 1;
                                continue;
                            }
                            let mut payload = bytes.as_slice();
                            dst.put(&object.key, &mut payload, WriteCondition::Any)?;
                            let (_, written) = read_object(dst, &object.key)?;
                            if written != check {
                                return Err(ApiError::new(
                                    "sync_target_verify_failed",
                                    &format!("向前合并后回读校验失败：{}", object.key),
                                ));
                            }
                            copied += 1;
                            verified_bytes += bytes.len() as u64;
                            continue;
                        }
                        skipped += 1;
                        continue;
                    }
                    Ok(_) => {
                        return Err(ApiError::new(
                            "sync_target_verify_failed",
                            &format!("新目标已有同名对象但长度不一致，操作已中止：{}", object.key),
                        ))
                    }
                    Err(e) if e.code != "storage_not_found" => return Err(e),
                    Err(_) => {}
                }
                let mut payload = bytes.as_slice();
                match dst.put(&object.key, &mut payload, WriteCondition::CreateOnly) {
                    Ok(_) => {}
                    Err(e) if e.code == "storage_precondition" => {
                        // 并发写入竞态：以回读一致为准。
                        let (_, existing) = read_object(dst, &object.key)?;
                        if existing != check {
                            return Err(ApiError::new(
                                "sync_target_verify_failed",
                                "并发写入的对象字节不一致，操作已中止",
                            ));
                        }
                        skipped += 1;
                        continue;
                    }
                    Err(e) => return Err(e),
                }
                let (_, written) = read_object(dst, &object.key)?;
                if written != check {
                    return Err(ApiError::new(
                        "sync_target_verify_failed",
                        "复制后回读校验失败，操作已中止",
                    ));
                }
                copied += 1;
                verified_bytes += bytes.len() as u64;
            }
            cursor = page.cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    Ok(json!({"copied":copied,"skipped":skipped,"verified_bytes":verified_bytes}))
}

impl Application {
    fn audit_target_change(
        &self,
        operation: &str,
        from: Option<&str>,
        to: Option<&str>,
        stats: &Value,
    ) -> Result<(), ApiError> {
        let identity = self.sync_identity()?;
        let tx = self.begin_write()?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,?2,?3)",
            params![
                string(&identity, "space_id")?,
                operation,
                json!({"from":from,"to":to,"stats":stats}).to_string()
            ],
        )?;
        tx.commit().map_err(|_| error("storage_error"))
    }

    /// 暂停本机主目标的同步批次。队列任务保留；恢复后继续。
    pub(super) fn sync_pause(&self, _input: Value) -> Result<Value, ApiError> {
        let mut target = primary_target(self)?;
        target["paused"] = json!(true);
        write_primary_target(self, &target)?;
        Ok(json!({"schema_version":1,"paused":true}))
    }

    pub(super) fn sync_resume(&self, _input: Value) -> Result<Value, ApiError> {
        let mut target = primary_target(self)?;
        if let Some(o) = target.as_object_mut() {
            o.remove("paused");
        }
        write_primary_target(self, &target)?;
        Ok(json!({"schema_version":1,"paused":false}))
    }

    /// worker 前置校验：主目标已暂停时以 sync_paused 拒绝新批次。
    pub(super) fn sync_pause_gate(&self) -> Result<(), ApiError> {
        let target = primary_target(self)?;
        if target_paused(&target) {
            return Err(ApiError::new(
                "sync_paused",
                "同步已暂停：本机主目标停止发布与读取，恢复后继续",
            ));
        }
        Ok(())
    }

    /// 更换主目标：先完整复制并逐对象校验，全部通过才切换；失败保持旧目标。
    pub(super) fn sync_target_switch(&self, input: Value) -> Result<Value, ApiError> {
        let identity = self.sync_identity()?;
        let old = primary_target(self)?;
        let new = build_target(self, &input)?;
        let storage = storage_from_target(&old, string(&old, "target_key")?)?;
        let next = storage_from_target(&new, string(&new, "target_key")?)?;
        // 完整发布 + 校验：任一对象失败即中止，主目标保持不变（可回退）。
        let stats = copy_objects(self, &identity, storage.as_ref(), next.as_ref())?;
        let tx = self.begin_write()?;
        tx.execute(
            "UPDATE sync_targets SET document=?1 WHERE id='primary'",
            [new.to_string()],
        )?;
        // 对象逐字节一致复制后，按旧目标键记录的传输状态（游标/发布/挂起/
        // 隔离/索引）对 New 目标同样有效：切换保持幂等，不整库重收。
        carry_target_state(
            &tx,
            string(&old, "target_key")?,
            string(&new, "target_key")?,
        )?;
        tx.commit().map_err(|_| error("storage_error"))?;
        self.audit_target_change(
            "sync.target.switch",
            old["target_key"].as_str(),
            new["target_key"].as_str(),
            &stats,
        )?;
        Ok(merge(
            json!({"schema_version":1,"target":new,"previous_target":old}),
            stats,
        ))
    }

    /// 配置镜像备份目标并完成首次完整复制校验；镜像是单向副本，不参与协调。
    pub(super) fn sync_mirror_configure(&self, input: Value) -> Result<Value, ApiError> {
        let identity = self.sync_identity()?;
        let mut primary = primary_target(self)?;
        let mirror = build_target(self, &input)?;
        if mirror["target_key"] == primary["target_key"] {
            return Err(ApiError::new("invalid_input", "镜像目标不得与主目标相同"));
        }
        let storage = storage_from_target(&primary, string(&primary, "target_key")?)?;
        let copy = storage_from_target(&mirror, string(&mirror, "target_key")?)?;
        let stats = copy_objects(self, &identity, storage.as_ref(), copy.as_ref())?;
        primary["mirror"] = mirror.clone();
        write_primary_target(self, &primary)?;
        self.audit_target_change(
            "sync.mirror.configure",
            None,
            mirror["target_key"].as_str(),
            &stats,
        )?;
        Ok(merge(json!({"schema_version":1,"mirror":mirror}), stats))
    }

    /// 增量单向复制主目标 → 镜像；重复执行幂等。
    pub(super) fn sync_mirror_sync(&self, _input: Value) -> Result<Value, ApiError> {
        let identity = self.sync_identity()?;
        let primary = primary_target(self)?;
        let mirror = primary["mirror"]
            .as_object()
            .map(|_| primary["mirror"].clone())
            .ok_or_else(|| ApiError::new("sync_mirror_not_configured", "尚未配置镜像备份目标"))?;
        let storage = storage_from_target(&primary, string(&primary, "target_key")?)?;
        let copy = storage_from_target(&mirror, string(&mirror, "target_key")?)?;
        let stats = copy_objects(self, &identity, storage.as_ref(), copy.as_ref())?;
        Ok(merge(json!({"schema_version":1,"mirror":mirror}), stats))
    }

    /// 显式提升镜像为主目标：先校验（主目标每个对象在镜像上逐字节一致，
    /// 且镜像 changes 发布标记可解密并与对象字节自洽），通过后切换；旧主
    /// 目标原样保留为镜像，可再次提升回退。
    pub(super) fn sync_mirror_promote(&self, _input: Value) -> Result<Value, ApiError> {
        let identity = self.sync_identity()?;
        let primary = primary_target(self)?;
        let mirror = primary["mirror"]
            .as_object()
            .map(|_| primary["mirror"].clone())
            .ok_or_else(|| ApiError::new("sync_mirror_not_configured", "尚未配置镜像备份目标"))?;
        let storage = storage_from_target(&primary, string(&primary, "target_key")?)?;
        let copy = storage_from_target(&mirror, string(&mirror, "target_key")?)?;
        // 1) 完备性：主目标每个对象在镜像上存在且逐字节一致。
        let compared = verify_mirror_covers_primary(&identity, storage.as_ref(), copy.as_ref())?;
        // 2) 完整性：镜像 changes 发布标记可解密且与对象字节自洽。
        let markers = self.verify_marker_integrity(copy.as_ref())?;
        let mut next_primary = mirror.clone();
        next_primary["id"] = json!("primary");
        if let Some(o) = next_primary.as_object_mut() {
            o.remove("paused");
        }
        let mut next_mirror = primary.clone();
        next_mirror["id"] = json!("primary");
        if let Some(o) = next_mirror.as_object_mut() {
            o.remove("mirror");
        }
        next_mirror["degraded"] = json!(true);
        let mut stored = next_primary.clone();
        stored["mirror"] = next_mirror.clone();
        let tx = self.begin_write()?;
        tx.execute(
            "UPDATE sync_targets SET document=?1 WHERE id='primary'",
            [stored.to_string()],
        )?;
        // 提升后对象一致：沿用主目标时期的传输状态，保持幂等。
        carry_target_state(
            &tx,
            string(&primary, "target_key")?,
            string(&next_primary, "target_key")?,
        )?;
        tx.commit().map_err(|_| error("storage_error"))?;
        self.audit_target_change(
            "sync.mirror.promote",
            primary["target_key"].as_str(),
            next_primary["target_key"].as_str(),
            &json!({"markers_verified":markers,"objects_compared":compared}),
        )?;
        Ok(
            json!({"schema_version":1,"target":next_primary,"mirror":next_mirror,"markers_verified":markers,"objects_compared":compared}),
        )
    }

    /// 移除镜像配置并清理其凭据；主目标不变。
    pub(super) fn sync_mirror_remove(&self, _input: Value) -> Result<Value, ApiError> {
        let mut primary = primary_target(self)?;
        let mirror = primary["mirror"].clone();
        if !mirror.is_null() {
            if let Some(key) = mirror["target_key"].as_str() {
                delete_sync_credential(key);
            }
        }
        if let Some(o) = primary.as_object_mut() {
            o.remove("mirror");
        }
        write_primary_target(self, &primary)?;
        Ok(json!({"schema_version":1,"removed":true}))
    }

    /// changes 发布标记可解密（签名/成员资格校验）且与其引用的对象字节
    /// 自洽；返回校验通过的标记数。
    fn verify_marker_integrity(&self, storage: &dyn SyncStorage) -> Result<u64, ApiError> {
        let identity = self.sync_identity()?;
        let mut verified = 0u64;
        for folder in changes_folders(&identity)? {
            let mut cursor = None;
            loop {
                let page = storage.list(&folder, cursor.as_deref(), 64)?;
                for object in page.items {
                    if !object.key.ends_with(".commit") {
                        continue;
                    }
                    let (marker_bytes, _) = read_object(storage, &object.key)?;
                    let envelope: Value = serde_json::from_slice(&marker_bytes)
                        .map_err(|_| ApiError::new("storage_corrupt", "发布标记不是有效信封"))?;
                    let marker = self.sync_decrypt("changes", &envelope)?;
                    let payload_key = string(&marker, "key")?;
                    let (payload, payload_digest) = read_object(storage, payload_key)?;
                    if marker["sha256"] != json!(payload_digest) || marker["bytes"] != payload.len()
                    {
                        return Err(ApiError::new(
                            "sync_target_verify_failed",
                            "镜像发布标记与对象字节不一致，提升已中止",
                        ));
                    }
                    verified += 1;
                }
                cursor = page.cursor;
                if cursor.is_none() {
                    break;
                }
            }
        }
        Ok(verified)
    }
}

/// 完备性校验：主目标每个对象在镜像上存在且逐字节一致。
fn verify_mirror_covers_primary(
    identity: &Value,
    primary: &dyn SyncStorage,
    mirror: &dyn SyncStorage,
) -> Result<u64, ApiError> {
    let mut compared = 0u64;
    for folder in target_folders(identity)? {
        let mut cursor = None;
        loop {
            let page = primary.list(&folder, cursor.as_deref(), 64)?;
            for object in page.items {
                let (bytes, check) = read_object(primary, &object.key)?;
                match mirror.stat(&object.key) {
                    Ok(meta) if meta.bytes == bytes.len() as u64 => {}
                    _ => {
                        return Err(ApiError::new(
                            "sync_target_verify_failed",
                            "镜像缺少主目标对象，提升已中止",
                        ))
                    }
                }
                let (_, existing) = read_object(mirror, &object.key)?;
                if existing != check {
                    return Err(ApiError::new(
                        "sync_target_verify_failed",
                        "镜像对象字节与主目标不一致，提升已中止",
                    ));
                }
                compared += 1;
            }
            cursor = page.cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    Ok(compared)
}

// ---------------------------------------------------------------------------
// 设备水位与安全回收
// ---------------------------------------------------------------------------

/// 从 changes 提交键取 op_id：`{prefix}/{op_id}.commit`。
pub(super) fn op_id_of_key(key: &str) -> Option<String> {
    let name = key.rsplit('/').next()?;
    let stem = name.strip_suffix(".commit")?;
    uuid::Uuid::parse_str(stem).ok()?;
    Some(stem.to_owned())
}

impl Application {
    /// 本机设备的消费水位：对每个发布者的已完整消费页位置
    /// （sync_index_read.next_page——只前进的连续已消费前缀）。
    /// op_id 不保证时序，页位置才是单调可靠的消费进度。
    pub(crate) fn sync_consumption_positions(
        &self,
        target: &str,
        epoch: u64,
    ) -> Result<Value, ApiError> {
        let mut statement = self.connection.prepare(
            "SELECT publisher,next_page FROM sync_index_read WHERE target_id=?1 AND epoch=?2",
        )?;
        let rows = statement
            .query_map(params![target, epoch], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut positions = serde_json::Map::new();
        for (publisher, next_page) in rows {
            positions.insert(publisher, json!(next_page));
        }
        Ok(Value::Object(positions))
    }

    /// 同步步末发布本机水位到目标（best-effort：失败不阻塞同步，安全方向
    /// 是水位缺失只会阻止回收，不会导致误删）。
    pub(crate) fn sync_publish_watermark(
        &self,
        storage: &dyn SyncStorage,
        identity: &Value,
        target: &str,
    ) -> Result<(), ApiError> {
        let space = string(identity, "space_id")?;
        let epoch = identity["epoch"]
            .as_u64()
            .ok_or_else(|| error("sync_invalid"))?;
        let device = string(identity, "device_id")?;
        let positions = self.sync_consumption_positions(target, epoch)?;
        let captured_at = chrono::Utc::now().to_rfc3339();
        let record = json!({
            "protocol_version":1,
            "space_id":space,
            "epoch":epoch,
            "device_id":device,
            "positions":positions,
            "captured_at":captured_at,
        });
        let tx = self.begin_write()?;
        tx.execute(
            "INSERT INTO sync_watermark(device,positions,captured_at,epoch) VALUES(?1,?2,?3,?4) ON CONFLICT(device) DO UPDATE SET positions=excluded.positions,captured_at=excluded.captured_at,epoch=excluded.epoch",
            params![device, positions.to_string(), captured_at, epoch],
        )?;
        tx.commit()?;
        let folder = format!("{space}/epochs/{epoch}/watermarks");
        let key = format!("{folder}/{device}.commit");
        let cipher = self.sync_encrypt("changes", &record)?;
        let bytes = serde_json::to_vec(&cipher).map_err(|_| error("sync_invalid"))?;
        let mut payload = bytes.as_slice();
        match storage.put(&key, &mut payload, WriteCondition::CreateOnly) {
            Ok(_) => Ok(()),
            Err(e) if e.code == "storage_precondition" => {
                // 已有同名水位：水位单调前进（页位置/捕获时间），旧记录被
                // 更新记录覆盖；读回失败或更新失败都保持现状（安全方向是
                // 水位偏旧，只会阻止回收，不会导致误删）。
                let mut existing = storage.get(&key)?.take(2 * 1024 * 1024 + 1);
                let mut raw = vec![];
                if existing.read_to_end(&mut raw).is_err() {
                    return Ok(());
                }
                let stale = serde_json::from_slice::<Value>(&raw)
                    .ok()
                    .is_some_and(|envelope| {
                        self.sync_decrypt("changes", &envelope).is_ok_and(|record| {
                            record["captured_at"].as_str().unwrap_or("") < captured_at.as_str()
                        })
                    });
                if stale {
                    let mut payload = bytes.as_slice();
                    let _ = storage.put(&key, &mut payload, WriteCondition::Any);
                }
                Ok(())
            }
            Err(_) => Ok(()),
        }
    }

    /// 安全回收：仅在「全部当前配对设备水位覆盖 + 保留窗口已过 + 备份基线
    /// 确认」时删除已确认的 changes 提交（标记先删、对象后删）；已处理
    /// blob 键登记在远端对象消失后清除。绝不按固定时间盲删。
    ///
    /// 水位是每个发布者的索引页位置（sync_index_read.next_page，只前进的
    /// 连续已消费前缀）：某页在「所有设备都已完整消费过更靠前的页」且
    /// 「全部水位捕获时间早于保留窗口」时才可回收；页文件与其引用的
    /// 提交键之间的映射由发布者页信封给出，无需解密被删对象。
    pub(super) fn sync_reclaim(&self, input: Value) -> Result<Value, ApiError> {
        if input.get("backup_confirmed") != Some(&json!(true)) {
            return Err(ApiError::new(
                "sync_reclaim_baseline_required",
                "回收前须确认存在可恢复备份基线（本地备份或加密快照包）",
            ));
        }
        let dry_run = input.get("dry_run").is_some_and(|v| v == &json!(true));
        let retention = input["retention_seconds"]
            .as_u64()
            .unwrap_or(RECLAIM_RETENTION_DEFAULT);
        let identity = self.sync_identity()?;
        let target_doc = primary_target(self)?;
        let target_key = string(&target_doc, "target_key")?;
        let storage = storage_from_target(&target_doc, target_key)?;
        let storage = storage.as_ref();
        let space = string(&identity, "space_id")?;
        let epoch = identity["epoch"]
            .as_u64()
            .ok_or_else(|| error("sync_invalid"))?;
        let device = string(&identity, "device_id")?;

        // 1) 读取目标上的全部设备水位；任何当前配对设备缺水位 → 整体不回收。
        let mut watermarks = Vec::new();
        let watermark_folder = format!("{space}/epochs/{epoch}/watermarks");
        let mut cursor = None;
        loop {
            let page = storage.list(&watermark_folder, cursor.as_deref(), 64)?;
            for object in page.items {
                if !object.key.ends_with(".commit") {
                    continue;
                }
                // 单条记录不可读（如已撤销设备的历史记录）时跳过，不阻塞。
                let Ok(raw) = bounded(storage, &object.key) else {
                    continue;
                };
                let Ok(envelope) = serde_json::from_slice::<Value>(&raw) else {
                    continue;
                };
                if let Ok(record) = self.sync_decrypt("changes", &envelope) {
                    watermarks.push(record);
                }
            }
            cursor = page.cursor;
            if cursor.is_none() {
                break;
            }
        }
        let devices = manifest_devices(&identity)?;
        let mut blocked_by = Vec::new();
        for device in &devices {
            if !watermarks
                .iter()
                .any(|w| w["device_id"].as_str() == Some(device.as_str()))
            {
                blocked_by.push(device.clone());
            }
        }
        let mut published_own = false;
        if !watermarks.iter().any(|w| w["device_id"] == json!(device)) {
            // 自己尚未发布过水位时先补发，避免其他设备被本机缺失阻塞下一轮。
            self.sync_publish_watermark(storage, &identity, target_key)?;
            published_own = true;
        }
        // 全部水位的最早捕获时间：保留窗口从「所有设备都已确认」时刻起算。
        let mut captured_floor: Option<String> = None;
        for device in &devices {
            let captured = watermarks
                .iter()
                .find(|w| w["device_id"].as_str() == Some(device.as_str()))
                .and_then(|w| w["captured_at"].as_str());
            match (captured, &captured_floor) {
                (Some(c), None) => captured_floor = Some(c.to_owned()),
                (Some(c), Some(floor)) if c < floor.as_str() => captured_floor = Some(c.to_owned()),
                _ => {}
            }
        }
        let retention_passed = captured_floor.as_ref().is_some_and(|floor| {
            chrono::DateTime::parse_from_rfc3339(floor)
                .map(|t| t.with_timezone(&chrono::Utc).timestamp_millis())
                .map(|ms| ms <= chrono::Utc::now().timestamp_millis() - retention as i64 * 1000)
                .unwrap_or(false)
        });

        // 2) 每个发布者：取所有设备的最小已消费页位置；更靠前的页可回收。
        let mut scanned = 0u64;
        let mut candidates = 0u64;
        let mut reclaimed_changes = 0u64;
        let mut has_more = false;
        if blocked_by.is_empty() && retention_passed {
            'publishers: for publisher in &devices {
                let namespace = format!("{space}/epochs/{epoch}/index/{publisher}");
                let min_position = watermarks
                    .iter()
                    .filter_map(|w| w["positions"][publisher.as_str()].as_u64())
                    .min()
                    .unwrap_or(0);
                let head_key = format!("{namespace}/head.index");
                let Ok(head_bytes) = bounded(storage, &head_key) else {
                    continue;
                };
                let Ok(head_envelope) = serde_json::from_slice::<Value>(&head_bytes) else {
                    continue;
                };
                let Ok(head) = self.sync_decrypt("changes", &head_envelope) else {
                    continue;
                };
                let Some(pages) = head["pages"].as_u64() else {
                    continue;
                };
                for number in 1..=pages.min(min_position.saturating_sub(1)) {
                    scanned += 1;
                    if scanned > RECLAIM_PAGE_BUDGET as u64 {
                        has_more = true;
                        break 'publishers;
                    }
                    let page_key = format!("{namespace}/{number:020}.page");
                    let Ok(page_bytes) = bounded(storage, &page_key) else {
                        continue;
                    };
                    let Ok(page_envelope) = serde_json::from_slice::<Value>(&page_bytes) else {
                        continue;
                    };
                    let Ok(index_page) = self.sync_decrypt("changes", &page_envelope) else {
                        continue;
                    };
                    let Some(entries) = index_page["entries"].as_array() else {
                        continue;
                    };
                    for entry in entries {
                        let Some(key) = entry["key"].as_str() else {
                            continue;
                        };
                        candidates += 1;
                        if dry_run {
                            continue;
                        }
                        // 标记先删（发布标记门槛），对象后删。
                        if storage.delete(key, WriteCondition::Any).is_ok() {
                            reclaimed_changes += 1;
                        }
                        let object_key = key.trim_end_matches(".commit").to_owned() + ".object";
                        match storage.delete(&object_key, WriteCondition::Any) {
                            Ok(()) | Err(_) => {}
                        }
                    }
                }
            }
        }

        // 3) 已处理 blob 键登记：远端对象已消失的行是死行，随扫描清除，
        //    登记不再永久膨胀。远端对象仍在的行保留（防止重复解密扫描）。
        let mut reclaimed_blob_registry = 0u64;
        let registry: Vec<String> = {
            let mut statement = self
                .connection
                .prepare("SELECT key FROM sync_asset_commits ORDER BY key LIMIT ?1")?;
            let rows = statement
                .query_map([RECLAIM_REGISTRY_BUDGET], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for key in registry {
            if matches!(storage.stat(&key), Err(e) if e.code == "storage_not_found") {
                if !dry_run {
                    self.connection
                        .execute("DELETE FROM sync_asset_commits WHERE key=?1", params![key])?;
                }
                reclaimed_blob_registry += 1;
            }
        }

        let summary = json!({
            "schema_version":1,
            "dry_run":dry_run,
            "retention_seconds":retention,
            "scanned_pages":scanned,
            "candidates":candidates,
            "reclaimed_changes":reclaimed_changes,
            "reclaimed_blob_registry":reclaimed_blob_registry,
            "blocked_by":blocked_by,
            "retention_passed":retention_passed,
            "has_more":has_more,
            "published_own_watermark":published_own,
            "watermarks":watermarks.iter().map(|w| json!({
                "device_id":w["device_id"],
                "positions":w["positions"],
                "captured_at":w["captured_at"],
            })).collect::<Vec<_>>(),
            "at":chrono::Utc::now().to_rfc3339(),
        });
        if !dry_run {
            let tx = self.begin_write()?;
            tx.execute(
                "INSERT INTO sync_maintenance(key,document) VALUES('reclaim',?1) ON CONFLICT(key) DO UPDATE SET document=excluded.document",
                params![summary.to_string()],
            )?;
            tx.execute(
                "INSERT INTO audit(entity_id,operation,document) VALUES(?1,'sync.reclaim',?2)",
                params![space, json!({"reclaimed_changes":reclaimed_changes,"candidates":candidates,"retention_seconds":retention,"blocked_by":blocked_by}).to_string()],
            )?;
            tx.commit().map_err(|_| error("storage_error"))?;
        }
        Ok(summary)
    }
}
