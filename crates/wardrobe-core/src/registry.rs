//! 操作注册表：公开操作的唯一注册条目（issue #70）。
//!
//! 一个操作一行：`declarations()` 声明 (名称, effect, 分发处理器)，
//! 字段白名单从操作自己的 inputSchema.properties 派生——分发、
//! 描述符 JSON 的 effect、分发前输入校验从同一条注册数据导出，
//! 不再各自维护能与契约漂移的平行表。
//! 声明与公开契约冻结快照逐字对照（operation_registry::public_operation_contracts_are_frozen）。
//! sync.*/schedule.* 家族公开契约本就未声明 effect，保持 None。

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// 操作处理器。
pub(crate) type Handler = fn(&crate::Application, Value) -> Result<Value, crate::ApiError>;
/// 家族路由器：仅用于注册表外的前缀误用回落，保持旧分发器的家族内拒绝语义。
pub(crate) type FamilyRouter = fn(&crate::Application, &str, Value) -> Result<Value, crate::ApiError>;

/// schema_version 注入沿用既有分发器前缀规则（backup.*/data_directory.*/schedule.*）；
/// 与旧实现按同一谓词导出，输出等价性由测试钉住（见文件末尾与 operation_registry）。
pub(crate) fn injects_schema_version(name: &str) -> bool {
    name.starts_with("backup.")
        || name.starts_with("data_directory.")
        || name.starts_with("schedule.")
}

pub(crate) struct OperationEntry {
    /// 「拒绝未知输入」的唯一依据：与公开 inputSchema.properties 逐字一致。
    pub(crate) fields: BTreeSet<String>,
}

/// 声明索引：仅由静态声明构建，绝不回调 operations()——
/// operations() 渲染 effect 时要读它，反向依赖会造成 OnceLock 重入死锁。
fn declaration_index(
) -> &'static BTreeMap<String, (Option<Value>, Option<Handler>)> {
    static INDEX: OnceLock<BTreeMap<String, (Option<Value>, Option<Handler>)>> = OnceLock::new();
    INDEX.get_or_init(|| {
        declarations()
            .iter()
            .map(|(name, effect, handler)| (name.to_string(), (effect.clone(), *handler)))
            .collect()
    })
}

pub(crate) fn effect(name: &str) -> Option<&'static Value> {
    declaration_index()
        .get(name)
        .and_then(|(effect, _)| effect.as_ref())
}

pub(crate) fn handler(name: &str) -> Option<Handler> {
    declaration_index()
        .get(name)
        .and_then(|(_, handler)| *handler)
}

/// 完整注册条目（含从公开 inputSchema 派生的字段白名单）。
/// 只在分发/校验期读取；operations() 不经过这里，避免重入。
fn entries() -> &'static BTreeMap<String, OperationEntry> {
    static ENTRIES: OnceLock<BTreeMap<String, OperationEntry>> = OnceLock::new();
    ENTRIES.get_or_init(build)
}

pub(crate) fn operation(name: &str) -> Option<&'static OperationEntry> {
    entries().get(name)
}

fn build() -> BTreeMap<String, OperationEntry> {
    let mut map = BTreeMap::new();
    for descriptor in crate::operations().as_array().unwrap() {
        let name = descriptor["name"].as_str().unwrap().to_owned();
        let fields = descriptor["inputSchema"]["properties"]
            .as_object()
            .map(|properties| properties.keys().cloned().collect())
            .unwrap_or_default();
        let (_, handler) = declaration_index()
            .get(&name)
            .cloned()
            .unwrap_or_else(|| panic!("操作 {name} 缺少注册声明（effect/分发处理器）"));
        debug_assert!(handler.is_some(), "操作 {name} 缺少分发处理器声明");
        map.insert(name, OperationEntry { fields });
    }
    map
}

