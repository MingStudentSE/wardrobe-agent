//! Member-scoped scenario & travel planning (issue #23, F17 extension).
//! 计划与打包清单是领域工件：结构性不进入 wear 面（不记 WearEvent），
//! 所有衣物归属当前成员且可用性服务端校验；库存不足给明确无解原因。
#[path = "scenarios_runtime.rs"]
mod runtime;
use crate::{string, ApiError, Application};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};

/// 贴身/主要衣物类别：旅行中不得连续两天重复（鞋、外层、包、配饰可跨天复用）。
const GARMENT_CATEGORIES: &[&str] = &[
    "top",
    "bottom",
    "dress",
    "one_piece",
    "sportswear",
    "underwear",
];
const MAX_DAYS: usize = 14;
const PLAN_LIMIT: usize = 50;

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS scenario_plans(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),operation_key TEXT NOT NULL,request TEXT NOT NULL,document TEXT NOT NULL,UNIQUE(member_id,operation_key));")?;
    Ok(())
}

fn invalid(message: &str) -> ApiError {
    ApiError::new("invalid_input", message)
}

fn parse(serialized: Option<String>) -> Result<Value, ApiError> {
    serde_json::from_str(&serialized.ok_or_else(|| ApiError::new("not_found", "计划不存在"))?)
        .map_err(|_| ApiError::new("storage_error", "计划记录损坏"))
}

/// 已解析并校验的场景计划请求（plan 与 update 的共享语义）。
pub(super) struct ScenarioRequest {
    pub member: String,
    pub kind: String,
    pub occasion: String,
    pub title: String,
    pub start_date: chrono::NaiveDate,
    pub days: usize,
    pub timezone: String,
    pub notes: Option<String>,
    pub fixed: Vec<String>,
    pub excluded: Vec<String>,
    pub weather: Option<Value>,
}

impl ScenarioRequest {
    /// 规划器使用的推荐域场合标签：婚礼/面试按正式，旅行按日常。
    pub fn rule_occasion(&self) -> &'static str {
        if self.kind == "travel" {
            "daily"
        } else {
            "formal"
        }
    }
    /// 原样回存的请求控制（计划可追溯、可据以修改）。
    pub fn controls(&self) -> Value {
        let mut controls = json!({
            "kind":self.kind,
            "occasion":self.occasion,
            "start_date":self.start_date.to_string(),
            "days":self.days,
            "timezone":self.timezone,
            "fixed_ids":self.fixed,
            "excluded_ids":self.excluded,
        });
        if !self.title.is_empty() {
            controls["title"] = json!(self.title);
        }
        if let Some(notes) = &self.notes {
            controls["notes"] = json!(notes);
        }
        if let Some(weather) = &self.weather {
            controls["weather"] = weather.clone();
        }
        controls
    }
    /// 带幂等键的持久化请求指纹（plan 入参与重放判定）。
    pub fn payload(&self, key: &str) -> Value {
        let mut payload = self.controls();
        payload["operation_key"] = json!(key);
        payload
    }
}

const PLAN_FIELDS: &[&str] = &[
    "member_id",
    "operation_key",
    "timezone",
    "kind",
    "occasion",
    "title",
    "start_date",
    "days",
    "fixed_ids",
    "excluded_ids",
    "weather",
    "strategy",
    "provider_id",
    "notes",
];

/// 校验并解析场景计划输入（不含 strategy/provider 路由判断）。
fn parse_plan_request(app: &Application, input: &Value) -> Result<ScenarioRequest, ApiError> {
    crate::recommendations::fields(input, PLAN_FIELDS)?;
    let member = app.member(input)?;
    let timezone = string(input, "timezone")?
        .parse::<chrono_tz::Tz>()
        .map_err(|_| invalid("时区须为有效IANA名称"))?
        .name()
        .to_owned();
    let kind = string(input, "kind")?.to_owned();
    if !["event", "travel"].contains(&kind.as_str()) {
        return Err(invalid("场景类型须为event或travel"));
    }
    let occasion = match input.get("occasion") {
        Some(value) => {
            let occasion = value
                .as_str()
                .ok_or_else(|| invalid("场合须为文字"))?
                .to_owned();
            let allowed = ["wedding", "interview"].contains(&occasion.as_str())
                || (kind == "travel" && occasion == "travel");
            if !allowed {
                return Err(invalid("单事件场合须为wedding或interview"));
            }
            occasion
        }
        None if kind == "travel" => "travel".to_owned(),
        None => return Err(invalid("单事件场合须为wedding或interview")),
    };
    if kind == "travel" && occasion != "travel" {
        return Err(invalid("旅行计划的场合由旅行本身决定，不接收wedding/interview"));
    }
    if kind == "event" && occasion == "travel" {
        return Err(invalid("单事件场合须为wedding或interview"));
    }
    let title = match input.get("title") {
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|title| !title.is_empty() && title.chars().count() <= 120)
            .ok_or_else(|| invalid("标题须为1–120字符"))?
            .to_owned(),
        None => match occasion.as_str() {
            "wedding" => "婚礼计划",
            "interview" => "面试计划",
            _ => "旅行计划",
        }
        .to_owned(),
    };
    let zone: chrono_tz::Tz = timezone.parse().map_err(|_| invalid("时区无效"))?;
    let start_date = match input.get("start_date") {
        Some(value) => chrono::NaiveDate::parse_from_str(value.as_str().unwrap_or(""), "%Y-%m-%d")
            .map_err(|_| invalid("开始日期须为YYYY-MM-DD"))?,
        None => chrono::Utc::now().with_timezone(&zone).date_naive(),
    };
    let days = match kind.as_str() {
        "event" => 1,
        _ => input
            .get("days")
            .map(|value| {
                value
                    .as_u64()
                    .filter(|days| (2..=MAX_DAYS as u64).contains(days))
                    .map(|days| days as usize)
                    .ok_or_else(|| invalid("旅行天数须为2–14"))
            })
            .transpose()?
            .ok_or_else(|| invalid("旅行须提供2–14的天数"))?,
    };
    let notes = match input.get("notes") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_str()
                .map(str::trim)
                .filter(|notes| !notes.is_empty() && notes.chars().count() <= 1000)
                .ok_or_else(|| invalid("手工说明须为1–1000字符"))?
                .to_owned(),
        ),
    };
    let fixed = crate::recommendations::ids(input, "fixed_ids")?;
    let excluded = crate::recommendations::ids(input, "excluded_ids")?;
    if fixed.iter().any(|id| excluded.contains(id)) {
        return Err(ApiError::new("constraint_conflict", "固定衣物不能同时排除"));
    }
    Ok(ScenarioRequest {
        member,
        kind,
        occasion,
        title,
        start_date,
        days,
        timezone,
        notes,
        fixed,
        excluded,
        weather: input.get("weather").cloned(),
    })
}

