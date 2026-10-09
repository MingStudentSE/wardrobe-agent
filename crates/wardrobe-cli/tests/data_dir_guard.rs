//! CLI 端到端护栏（issue #51）：debug 构建下不带 --db、不设 WARDROBE_DATA_DIR 时，
//! 默认目录解析必须以 storage_error 拒绝（exit 1），不得触达生产数据目录。

use serde_json::Value;
use std::process::Command;

#[test]
fn debug_cli_without_isolation_refuses_default_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["doctor"])
        .env_remove("WARDROBE_DATA_DIR")
        .env_remove("WARDROBE_ALLOW_DEFAULT_DATA_DIR")
        .env_remove("WARDROBE_PROFILES_PATH")
        .output()
        .unwrap();
    #[cfg(debug_assertions)]
    {
        assert_eq!(output.status.code(), Some(1), "护栏拒绝应走 exit 1 通道");
        let body: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "非JSON stdout: {e}: {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
        assert_eq!(body["error"]["code"], "storage_error");
        let message = body["error"]["message"].as_str().unwrap();
        assert!(
            message.contains("WARDROBE_DATA_DIR"),
            "应提示隔离变量：{message}"
        );
    }
    // release 构建放行默认目录；本测试不真正打开数据库，release 分支到此为止。
    #[cfg(not(debug_assertions))]
    let _ = output;
}
