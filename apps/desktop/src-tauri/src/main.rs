#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[tauri::command]
async fn application(
    operation: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, wardrobe_core::ApiError> {
    tauri::async_runtime::spawn_blocking(move || {
        wardrobe_core::Application::open(&wardrobe_core::default_database_path())?
            .execute(&operation, input)
    })
    .await
    .map_err(|_| wardrobe_core::ApiError {
        code: "execution_failed".into(),
        message: "本地操作未完成，请重试".into(),
    })?
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![application])
        .run(tauri::generate_context!())
        .expect("无法启动衣橱应用");
}