impl Application {
    pub(super) fn scenario_execute(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        match op {
            "scenario.plan" => self.scenario_plan(input),
            "scenario.get" => self.scenario_get(input),
            "scenario.list" => self.scenario_list(input),
            "scenario.update" => self.scenario_update(input),
            "scenario.delete" => self.scenario_delete(input),
            "scenario.worker.run" => self.scenario_worker(input),
            _ => Err(ApiError::new("unsupported_operation", "场景计划操作不可用")),
        }
    }

    /// 生成并保存场景计划。strategy=rules（默认）同步本地规则生成；
    /// strategy=ai 先持久化任务并立即返回回执，模型请求只发生在
    /// scenario.worker.run（与 #22/#25 相同的收据先行语义）。
    fn scenario_plan(&self, input: Value) -> Result<Value, ApiError> {
        crate::recommendations::fields(&input, PLAN_FIELDS)?;
        let strategy = input
            .get("strategy")
            .map(|value| {
                value
                    .as_str()
                    .filter(|s| ["rules", "ai"].contains(s))
                    .ok_or_else(|| invalid("策略须为rules或ai"))
            })
            .transpose()?
            .unwrap_or("rules")
            .to_owned();
        if strategy == "ai" {
            // 收据先行：只做形状与模型配置校验，库存/天气工作留给 worker。
            let member = self.member(&input)?;
            let key = string(&input, "operation_key")?;
            if key.trim().is_empty() || key.len() > 200 {
                return Err(invalid("操作标识须为1–200字节"));
            }
            let provider = self.provider_get(string(&input, "provider_id")?)?;
            if provider["adapter"] != "openai_compatible"
                || !provider["capabilities"]
                    .as_array()
                    .is_some_and(|values| values.contains(&json!("text")))
            {
                return Err(ApiError::new(
                    "unsupported_capability",
                    "需要配置兼容聊天接口的文本模型",
                ));
            }
            // 与 rules 路径/purchase 相同的幂等语义：同键重放必须比对
            // payload，输入变化时显式冲突而不是静默返回旧任务回执。
            if let Some(replay) = self.scenario_existing_receipt(&member, key)? {
                let mut expected = input.clone();
                expected["member_id"] = json!(member);
                let stored_payload = replay["payload"].clone();
                if stored_payload != expected {
                    return Err(ApiError::new(
                        "idempotency_conflict",
                        "同一标识不能替换计划输入",
                    ));
                }
                return Ok(replay["receipt"].clone());
            }
            let mut payload = input.clone();
            payload["member_id"] = json!(member);
            let job = self.job_enqueue(&member, "scenario_plan", key, payload)?;
            return Ok(receipt(&job));
        }
        let request = parse_plan_request(self, &input)?;
        let key = string(&input, "operation_key")?;
        let request_json = request.payload(key).to_string();
        let tx = self.begin_write()?;
        if let Some((old, document)) = tx
            .query_row(
                "SELECT request,document FROM scenario_plans WHERE member_id=?1 AND operation_key=?2",
                params![request.member, key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if old != request_json {
                return Err(ApiError::new(
                    "idempotency_conflict",
                    "同一标识不能替换计划输入",
                ));
            }
            return parse(Some(document));
        }
        let plan = self.scenario_compute(&request, generation_rules(0, &[]), None)?;
        tx.execute(
            "INSERT INTO scenario_plans VALUES(?1,?2,?3,?4,?5)",
            params![
                plan["id"].as_str(),
                request.member,
                key,
                request_json,
                plan.to_string()
            ],
        )?;
        tx.commit()?;
        Ok(plan)
    }

    /// 同 (member, operation_key) 的 AI 计划重放：返回任务收据（含终态与计划ID）。
    /// 附带存储的 payload 供幂等重放比对。
    fn scenario_existing_receipt(&self, member: &str, key: &str) -> Result<Option<Value>, ApiError> {
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM jobs WHERE member_id=?1 AND kind='scenario_plan' AND operation_key=?2",
                params![member, key],
                |row| row.get(0),
            )
            .optional()?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let job: Value = serde_json::from_str(&stored)
            .map_err(|_| ApiError::new("storage_error", "任务记录损坏"))?;
        Ok(Some(json!({"payload":job["payload"],"receipt":receipt(&job)})))
    }

