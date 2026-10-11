//! 基础检查与 CLI doctor（#48，#73 迁出独立模块）：固定十项只读体检、
//! CLI `doctor` 的同源报告与数据库不可打开时的降级路径。对外 interface 不变：
//! diagnostics.checks 行为照旧，doctor_report 经 diagnostics 模块再导出
//! （wardrobe_core::diagnostics::doctor_report 路径保持）；卡住任务的租约
//! 过期判定从 jobs 模块单一来源导入，不再手抄。本模块与日志引擎仅名义耦合：
//! logs_writable 一项触碰 logs 路径，其余检查与日志读写互不依赖。

use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::{ApiError, Application};

impl Application {
    /// 基础检查（#48）：只读体检，返回固定十项（id、pass/warn/fail、通俗中文消息、
    /// 建议去向设置页标识、技术明细）。绝不修改配置或数据、不自动修复；
    /// 唯一的临时产物是试写探测文件，且无论成败都在返回前删除。
    pub(super) fn diagnostics_checks(&self, _input: Value) -> Result<Value, ApiError> {
        let data_dir = self
            .asset_root
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let db_path = PathBuf::from(self.connection.path().unwrap_or_default());
        let db_file = db_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let mut checks: Vec<Value> = Vec::new();
        // 1. 数据目录解析与可写（解析失败在 Application 入口即被拒绝；此处验证可写与存在）
        checks.push(check_data_directory(&data_dir));
        // 2. 数据库可打开且模式版本与当前应用一致（打开失败在入口已拒绝；过高版本见 doctor 降级路径）
        let user_version: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        checks.push(match user_version.cmp(&crate::DATABASE_SCHEMA_VERSION) {
            std::cmp::Ordering::Equal => check_item(
                "database_schema",
                "pass",
                "数据库正常，模式版本与当前应用一致",
                None,
                &format!(
                    "user_version={user_version} required={}",
                    crate::DATABASE_SCHEMA_VERSION
                ),
            ),
            std::cmp::Ordering::Less => check_item(
                "database_schema",
                "warn",
                "数据库模式版本低于当前应用，下次打开时会自动升级",
                None,
                &format!(
                    "user_version={user_version} required={}",
                    crate::DATABASE_SCHEMA_VERSION
                ),
            ),
            std::cmp::Ordering::Greater => check_item(
                "database_schema",
                "fail",
                "数据库由更新版本的应用创建，请升级应用后再打开",
                None,
                &format!(
                    "user_version={user_version} required={}",
                    crate::DATABASE_SCHEMA_VERSION
                ),
            ),
        });
        // 3. WAL 生效（pragma 只读查询）
        let journal_mode: String = self
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        checks.push(if journal_mode == "wal" {
            check_item(
                "database_wal",
                "pass",
                "数据库日志模式正常",
                None,
                "journal_mode=wal",
            )
        } else {
            check_item(
                "database_wal",
                "warn",
                "数据库未启用 WAL 日志模式，可靠性与并发读取受限",
                None,
                &format!("journal_mode={journal_mode}"),
            )
        });
        // 4. 日志目录试写（写失败只降级诊断能力，不阻塞业务，故为 warn）
        checks.push(check_logs_writable(&data_dir));
        // 5. 数据卷剩余空间：fail < 100 MiB，warn < 1 GiB（初始阈值，与 ADR 语义一致）
        checks.push(check_disk_space(&data_dir));
        // 6. 本机安装代际一致（与其他进程并发恢复/更换后须重启加载）
        let generation: i64 = self.connection.query_row(
            "SELECT generation FROM local_installation_state WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        checks.push(if generation == self.installation_generation {
            check_item(
                "installation_generation",
                "pass",
                "本机安装状态正常",
                None,
                &format!("generation={generation}"),
            )
        } else {
            check_item(
                "installation_generation",
                "warn",
                "检测到数据在本机被恢复或更换，请重启应用后继续使用",
                None,
                &format!(
                    "opened_generation={} current_generation={generation}",
                    self.installation_generation
                ),
            )
        });
        // 7. 加密钱包状态：未配置 / 已解锁 / 已锁定（只报状态，绝不触碰密钥材料）
        checks.push(match crate::encryption::wallet_status(self) {
            crate::encryption::WalletStatus::NotConfigured => check_item(
                "encryption_wallet",
                "pass",
                "未启用加密；如需保护隐私，可到「加密与设备」开启",
                None,
                "wallet=not_configured",
            ),
            crate::encryption::WalletStatus::Unlocked => check_item(
                "encryption_wallet",
                "pass",
                "加密空间已解锁，运行正常",
                None,
                "wallet=unlocked",
            ),
            crate::encryption::WalletStatus::Locked => check_item(
                "encryption_wallet",
                "warn",
                "加密空间存在但本机无法解锁，需要恢复材料；请到「加密与设备」处理",
                Some("encryption"),
                "wallet=locked",
            ),
        });
        // 8. 模型凭据是否已配置（只报是/否计数；未配置模型属正常状态）
        checks.push(match self.credential_check_summary() {
            Ok((0, _, _, _)) => check_item(
                "model_credentials",
                "pass",
                "尚未配置模型；AI 功能需先到「模型与连接」设置",
                Some("model"),
                "providers=0",
            ),
            Ok((total, missing, local, false)) if missing == 0 => check_item(
                "model_credentials",
                "pass",
                "模型凭据已配置",
                None,
                &format!("providers={total} local_without_key={local}"),
            ),
            Ok((total, missing, local, false)) => check_item(
                "model_credentials",
                "warn",
                &format!("{missing} 个模型的 API 密钥未配置，相关功能暂不可用；请到「模型与连接」补全"),
                Some("model"),
                &format!("providers={total} credential_missing={missing} local_without_key={local}"),
            ),
            Ok((total, missing, local, true)) => check_item(
                "model_credentials",
                "warn",
                "系统安全存储不可用，无法确认模型密钥状态；请解锁系统钥匙串后重试",
                Some("model"),
                &format!("providers={total} credential_missing={missing} local_without_key={local} store_unavailable=true"),
            ),
            Err(error) => check_item(
                "model_credentials",
                "warn",
                "暂时无法读取模型配置",
                Some("model"),
                &format!("error_code={}", error.code),
            ),
        });
        // 9. 卡住任务：租约已过期仍处于 running 的持久任务计数
        //（租约过期谓词从 jobs 导出：与 job_claim 接管资格同一口径，单一来源）
        let now = chrono::Utc::now().timestamp_millis();
        let stuck: i64 = self.connection.query_row(
            &format!(
                "SELECT count(*) FROM jobs WHERE {}",
                crate::jobs::lease_expired_sql()
            ),
            [now],
            |row| row.get(0),
        )?;
        checks.push(if stuck == 0 {
            check_item("stuck_jobs", "pass", "没有卡住的任务", None, "stuck_jobs=0")
        } else {
            check_item(
                "stuck_jobs",
                "warn",
                &format!("{stuck} 个任务卡在执行中；请到「任务中心」重试或取消"),
                Some("tasks"),
                &format!("stuck_jobs={stuck}"),
            )
        });
        // 10. 待发布数据位置标记残留（.wardrobe-location.json phase=pending）
        checks.push(check_pending_marker(&data_dir, db_file));
        Ok(checks_result(checks))
    }
}

