//! 默认数据目录调试护栏（issue #51）：debug 构建未隔离时拒绝默认生产目录。

use wardrobe_core::guard_debug_default_data_dir;

#[test]
fn guard_rejects_unisolated_default_only_in_debug_builds() {
    let result = guard_debug_default_data_dir(false, false);
    #[cfg(debug_assertions)]
    {
        let error = result.expect_err("debug 构建未隔离必须拒绝默认目录");
        assert_eq!(error.code, "storage_error");
        assert!(error.message.contains("WARDROBE_DATA_DIR"));
    }
    #[cfg(not(debug_assertions))]
    assert!(result.is_ok(), "release 构建始终放行");
}

#[test]
fn guard_allows_isolated_and_explicitly_exempt_paths() {
    assert!(guard_debug_default_data_dir(true, false).is_ok());
    assert!(guard_debug_default_data_dir(false, true).is_ok());
    assert!(guard_debug_default_data_dir(true, true).is_ok());
}