    fn scenario_get(&self, input: Value) -> Result<Value, ApiError> {
        crate::recommendations::fields(&input, &["member_id", "id"])?;
        let member = self.member(&input)?;
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM scenario_plans WHERE member_id=?1 AND id=?2",
                params![member, string(&input, "id")?],
                |row| row.get(0),
            )
            .optional()?;
        parse(stored)
    }

    fn scenario_list(&self, input: Value) -> Result<Value, ApiError> {
        crate::recommendations::fields(&input, &["member_id"])?;
        let member = self.member(&input)?;
        let mut statement = self.connection.prepare(
            "SELECT document FROM scenario_plans WHERE member_id=?1 AND json_extract(document,'$.status')='active' ORDER BY id DESC LIMIT ?2",
        )?;
        let mut items = vec![];
        for row in statement.query_map(params![member, PLAN_LIMIT], |row| {
            row.get::<_, String>(0)
        })? {
            items.push(parse(Some(row?))?);
        }
        Ok(json!({"schema_version":1,"member_id":member,"items":items}))
    }

    /// 修改计划：替换标题/说明/固定/排除/天气并按本地规则整体重算保存；
    /// expected_version 乐观锁防止覆盖并发修改。重算始终使用本地规则
    /// 与当时真实库存，版本递增并持久保存。
    fn scenario_update(&self, input: Value) -> Result<Value, ApiError> {
        crate::recommendations::fields(
            &input,
            &[
                "member_id",
                "id",
                "expected_version",
                "title",
                "notes",
                "fixed_ids",
                "excluded_ids",
                "weather",
            ],
        )?;
        let member = self.member(&input)?;
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM scenario_plans WHERE member_id=?1 AND id=?2",
                params![member, string(&input, "id")?],
                |row| row.get(0),
            )
            .optional()?;
        let plan = parse(stored)?;
        if plan["status"] != "active" {
            return Err(ApiError::new("invalid_state", "回收站计划须先恢复后再修改"));
        }
        let expected = input["expected_version"]
            .as_u64()
            .filter(|version| *version > 0)
            .ok_or_else(|| invalid("修改计划需要明确expected_version"))?;
        if expected != plan["version"].as_u64().unwrap_or(0) {
            return Err(ApiError::new("version_conflict", "计划已被修改，请重新读取"));
        }
        // 合并控制：从已存 controls 出发，仅替换显式提供的字段。
        // 显式排除优先于遗留固定：用户排除原固定单品时自动解除其固定，
        // 避免固定/排除死锁（两者同时提供仍被 parse 拒绝）。
        let mut merged = plan["controls"].clone();
        for field in ["title", "notes", "weather", "fixed_ids", "excluded_ids"] {
            if let Some(value) = input.get(field) {
                merged[field] = value.clone();
            }
        }
        if input.get("excluded_ids").is_some() {
            if let (Some(fixed), Some(excluded)) =
                (merged["fixed_ids"].as_array().cloned(), merged["excluded_ids"].as_array().cloned())
            {
                merged["fixed_ids"] = json!(fixed
                    .into_iter()
                    .filter(|id| !excluded.contains(id))
                    .collect::<Vec<_>>());
            }
        }
        if merged["notes"].is_null() {
            merged
                .as_object_mut()
                .expect("controls 是对象")
                .remove("notes");
        }
        merged["member_id"] = json!(member);
        let request = parse_plan_request(self, &merged)?;
        let mut recomputed = self.scenario_compute(&request, generation_rules(0, &[]), None)?;
        recomputed["id"] = plan["id"].clone();
        recomputed["created_at"] = plan["created_at"].clone();
        recomputed["version"] = json!(plan["version"].as_u64().unwrap_or(0) + 1);
        recomputed["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        let tx = self.begin_write()?;
        // 乐观锁落在写事务内：版本谓词不匹配即 version_conflict，
        // 读取与写入之间的并发修改绝不静默丢失。
        let updated = tx
            .execute(
                "UPDATE scenario_plans SET document=?1 WHERE member_id=?2 AND id=?3 AND json_extract(document,'$.version')=?4",
                params![
                    recomputed.to_string(),
                    member,
                    plan["id"].as_str(),
                    plan["version"].as_u64().unwrap_or(0)
                ],
            )
            .map_err(|_| ApiError::new("storage_error", "计划保存失败"))?;
        if updated != 1 {
            return Err(ApiError::new("version_conflict", "计划已被修改，请重新读取"));
        }
        tx.commit()?;
        Ok(recomputed)
    }

    fn scenario_delete(&self, input: Value) -> Result<Value, ApiError> {
        crate::recommendations::fields(&input, &["member_id", "id"])?;
        let member = self.member(&input)?;
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM scenario_plans WHERE member_id=?1 AND id=?2",
                params![member, string(&input, "id")?],
                |row| row.get(0),
            )
            .optional()?;
        let mut plan = parse(stored)?;
        if plan["status"] == "deleted" {
            return Ok(plan);
        }
        plan["status"] = json!("deleted");
        plan["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        let tx = self.begin_write()?;
        // 与 update 同型的乐观锁：删除只作用于读取时看到的版本，
        // 并发修改先行提交时显式冲突而不是覆盖。
        let updated = tx
            .execute(
                "UPDATE scenario_plans SET document=?1 WHERE member_id=?2 AND id=?3 AND json_extract(document,'$.version')=?4",
                params![
                    plan.to_string(),
                    member,
                    plan["id"].as_str(),
                    plan["version"].as_u64().unwrap_or(0)
                ],
            )
            .map_err(|_| ApiError::new("storage_error", "计划删除失败"))?;
        if updated != 1 {
            return Err(ApiError::new("version_conflict", "计划已被修改，请重新读取"));
        }
        tx.commit()?;
        Ok(plan)
    }

    /// 规则计算：真实可用库存 + 逐天轮换 + 去重打包清单 + 无解原因。
    /// generation 是调用方给定的来源元数据（rules / text_model / rule_fallback）；
    /// model_days 是 runtime 已按结构规则校验通过的模型逐天指派（AI 采纳）。
    pub(super) fn scenario_compute(
        &self,
        request: &ScenarioRequest,
        generation: Value,
        model_days: Option<&Vec<Vec<String>>>,
    ) -> Result<Value, ApiError> {
        let member = &request.member;
        // 1. 可用性快照：生命周期（非 active 拒绝）+ 洗护状态一次读取。
        let wears = self.effective_wears(member)?;
        let statuses = self.care_batch_from_wears(member, &wears)?;
        // 2. 固定单品硬校验：归属（clothing_get 按成员查）+ active + 可用。
        for id in &request.fixed {
            let garment = self.clothing_get(json!({"member_id":member,"id":id}))?;
            let reason = if garment["status"] != "active" {
                garment["status"].as_str().unwrap_or("unknown").to_owned()
            } else {
                match statuses
                    .iter()
                    .find(|status| status["clothing_id"] == json!(id))
                {
                    Some(status) if status["available"] == true => continue,
                    Some(status) => status["exclusion_reasons"].to_string(),
                    None => "not_in_availability_snapshot".to_owned(),
                }
            };
            return Err(ApiError::new(
                "constraint_conflict",
                &format!("固定衣物不可用：{}（{}）", garment["name"], reason),
            ));
        }
        for id in &request.excluded {
            self.clothing_get(json!({"member_id":member,"id":id}))?;
        }
        // 3. 候选池：真实库存 − 排除 − 不可用；确定性排序（场合适配优先，其次ID）。
        let mut pool = self.scenario_available_pool(member, &request.excluded)?;
        let occasion_tag = request.rule_occasion();
        pool.sort_by_key(|garment| {
            (
                !garment["occasion"]
                    .as_array()
                    .is_some_and(|tags| tags.iter().any(|tag| tag == occasion_tag)),
                garment["id"].as_str().unwrap().to_owned(),
            )
        });
        // 4. 天气参考：自动模式复用天气能力；失败/未来日期如实降级，不阻断。
        let (cold, weather_context) = self.recommendation_weather_for(
            member,
            request.weather.as_ref(),
            Some(&json!(request.start_date.to_string())),
            &request.timezone,
        )?;
        let pair_weights = self.learning_pair_weights(member)?;
        // 5. 逐天轮换：贴身/主要衣物不连续两天重复；库存不足时如实降级并说明。
        let mut used_before: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut yesterday: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut shortage_reasons: Vec<String> = vec![];
        let mut days: Vec<Value> = vec![];
        for index in 0..request.days {
            let date = request.start_date + chrono::Duration::days(index as i64);
            let fixed_here: Vec<String> = if index == 0 {
                request.fixed.clone()
            } else {
                vec![]
            };
            let soft_pool: Vec<&Value> = pool
                .iter()
                .filter(|garment| {
                    let garment_class = GARMENT_CATEGORIES
                        .contains(&garment["category"].as_str().unwrap_or(""));
                    !garment_class || !yesterday.contains(garment["id"].as_str().unwrap())
                })
                .collect();
            let (strict, _) = plan_day(
                &soft_pool,
                &pair_weights,
                &fixed_here,
                cold,
                occasion_tag,
                &used_before,
            );
            let strict_complete = strict
                .first()
                .and_then(|proposal| proposal["complete"].as_bool())
                .unwrap_or(false);
            let proposal = if strict_complete {
                strict.into_iter().next().unwrap()
            } else {
                // 降级：允许贴身衣物连续重复，库存不足须如实说明。
                let (relaxed, _) = plan_day(
                    &pool.iter().collect::<Vec<_>>(),
                    &pair_weights,
                    &fixed_here,
                    cold,
                    occasion_tag,
                    &used_before,
                );
                relaxed
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| json!({"items":[],"complete":false,"missing_roles":missing_basics(&pool, cold),"reasons":[]}))
            };
            let items = proposal["items"].as_array().unwrap().clone();
            let complete = proposal["complete"].as_bool().unwrap_or(false);
            let missing_roles = proposal["missing_roles"].as_array().unwrap().clone();
            let mut reasons: Vec<Value> = vec![];
            if complete && yesterday.iter().any(|id| {
                items
                    .iter()
                    .any(|item| item["clothing_id"].as_str() == Some(id.as_str()))
            }) {
                let repeated: Vec<String> = items
                    .iter()
                    .filter(|item| {
                        yesterday.contains(item["clothing_id"].as_str().unwrap_or(""))
                            && GARMENT_CATEGORIES
                                .contains(&item["category"].as_str().unwrap_or(""))
                    })
                    .map(|item| item["name"].as_str().unwrap_or("").to_owned())
                    .collect();
                if !repeated.is_empty() {
                    let note = format!(
                        "库存不足：{} 需连续两天重复穿着；不会虚构新库存",
                        repeated.join("、")
                    );
                    reasons.push(json!(note.clone()));
                    shortage_reasons.push(note);
                }
            }
            if !complete {
                let missing: Vec<String> = missing_roles
                    .iter()
                    .filter_map(|role| role.as_str())
                    .map(str::to_owned)
                    .collect();
                let note = if missing.is_empty() {
                    format!("第{}天没有满足硬约束的真实衣物组合", index + 1)
                } else {
                    format!(
                        "第{}天缺少可用角色：{}；检查库存、排除和洗护状态",
                        index + 1,
                        missing.join("、")
                    )
                };
                reasons.push(json!(note));
                shortage_reasons.push(note);
            }
            reasons.push(json!(match request.kind.as_str() {
                "travel" => "场合：旅行（按日常适配；只采用已记录标签，不推断未知事实）".to_owned(),
                _ => format!(
                    "场合：{}（按正式场合适配；只采用已记录标签，不推断未知事实）",
                    if request.occasion == "wedding" { "婚礼" } else { "面试" }
                ),
            }));
            if cold {
                reasons.push(json!(if weather_context["source"] == "manual" {
                    "手动温度不高于15°C，要求外层；不承诺衣物保暖性能"
                } else {
                    "天气估计温度不高于15°C，要求外层；不承诺衣物保暖性能"
                }));
            } else if weather_context["source"] == "unavailable" {
                reasons.push(json!("自动天气不可用（未来日期或服务失败）：可手动输入温度或补充说明，不影响计划"));
            }
            for item in &items {
                used_before.insert(item["clothing_id"].as_str().unwrap().to_owned());
            }
            days.push(json!({
                "index":index,
                "date":date.to_string(),
                "complete":complete,
                "missing_roles":missing_roles,
                "items":items,
                "reasons":reasons,
                "weather_context":weather_context,
            }));
            yesterday = days[days.len() - 1]["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| {
                    GARMENT_CATEGORIES.contains(&item["category"].as_str().unwrap_or(""))
                })
                .map(|item| item["clothing_id"].as_str().unwrap().to_owned())
                .collect();
        }
        // 6. 固定单品必须真实落位；整天无解时如实说明，不静默丢弃约束。
        let placed: std::collections::HashSet<&str> = days
            .iter()
            .flat_map(|day| {
                day["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|item| item["clothing_id"].as_str().unwrap())
            })
            .collect();
        for id in &request.fixed {
            if !placed.contains(id.as_str()) {
                shortage_reasons
                    .push("固定单品无法安排进任何一天：检查库存与可用性".to_owned());
            }
        }
        if pool.is_empty() {
            shortage_reasons
                .push("没有满足成员、生命周期、洗护及排除条件的真实衣物".to_owned());
        }
        // 7. AI 采纳：runtime 已校验的模型逐天指派覆盖对应天，并按同一结构规则重评完整性。
        if let Some(model_days) = model_days {
            let fallback_pool = pool.clone();
            for (index, ids) in model_days.iter().enumerate() {
                let Some(day) = days.get_mut(index) else {
                    continue;
                };
                let rebuilt: Vec<Value> = ids
                    .iter()
                    .filter_map(|id| {
                        fallback_pool
                            .iter()
                            .find(|garment| garment["id"].as_str() == Some(id.as_str()))
                            .map(|garment| {
                                let (role, base) = crate::recommendations::role(
                                    garment["category"].as_str().unwrap(),
                                );
                                json!({"clothing_id":garment["id"],"name":garment["name"],"category":garment["category"],"role":role,"layer":base})
                            })
                    })
                    .collect();
                let chosen: Vec<Value> = rebuilt.clone();
                let missing = missing_basics(&chosen, cold);
                day["complete"] = json!(missing.is_empty());
                day["missing_roles"] = json!(missing);
                day["items"] = json!(rebuilt);
                day["reasons"]
                    .as_array_mut()
                    .expect("方案理由是数组")
                    .push(json!("文本模型选择已通过与离线规则相同的真实库存、固定/排除与逐天轮换检查"));
            }
        }
        // 8. 去重打包清单：每个衣物一条，列出使用的天与重复语义。
        let packing = build_packing(&days);
        let now = chrono::Utc::now().to_rfc3339();
        let mut doc = json!({
            "schema_version":1,
            "id":uuid::Uuid::now_v7().to_string(),
            "member_id":member,
            "kind":request.kind,
            "title":request.title,
            "occasion":request.occasion,
            "status":"active",
            "version":1,
            "start_date":request.start_date.to_string(),
            "days_count":request.days,
            "timezone":request.timezone,
            "created_at":now,
            "updated_at":now,
            "controls":request.controls(),
            "days":days,
            "packing":packing,
            "creates_wear":false,
            "shortage_reasons":shortage_reasons,
            "generation":generation,
            "weather_context":weather_context,
        });
        if let Some(notes) = &request.notes {
            doc["notes"] = json!(notes);
        }
        Ok(doc)
    }

    /// 当前成员的真实可用候选池：active 生命周期 + 洗护可用 − 显式排除。
    /// 规则计算与模型输出校验共用同一池，保证「库存不足给原因，绝不虚构」。
    fn scenario_available_pool(
        &self,
        member: &str,
        excluded: &[String],
    ) -> Result<Vec<Value>, ApiError> {
        let wears = self.effective_wears(member)?;
        let statuses = self.care_batch_from_wears(member, &wears)?;
        let available: std::collections::HashSet<&str> = statuses
            .iter()
            .filter(|status| status["available"] == true)
            .map(|status| status["clothing_id"].as_str().unwrap())
            .collect();
        let excluded: std::collections::HashSet<&str> = excluded.iter().map(String::as_str).collect();
        let mut statement = self
            .connection
            .prepare("SELECT document FROM clothing WHERE member_id=?1 ORDER BY id")?;
        let mut pool = vec![];
        for row in statement.query_map([member], |row| row.get::<_, String>(0))? {
            let garment: Value = serde_json::from_str(&row?)
                .map_err(|_| ApiError::new("storage_error", "单品数据无效"))?;
            let id = garment["id"].as_str().unwrap();
            if garment["status"] == "active" && available.contains(id) && !excluded.contains(id) {
                pool.push(garment);
            }
        }
        Ok(pool)
    }
}

