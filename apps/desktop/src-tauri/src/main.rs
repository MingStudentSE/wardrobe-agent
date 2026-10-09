#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[tauri::command]
async fn application(
    operation: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, wardrobe_core::ApiError> {
    tauri::async_runtime::spawn_blocking(move || {
        wardrobe_core::Application::open(&wardrobe_core::default_database_path_checked()?)?
            .execute(&operation, input)
    })
    .await
    .map_err(|_| wardrobe_core::ApiError {
        code: "execution_failed".into(),
        message: "本地操作未完成，请重试".into(),
    })?
}
#[tauri::command]
async fn search_weather_city(name: String) -> Result<serde_json::Value, String> {
    let name = name.trim().to_owned();
    if name.chars().count() < 2 || name.chars().count() > 120 {
        return Err("请输入 2 至 120 个字符的城市名称".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(12)).build()
            .and_then(|client| client.get("https://geocoding-api.open-meteo.com/v1/search")
                .query(&[("name", name.as_str()), ("count", "6"), ("language", "zh"), ("format", "json")])
                .send()?.error_for_status()?.json::<serde_json::Value>())
            .map_err(|_| "城市搜索暂不可用，请重试或手填坐标。".to_string())
    }).await.map_err(|_| "城市搜索未完成，请重试。".to_string())?
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![application, search_weather_city])
        .run(tauri::generate_context!())
        .expect("无法启动衣橱应用");
}
