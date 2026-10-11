//! 分页 limit 的共享校验（#74）：clothing/diagnostics/pairing/outfits/wears
//! 等处逐份复写的「每页数量 1–100」校验收拢为一个实现。错误码（invalid_input）
//! 与中文文案逐字保持各操作既有形态；各操作游标编码语义不在本模块范围。

use crate::ApiError;
use serde_json::Value;

/// 读取并校验分页 limit：缺省用 `default`；非整数用 `integer_message` 拒绝，
/// 不在 1–100 用 `range_message` 拒绝。两类文案允许相同——既有实现里
/// clothing.history 等处对两种非法形态本就返回同一句文案。
pub(crate) fn parse_limit(
    input: &Value,
    default: u64,
    integer_message: &str,
    range_message: &str,
) -> Result<u64, ApiError> {
    match input.get("limit") {
        None => Ok(default),
        Some(value) => {
            let limit = value
                .as_u64()
                .ok_or_else(|| ApiError::new("invalid_input", integer_message))?;
            if !(1..=100).contains(&limit) {
                return Err(ApiError::new("invalid_input", range_message));
            }
            Ok(limit)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const INTEGER: &str = "每页数量必须为整数";
    const RANGE: &str = "每页数量必须为 1–100";

    #[test]
    fn missing_limit_falls_back_to_default() {
        assert_eq!(parse_limit(&json!({}), 50, INTEGER, RANGE).unwrap(), 50);
        assert_eq!(parse_limit(&json!({}), 24, INTEGER, RANGE).unwrap(), 24);
    }

    #[test]
    fn in_range_limit_passes_through() {
        assert_eq!(parse_limit(&json!({"limit":1}), 50, INTEGER, RANGE).unwrap(), 1);
        assert_eq!(parse_limit(&json!({"limit":100}), 50, INTEGER, RANGE).unwrap(), 100);
    }

    #[test]
    fn non_integer_limit_is_rejected_with_integer_message() {
        let error = parse_limit(&json!({"limit":"50"}), 50, INTEGER, RANGE).unwrap_err();
        assert_eq!(error.code, "invalid_input");
        assert_eq!(error.message, "每页数量必须为整数");
    }

    #[test]
    fn out_of_range_limit_is_rejected_with_range_message() {
        for limit in [0u64, 101, 1000] {
            let error = parse_limit(&json!({"limit":limit}), 50, INTEGER, RANGE).unwrap_err();
            assert_eq!(error.code, "invalid_input");
            assert_eq!(error.message, "每页数量必须为 1–100");
        }
    }

    #[test]
    fn identical_messages_keep_history_style_rejections_verbatim() {
        // clothing.history 等处两种非法形态共用一句文案：助手必须允许同样配置。
        let error = parse_limit(&json!({"limit":"50"}), 50, RANGE, RANGE).unwrap_err();
        assert_eq!(error.message, "每页数量必须为 1–100");
        let error = parse_limit(&json!({"limit":0}), 50, RANGE, RANGE).unwrap_err();
        assert_eq!(error.message, "每页数量必须为 1–100");
    }

    #[test]
    fn null_limit_is_rejected_like_existing_implementations() {
        // 既有 map/filter 形态对显式 null 同样报非整数/越界文案，而不是回落默认值。
        let error = parse_limit(&json!({"limit":null}), 50, INTEGER, RANGE).unwrap_err();
        assert_eq!(error.message, "每页数量必须为整数");
    }
}
