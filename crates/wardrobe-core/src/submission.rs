//! 供应商提交台账：「已提交、结果未知、不得盲重发」这条领域不变量
//! （GLOSSARY「供应商结果未知」）的唯一实现。任务在真实发送前在此登记，
//! 提交后的任务文档由台账判读；重试守卫与诊断投影从同一判定导出。
//!
//! 领域语义（保持不变）：提交结果未知时保留 waiting_input，不能自动或
//! 同任务重发；重发只能是新的明确意图。允许的有限重试只发生在结论明确
//! 的场合（模型输出无效的补试、限流后的有限自动重试），由台账放行。

use crate::{json, ApiError, Application};
use serde_json::Value;

/// 提交台账的任务 kind 登记：每种供应商任务一行。新增供应商任务先在此
/// 登记编码，不再复制台账判定。
mod kinds {
    /// 顶部 submitted_at 编码：图像生成、文本轮次与 stylist 多轮任务的共同
    /// 判定字段（文本轮次与 stylist 另有各自的扩展台账字段，由对应任务使用）。
    pub const SUBMITTED_AT: &[&str] = &[
        "product_display",
        "outfit_flatlay",
        "virtual_tryon",
        "text_recommendation",
        "scenario_plan",
        "stylist_query",
    ];
    /// 视觉提交编码：台账字段在提交载荷里（submission 与发送清单）。
    pub const VISION_SUBMISSION: &[&str] = &["recognition", "purchase_recognition"];
}

/// 视觉提交编码写在提交载荷里的台账字段。
const VISION_PAYLOAD_FIELDS: &[&str] = &["submission", "transferred_images"];

/// 共享图像分发（product/flatlay/tryon.process 同一入口）按调用方请求的
/// 任务种类报错；领域名词与各任务自己的描述符一致。
pub(super) fn image_task_label(kind: &str) -> &'static str {
    match kind {
        "product_display" => "商品图",
        "outfit_flatlay" => "搭配图",
        "virtual_tryon" => "试穿",
        _ => "图像",
    }
}

/// 「已提交」的统一判定：一旦成立，同任务不得盲重发（jobs.retry 拒绝、
/// worker 入口收口、恢复守卫与诊断投影共用这一份）。持久字段编码经此一处判读。
pub(super) fn is_submitted(doc: &Value) -> bool {
    let kind = doc["kind"].as_str().unwrap_or("");
    if kinds::VISION_SUBMISSION.contains(&kind) {
        return doc["payload"]["submission"] != "not_submitted";
    }
    kinds::SUBMITTED_AT.contains(&kind) && doc["submitted_at"].is_string()
}