/// 单天规则方案：复用推荐域规划器（limit=1，priority=已用衣物优先压缩打包清单）。
fn plan_day(
    pool: &[&Value],
    pair_weights: &crate::pair_search::PairWeights,
    fixed: &[String],
    cold: bool,
    occasion: &str,
    used_before: &std::collections::HashSet<String>,
) -> (Vec<Value>, bool) {
    let mut clothes: Vec<Value> = pool.iter().map(|garment| (*garment).clone()).collect();
    clothes.sort_by_key(|garment| {
        (
            !used_before.contains(garment["id"].as_str().unwrap()),
            garment["id"].as_str().unwrap().to_owned(),
        )
    });
    crate::recommendations::plan_single(
        &clothes,
        pair_weights,
        fixed,
        cold,
        occasion,
        used_before,
        &std::collections::BTreeSet::new(),
    )
}

/// 基础角色缺失检查：body(dress/one_piece) 或 top+bottom，加 shoes；
/// 低温时 outerwear 也属必需。只报告池内真实缺失，不推断未知事实。
fn missing_basics(pool: &[Value], cold: bool) -> Vec<&'static str> {
    let has = |categories: &[&str]| {
        pool.iter()
            .any(|garment| categories.contains(&garment["category"].as_str().unwrap_or("")))
    };
    let mut missing = vec![];
    if has(&["dress", "one_piece"]) {
        // body 模式成立
    } else if has(&["top"]) && has(&["bottom"]) {
        // 分体模式成立
    } else if has(&["top"]) {
        missing.push("bottom");
    } else if has(&["bottom"]) {
        missing.push("top");
    } else {
        missing.push("top");
        missing.push("bottom");
    }
    if !has(&["shoes"]) {
        missing.push("shoes");
    }
    if cold && !has(&["outerwear"]) {
        missing.push("outerwear");
    }
    missing
}

