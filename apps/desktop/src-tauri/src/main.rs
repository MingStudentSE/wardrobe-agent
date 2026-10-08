#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod scheduler;
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
    .map_err(|_| wardrobe_core::ApiError::of("execution_failed", "本地操作未完成，请重试"))?
}
#[tauri::command]
async fn search_weather_city(name: String) -> Result<serde_json::Value, String> {
    let name = name.trim().to_owned();
    if name.chars().count() < 2 || name.chars().count() > 120 {
        return Err("请输入 2 至 120 个字符的城市名称".into());
    }
    let search = name.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(12)).build()
            .and_then(|client| client.get("https://geocoding-api.open-meteo.com/v1/search")
                .query(&[("name", search.as_str()), ("count", "6"), ("language", "zh"), ("format", "json")])
                .send()?.error_for_status()?.json::<serde_json::Value>())
            .map_err(|_| "城市搜索暂不可用，请重试或手填坐标。".to_string())
    }).await.map_err(|_| "城市搜索未完成，请重试。".to_string())?;
    // 桌面旁路的天气城市搜索记录结果类别（同一日志流，不含查询原文以外内容）
    let data_dir = wardrobe_core::default_database_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    wardrobe_core::logging::emit_to(
        &data_dir,
        "info",
        "provider",
        "weather.city_search",
        if result.is_ok() { "ok" } else { "error" },
        serde_json::json!({"endpoint_category": "geocoding", "query_length": name.chars().count()}),
    );
    result
}

/// 在系统文件管理器中定位导出的诊断文件（桌面端专用）。
#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    let path = std::path::PathBuf::from(&path);
    if !path.exists() {
        return Err("文件不存在，可能已被移动或删除".into());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.to_string_lossy()))
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let directory = path.parent().unwrap_or(&path).to_string_lossy().to_string();
        std::process::Command::new("xdg-open")
            .arg(directory)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
fn main() {
    wardrobe_core::logging::install_panic_hook();
    wardrobe_core::logging::begin_run();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(scheduler::setup)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if scheduler::background_enabled() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            application,
            search_weather_city,
            reveal_path,
            scheduler::native_schedule
        ])
        .run(tauri::generate_context!())
        .expect("无法启动衣橱应用");
}