/// 单个检查项：id、pass/warn/fail、通俗中文消息、建议去向（设置页标识或 null）、技术明细。
fn check_item(id: &str, status: &str, message: &str, setting: Option<&str>, detail: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "message": message,
        "setting": setting.map(Value::from).unwrap_or(Value::Null),
        "detail": detail,
    })
}

/// 试写探测：新建唯一命名文件写入一字节后立即删除；探测绝不留下文件，也绝不改动既有内容。
fn probe_writable(dir: &Path) -> std::io::Result<()> {
    let probe = dir.join(format!(".wardrobe-check-{}.tmp", uuid::Uuid::now_v7()));
    let result = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
        .and_then(|mut file| file.write_all(b"ok"));
    let _ = fs::remove_file(&probe);
    result
}

/// 数据目录检查（Application 路径与 doctor 降级路径共用同一消息与结构）。
fn check_data_directory(data_dir: &Path) -> Value {
    match probe_writable(data_dir) {
        Ok(()) => check_item(
            "data_directory",
            "pass",
            "数据目录正常",
            None,
            &format!("dir={} writable=true", data_dir.display()),
        ),
        Err(error) => check_item(
            "data_directory",
            "fail",
            "数据目录无法写入，新数据可能无法保存，请到「备份与恢复」更换数据位置",
            Some("backup"),
            &format!("dir={} writable=false error={error}", data_dir.display()),
        ),
    }
}

