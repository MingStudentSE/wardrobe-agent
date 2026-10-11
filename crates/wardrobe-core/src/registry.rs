//! 操作注册表：公开操作契约的唯一运行时索引（issue #70）。
//!
//! 每个操作在 `operations()` 的描述符 JSON 里声明一次自己的契约；
//! 本模块把描述符收进一张按名索引的表，让分发前的输入校验、
//! effect/授权元数据与描述符 JSON 从同一份声明导出，
//! 不再各自维护能与契约漂移的平行表。

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub(crate) struct OperationEntry {
    /// 「拒绝未知输入」的唯一依据：与公开 inputSchema.properties 逐字一致。
    /// 校验发生在分发之前，对所有操作一致（含此前从不校验的 category.* 等）。
    pub(crate) fields: BTreeSet<String>,
}

pub(crate) fn entries() -> &'static BTreeMap<String, OperationEntry> {
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
        map.insert(name, OperationEntry { fields });
    }
    map
}

/// 未公开注册的操作没有可依据的契约，不做字段校验；
/// 分发层会以 unsupported_operation 拒绝。
pub(crate) fn rejects_unknown_fields(name: &str, input: &Value) -> bool {
    operation(name).is_some_and(|entry| {
        input
            .as_object()
            .is_some_and(|object| object.keys().any(|key| !entry.fields.contains(key)))
    })
}