/// 任务意图哈希：剥去台账自有字段后对载荷取摘要。恢复守卫据此识别
/// 「恢复前同一意图已提交」，发送清单差异不参与判定。
pub(super) fn submission_hash(payload: Value, kind: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut payload = payload;
    if kinds::VISION_SUBMISSION.contains(&kind) {
        if let Some(object) = payload.as_object_mut() {
            for field in VISION_PAYLOAD_FIELDS {
                object.remove(*field);
            }
        }
    }
    Sha256::digest(payload.to_string().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 「已提交」谓词的 SQL 投影（恢复守卫与诊断投影共用；视觉提交类 kind 列表
/// 来自登记，不再手抄）。与 is_submitted 判定同一语义、同一处维护。
pub(super) fn submitted_sql() -> String {
    let vision = kinds::VISION_SUBMISSION
        .iter()
        .map(|kind| format!("'{kind}'"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "(json_type(document,'$.submitted_at')='text' OR (kind IN ({vision}) AND json_extract(document,'$.payload.submission') IN ('submitted','received','unknown')))"
    )
}

// ---------------------------------------------------------------------------
// 登记：每次真实发送前，把「已提交」写进任务文档。台账字段只在此处编码。
// ---------------------------------------------------------------------------

/// 登记一次真实提交（图像编码）：只有 submitted_at。图像接口没有查询能力，
/// 提交后即视为结果未知，直到任务显式收敛。
pub(super) fn mark_image_submitted(job: &mut Value) {
    job["submitted_at"] = json!(chrono::Utc::now().to_rfc3339());
}

/// 登记一次真实提交（视觉编码）：submission 进入已提交，发送清单入账
/// （哈希与恢复守卫剥去的正是这两个字段）。
pub(super) fn mark_vision_submitted(job: &mut Value, transferred: Value) {
    job["payload"]["submission"] = json!("submitted");
    job["payload"]["transferred_images"] = transferred;
}

/// 明确被拒（连接完整、非 2xx 且非 5xx）：提交未被接受，台账复位为未提交，
/// 之后的重试是新的明确发送，不属盲重发。
pub(super) fn reset_vision_submission(job: &mut Value) {
    job["payload"]["submission"] = json!("not_submitted");
}

/// 视觉编码的收敛结论：完整响应已收到并解析。
pub(super) fn mark_vision_received(job: &mut Value) {
    job["payload"]["submission"] = json!("received");
}

/// 轮次类编码的共同台账字段：提交时刻、提交计数、结论复位为 unknown。
fn mark_submitted_common(job: &mut Value) {
    job["submitted_at"] = json!(chrono::Utc::now().to_rfc3339());
    job["submission_count"] = json!(job["submission_count"].as_u64().unwrap_or(0) + 1);
    job["last_outcome"] = json!("unknown");
}

/// 登记一次真实提交（文本轮次编码）：结论复位为 unknown、尝试号与提交
/// 计数入账、排队时刻清零；此后到结论明确之前，任务处于结果未知。
pub(super) fn mark_text_submitted(job: &mut Value, attempt: u64) {
    mark_submitted_common(job);
    job["output_attempt"] = json!(attempt);
    job["next_due"] = json!(0);
    job["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
}

/// 登记一次真实提交（stylist 编码）：计数与提交时刻入账、结论复位；
/// 轮次簿的 submitted 帧由调用方在同一事务写入。
pub(super) fn mark_stylist_submitted(job: &mut Value) {
    mark_submitted_common(job);
}

/// 文本轮次账本的有限重试闸门：已提交后仅当上一轮结论明确且预算未耗尽
/// 才放行——模型输出无效后的紧随补试（attempt = 上次 + 1，且不超过模型
/// 尝试上限），或限流后的到期自动重试（累计不超过两次）。其余一律
/// 「结果未知或重试预算已耗尽，不能盲重发」。返回是否按限流重试记账
/// （rate_retries +1）；尚未提交过时放行且不记账。
pub(super) fn text_retry_gate(
    job: &Value,
    attempt: u64,
    model_attempts: u64,
) -> Result<bool, ApiError> {
    if job["submitted_at"].as_str().is_none() {
        return Ok(false);
    }
    let invalid = job["last_outcome"] == "invalid_output"
        && attempt == job["output_attempt"].as_u64().unwrap_or(1) + 1
        && attempt <= model_attempts;
    let limited = job["last_outcome"] == "rate_limited"
        && attempt == job["output_attempt"].as_u64().unwrap_or(1)
        && job["rate_retries"].as_u64().unwrap_or(0) < 2
        && job["next_due"].as_i64().unwrap_or(0) <= chrono::Utc::now().timestamp_millis();
    if invalid || limited {
        Ok(limited)
    } else {
        Err(ApiError::new(
            "submission_already_sent",
            "结果未知或重试预算已耗尽，不能盲重发",
        ))
    }
}

/// 文本轮次账本：已提交且结论不处于有限重试中——结果未知且无法续试。
pub(super) fn text_outcome_unknown(job: &Value) -> bool {
    job["submitted_at"].is_string()
        && !["rate_limited", "invalid_output"].contains(&job["last_outcome"].as_str().unwrap_or(""))
}

impl Application {
    /// 视觉编码的「结果未知」收口：任务已提交但无法证明结果 → waiting_input
    /// （submission_unknown），请人工核对供应商记录，不自动重发。
    pub(super) fn vision_submission_unknown(
        &self,
        member: &str,
        id: &str,
        lease: &str,
    ) -> Result<Value, ApiError> {
        self.job_finish(
            member,
            id,
            lease,
            "waiting_input",
            Value::Null,
            json!({"code":"submission_unknown","message":"上次提交结果未知，请核实供应商后另建明确任务","retryable":false}),
        )
    }

    /// 图像编码的「结果未知」判定：已提交、仍在 running、租约已过期且没有
    /// 可验证结果 → 收敛为 waiting_input（provider_outcome_unknown）。收敛后
    /// 只能人工核对供应商记录，不自动重发。
    /// 返回 Some(doc)：任务已提交，调用方把 doc 原样返回，不再推进；
    /// 返回 None：尚未提交，可正常推进。
    pub(super) fn image_submission_reconcile(
        &self,
        member: &str,
        id: &str,
        existing: &Value,
    ) -> Result<Option<Value>, ApiError> {
        if !is_submitted(existing) {
            return Ok(None);
        }
        if existing["state"] == "running"
            && existing["lease_expires"].as_i64().unwrap_or(0)
                <= chrono::Utc::now().timestamp_millis()
        {
            let tx = self.begin_write()?;
            let mut doc = self.job_get(json!({"member_id":member,"id":id}))?;
            if doc["state"] != "running"
                || doc["lease_expires"].as_i64().unwrap_or(0)
                    > chrono::Utc::now().timestamp_millis()
            {
                return Ok(Some(doc));
            }
            doc["state"] = json!("waiting_input");
            doc["lease_token"] = Value::Null;
            doc["error"] = json!({"code":"provider_outcome_unknown","message":"请求已提交且结果未知；供应商无查询接口，请检查供应商记录，不能重发此任务","retryable":false});
            let pending = self.save_job(&tx, &doc)?;
            self.commit_job_events(tx, pending)?;
            return Ok(Some(doc));
        }
        Ok(Some(existing.clone()))
    }
}

#[cfg(test)]
mod ledger_contract {
    use super::*;
    use serde_json::json;

    #[test]
    fn shared_dispatch_labels_name_each_registered_image_task() {
        assert_eq!(image_task_label("product_display"), "商品图");
        assert_eq!(image_task_label("outfit_flatlay"), "搭配图");
        assert_eq!(image_task_label("virtual_tryon"), "试穿");
        assert_eq!(
            image_task_label("recognition"),
            "图像",
            "未登记的图像入口一并兜底"
        );
    }

    #[test]
    fn submitted_predicate_reads_every_registered_encoding() {
        // 顶部 submitted_at 编码：图像生成、文本轮次、stylist。
        for kind in [
            "product_display",
            "outfit_flatlay",
            "virtual_tryon",
            "text_recommendation",
            "scenario_plan",
            "stylist_query",
        ] {
            let submitted = json!({"kind":kind,"submitted_at":"2026-10-11T00:00:00Z"});
            let fresh = json!({"kind":kind});
            assert!(is_submitted(&submitted), "{kind} 已提交");
            assert!(!is_submitted(&fresh), "{kind} 未提交");
        }
        // 视觉提交编码：台账在提交载荷里。
        let submitted = json!({"kind":"recognition","payload":{"submission":"submitted"}});
        let received = json!({"kind":"purchase_recognition","payload":{"submission":"received"}});
        let fresh = json!({"kind":"recognition","payload":{"submission":"not_submitted"}});
        assert!(is_submitted(&submitted));
        assert!(is_submitted(&received), "已收到结论同样不得盲重发");
        assert!(!is_submitted(&fresh));
        // 台账外的任务没有「已提交」形态。
        assert!(!is_submitted(
            &json!({"kind":"sync","submitted_at":"2026-10-11T00:00:00Z"})
        ));
    }

    #[test]
    fn vision_intent_hash_ignores_ledger_owned_payload_fields() {
        let payload = |submission: Value| json!({"request":"r","submission":submission,"transferred_images":[{"asset_id":"a"}]});
        assert_eq!(
            submission_hash(payload(json!("submitted")), "recognition"),
            submission_hash(payload(json!("not_submitted")), "recognition"),
            "发送清单与提交状态不参与意图哈希"
        );
        assert_eq!(
            submission_hash(payload(json!("submitted")), "purchase_recognition"),
            submission_hash(payload(json!("not_submitted")), "purchase_recognition")
        );
        assert_ne!(
            submission_hash(json!({"request":"a"}), "recognition"),
            submission_hash(json!({"request":"b"}), "recognition"),
            "其余载荷字段仍参与哈希"
        );
        // 非视觉编码不剥字段：载荷即意图。
        assert_ne!(
            submission_hash(
                json!({"request":"r","submission":"submitted"}),
                "product_display"
            ),
            submission_hash(json!({"request":"r"}), "product_display"),
            "非视觉编码的载荷字段不剥除"
        );
    }

    #[test]
    fn text_gate_admits_only_bounded_retries() {
        let now = chrono::Utc::now().timestamp_millis();
        // 尚未提交：放行且不按限流记账。
        assert!(!text_retry_gate(&json!({}), 1, 2).unwrap());
        // 已提交、结论未知：拒绝盲重发。
        let unknown = json!({"submitted_at":"2026-10-11T00:00:00Z","last_outcome":"unknown","output_attempt":1,"rate_retries":0,"next_due":0});
        assert_eq!(
            text_retry_gate(&unknown, 1, 2).unwrap_err().code,
            "submission_already_sent"
        );
        // invalid_output 后的紧随补试放行；超上限或跳号不放行。
        let invalid = json!({"submitted_at":"2026-10-11T00:00:00Z","last_outcome":"invalid_output","output_attempt":1,"rate_retries":0,"next_due":0});
        assert!(!text_retry_gate(&invalid, 2, 2).unwrap());
        assert!(
            text_retry_gate(&invalid, 2, 1).is_err(),
            "超过模型尝试上限不放行"
        );
        assert!(text_retry_gate(&invalid, 3, 2).is_err(), "跳号补试不放行");
        // rate_limited 到期且预算未耗尽：放行并按限流记账；预算耗尽或未到期拒绝。
        let limited = json!({"submitted_at":"2026-10-11T00:00:00Z","last_outcome":"rate_limited","output_attempt":1,"rate_retries":1,"next_due":now - 1});
        assert!(text_retry_gate(&limited, 1, 2).unwrap());
        let exhausted = json!({"submitted_at":"2026-10-11T00:00:00Z","last_outcome":"rate_limited","output_attempt":1,"rate_retries":2,"next_due":now - 1});
        assert!(text_retry_gate(&exhausted, 1, 2).is_err());
        let not_due = json!({"submitted_at":"2026-10-11T00:00:00Z","last_outcome":"rate_limited","output_attempt":1,"rate_retries":1,"next_due":now + 60_000});
        assert!(text_retry_gate(&not_due, 1, 2).is_err());
    }

    #[test]
    fn text_outcome_unknown_excludes_bounded_retry_states() {
        let submitted =
            |outcome: &str| json!({"submitted_at":"2026-10-11T00:00:00Z","last_outcome":outcome});
        assert!(text_outcome_unknown(&submitted("unknown")));
        assert!(text_outcome_unknown(&submitted("received")));
        assert!(!text_outcome_unknown(&submitted("rate_limited")));
        assert!(!text_outcome_unknown(&submitted("invalid_output")));
        assert!(!text_outcome_unknown(&json!({"last_outcome":"unknown"})));
    }
}
