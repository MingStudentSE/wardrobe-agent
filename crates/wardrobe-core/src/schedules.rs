use crate::{string, ApiError, Application};
use chrono::{Datelike, TimeZone};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS schedule_plans(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS schedule_executions(id TEXT PRIMARY KEY,plan_id TEXT NOT NULL REFERENCES schedule_plans(id),member_id TEXT NOT NULL REFERENCES members(id),due_at TEXT NOT NULL,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS schedule_notifications(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS schedule_local_cursors(space_id TEXT NOT NULL,plan_id TEXT NOT NULL,revision INTEGER NOT NULL,cursor TEXT NOT NULL,PRIMARY KEY(space_id,plan_id,revision)); CREATE TABLE IF NOT EXISTS schedule_local_receipts(space_id TEXT NOT NULL,member_id TEXT NOT NULL,execution_id TEXT NOT NULL,document TEXT NOT NULL,PRIMARY KEY(space_id,member_id,execution_id)); CREATE TABLE IF NOT EXISTS schedule_local_runtime(id INTEGER PRIMARY KEY CHECK(id=1),document TEXT NOT NULL); INSERT OR IGNORE INTO schedule_local_runtime VALUES(1,'{\"revision\":1,\"background_enabled\":false,\"notifications_enabled\":false}'); CREATE TABLE IF NOT EXISTS schedule_local_device(id INTEGER PRIMARY KEY CHECK(id=1),device_id TEXT NOT NULL);")?;
    tx.execute(
        "INSERT OR IGNORE INTO schedule_local_device(id,device_id) VALUES(1,?1)",
        [uuid::Uuid::now_v7().to_string()],
    )?;
    Ok(())
}
fn bad(message: &str) -> ApiError {
    ApiError::new("invalid_input", message)
}
pub(super) fn validate(doc: &Value) -> Result<(), ApiError> {
    let name = string(doc, "name")?;
    if name.trim().is_empty() || name.chars().count() > 120 {
        return Err(bad("计划名称须为1至120字"));
    }
    if !matches!(string(doc, "kind")?, "recommendation" | "care") {
        return Err(bad("请选择推荐或护理提醒"));
    }
    let time = string(doc, "local_time")?;
    if time.len() != 5 || chrono::NaiveTime::parse_from_str(time, "%H:%M").is_err() {
        return Err(bad("时间须为HH:MM"));
    }
    string(doc, "timezone")?
        .parse::<chrono_tz::Tz>()
        .map_err(|_| bad("请选择有效IANA时区"))?;
    let days = doc["weekdays"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 7)
        .ok_or_else(|| bad("请选择每周执行日"))?;
    let mut seen = std::collections::HashSet::new();
    for day in days {
        let n = day
            .as_u64()
            .filter(|n| (1..=7).contains(n))
            .ok_or_else(|| bad("星期为1至7"))?;
        if !seen.insert(n) {
            return Err(bad("星期不得重复"));
        }
    }
    uuid::Uuid::parse_str(string(doc, "execution_device_id")?)
        .map_err(|_| bad("执行设备须为设备ID"))?;
    if doc["enabled"].as_bool().is_none() {
        return Err(bad("enabled须为布尔值"));
    }
    if !matches!(doc["target_day_offset"].as_u64(), Some(0 | 1)) {
        return Err(bad("推荐日期为当天或次日"));
    }
    if !matches!(doc["catchup_minutes"].as_u64(), Some(1..=1440)) {
        return Err(bad("补跑窗口为1至1440分钟"));
    }
    if !matches!(
        doc["occasion"].as_str(),
        Some("daily" | "work" | "formal" | "sport")
    ) {
        return Err(bad("请选择日常、工作、正式或运动场合"));
    }
    if doc
        .get("strategy")
        .is_some_and(|v| !matches!(v.as_str(), Some("rules" | "ai")))
    {
        return Err(bad("推荐策略为rules或ai"));
    }
    if doc["strategy"] == "ai" && doc["provider_id"].as_str().is_none_or(|v| v.is_empty()) {
        return Err(bad("AI计划须明确供应商"));
    }
    if let Some(weather) = doc.get("weather") {
        if weather["mode"] != "automatic" {
            crate::recommendations::weather(Some(weather))?;
        } else if weather.as_object().is_none_or(|m| m.len() != 1) {
            return Err(bad("自动天气只接受mode"));
        }
    }
    Ok(())
}
impl Application {
    pub(super) fn schedule_execute(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        let keys: &[&str] = match op {
            "schedule.device" | "schedule.runtime.get" => &[],
            "schedule.runtime.configure" => &[
                "expected_revision",
                "background_enabled",
                "notifications_enabled",
            ],
            "schedule.list" | "schedule.notifications" | "schedule.tick" | "schedule.pending" => {
                &["member_id"]
            }
            "schedule.history" | "schedule.worker.run" | "schedule.notification.retry" => {
                &["member_id", "id"]
            }
            _ => &[
                "member_id",
                "id",
                "expected_revision",
                "name",
                "kind",
                "local_time",
                "timezone",
                "weekdays",
                "execution_device_id",
                "occasion",
                "target_day_offset",
                "enabled",
                "catchup_minutes",
                "strategy",
                "provider_id",
                "weather",
            ],
        };
        if input
            .as_object()
            .is_none_or(|o| o.keys().any(|k| !keys.contains(&k.as_str())))
        {
            return Err(bad("计划包含不支持字段"));
        }
        if op.starts_with("schedule.runtime.") {
            let tx = self.begin_write()?;
            let raw: String = tx.query_row(
                "SELECT document FROM schedule_local_runtime WHERE id=1",
                [],
                |r| r.get(0),
            )?;
            let mut doc: Value = serde_json::from_str(&raw)
                .map_err(|_| ApiError::new("storage_error", "本机调度设置损坏"))?;
            if op == "schedule.runtime.configure" {
                if input["expected_revision"] != doc["revision"] {
                    return Err(ApiError::new("revision_conflict", "本机设置已更新"));
                }
                for key in ["background_enabled", "notifications_enabled"] {
                    if let Some(value) = input.get(key) {
                        if !value.is_boolean() {
                            return Err(bad("后台与通知选项须为布尔值"));
                        }
                        doc[key] = value.clone();
                    }
                }
                doc["revision"] = json!(doc["revision"].as_u64().unwrap() + 1);
                tx.execute(
                    "UPDATE schedule_local_runtime SET document=?1 WHERE id=1",
                    [doc.to_string()],
                )?;
            } else if op != "schedule.runtime.get" {
                return Err(bad("本机调度设置操作无效"));
            }
            tx.commit()?;
            return Ok(doc);
        }
        if op == "schedule.device" {
            if input.as_object().unwrap().len() != 0 {
                return Err(bad("设备查询不接受参数"));
            }
            return Ok(
                json!({"id":self.connection.query_row("SELECT device_id FROM schedule_local_device WHERE id=1",[],|r|r.get::<_,String>(0))?}),
            );
        }
        let member = self.member(&input)?;
        if op == "schedule.tick" {
            return self.schedule_tick_member_at(Some(&member), chrono::Utc::now());
        }
        if op == "schedule.pending" {
            return Ok(json!({"items":self.schedule_pending_for(Some(&member))?}));
        }
        if op == "schedule.notification.retry" {
            let tx = self.begin_write()?;
            let id = string(&input, "id")?;
            let raw: Option<String> = tx
                .query_row(
                    "SELECT document FROM schedule_notifications WHERE id=?1 AND member_id=?2",
                    params![id, member],
                    |r| r.get(0),
                )
                .optional()?;
            let mut doc: Value =
                serde_json::from_str(&raw.ok_or_else(|| ApiError::new("not_found", "通知不存在"))?)
                    .map_err(|_| ApiError::new("storage_error", "通知损坏"))?;
            if doc["status"] != "failed" {
                return Err(ApiError::new(
                    "notification_not_retryable",
                    "只有明确未提交的失败通知可重试；未知结果不能盲重发",
                ));
            }
            doc["status"] = json!("pending");
            doc["error"] = Value::Null;
            doc.as_object_mut().unwrap().remove("claim_token");
            tx.execute(
                "UPDATE schedule_notifications SET document=?1 WHERE id=?2",
                params![doc.to_string(), id],
            )?;
            tx.commit()?;
            return Ok(doc);
        }
        if op == "schedule.worker.run" {
            return self.schedule_run(&member, string(&input, "id")?);
        }
        if op == "schedule.notifications" {
            let mut stmt = self.connection.prepare(
                "SELECT document FROM schedule_notifications WHERE member_id=?1 ORDER BY id",
            )?;
            let raw = stmt
                .query_map([&member], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let items = raw
                .into_iter()
                .map(|s| {
                    serde_json::from_str::<Value>(&s)
                        .map_err(|_| ApiError::new("storage_error", "通知记录损坏"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(json!({"items":items}));
        }
        if op == "schedule.history" {
            let id = string(&input, "id")?;
            let mut stmt=self.connection.prepare("SELECT document FROM schedule_executions WHERE plan_id=?1 AND member_id=?2 ORDER BY due_at DESC LIMIT 100")?;
            let raw = stmt
                .query_map(params![id, member], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let docs = raw
                .into_iter()
                .map(|s| {
                    serde_json::from_str::<Value>(&s)
                        .map_err(|_| ApiError::new("storage_error", "执行记录损坏"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(json!({"executions":docs}));
        }
        if op == "schedule.list" {
            if input.as_object().unwrap().len() != 1 {
                return Err(bad("列表只接受成员"));
            }
            let mut stmt = self
                .connection
                .prepare("SELECT document FROM schedule_plans WHERE member_id=?1 ORDER BY id")?;
            let docs = stmt
                .query_map([&member], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let plans = docs
                .into_iter()
                .map(|s| {
                    serde_json::from_str::<Value>(&s)
                        .map_err(|_| ApiError::new("storage_error", "计划数据无效"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(json!({"plans":plans}));
        }
        let tx = self.begin_write()?;
        let mut doc = match op {
            "schedule.create" => {
                if input.get("id").is_some() || input.get("expected_revision").is_some() {
                    return Err(bad("创建不接受ID或版本"));
                }
                let mut d = input.clone();
                d["id"] = json!(uuid::Uuid::now_v7().to_string());
                d["member_id"] = json!(member);
                d["revision"] = json!(1);
                d["schema_version"] = json!(1);
                if d.get("catchup_minutes").is_none() {
                    d["catchup_minutes"] = json!(120);
                }
                d
            }
            "schedule.update" => {
                let id = string(&input, "id")?;
                let serialized = tx
                    .query_row(
                        "SELECT document FROM schedule_plans WHERE id=?1 AND member_id=?2",
                        params![id, member],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()?
                    .ok_or_else(|| ApiError::new("not_found", "计划不存在"))?;
                let mut d: Value = serde_json::from_str(&serialized)
                    .map_err(|_| ApiError::new("storage_error", "计划数据无效"))?;
                if input["expected_revision"].as_u64().is_none() {
                    return Err(bad("修改须提供版本"));
                }
                if input["expected_revision"] != d["revision"] {
                    return Err(ApiError::new("revision_conflict", "计划已更新，请重新加载"));
                }
                for (k, v) in input.as_object().unwrap() {
                    if !matches!(k.as_str(), "member_id" | "id" | "expected_revision") {
                        d[k] = v.clone();
                    }
                }
                d["revision"] = json!(d["revision"].as_u64().unwrap() + 1);
                d
            }
            _ => return Err(ApiError::new("unsupported_operation", "此计划操作尚不可用")),
        };
        validate(&doc)?;
        if doc["kind"] == "recommendation" && doc["strategy"] == "ai" {
            let provider = self.provider_get(string(&doc, "provider_id")?)?;
            if !provider["capabilities"]
                .as_array()
                .is_some_and(|v| v.contains(&json!("text")))
            {
                return Err(bad("计划模型须支持text"));
            }
        }
        if input.get("weather").is_some()
            && doc["weather"]["mode"] == "manual"
            && doc["weather"].get("obtained_at").is_none()
        {
            doc["weather"]["obtained_at"] = json!(chrono::Utc::now().to_rfc3339());
        }
        if op == "schedule.update" {
            tx.execute("UPDATE schedule_executions SET document=json_set(document,'$.status','cancelled','$.reason','计划修订已改变') WHERE plan_id=?1 AND json_extract(document,'$.status') IN ('queued','running')",[doc["id"].as_str()])?;
            tx.execute("UPDATE schedule_notifications SET document=json_set(document,'$.status','cancelled') WHERE json_extract(document,'$.job_id') IN (SELECT json_extract(document,'$.job_id') FROM schedule_executions WHERE plan_id=?1) AND json_extract(document,'$.status')='pending'",[doc["id"].as_str()])?;
            let pending = {
                let mut stmt=tx.prepare("SELECT document FROM jobs WHERE member_id=?1 AND id IN (SELECT json_extract(document,'$.job_id') FROM schedule_executions WHERE plan_id=?2) AND json_extract(document,'$.state') IN ('queued','running')")?;
                let rows = stmt
                    .query_map(params![member, doc["id"].as_str()], |r| {
                        r.get::<_, String>(0)
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };
            for raw in pending {
                let mut job: Value = serde_json::from_str(&raw)
                    .map_err(|_| ApiError::new("storage_error", "任务损坏"))?;
                job["state"] = json!("cancelled");
                job["lease_token"] = Value::Null;
                job["lease_expires"] = json!(0);
                crate::jobs::save(&tx, &job)?;
            }
        }
        doc["eligible_since"] = json!(chrono::Utc::now().to_rfc3339());
        doc["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute("INSERT INTO schedule_plans(id,member_id,document) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET document=excluded.document",params![doc["id"].as_str(),member,doc.to_string()])?;
        tx.execute(
            "INSERT INTO audit(entity_id,operation,document) VALUES(?1,?2,?3)",
            params![doc["id"].as_str(), op, doc.to_string()],
        )?;
        tx.execute(
            "INSERT INTO outbox(entity_id,document) VALUES(?1,?2)",
            params![doc["id"].as_str(), doc.to_string()],
        )?;
        tx.commit()?;
        Ok(doc)
    }
}

impl Application {
    /// Trusted clock seam for the local scheduler. External clients cannot supply wall-clock time.
    pub fn schedule_tick_at(&self, now: chrono::DateTime<chrono::Utc>) -> Result<Value, ApiError> {
        self.schedule_tick_member_at(None, now)
    }
    fn schedule_tick_member_at(
        &self,
        member: Option<&str>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Value, ApiError> {
        let tx = self.begin_write()?;
        let device: String = tx.query_row(
            "SELECT device_id FROM schedule_local_device WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let plans = {
            let mut stmt = tx.prepare(
                "SELECT document FROM schedule_plans WHERE (?1 IS NULL OR member_id=?1)",
            )?;
            let raw = stmt
                .query_map([member], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            raw
        };
        let space: String = tx.query_row("SELECT id FROM workspace", [], |r| r.get(0))?;
        let mut queued = vec![];
        for raw in plans {
            let doc: Value = serde_json::from_str(&raw)
                .map_err(|_| ApiError::new("storage_error", "计划损坏"))?;
            validate(&doc)?;
            if doc["enabled"] != true || doc["execution_device_id"] != device {
                continue;
            }
            let zone: chrono_tz::Tz = doc["timezone"].as_str().unwrap().parse().unwrap();
            let today = now.with_timezone(&zone).date_naive();
            let time =
                chrono::NaiveTime::parse_from_str(doc["local_time"].as_str().unwrap(), "%H:%M")
                    .unwrap();
            let eligible =
                chrono::DateTime::parse_from_rfc3339(doc["eligible_since"].as_str().unwrap())
                    .map_err(|_| ApiError::new("storage_error", "计划生效时间损坏"))?
                    .with_timezone(&chrono::Utc);
            let stored_cursor:Option<String>=tx.query_row("SELECT cursor FROM schedule_local_cursors WHERE space_id=?1 AND plan_id=?2 AND revision=?3",params![space,doc["id"].as_str(),doc["revision"].as_u64()],|r|r.get(0)).optional()?;
            let cursor = stored_cursor
                .as_deref()
                .map(chrono::DateTime::parse_from_rfc3339)
                .transpose()
                .map_err(|_| ApiError::new("storage_error", "计划评估时间损坏"))?
                .map(|v| v.with_timezone(&chrono::Utc))
                .unwrap_or(eligible)
                .max(eligible);
            if now < cursor {
                continue;
            }
            let recent_boundary = (today - chrono::Duration::days(7))
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let boundary = first_valid_local(zone, recent_boundary)?;
            if cursor < boundary && has_due_between(&doc, zone, time, cursor, boundary) {
                let id = format!(
                    "{}:missed:{}:{}",
                    doc["id"].as_str().unwrap(),
                    cursor.timestamp(),
                    boundary.timestamp()
                );
                let summary = json!({"schema_version":1,"id":id,"plan_id":doc["id"],"member_id":doc["member_id"],"plan_revision":doc["revision"],"status":"missed","due_at":boundary.to_rfc3339(),"missed_interval":{"from":cursor.to_rfc3339(),"through_exclusive":boundary.to_rfc3339()},"reason":"设备停止期间的较早执行窗口未补跑"});
                tx.execute(
                    "INSERT OR IGNORE INTO schedule_executions VALUES(?1,?2,?3,?4,?5)",
                    params![
                        id,
                        doc["id"].as_str(),
                        doc["member_id"].as_str(),
                        boundary.to_rfc3339(),
                        summary.to_string()
                    ],
                )?;
            }
            let mut selected = false;
            for offset in 0..=7 {
                let date = today - chrono::Duration::days(offset);
                if !doc["weekdays"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(date.weekday().number_from_monday()))
                {
                    continue;
                }
                // Autumn overlap executes once at the earlier instant; spring gaps are skipped.
                let local = date.and_time(time);
                let Some(due) = zone
                    .from_local_datetime(&local)
                    .earliest()
                    .map(|v| v.with_timezone(&chrono::Utc))
                else {
                    if local <= now.with_timezone(&zone).naive_local()
                        && local >= cursor.with_timezone(&zone).naive_local()
                    {
                        let id = format!(
                            "{}:{}:gap",
                            doc["id"].as_str().unwrap(),
                            local.format("%Y-%m-%dT%H:%M")
                        );
                        let skipped = json!({"schema_version":1,"id":id,"plan_id":doc["id"],"member_id":doc["member_id"],"plan_revision":doc["revision"],"status":"missed","reason":"nonexistent_local_time","scheduled_local_time":local.format("%Y-%m-%dT%H:%M").to_string(),"due_at":null,"evaluated_at":now.to_rfc3339(),"plan":doc});
                        tx.execute(
                            "INSERT OR IGNORE INTO schedule_executions VALUES(?1,?2,?3,?4,?5)",
                            params![
                                id,
                                doc["id"].as_str(),
                                doc["member_id"].as_str(),
                                now.to_rfc3339(),
                                skipped.to_string()
                            ],
                        )?;
                    }
                    continue;
                };
                if due > now {
                    continue;
                }
                if due < cursor {
                    continue;
                }
                let id = format!("{}:{}", doc["id"].as_str().unwrap(), due.timestamp());
                let status = if !selected
                    && now.signed_duration_since(due)
                        <= chrono::Duration::minutes(doc["catchup_minutes"].as_i64().unwrap())
                {
                    "queued"
                } else {
                    "missed"
                };
                selected = true;
                let mut execution = json!({"schema_version":1,"id":id,"plan_id":doc["id"],"member_id":doc["member_id"],"plan_revision":doc["revision"],"due_at":due.to_rfc3339(),"target_date":(date+chrono::Duration::days(doc["target_day_offset"].as_i64().unwrap())).to_string(),"status":status,"plan":doc,"created_at":now.to_rfc3339()});
                let retained:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM schedule_local_receipts WHERE space_id=?1 AND member_id=?2 AND execution_id=?3)",params![space,doc["member_id"].as_str(),id],|r|r.get(0))?;
                if retained {
                    continue;
                }
                let inserted=tx.execute("INSERT OR IGNORE INTO schedule_executions(id,plan_id,member_id,due_at,document) VALUES(?1,?2,?3,?4,?5)",params![id,doc["id"].as_str(),doc["member_id"].as_str(),due.to_rfc3339(),execution.to_string()])?;
                if inserted == 1 && status == "queued" {
                    let (kind, payload) =
                        if doc["kind"] == "recommendation" && doc["strategy"] == "ai" {
                            (
                                "text_recommendation",
                                recommendation_input(&doc, &execution),
                            )
                        } else {
                            ("schedule", execution.clone())
                        };
                    let job = crate::jobs::insert(
                        &tx,
                        doc["member_id"].as_str().unwrap(),
                        kind,
                        &id,
                        payload,
                    )?;
                    execution["job_id"] = job["id"].clone();
                    tx.execute(
                        "UPDATE schedule_executions SET document=?1 WHERE id=?2",
                        params![execution.to_string(), id],
                    )?;
                    tx.execute(
                        "INSERT INTO schedule_local_receipts VALUES(?1,?2,?3,?4)",
                        params![space, doc["member_id"].as_str(), id, execution.to_string()],
                    )?;
                    queued.push(execution);
                }
            }
            tx.execute("INSERT INTO schedule_local_cursors VALUES(?1,?2,?3,?4) ON CONFLICT(space_id,plan_id,revision) DO UPDATE SET cursor=excluded.cursor",params![space,doc["id"].as_str(),doc["revision"].as_u64(),now.to_rfc3339()])?;
        }
        tx.commit()?;
        Ok(json!({"queued":queued}))
    }
}

impl Application {
    fn schedule_run(&self, member: &str, id: &str) -> Result<Value, ApiError> {
        let before = self.job_get(json!({"member_id":member,"id":id}))?;
        let associated:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM schedule_executions WHERE member_id=?1 AND json_extract(document,'$.job_id')=?2)",params![member,id],|r|r.get(0))?;
        if !associated {
            return Err(bad("此任务未由成员定时计划登记"));
        }
        if before["kind"] == "text_recommendation" {
            let tx = self.begin_write()?;
            validate_related_job(&tx, &before)?;
            tx.commit()?;
            let job = self.ai_worker(json!({"member_id":member,"id":id}))?;
            if matches!(
                job["state"].as_str(),
                Some("succeeded" | "failed" | "waiting_input")
            ) {
                let tx = self.begin_write()?;
                validate_related_job(&tx, &job)?;
                let raw:String=tx.query_row("SELECT document FROM schedule_executions WHERE member_id=?1 AND json_extract(document,'$.job_id')=?2",params![member,id],|r|r.get(0))?;
                let execution: Value = serde_json::from_str(&raw)
                    .map_err(|_| ApiError::new("storage_error", "执行记录损坏"))?;
                publish(&tx, &job, &execution)?;
                tx.commit()?;
            }
            return Ok(job);
        }
        if before["kind"] != "schedule" {
            return Err(bad("此worker只执行定时任务"));
        }
        let job = self.job_claim(member, id, 300)?;
        let execution = &job["payload"];
        let plan = &execution["plan"];
        let result = if plan["kind"] == "care" {
            self.execute_current("care.reminders", json!({"member_id":member}))
        } else {
            self.execute_current(
                "recommendation.generate",
                recommendation_input(plan, execution),
            )
        };
        let tx = self.begin_write()?;
        let raw: String = tx.query_row(
            "SELECT document FROM jobs WHERE id=?1 AND member_id=?2",
            params![id, member],
            |r| r.get(0),
        )?;
        validate_job_plan(&tx, &job)?;
        let mut current: Value =
            serde_json::from_str(&raw).map_err(|_| ApiError::new("storage_error", "任务损坏"))?;
        if current["state"] != "running"
            || current["lease_token"] != job["lease_token"]
            || current["lease_expires"].as_i64().unwrap_or(0)
                < chrono::Utc::now().timestamp_millis()
        {
            return Err(ApiError::new("lease_lost", "任务已取消或lease失效"));
        }
        match result {
            Ok(value) => {
                current["state"] = json!("succeeded");
                current["result"] = value;
                current["progress"] = json!(100);
            }
            Err(error) => {
                current["state"] = json!("failed");
                current["error"] = json!({"code":error.code,"message":error.message});
            }
        }
        current["lease_token"] = Value::Null;
        current["lease_expires"] = json!(0);
        current["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        crate::jobs::save(&tx, &current)?;
        publish(&tx, &current, execution)?;
        tx.commit()?;
        Ok(current)
    }
}

fn plan_schema() -> Value {
    json!({"type":"object","required":["schema_version","id","member_id","revision","name","kind","local_time","timezone","weekdays","execution_device_id","occasion","target_day_offset","enabled","catchup_minutes","eligible_since","updated_at"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"revision":{"type":"integer","minimum":1},"name":{"type":"string","minLength":1,"maxLength":120},"kind":{"enum":["care","recommendation"]},"local_time":{"type":"string","pattern":"^[0-2][0-9]:[0-5][0-9]$"},"timezone":{"type":"string"},"weekdays":{"type":"array","minItems":1,"maxItems":7,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":7}},"execution_device_id":{"type":"string"},"occasion":{"enum":["daily","work","formal","sport"]},"target_day_offset":{"type":"integer","minimum":0,"maximum":1},"enabled":{"type":"boolean"},"catchup_minutes":{"type":"integer","minimum":1,"maximum":1440},"strategy":{"enum":["rules","ai"]},"provider_id":{"type":"string"},"weather":{"type":"object"},"eligible_since":{"type":"string","format":"date-time"},"updated_at":{"type":"string","format":"date-time"}}})
}
fn execution_schema() -> Value {
    json!({"type":"object","required":["schema_version","id","member_id","plan_id","plan_revision","status","due_at"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"plan_id":{"type":"string"},"plan_revision":{"type":"integer","minimum":1},"status":{"enum":["queued","running","succeeded","failed","waiting_input","cancelled","missed"]},"due_at":{"type":["string","null"]},"target_date":{"type":"string","format":"date"},"plan":plan_schema(),"job_id":{"type":"string"},"result":{},"error":{},"missed_interval":{"type":"object"},"reason":{"type":"string"}}})
}
fn notification_schema() -> Value {
    json!({"type":"object","required":["id","member_id","job_id","title","body","status","created_at"],"properties":{"id":{"type":"string"},"member_id":{"type":"string"},"job_id":{"type":"string"},"title":{"type":"string"},"body":{"type":"string"},"status":{"enum":["pending","submitting","submitted","unknown","failed","cancelled"]},"created_at":{"type":"string","format":"date-time"},"error":{"type":["string","null"]}}})
}
pub(super) fn descriptors() -> Vec<Value> {
    ["device","runtime.get","runtime.configure","create","update","list","history","tick","pending","worker.run","notifications","notification.retry"].into_iter().map(|op|{
 let mut properties=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];
 if matches!(op,"device"|"runtime.get"|"runtime.configure"){properties=json!({});required.clear();}
 if matches!(op,"update"|"history"|"worker.run"|"notification.retry"){properties["id"]=json!({"type":"string"});required.push("id");}
 if matches!(op,"create"|"update"){
  let fields=plan_schema();for key in ["name","kind","local_time","timezone","weekdays","execution_device_id","occasion","target_day_offset","enabled","catchup_minutes","strategy","provider_id","weather"]{properties[key]=fields["properties"][key].clone();}
  if op=="create"{required.extend(["name","kind","local_time","timezone","weekdays","execution_device_id","occasion","target_day_offset","enabled"]);}else{properties["expected_revision"]=json!({"type":"integer","minimum":1});required.push("expected_revision");}
 }
 if op=="runtime.configure" {properties=json!({"expected_revision":{"type":"integer","minimum":1},"background_enabled":{"type":"boolean"},"notifications_enabled":{"type":"boolean"}});required.push("expected_revision");}
 let output=match op {
 "device"=>json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}),
 "runtime.get"|"runtime.configure"=>json!({"type":"object","required":["revision","background_enabled","notifications_enabled"],"properties":{"revision":{"type":"integer","minimum":1},"background_enabled":{"type":"boolean"},"notifications_enabled":{"type":"boolean"},"last_error":{"type":["object","null"]}}}),
 "create"|"update"=>plan_schema(),
 "worker.run"=>crate::jobs::output_schema(),
 "notification.retry"=>notification_schema(),
 "list"=>json!({"type":"object","required":["plans"],"properties":{"plans":{"type":"array","items":plan_schema()}}}),
 "history"=>json!({"type":"object","required":["executions"],"properties":{"executions":{"type":"array","items":execution_schema()}}}),
 "tick"=>json!({"type":"object","required":["queued"],"properties":{"queued":{"type":"array","items":execution_schema()}}}),
 "pending"=>json!({"type":"object","required":["items"],"properties":{"items":{"type":"array","items":crate::jobs::output_schema()}}}),
 _=>json!({"type":"object","required":["items"],"properties":{"items":{"type":"array","items":notification_schema()}}}),
 };
 json!({"name":format!("schedule.{op}"),"description":"成员持久计划与执行记录；worker须原业务scope；生成不记录穿着；原生系统显示需桌面宿主","inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"outputSchema":output})
 }).collect()
}
impl Application {
    pub(super) fn schedule_required_operation(
        &self,
        operation: &str,
        input: &Value,
    ) -> Result<&'static str, ApiError> {
        let member = self.member(input)?;
        let kind = if operation == "schedule.worker.run" {
            let job = self.job_get(json!({"member_id":member,"id":input["id"]}))?;
            if job["kind"] == "text_recommendation" {
                return Ok("recommendation.generate");
            }
            if job["kind"] != "schedule" {
                return Err(bad("不是定时任务"));
            }
            job["payload"]["plan"]["kind"]
                .as_str()
                .unwrap_or("")
                .to_owned()
        } else if operation == "schedule.update" {
            let raw: String = self
                .connection
                .query_row(
                    "SELECT document FROM schedule_plans WHERE id=?1 AND member_id=?2",
                    params![string(input, "id")?, member],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| ApiError::new("not_found", "计划不存在"))?;
            let doc: Value = serde_json::from_str(&raw)
                .map_err(|_| ApiError::new("storage_error", "计划损坏"))?;
            input
                .get("kind")
                .unwrap_or(&doc["kind"])
                .as_str()
                .unwrap_or("")
                .to_owned()
        } else {
            input["kind"].as_str().unwrap_or("").to_owned()
        };
        Ok(if kind == "care" {
            "care.reminders"
        } else {
            "recommendation.generate"
        })
    }
}

pub(super) fn validate_job_plan(tx: &Transaction<'_>, job: &Value) -> Result<(), ApiError> {
    let snapshot = &job["payload"]["plan"];
    let raw: Option<String> = tx
        .query_row(
            "SELECT document FROM schedule_plans WHERE id=?1 AND member_id=?2",
            params![snapshot["id"].as_str(), job["member_id"].as_str()],
            |r| r.get(0),
        )
        .optional()?;
    let plan: Value =
        serde_json::from_str(&raw.ok_or_else(|| ApiError::new("plan_changed", "计划已移除"))?)
            .map_err(|_| ApiError::new("storage_error", "计划损坏"))?;
    let device: String = tx.query_row(
        "SELECT device_id FROM schedule_local_device WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    if plan["enabled"] != true
        || plan["revision"] != snapshot["revision"]
        || plan["execution_device_id"] != device
    {
        return Err(ApiError::new(
            "plan_changed",
            "计划已暂停、修改或由其他设备执行",
        ));
    }
    Ok(())
}

fn recommendation_input(plan: &Value, execution: &Value) -> Value {
    let mut input = json!({"member_id":execution["member_id"],"operation_key":execution["id"],"timezone":plan["timezone"],"occasion":plan["occasion"],"target_date":execution["target_date"],"weather":plan.get("weather").cloned().unwrap_or(json!({"mode":"off"}))});
    if plan["strategy"] == "ai" {
        input["strategy"] = json!("ai");
        input["provider_id"] = plan["provider_id"].clone();
    }
    input
}
fn publish(tx: &Transaction<'_>, job: &Value, execution: &Value) -> Result<(), ApiError> {
    let member = job["member_id"].as_str().unwrap();
    let mut finished = execution.clone();
    finished["status"] = job["state"].clone();
    finished["result"] = job["result"].clone();
    finished["error"] = job["error"].clone();
    tx.execute(
        "UPDATE schedule_executions SET document=?1 WHERE id=?2",
        params![finished.to_string(), execution["id"].as_str()],
    )?;
    let notification = json!({"id":execution["id"],"member_id":member,"job_id":job["id"],"title":execution["plan"]["name"],"body":match job["state"].as_str(){Some("succeeded")=>"计划已完成，可打开衣橱查看结果",Some("waiting_input")=>"请求结果未知，请打开衣橱核对；不会自动重复提交",_=>"计划执行失败，请打开衣橱查看原因"},"status":"pending","created_at":chrono::Utc::now().to_rfc3339()});
    tx.execute(
        "INSERT OR IGNORE INTO schedule_notifications VALUES(?1,?2,?3)",
        params![execution["id"].as_str(), member, notification.to_string()],
    )?;
    tx.execute("UPDATE schedule_local_receipts SET document=?1 WHERE space_id=(SELECT id FROM workspace) AND member_id=?2 AND execution_id=?3",params![finished.to_string(),member,execution["id"].as_str()])?;
    Ok(())
}
pub(super) fn validate_related_job(tx: &Transaction<'_>, job: &Value) -> Result<(), ApiError> {
    if job["kind"] == "schedule" {
        return validate_job_plan(tx, job);
    }
    let raw:Option<String>=tx.query_row("SELECT document FROM schedule_executions WHERE member_id=?1 AND json_extract(document,'$.job_id')=?2",params![job["member_id"].as_str(),job["id"].as_str()],|r|r.get(0)).optional()?;
    if let Some(raw) = raw {
        let doc: Value = serde_json::from_str(&raw)
            .map_err(|_| ApiError::new("storage_error", "执行记录损坏"))?;
        let virtual_job = json!({"member_id":job["member_id"],"payload":doc});
        validate_job_plan(tx, &virtual_job)?;
    }
    Ok(())
}

impl Application {
    /// Trusted native adapter seam. A durable claim precedes every OS submission.
    pub fn schedule_claim_notification(&self, member: &str) -> Result<Option<Value>, ApiError> {
        let member = self.member(&json!({"member_id":member}))?;
        let tx = self.begin_write()?;
        // A process may disappear after the durable claim and before its OS result.
        // Expose uncertainty after the lease; never retry an ambiguous delivery.
        let stale: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT id,document FROM schedule_notifications WHERE member_id=?1 AND json_extract(document,'$.status')='submitting'")?;
            let rows = stmt.query_map([&member], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        for (id, raw) in stale {
            let mut notice: Value = serde_json::from_str(&raw)
                .map_err(|_| ApiError::new("storage_error", "通知记录损坏"))?;
            let expired = notice["claimed_at"]
                .as_str()
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .map(|at| {
                    chrono::Utc::now().signed_duration_since(at) > chrono::Duration::seconds(60)
                })
                .unwrap_or(true);
            if expired {
                notice["status"] = json!("unknown");
                notice["error"] =
                    json!("通知提交进程未返回结果，无法确认系统是否接收；不会自动重复发送");
                notice["finished_at"] = json!(chrono::Utc::now().to_rfc3339());
                tx.execute(
                    "UPDATE schedule_notifications SET document=?1 WHERE id=?2",
                    params![notice.to_string(), id],
                )?;
            }
        }
        let enabled: bool = tx.query_row("SELECT COALESCE(json_extract(document,'$.notifications_enabled'),0) FROM schedule_local_runtime WHERE id=1",[],|r|r.get(0))?;
        if !enabled {
            tx.commit()?;
            return Ok(None);
        }
        let raw:Option<String>=tx.query_row("SELECT document FROM schedule_notifications WHERE member_id=?1 AND json_extract(document,'$.status')='pending' ORDER BY id LIMIT 1",[&member],|r|r.get(0)).optional()?;
        let Some(raw) = raw else {
            tx.commit()?;
            return Ok(None);
        };
        let mut doc: Value = serde_json::from_str(&raw)
            .map_err(|_| ApiError::new("storage_error", "通知记录损坏"))?;
        doc["status"] = json!("submitting");
        doc["claim_token"] = json!(uuid::Uuid::now_v7().to_string());
        doc["claimed_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE schedule_notifications SET document=?1 WHERE id=?2",
            params![doc.to_string(), doc["id"].as_str()],
        )?;
        tx.commit()?;
        Ok(Some(doc))
    }
    pub fn schedule_finish_notification(
        &self,
        member: &str,
        id: &str,
        claim: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<Value, ApiError> {
        if !matches!(status, "submitted" | "failed" | "unknown") {
            return Err(bad("原生通知提交状态无效"));
        }
        let member = self.member(&json!({"member_id":member}))?;
        let tx = self.begin_write()?;
        let raw: Option<String> = tx
            .query_row(
                "SELECT document FROM schedule_notifications WHERE id=?1 AND member_id=?2",
                params![id, member],
                |r| r.get(0),
            )
            .optional()?;
        let mut doc: Value =
            serde_json::from_str(&raw.ok_or_else(|| ApiError::new("not_found", "通知不存在"))?)
                .map_err(|_| ApiError::new("storage_error", "通知记录损坏"))?;
        if doc["status"] != "submitting" || doc["claim_token"] != claim {
            return Err(ApiError::new("lease_lost", "通知提交身份失效"));
        }
        doc["status"] = json!(status);
        doc["error"] = json!(error);
        doc["finished_at"] = json!(chrono::Utc::now().to_rfc3339());
        tx.execute(
            "UPDATE schedule_notifications SET document=?1 WHERE id=?2",
            params![doc.to_string(), id],
        )?;
        tx.commit()?;
        Ok(doc)
    }
}

pub(super) fn validate_portable_plan(doc: &Value) -> Result<(), ApiError> {
    let allowed = [
        "id",
        "member_id",
        "schema_version",
        "revision",
        "name",
        "kind",
        "local_time",
        "timezone",
        "weekdays",
        "execution_device_id",
        "occasion",
        "target_day_offset",
        "enabled",
        "catchup_minutes",
        "strategy",
        "provider_id",
        "weather",
        "eligible_since",
        "updated_at",
    ];
    if doc
        .as_object()
        .is_none_or(|m| m.keys().any(|k| !allowed.contains(&k.as_str())))
        || doc["schema_version"] != 1
        || doc["revision"].as_u64().is_none_or(|v| v < 1)
    {
        return Err(bad("计划格式无效"));
    }
    uuid::Uuid::parse_str(string(doc, "id")?).map_err(|_| bad("计划身份无效"))?;
    uuid::Uuid::parse_str(string(doc, "member_id")?).map_err(|_| bad("计划成员无效"))?;
    for field in ["eligible_since", "updated_at"] {
        chrono::DateTime::parse_from_rfc3339(string(doc, field)?)
            .map_err(|_| bad("计划时刻无效"))?;
    }
    validate(doc)
}
pub(super) fn validate_portable_execution(
    db: &rusqlite::Connection,
    doc: &Value,
) -> Result<(), ApiError> {
    let member = string(doc, "member_id")?;
    let plan = string(doc, "plan_id")?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM schedule_plans WHERE id=?1 AND member_id=?2)",
        params![plan, member],
        |r| r.get(0),
    )?;
    if !exists
        || doc["schema_version"] != 1
        || !matches!(
            doc["status"].as_str(),
            Some(
                "queued"
                    | "running"
                    | "succeeded"
                    | "failed"
                    | "waiting_input"
                    | "cancelled"
                    | "missed"
            )
        )
    {
        return Err(bad("执行引用或状态无效"));
    }
    let (sql_plan, sql_due): (String, String) = db.query_row(
        "SELECT plan_id,due_at FROM schedule_executions WHERE id=?1 AND member_id=?2",
        params![string(doc, "id")?, member],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if sql_plan != plan || doc["plan_revision"].as_u64().is_none_or(|n| n < 1) {
        return Err(bad("执行行与文档不一致"));
    }
    if doc["reason"] == "nonexistent_local_time" {
        let snapshot = &doc["plan"];
        validate_portable_plan(snapshot)?;
        let local = chrono::NaiveDateTime::parse_from_str(
            string(doc, "scheduled_local_time")?,
            "%Y-%m-%dT%H:%M",
        )
        .map_err(|_| bad("缺失当地时刻无效"))?;
        let zone: chrono_tz::Tz = string(snapshot, "timezone")?.parse().unwrap();
        if !doc["due_at"].is_null()
            || doc["status"] != "missed"
            || zone.from_local_datetime(&local).earliest().is_some()
            || snapshot["id"] != plan
            || snapshot["member_id"] != member
            || snapshot["revision"] != doc["plan_revision"]
            || snapshot["local_time"] != local.format("%H:%M").to_string()
            || doc["id"] != format!("{plan}:{}:gap", local.format("%Y-%m-%dT%H:%M"))
            || doc["evaluated_at"] != sql_due
        {
            return Err(bad("缺失当地时刻身份不匹配"));
        }
        chrono::DateTime::parse_from_rfc3339(string(doc, "evaluated_at")?)
            .map_err(|_| bad("评估时刻无效"))?;
        return Ok(());
    }
    if doc["due_at"] != sql_due {
        return Err(bad("执行到期行与文档不一致"));
    }
    let due = chrono::DateTime::parse_from_rfc3339(string(doc, "due_at")?)
        .map_err(|_| bad("执行到期无效"))?;
    if let Some(snapshot) = doc.get("plan") {
        validate_portable_plan(snapshot)?;
        if snapshot["id"] != plan
            || snapshot["member_id"] != member
            || snapshot["revision"] != doc["plan_revision"]
            || doc["id"] != format!("{plan}:{}", due.timestamp())
        {
            return Err(bad("执行身份不匹配"));
        }
        let zone: chrono_tz::Tz = string(snapshot, "timezone")?.parse().unwrap();
        let date = due.with_timezone(&zone).date_naive()
            + chrono::Duration::days(snapshot["target_day_offset"].as_i64().unwrap());
        if doc["target_date"] != date.to_string() {
            return Err(bad("执行目标日期无效"));
        }
    } else {
        if doc["status"] != "missed" {
            return Err(bad("执行快照缺失"));
        }
        let interval = &doc["missed_interval"];
        let from = chrono::DateTime::parse_from_rfc3339(string(interval, "from")?)
            .map_err(|_| bad("错过区间无效"))?;
        let through = chrono::DateTime::parse_from_rfc3339(string(interval, "through_exclusive")?)
            .map_err(|_| bad("错过区间无效"))?;
        if from >= through
            || through != due
            || doc["id"] != format!("{plan}:missed:{}:{}", from.timestamp(), through.timestamp())
        {
            return Err(bad("错过区间身份无效"));
        }
    }
    Ok(())
}

impl Application {
    pub fn schedule_pending_jobs(&self) -> Result<Vec<Value>, ApiError> {
        self.schedule_pending_for(None)
    }
    fn schedule_pending_for(&self, member: Option<&str>) -> Result<Vec<Value>, ApiError> {
        let mut statement=self.connection.prepare("SELECT j.document FROM schedule_executions e JOIN jobs j ON j.id=json_extract(e.document,'$.job_id') WHERE (?1 IS NULL OR j.member_id=?1) AND json_extract(j.document,'$.state') IN ('queued','running') AND json_extract(j.document,'$.next_due')<=?2 AND (json_extract(j.document,'$.state')='queued' OR json_extract(j.document,'$.lease_expires')<=?2) ORDER BY j.id LIMIT 16")?;
        let raw = statement
            .query_map(
                params![member, chrono::Utc::now().timestamp_millis()],
                |r| r.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|s| {
                serde_json::from_str(&s).map_err(|_| ApiError::new("storage_error", "定时任务损坏"))
            })
            .collect()
    }
    pub(super) fn schedule_tick_required_operations(
        &self,
        input: &Value,
    ) -> Result<Vec<&'static str>, ApiError> {
        let member = self.member(input)?;
        let mut statement=self.connection.prepare("SELECT DISTINCT json_extract(document,'$.kind') FROM schedule_plans WHERE member_id=?1 AND json_extract(document,'$.enabled')=1")?;
        let kinds = statement
            .query_map([member], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(kinds
            .into_iter()
            .map(|kind| {
                if kind == "care" {
                    "care.reminders"
                } else {
                    "recommendation.generate"
                }
            })
            .collect())
    }
}

impl Application {
    /// Native host supplies actual visible/non-minimized window state; hidden hosts require explicit background consent.
    pub fn schedule_host_should_run(&self, foreground: bool) -> Result<bool, ApiError> {
        let settings = self.execute("schedule.runtime.get", json!({}))?;
        Ok(foreground || settings["background_enabled"] == true)
    }
    pub fn schedule_report_runtime_error(&self, code: &str) -> Result<(), ApiError> {
        let safe_code = if !code.is_empty()
            && code.len() <= 80
            && code
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            code
        } else {
            "scheduler_failed"
        };
        let tx = self.begin_write()?;
        let raw: String = tx.query_row(
            "SELECT document FROM schedule_local_runtime WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let mut doc: Value = serde_json::from_str(&raw)
            .map_err(|_| ApiError::new("storage_error", "本机设置损坏"))?;
        doc["last_error"] = json!({"code":safe_code,"message":"定时执行未完成，请查看计划记录或重新打开衣橱","occurred_at":chrono::Utc::now().to_rfc3339()});
        tx.execute(
            "UPDATE schedule_local_runtime SET document=?1 WHERE id=1",
            [doc.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn first_valid_local(
    zone: chrono_tz::Tz,
    local: chrono::NaiveDateTime,
) -> Result<chrono::DateTime<chrono::Utc>, ApiError> {
    for minutes in 0..=1440 {
        let Some(at) = local.checked_add_signed(chrono::Duration::minutes(minutes)) else {
            break;
        };
        if let Some(at) = zone.from_local_datetime(&at).earliest() {
            return Ok(at.with_timezone(&chrono::Utc));
        }
    }
    Err(bad("当地日期超出可执行时区范围"))
}
fn has_due_between(
    plan: &Value,
    zone: chrono_tz::Tz,
    time: chrono::NaiveTime,
    from: chrono::DateTime<chrono::Utc>,
    through: chrono::DateTime<chrono::Utc>,
) -> bool {
    let first = from.with_timezone(&zone).date_naive();
    (0..=8).any(|offset| {
        let Some(date) = first.checked_add_signed(chrono::Duration::days(offset)) else {
            return false;
        };
        plan["weekdays"]
            .as_array()
            .unwrap()
            .contains(&json!(date.weekday().number_from_monday()))
            && zone
                .from_local_datetime(&date.and_time(time))
                .earliest()
                .is_some_and(|at| {
                    let at = at.with_timezone(&chrono::Utc);
                    at >= from && at < through
                })
    })
}