/// 日志目录试写检查（共用）。
fn check_logs_writable(data_dir: &Path) -> Value {
    let logs_dir = data_dir.join("logs");
    match probe_writable(&logs_dir) {
        Ok(()) => check_item(
            "logs_writable",
            "pass",
            "日志记录正常",
            None,
            &format!("logs={} writable=true", logs_dir.display()),
        ),
        Err(error) => check_item(
            "logs_writable",
            "warn",
            "日志无法写入，问题排查能力受限；请到「备份与恢复」检查数据位置所在磁盘",
            Some("backup"),
            &format!("logs={} writable=false error={error}", logs_dir.display()),
        ),
    }
}

/// 数据卷剩余空间检查：fail < 100 MiB，warn < 1 GiB（初始阈值，与 ADR 语义一致）。
fn check_disk_space(data_dir: &Path) -> Value {
    match available_bytes(data_dir) {
        Some(free) if free < 100 * 1024 * 1024 => check_item(
            "disk_space",
            "fail",
            "磁盘空间严重不足（不足 100 MB），可能无法保存新数据，请清理或更换数据位置",
            Some("backup"),
            &format!("free_bytes={free}"),
        ),
        Some(free) if free < 1024 * 1024 * 1024 => check_item(
            "disk_space",
            "warn",
            "磁盘空间不足 1 GB，请尽快清理或到「备份与恢复」更换数据位置",
            Some("backup"),
            &format!("free_bytes={free}"),
        ),
        Some(free) => check_item(
            "disk_space",
            "pass",
            "磁盘空间充足",
            None,
            &format!("free_bytes={free}"),
        ),
        None => check_item(
            "disk_space",
            "warn",
            "暂时无法检测磁盘剩余空间",
            None,
            "free_space=unknown",
        ),
    }
}

/// 依赖数据库的检查项在降级路径下的占位：如实说明无法检查。
fn check_unavailable_without_database(id: &str) -> Value {
    check_item(
        id,
        "warn",
        "数据库未能打开，无法检查此项；请先按 database_schema 项的提示处理",
        None,
        "database_unavailable",
    )
}

/// 组装检查结果封套（Application 路径与降级路径共用同一 schema）。
fn checks_result(checks: Vec<Value>) -> Value {
    let counts = json!({
        "pass": checks.iter().filter(|c| c["status"] == "pass").count(),
        "warn": checks.iter().filter(|c| c["status"] == "warn").count(),
        "fail": checks.iter().filter(|c| c["status"] == "fail").count(),
    });
    json!({"schema_version": 1, "checks": checks, "counts": counts})
}

/// CLI `doctor` 的同源体检报告（#48）：正常路径直接执行 diagnostics.summary 与
/// diagnostics.checks（与 GUI 诊断页完全同一操作）；数据库无法打开（过高模式版本、
/// 位置切换锁占用、文件损坏等）时降级为文件系统级检查与 database_schema=fail——
/// 「打不开」正是最需要体检的场景。产物只读：除打开自身的既有迁移语义外无任何写入。
pub fn doctor_report(db_path: &Path) -> Value {
    match Application::open(db_path) {
        Ok(app) => {
            let summary = app.execute("diagnostics.summary", json!({})).ok();
            let checks = app.execute("diagnostics.checks", json!({})).ok();
            json!({
                "schema_version": 1,
                "error": Value::Null,
                "summary": summary,
                "checks": checks,
            })
        }
        Err(error) => json!({
            "schema_version": 1,
            "error": serde_json::to_value(&error).unwrap_or(Value::Null),
            "summary": Value::Null,
            "checks": checks_without_database(db_path, &error),
        }),
    }
}

