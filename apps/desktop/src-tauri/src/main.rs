#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[tauri::command]
fn application(
    operation: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, wardrobe_core::ApiError> {
    wardrobe_core::Application::open(&wardrobe_core::default_database_path())?
        .execute(&operation, input)
}
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![application])
        .run(tauri::generate_context!())
        .expect("无法启动衣橱应用");
}