/// 去重打包清单：每个衣物一条，列出使用的天；跨天复用带 reused，
/// 相邻天重复带 consecutive_repeat（库存不足的连续重复另有短缺原因）。
fn build_packing(days: &[Value]) -> Value {
    let mut order: Vec<String> = vec![];
    let mut entries: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for day in days {
        let index = day["index"].as_u64().unwrap_or(0);
        for item in day["items"].as_array().unwrap() {
            let id = item["clothing_id"].as_str().unwrap().to_owned();
            let entry = entries.entry(id.clone()).or_insert_with(|| {
                order.push(id.clone());
                json!({
                    "clothing_id":item["clothing_id"],
                    "name":item["name"],
                    "category":item["category"],
                    "role":item["role"],
                    "days":[],
                })
            });
            entry["days"].as_array_mut().unwrap().push(json!(index));
        }
    }
    let mut items = vec![];
    for id in order {
        let mut entry = entries.remove(&id).unwrap();
        let used = entry["days"].as_array().unwrap().clone();
        entry["reused"] = json!(used.len() > 1);
        let consecutive = used.windows(2).any(|pair| {
            pair[1].as_u64().is_some_and(|later| later == pair[0].as_u64().unwrap_or(0) + 1)
        });
        entry["consecutive_repeat"] = json!(consecutive);
        items.push(entry);
    }
    json!({"total":items.len(),"items":items})
}

