//! Member-scoped built-in Stylist. Root owns tool dispatch and submission safety.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum StylistOrigin {
    TrustedLocal,
    Client { grant_id: String },
}

#[path = "stylist_queue.rs"]
mod queue;

#[path = "stylist_runtime.rs"]
mod runtime;
impl crate::Application {
    pub(super) fn stylist_execute(
        &self,
        op: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, crate::ApiError> {
        match op {
            "stylist.ask" => self.stylist_enqueue(input, StylistOrigin::TrustedLocal),
            "stylist.worker.run" => self.stylist_worker(input),
            _ => Err(crate::ApiError::new(
                "unsupported_operation",
                "着装问答操作不可用",
            )),
        }
    }
}
pub(super) fn descriptors() -> Vec<serde_json::Value> {
    queue::descriptors()
}
