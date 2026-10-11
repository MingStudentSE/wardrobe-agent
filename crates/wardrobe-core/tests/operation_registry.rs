//! 操作注册表契约（issue #70）：
//! - 公开描述符 JSON 逐字冻结（inputSchema/outputSchema/effect/scope 不变）；
//! - 「拒绝未知字段」成为全操作一致的行为：白名单从各操作自己的
//!   inputSchema.properties 派生（单一来源），分发前统一执行。
//!
//! category.* 等此前从不校验未知字段的操作开始拒绝——这是 #70 唯一允许的
//! 对外行为变化，由本文件逐字钉住并写入提交信息。

use serde_json::{json, Value};
use wardrobe_core::{operations, Application};

fn app() -> (tempfile::TempDir, Application, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("registry.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    (dir, app, member)
}

/// 公开契约冻结：214 个操作的描述符 JSON（name/description/inputSchema/
/// outputSchema/effect/examples/scope）必须与重构前逐字一致。
/// 有意的契约变更必须重新生成快照并在工单中显式记录。
#[test]
fn public_operation_contracts_are_frozen() {
    let current = operations();
    let frozen: Value =
        serde_json::from_str(include_str!("fixtures/capabilities-snapshot.json")).unwrap();
    assert_eq!(
        current,
        frozen,
        "公开操作契约发生变化：若为有意变更，重新生成 fixtures/capabilities-snapshot.json 并在工单记录"
    );
}

/// 此前从不校验未知字段的操作族开始拒绝（#70 唯一允许的对外行为变化）。
#[test]
fn unknown_fields_are_now_rejected_for_previously_unvalidated_families() {
    let (_dir, app, member) = app();

    // category.*：此前完全不知道「未知字段」这个概念
    let error = app
        .execute(
            "category.create",
            json!({"member_id":member,"name":"围巾","unknown_field":1}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");
    assert_eq!(error.message, "操作包含不支持的字段");
    let error = app
        .execute("category.list", json!({"member_id":member,"zzz":true}))
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");
    let error = app
        .execute(
            "category.delete",
            json!({"member_id":member,"id":"other","extra":null}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");

    // assets.import：校验发生在分发之前（路径还没被读取）
    let error = app
        .execute(
            "assets.import",
            json!({"member_id":member,"path":"/definitely/not/a/file","oops":1}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");

    // clothing.list / insights.summary：合法字段之外的键一律拒绝
    let error = app
        .execute("clothing.list", json!({"member_id":member,"wat":1}))
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");
    let error = app
        .execute(
            "insights.summary",
            json!({"member_id":member,"as_of_date":"2026-01-01","timezone":"UTC","hack":0}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");

    // 拒绝发生在业务校验之前，但合法输入不受影响
    let created = app
        .execute("category.create", json!({"member_id":member,"name":"围巾"}))
        .unwrap();
    assert_eq!(created["name"], json!("围巾"));
}

/// 未知字段拒绝文案统一为「操作包含不支持的字段」（ADR-0041）：注册表白名单
/// 在分发前统一拒绝，images.*（曾「图片操作包含不支持字段」）与 care.*
///（曾「洗护操作包含不支持的字段」）等族专属文案不再可达，错误码不变。
#[test]
fn unknown_field_message_is_unified_across_families() {
    let (_dir, app, member) = app();
    for (operation, input) in [
        (
            "images.rotate",
            json!({"member_id":member,"asset_id":"a","degrees":90,"surprise":1}),
        ),
        (
            "care.status",
            json!({"member_id":member,"clothing_id":"c","surprise":1}),
        ),
    ] {
        let error = app.execute(operation, input).unwrap_err();
        assert_eq!(error.code, "invalid_input", "{operation}");
        assert_eq!(error.message, "操作包含不支持的字段", "{operation}");
    }
}

/// 已有白名单的操作保持既有接受集：合法输入照常、未知字段依旧拒绝。
#[test]
fn previously_validated_operations_keep_their_acceptance_sets() {
    let (_dir, app, member) = app();

    // imports.create：原本就拒绝未知字段，行为不变
    let error = app
        .execute(
            "imports.create",
            json!({"member_id":member,"operation_key":"k","paths":[],"surprise":1}),
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");

    // member.create 同上
    let error = app
        .execute("member.create", json!({"name":"成员","nope":1}))
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");

    // jobs.list：输入面仅 member_id（2026-10-11 code-review 收回 #70 派生放宽：
    // 描述符 properties 曾声明无语义的 id，实现只读 member_id，带 id 拒绝）
    let error = app
        .execute("jobs.list", json!({"member_id":member,"id":"ignored"}))
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");
    assert_eq!(error.message, "操作包含不支持的字段");
    // id 在 jobs.get/cancel/retry 有真实语义，不受影响
    let error = app
        .execute("jobs.get", json!({"member_id":member,"id":"not-a-job"}))
        .unwrap_err();
    assert_eq!(error.code, "not_found");
}

/// 分发表驱动后（#70 增量4）：
/// - 旧分发器的 schema_version 注入前缀规则（backup.*/data_directory.*/schedule.*）
///   输出保持等价；
/// - 未注册的前缀误用统一落 unsupported_operation（旧实现落入家族分发器、
///   文案各异；错误码不变——这是分发表收编的既有对外面收缩，记录于报告）。
#[test]
fn schema_version_injection_still_covers_the_legacy_families() {
    let (_dir, app, member) = app();
    // 注入族：分发层补注 schema_version=1（与 handler 自拼等价，输出不变）
    let settings = app.execute("backup.settings.get", json!({})).unwrap();
    assert_eq!(settings["schema_version"], 1, "{settings}");
    let status = app.execute("data_directory.status", json!({})).unwrap();
    assert_eq!(status["schema_version"], 1, "{status}");
    let schedules = app
        .execute("schedule.list", json!({"member_id":member}))
        .unwrap();
    assert_eq!(schedules["schema_version"], 1, "{schedules}");
    let pending = app
        .execute("schedule.pending", json!({"member_id":member}))
        .unwrap();
    assert_eq!(pending["schema_version"], 1, "{pending}");
}

#[test]
fn unregistered_prefix_misuse_falls_back_to_unsupported_operation() {
    let (_dir, app, _member) = app();
    let error = app.execute("care.bogus", json!({})).unwrap_err();
    assert_eq!(error.code, "unsupported_operation");
    let error = app.execute("totally.unknown", json!({})).unwrap_err();
    assert_eq!(error.code, "unsupported_operation");
}