/// 注册表外的前缀误用回落到旧家族分发器（保持旧匹配臂顺序与家族内拒绝语义）；
/// 公开操作永远走精确行，不经过这里。
pub(crate) fn family_router(name: &str) -> Option<FamilyRouter> {
    const ROUTERS: &[(&str, FamilyRouter)] = &[
        ("sync.", |app, name, input| app.sync_operation(name, input)),
        ("maintenance.", |app, name, input| app.maintenance_operation(name, input)),
        ("data_directory.", |app, name, input| app.directory_operation(name, input)),
        ("backup.", |app, name, input| app.backup_operation(name, input)),
        ("encryption.", |app, name, input| app.encryption_operation(name, input)),
        ("diagnostics.", |app, name, input| app.diagnostics_execute(name, input)),
        ("outfit.", |app, name, input| app.outfit_execute(name, input)),
        ("wear.", |app, name, input| app.wear_execute(name, input)),
        ("learning.", |app, name, input| app.learning_execute(name, input)),
        ("stylist.", |app, name, input| app.stylist_execute(name, input)),
        ("purchase.", |app, name, input| app.purchase_execute(name, input)),
        ("feedback.", |app, name, input| app.feedback_execute(name, input)),
        ("weather.", |app, name, input| app.weather_execute(name, input)),
        ("schedule.", |app, name, input| app.schedule_execute(name, input)),
        ("scenario.", |app, name, input| app.scenario_execute(name, input)),
        ("recommendation.", |app, name, input| app.recommendation_execute(name, input)),
        ("pairing.", |app, name, input| app.pairing_execute(name, input)),
        ("care.", |app, name, input| app.care_execute(name, input)),
    ];
    ROUTERS
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map(|(_, router)| *router)
}

/// 未公开注册的操作没有可依据的契约，不做字段校验；
/// 分发层会以 unsupported_operation（或家族路由器的既有拒绝）处理。
pub(crate) fn rejects_unknown_fields(name: &str, input: &Value) -> bool {
    operation(name).is_some_and(|entry| {
        input
            .as_object()
            .is_some_and(|object| object.keys().any(|key| !entry.fields.contains(key)))
    })
}