fn receipt(job: &Value) -> Value {
    let mut body = json!({
        "schema_version":1,
        "member_id":job["member_id"],
        "job_id":job["id"],
        "state":job["state"],
    });
    if let Some(plan_id) = job["result"]["plan_id"].as_str() {
        body["plan_id"] = json!(plan_id);
    }
    body
}

pub(super) fn generation_rules(attempts: u64, failure_codes: &[&str]) -> Value {
    json!({
        "source":"rules",
        "attempts":attempts,
        "failure_codes":failure_codes,
    })
}

/// 供根接线汇总的描述符：scenario.* 经共享 operations 注册源自动暴露
/// 到 CLI / MCP / GUI，不新增第二张调度表。
pub(super) fn descriptors() -> Vec<Value> {
    let plan_day_schema = json!({
        "type":"object",
        "required":["index","date","complete","missing_roles","items","reasons"],
        "properties":{
            "index":{"type":"integer","minimum":0},
            "date":{"type":"string","format":"date"},
            "complete":{"type":"boolean"},
            "missing_roles":{"type":"array","items":{"type":"string"}},
            "items":{"type":"array","items":{"type":"object","required":["clothing_id","name","category","role","layer"],"properties":{"clothing_id":{"type":"string"},"name":{"type":"string"},"category":{"type":"string"},"role":{"type":"string"},"layer":{"type":"integer"}},"additionalProperties":false}},
            "reasons":{"type":"array","items":{"type":"string"}},
            "weather_context":{"type":"object"},
        },
        "additionalProperties":false,
    });
    let packing_schema = json!({
        "type":"object",
        "required":["total","items"],
        "properties":{
            "total":{"type":"integer","minimum":0},
            "items":{"type":"array","items":{"type":"object","required":["clothing_id","name","category","role","days","reused","consecutive_repeat"],"properties":{"clothing_id":{"type":"string"},"name":{"type":"string"},"category":{"type":"string"},"role":{"type":"string"},"days":{"type":"array","items":{"type":"integer","minimum":0}},"reused":{"type":"boolean"},"consecutive_repeat":{"type":"boolean"}},"additionalProperties":false}},
        },
        "additionalProperties":false,
    });
    let plan_schema = json!({
        "type":"object",
        "required":["schema_version","id","member_id","kind","title","occasion","status","version","start_date","days_count","timezone","created_at","updated_at","controls","days","packing","creates_wear","shortage_reasons","generation","weather_context"],
        "properties":{
            "schema_version":{"const":1},
            "id":{"type":"string"},
            "member_id":{"type":"string"},
            "kind":{"enum":["event","travel"]},
            "title":{"type":"string"},
            "occasion":{"enum":["wedding","interview","travel"]},
            "status":{"enum":["active","deleted"]},
            "version":{"type":"integer","minimum":1},
            "start_date":{"type":"string","format":"date"},
            "days_count":{"type":"integer","minimum":1,"maximum":14},
            "timezone":{"type":"string"},
            "created_at":{"type":"string"},
            "updated_at":{"type":"string"},
            "controls":{"type":"object"},
            "days":{"type":"array","items":plan_day_schema},
            "packing":packing_schema,
            "creates_wear":{"const":false},
            "shortage_reasons":{"type":"array","items":{"type":"string"}},
            "generation":{"type":"object"},
            "weather_context":{"type":"object"},
            "notes":{"type":"string"},
        },
        "additionalProperties":false,
    });
    let receipt_schema = json!({
        "type":"object",
        "required":["schema_version","member_id","job_id","state"],
        "properties":{
            "schema_version":{"const":1},
            "member_id":{"type":"string"},
            "job_id":{"type":"string"},
            "state":{"enum":["queued","running","waiting_input","succeeded","failed","cancelled"]},
            "plan_id":{"type":"string"},
        },
        "additionalProperties":false,
    });
    let weather_property = json!({"type":"object","required":["mode"],"additionalProperties":false,"properties":{"mode":{"enum":["off","manual","automatic"]},"temperature_c":{"type":"number","minimum":-80,"maximum":60},"location":{"type":"string","maxLength":120},"obtained_at":{"type":"string","format":"date-time"}}});
    let ids_property = json!({"type":"array","items":{"type":"string"},"maxItems":100,"uniqueItems":true});
    let plan_input = json!({
        "type":"object",
        "required":["member_id","operation_key","timezone","kind"],
        "additionalProperties":false,
        "properties":{
            "member_id":{"type":"string"},
            "operation_key":{"type":"string","minLength":1,"maxLength":200},
            "timezone":{"type":"string","description":"有效IANA时区名称"},
            "kind":{"enum":["event","travel"]},
            "occasion":{"enum":["wedding","interview"]},
            "title":{"type":"string","minLength":1,"maxLength":120},
            "start_date":{"type":"string","format":"date"},
            "days":{"type":"integer","minimum":2,"maximum":14},
            "fixed_ids":ids_property,
            "excluded_ids":ids_property,
            "weather":weather_property,
            "strategy":{"enum":["rules","ai"]},
            "provider_id":{"type":"string"},
            "notes":{"type":"string","maxLength":1000},
        },
    });
    let plan_async = json!({"job_kind":"scenario_plan","process":"scenario.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]});
    vec![
        json!({
            "name":"scenario.plan",
            "description":"婚礼/面试单事件与多日旅行场景计划：基于真实可用库存逐天搭配并生成去重打包清单；固定/排除/天气参考/手工说明可用，缺库存给明确原因；计划是领域工件，结构性不记录实际穿着；strategy=ai 时先持久化任务并立即返回回执，模型失败有限重试后按本地规则降级并如实反馈",
            "effect":{"kind":"mutation","network":"none","local_files":false,"async":null,"conditional_effects":[
                {"when":{"strategy":"ai"},"effect":{"kind":"mutation","network":"none","async":plan_async}},
                {"when":{"weather.mode":"automatic"},"effect":{"kind":"query","network":"web-weather"}},
            ]},
            "examples":[json!({"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>","timezone":"Asia/Shanghai","kind":"travel","days":3,"start_date":"2026-10-10","weather":{"mode":"manual","temperature_c":8}},"prerequisites":["仅使用同成员真实可用衣物（归属、生命周期与洗护状态服务端校验）","计划与打包清单结构性不产生穿着（creates_wear恒为false）","strategy=ai先入队回执，再由scenario.worker.run真实调用模型并可能消耗额度","缺库存给短缺原因，不虚构库存或重复计入打包清单"]})],
            "inputSchema":plan_input,
            "outputSchema":{"type":"object","required":["schema_version"],"properties":{
                "schema_version":{"const":1},
                "id":{"type":"string"},
                "member_id":{"type":"string"},
                "job_id":{"type":"string"},
                "state":{"type":"string"},
                "plan_id":{"type":"string"},
            },"anyOf":[plan_schema, receipt_schema]},
        }),
        json!({
            "name":"scenario.get",
            "description":"读取已保存场景计划完整文档（逐天方案、去重打包清单、短缺原因与生成来源）",
            "effect":{"kind":"query","network":"none","local_files":false,"async":null},
            "examples":[json!({"request":{"member_id":"<已授权成员ID>","id":"<scenario.plan返回的计划ID>"},"prerequisites":["读取已保存计划；不产生穿着事实"]})],
            "inputSchema":{"type":"object","required":["member_id","id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}},
            "outputSchema":plan_schema,
        }),
        json!({
            "name":"scenario.list",
            "description":"按计划ID倒序列出当前成员最多50个未删除场景计划",
            "effect":{"kind":"query","network":"none","local_files":false,"async":null},
            "examples":[json!({"request":{"member_id":"<已授权成员ID>"},"prerequisites":["只读查询；已删除计划不出现在默认列表"]})],
            "inputSchema":{"type":"object","required":["member_id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"}}},
            "outputSchema":{"type":"object","required":["schema_version","member_id","items"],"properties":{"schema_version":{"const":1},"member_id":{"type":"string"},"items":{"type":"array","items":plan_schema}},"additionalProperties":false},
        }),
        json!({
            "name":"scenario.update",
            "description":"修改已保存场景计划（标题/手工说明/固定/排除/天气）并按本地规则整体重算保存；expected_version乐观锁；修改不产生穿着",
            "effect":{"kind":"mutation","network":"none","local_files":false,"async":null,"conditional_effects":[
                {"when":{"weather.mode":"automatic"},"effect":{"kind":"query","network":"web-weather"}},
            ]},
            "examples":[json!({"request":{"member_id":"<已授权成员ID>","id":"<计划ID>","expected_version":1,"excluded_ids":["<单品ID>"]},"prerequisites":["expected_version须为当前版本，冲突时version_conflict拒绝","重算始终使用本地规则与当时真实可用库存","不产生穿着事实"]})],
            "inputSchema":{"type":"object","required":["member_id","id","expected_version"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"},"expected_version":{"type":"integer","minimum":1},"title":{"type":"string","minLength":1,"maxLength":120},"notes":{"type":"string","maxLength":1000},"fixed_ids":ids_property,"excluded_ids":ids_property,"weather":weather_property}},
            "outputSchema":plan_schema,
        }),
        json!({
            "name":"scenario.delete",
            "description":"软删除场景计划入回收站（默认列表不再显示）",
            "effect":{"kind":"mutation","network":"none","local_files":false,"async":null},
            "examples":[json!({"request":{"member_id":"<已授权成员ID>","id":"<计划ID>"},"prerequisites":["软删除不产生穿着事实；重复删除幂等返回"]})],
            "inputSchema":{"type":"object","required":["member_id","id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}},
            "outputSchema":plan_schema,
        }),
        json!({
            "name":"scenario.worker.run",
            "description":"本机驱动一次已排队的AI场景计划任务：先本地规则生成候选，再真实调用已配置文本模型；模型输出经服务端逐天校验（真实ID、归属可用性、固定/排除、逐天轮换、基础覆盖），失败有限重试后按本地规则降级并如实反馈；提交结果未知进入等待核对，不盲重发",
            "effect":{"kind":"mutation","network":"none","local_files":false,"async":null,"conditional_effects":[
                {"when":{"submission_pending":true},"effect":{"kind":"mutation","network":"explicit-provider"}},
            ]},
            "examples":[json!({"request":{"member_id":"<已授权成员ID>","id":"<scenario_plan任务ID>"},"prerequisites":["首次真实提交调用已配置文本模型并可能消耗额度","模型失败最多三次尝试后按本地规则降级并写入失败反馈","任务已达终态或提交结果未知时幂等返回任务文档","本示例不自动执行"]})],
            "inputSchema":{"type":"object","required":["member_id","id"],"additionalProperties":false,"properties":{"member_id":{"type":"string"},"id":{"type":"string"}}},
            "outputSchema":crate::jobs::output_schema(),
        }),
    ]
}