/// doctor 降级路径：数据库打不开时仍给出十项体检——文件系统级四项照常真实检查，
/// database_schema 报 fail 并携带打开失败原因，其余依赖数据库的项如实报 warn。
fn checks_without_database(db_path: &Path, error: &crate::ApiError) -> Value {
    let data_dir = db_path.parent().unwrap_or(Path::new("."));
    let db_file = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let detail = match error.reason() {
        Some(reason) => format!(
            "open_failed code={} message={} reason={reason}",
            error.code, error.message
        ),
        None => format!("open_failed code={} message={}", error.code, error.message),
    };
    checks_result(vec![
        check_data_directory(data_dir),
        check_item(
            "database_schema",
            "fail",
            "数据库无法打开；请按技术明细中的原因处理（例如升级应用）",
            None,
            &detail,
        ),
        check_unavailable_without_database("database_wal"),
        check_logs_writable(data_dir),
        check_disk_space(data_dir),
        check_unavailable_without_database("installation_generation"),
        check_unavailable_without_database("encryption_wallet"),
        check_unavailable_without_database("model_credentials"),
        check_unavailable_without_database("stuck_jobs"),
        check_pending_marker(data_dir, db_file),
    ])
}

/// 数据卷剩余空间（fdp 需要读取目录本身，不做任何写入）；不可检测时返回 None。
#[cfg(unix)]
fn available_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let target = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut volume: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY：target 是合法的 NUL 结尾 C 字符串，volume 是有效的 statvfs 存储
    let result = unsafe { libc::statvfs(target.as_ptr(), &mut volume) };
    if result == 0 {
        Some(volume.f_bavail as u64 * volume.f_bsize as u64)
    } else {
        None
    }
}
#[cfg(not(unix))]
fn available_bytes(_path: &Path) -> Option<u64> {
    None
}

/// 待发布数据位置标记残留检查：与当前数据库同目录的 .wardrobe-location.json。
/// phase=active 是已完成切换的常态；phase=pending 且 source_file 与当前库不符为残留（warn），
/// 相符则说明切换未完成（fail，正常应已被打开时的恢复逻辑消费或因迁移锁拒绝打开）。
fn check_pending_marker(data_dir: &Path, db_file: &str) -> Value {
    let marker = data_dir.join(".wardrobe-location.json");
    if !marker.exists() {
        return check_item(
            "pending_location_marker",
            "pass",
            "数据位置正常",
            None,
            "marker=none",
        );
    }
    let parsed: Result<Value, String> = std::fs::File::open(&marker)
        .map_err(|error| format!("marker_read_error={error}"))
        .and_then(|file| {
            let mut bytes = Vec::new();
            file.take(16385)
                .read_to_end(&mut bytes)
                .map_err(|error| format!("marker_read_error={error}"))?;
            serde_json::from_slice(&bytes).map_err(|error| format!("marker_parse_error={error}"))
        });
    match parsed {
        Ok(record) if record["phase"] == "active" => check_item(
            "pending_location_marker",
            "pass",
            "数据位置正常",
            None,
            "marker=active",
        ),
        Ok(record) if record["phase"] == "pending" => {
            let matches_current = record["source_file"].as_str() == Some(db_file);
            let detail = format!(
                "marker=pending source_file={} matches_current={matches_current}",
                record["source_file"].as_str().unwrap_or(""),
            );
            if matches_current {
                check_item(
                    "pending_location_marker",
                    "fail",
                    "数据位置切换未完成，旧目录仍在等待发布；请到「备份与恢复」重新发起或确认数据位置",
                    Some("backup"),
                    &detail,
                )
            } else {
                check_item(
                    "pending_location_marker",
                    "warn",
                    "检测到未完成的数据位置切换标记残留；请到「备份与恢复」确认数据位置",
                    Some("backup"),
                    &detail,
                )
            }
        }
        Ok(record) => check_item(
            "pending_location_marker",
            "warn",
            "数据位置标记无法识别；请到「备份与恢复」确认数据位置",
            Some("backup"),
            &format!("marker=unknown phase={}", record["phase"]),
        ),
        Err(detail) => check_item(
            "pending_location_marker",
            "warn",
            "数据位置标记损坏或不可读；请到「备份与恢复」确认数据位置",
            Some("backup"),
            &detail,
        ),
    }
}