type Declaration = (&'static str, Option<Value>, Option<Handler>);
static DECLARATIONS: OnceLock<Vec<Declaration>> = OnceLock::new();
fn declarations() -> &'static Vec<Declaration> {
    DECLARATIONS.get_or_init(|| vec![
        ("space.default", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, _input: Value| app.default_space())),
        ("member.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, _input: Value| app.member_list())),
        ("member.create", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.member_create(input))),
        ("member.update", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.member_update(input))),
        ("member.select", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.member_select(input))),
        ("backup.create", Some(json!({"async":{"job_kind":"backup_create","process":"maintenance.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]},"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.maintenance_enqueue("backup.create", input))),
        ("backup.preview", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.maintenance_enqueue("backup.preview", input))),
        ("backup.restore", Some(json!({"async":{"job_kind":"backup_restore","process":"maintenance.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]},"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.maintenance_enqueue("backup.restore", input))),
        ("backup.settings.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.backup_operation("backup.settings.get", input))),
        ("backup.settings.set", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.backup_operation("backup.settings.set", input))),
        ("maintenance.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.maintenance_operation("maintenance.get", input))),
        ("maintenance.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.maintenance_operation("maintenance.list", input))),
        ("maintenance.cancel", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.maintenance_operation("maintenance.cancel", input))),
        ("maintenance.retry", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.maintenance_operation("maintenance.retry", input))),
        ("maintenance.worker.run", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.maintenance_operation("maintenance.worker.run", input))),
        ("data_directory.status", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.directory_operation("data_directory.status", input))),
        ("data_directory.move", Some(json!({"async":{"job_kind":"data_directory_move","process":"maintenance.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]},"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.maintenance_enqueue("data_directory.move", input))),
        ("encryption.package.export", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.package.export", input))),
        ("encryption.package.read", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.package.read", input))),
        ("encryption.status", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.status", input))),
        ("encryption.create", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.create", input))),
        ("encryption.restore", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.restore", input))),
        ("encryption.password.change", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.password.change", input))),
        ("encryption.unpair", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.unpair", input))),
        ("encryption.pair.offer", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.pair.offer", input))),
        ("encryption.pair.accept", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.pair.accept", input))),
        ("encryption.pair.confirm", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.pair.confirm", input))),
        ("encryption.device.apply", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.device.apply", input))),
        ("encryption.device.revoke", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.device.revoke", input))),
        ("encryption.encrypt", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.encrypt", input))),
        ("encryption.decrypt", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.decrypt", input))),
        ("encryption.recovery.create", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.recovery.create", input))),
        ("encryption.seal", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.seal", input))),
        ("encryption.open", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.encryption_operation("encryption.open", input))),
        ("sync.configure", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.configure", input))),
        ("sync.run", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.run", input))),
        ("sync.process", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.process", input))),
        ("sync.status", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.status", input))),
        ("sync.history", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.history", input))),
        ("sync.conflicts", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.conflicts", input))),
        ("sync.resolve", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.resolve", input))),
        ("sync.pause", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.pause", input))),
        ("sync.resume", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.resume", input))),
        ("sync.target.switch", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.target.switch", input))),
        ("sync.mirror.configure", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.mirror.configure", input))),
        ("sync.mirror.sync", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.mirror.sync", input))),
        ("sync.mirror.promote", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.mirror.promote", input))),
        ("sync.mirror.remove", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.mirror.remove", input))),
        ("sync.reclaim", None, Some(|app: &crate::Application, input: Value| app.sync_operation("sync.reclaim", input))),
        ("clothing.create", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.clothing_create(input))),
        ("clothing.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.clothing_get(input))),
        ("clothing.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.clothing_list(input))),
        ("clothing.history", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.clothing_history(input))),
        ("clothing.archive", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.clothing_lifecycle("clothing.archive", input))),
        ("clothing.activate", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.clothing_lifecycle("clothing.activate", input))),
        ("clothing.delete", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.clothing_lifecycle("clothing.delete", input))),
        ("clothing.restore", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.clothing_lifecycle("clothing.restore", input))),
        ("clothing.update", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.clothing_update(input))),
        ("category.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.category_operation("category.list", input))),
        ("category.create", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.category_operation("category.create", input))),
        ("category.delete", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.category_operation("category.delete", input))),
        ("category.rename", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.category_operation("category.rename", input))),
        ("provider.save", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.provider_save(input))),
        ("provider.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.provider_list(input))),
        ("provider.validate", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"explicit-provider"})), Some(|app: &crate::Application, input: Value| app.provider_validate(input))),
        ("provider.credential.set", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.provider_credential("provider.credential.set", input))),
        ("provider.credential.remove", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.provider_credential("provider.credential.remove", input))),
        ("provider.credential.status", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.provider_credential("provider.credential.status", input))),
        ("recognition.create", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"job_state":"after_real_submission"}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recognition_create(input))),
        ("recognition.process", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recognition_process(input))),
        ("recognition.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recognition_get(input))),
        ("recognition.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recognition_list(input))),
        ("recognition.review", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recognition_review(input, false))),
        ("recognition.dismiss", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recognition_review(input, true))),
        ("purchase.candidate.create", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.candidate.create", input))),
        ("purchase.candidate.recognize", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"job_state":"after_real_submission"}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.candidate.recognize", input))),
        ("purchase.candidate.process", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.candidate.process", input))),
        ("purchase.candidate.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.candidate.get", input))),
        ("purchase.candidate.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.candidate.list", input))),
        ("purchase.candidate.update", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.candidate.update", input))),
        ("purchase.candidate.dismiss", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.candidate.dismiss", input))),
        ("purchase.analysis.run", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.analysis.run", input))),
        ("purchase.analysis.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.analysis.get", input))),
        ("purchase.analysis.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.purchase_execute("purchase.analysis.list", input))),
        ("product.generate", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.product_generate(input))),
        ("product.process", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.product_process(input))),
        ("product.adopt", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.product_adopt(input))),
        ("flatlay.generate", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.flatlay_generate(input))),
        ("flatlay.process", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.flatlay_process(input))),
        ("flatlay.save", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.flatlay_save(input))),
        ("flatlay.preview", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.flatlay_preview(input))),
        ("tryon.person.import", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.tryon_person_import(input))),
        ("tryon.generate", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.tryon_generate(input))),
        ("tryon.compare", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.tryon_compare(input))),
        ("tryon.process", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.tryon_process(input))),
        ("tryon.save", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.tryon_save(input))),
        ("tryon.preview", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.tryon_preview(input))),
        ("tryon.person.preview", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.tryon_person_preview(input))),
        ("client.authorize", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.client_authorize(input))),
        ("client.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.client_list(input))),
        ("client.revoke", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.client_revoke(input))),
        ("assets.import", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.asset_import(input))),
        ("assets.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.asset_get(input))),
        ("assets.preview", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","local_files":true},"when":{"thumbnail_file_missing":true,"variant":"thumbnail"}}],"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.asset_preview(input))),
        ("assets.rebuild_thumbnail", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.asset_rebuild_thumbnail(input))),
        ("assets.adopt", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.asset_adopt(input))),
        ("assets.reorder", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.asset_reorder(input))),
        ("assets.detach", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.asset_detach(input))),
        ("assets.sync_manifest", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.asset_sync_manifest(input))),
        ("outfit.create", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.create", input))),
        ("outfit.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.get", input))),
        ("outfit.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.list", input))),
        ("outfit.update", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.update", input))),
        ("outfit.version", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.version", input))),
        ("outfit.favorite", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.favorite", input))),
        ("outfit.clone", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.clone", input))),
        ("outfit.delete", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.delete", input))),
        ("outfit.restore", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.restore", input))),
        ("outfit.render_local", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.outfit_execute("outfit.render_local", input))),
        ("wear.record", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.wear_execute("wear.record", input))),
        ("wear.correct", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.wear_execute("wear.correct", input))),
        ("wear.revoke", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.wear_execute("wear.revoke", input))),
        ("wear.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.wear_execute("wear.get", input))),
        ("wear.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.wear_execute("wear.list", input))),
        ("wear.history", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.wear_execute("wear.history", input))),
        ("wear.summary", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.wear_execute("wear.summary", input))),
        ("insights.summary", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.insights_summary(input))),
        ("insights.configure", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.insights_configure(input))),
        ("care.configure", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.configure", input))),
        ("care.mark_dirty", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.mark_dirty", input))),
        ("care.record", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.record", input))),
        ("care.correct", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.correct", input))),
        ("care.revoke", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.revoke", input))),
        ("care.status", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.status", input))),
        ("care.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.get", input))),
        ("care.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.list", input))),
        ("care.history", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.history", input))),
        ("care.dirty_list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.dirty_list", input))),
        ("care.reminders", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.reminders", input))),
        ("care.availability", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.care_execute("care.availability", input))),
        ("recommendation.generate", Some(json!({"async":null,"conditional_effects":[{"effect":{"async":{"job_kind":"text_recommendation","process":"recommendation.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]},"kind":"mutation","network":"none"},"when":{"strategy":"ai"}},{"effect":{"kind":"query","network":"web-weather"},"when":{"weather.mode":"automatic"}}],"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recommendation_execute("recommendation.generate", input))),
        ("recommendation.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recommendation_execute("recommendation.get", input))),
        ("recommendation.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recommendation_execute("recommendation.list", input))),
        ("recommendation.replace", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recommendation_execute("recommendation.replace", input))),
        ("recommendation.change", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recommendation_execute("recommendation.change", input))),
        ("recommendation.save", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.recommendation_execute("recommendation.save", input))),
        ("recommendation.worker.run", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"explicit-provider"})), Some(|app: &crate::Application, input: Value| app.recommendation_execute("recommendation.worker.run", input))),
        ("pairing.generate", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.pairing_execute("pairing.generate", input))),
        ("pairing.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.pairing_execute("pairing.list", input))),
        ("weather.configure", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.weather_execute("weather.configure", input))),
        ("weather.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.weather_execute("weather.get", input))),
        ("weather.refresh", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"query","network":"web-weather"},"when":{"config.mode":"automatic"}}],"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.weather_execute("weather.refresh", input))),
        ("schedule.device", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.device", input))),
        ("schedule.runtime.get", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.runtime.get", input))),
        ("schedule.runtime.configure", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.runtime.configure", input))),
        ("schedule.create", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.create", input))),
        ("schedule.update", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.update", input))),
        ("schedule.list", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.list", input))),
        ("schedule.history", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.history", input))),
        ("schedule.tick", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.tick", input))),
        ("schedule.pending", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.pending", input))),
        ("schedule.worker.run", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.worker.run", input))),
        ("schedule.notifications", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.notifications", input))),
        ("schedule.notification.retry", None, Some(|app: &crate::Application, input: Value| app.schedule_execute("schedule.notification.retry", input))),
        ("scenario.plan", Some(json!({"async":null,"conditional_effects":[{"effect":{"async":{"job_kind":"scenario_plan","process":"scenario.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]},"kind":"mutation","network":"none"},"when":{"strategy":"ai"}},{"effect":{"kind":"query","network":"web-weather"},"when":{"weather.mode":"automatic"}}],"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.scenario_execute("scenario.plan", input))),
        ("scenario.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.scenario_execute("scenario.get", input))),
        ("scenario.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.scenario_execute("scenario.list", input))),
        ("scenario.update", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"query","network":"web-weather"},"when":{"weather.mode":"automatic"}}],"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.scenario_execute("scenario.update", input))),
        ("scenario.delete", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.scenario_execute("scenario.delete", input))),
        ("scenario.worker.run", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.scenario_execute("scenario.worker.run", input))),
        ("feedback.record", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","wear":"wear.record"},"when":{"reaction":"actual_wore"}},{"effect":{"kind":"mutation","wear":"wear.record"},"when":{"reaction":"changed"}}],"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.feedback_execute("feedback.record", input))),
        ("feedback.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.feedback_execute("feedback.get", input))),
        ("feedback.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.feedback_execute("feedback.list", input))),
        ("feedback.correct", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.feedback_execute("feedback.correct", input))),
        ("feedback.revoke", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.feedback_execute("feedback.revoke", input))),
        ("feedback.history", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.feedback_execute("feedback.history", input))),
        ("feedback.try_another", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.feedback_execute("feedback.try_another", input))),
        ("learning.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.learning_execute("learning.get", input))),
        ("learning.set", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.learning_execute("learning.set", input))),
        ("learning.edit", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.learning_execute("learning.edit", input))),
        ("learning.delete", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.learning_execute("learning.delete", input))),
        ("learning.reset", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.learning_execute("learning.reset", input))),
        ("learning.recompute", Some(json!({"async":{"job_kind":"learning_recompute","process":"learning.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]},"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.learning_execute("learning.recompute", input))),
        ("learning.history", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.learning_execute("learning.history", input))),
        ("learning.worker.run", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.learning_execute("learning.worker.run", input))),
        ("stylist.ask", Some(json!({"async":{"job_kind":"stylist_query","process":"stylist.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]},"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.stylist_execute("stylist.ask", input))),
        ("stylist.worker.run", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"explicit-provider"},"when":{"submission_pending":true}}],"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.stylist_execute("stylist.worker.run", input))),
        ("jobs.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.job_get(input))),
        ("jobs.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.job_list(input))),
        ("jobs.cancel", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.job_action("jobs.cancel", input))),
        ("jobs.retry", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.job_action("jobs.retry", input))),
        ("imports.create", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.imports_create(input))),
        ("imports.get", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.imports_get(input))),
        ("imports.list", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.imports_list(input))),
        ("imports.process", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.imports_process(input))),
        ("imports.group", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.imports_group(input))),
        ("imports.commit", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.imports_commit(input))),
        ("imports.update", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.imports_update(input))),
        ("images.rotate", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.image_transform("images.rotate", input))),
        ("images.restore", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.image_transform("images.restore", input))),
        ("images.merge", Some(json!({"async":null,"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.image_links("images.merge", input))),
        ("images.replace", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.image_links("images.replace", input))),
        ("images.main", Some(json!({"async":null,"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.image_links("images.main", input))),
        ("images.model.status", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.cutout_model_status(input))),
        ("images.model.download", Some(json!({"async":{"job_kind":"local_model_download","process":"images.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]},"kind":"mutation","local_files":false,"network":"none"})), Some(|app: &crate::Application, input: Value| app.cutout_enqueue("images.model.download", input))),
        ("images.cutout", Some(json!({"async":{"job_kind":"local_cutout","process":"images.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]},"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.cutout_enqueue("images.cutout", input))),
        ("images.worker.run", Some(json!({"async":null,"conditional_effects":[{"effect":{"kind":"mutation","network":"model-download"},"when":{"kind":"local_model_download"}}],"kind":"mutation","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.cutout_run(input))),
        ("diagnostics.summary", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.diagnostics_execute("diagnostics.summary", input))),
        ("diagnostics.checks", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.diagnostics_execute("diagnostics.checks", input))),
        ("diagnostics.logs.query", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.diagnostics_execute("diagnostics.logs.query", input))),
        ("diagnostics.export", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.diagnostics_execute("diagnostics.export", input))),
        ("diagnostics.ui_event", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.diagnostics_execute("diagnostics.ui_event", input))),
        ("diagnostics.reproduction.start", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.diagnostics_execute("diagnostics.reproduction.start", input))),
        ("diagnostics.reproduction.stop", Some(json!({"async":null,"kind":"query","local_files":true,"network":"none"})), Some(|app: &crate::Application, input: Value| app.diagnostics_execute("diagnostics.reproduction.stop", input))),
        ("capabilities", Some(json!({"async":null,"kind":"query","local_files":false,"network":"none"})), Some(|app: &crate::Application, _input: Value| app.capabilities_result())),
    ])
}

#[cfg(test)]
mod schema_version_rule_tests {
    use super::{declaration_index, entries, injects_schema_version};
    /// 每个注册行的注入旗标必须与旧分发器的字面前缀规则一致（#70 等价性对比）：
    /// 新增操作若需注入，须有意识地扩展这一谓词，而不是悄悄依赖旧行为。
    #[test]
    fn injection_flag_matches_the_legacy_dispatcher_prefix_rule() {
        let registered = entries();
        assert!(registered.len() >= 200, "注册表不应为空: {}", registered.len());
        for (name, _) in registered {
            let legacy = name.starts_with("backup.")
                || name.starts_with("data_directory.")
                || name.starts_with("schedule.");
            assert_eq!(
                injects_schema_version(name),
                legacy,
                "{name} 的 schema_version 注入旗标偏离旧分发器前缀规则"
            );
        }
    }
    /// 每个注册行都必须声明分发处理器（公开操作不允许无 handler）。
    #[test]
    fn every_registered_operation_has_a_handler() {
        for (name, (_, handler)) in declaration_index() {
            assert!(handler.is_some(), "{name} 缺少分发处理器声明");
        }
    }
}
