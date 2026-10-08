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
    .map_err(|_| wardrobe_core::ApiError::custom("execution_failed", "本地操作未完成，请重试"))?
}
#[tauri::command]
async fn search_weather_city(name: String) -> Result<serde_json::Value, String> {
    let name = name.trim().to_owned();
    let started = std::time::Instant::now();
    // 诊断（#41）：结果类别事件；不含用户输入的城市名全文，只记长度（脱敏按构造）。
    // application 命令每次 Application::open 初始化 logger，本旁路命令在此之前被调用
    // 也必须可用：log_bypass_event 进程内幂等初始化。
    let query_length = name.chars().count();
    let log = |outcome: &str| {
        wardrobe_core::diagnostics::log_bypass_event(
            "weather_search",
            serde_json::json!({
                "operation": "weather.city_search",
                "outcome": outcome,
                "duration_ms": started.elapsed().as_millis() as u64,
                "query_length": query_length,
            }),
        );
    };
    if query_length < 2 || query_length > 120 {
        log("validation_error");
        return Err("请输入 2 至 120 个字符的城市名称".into());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(12)).build()
            .and_then(|client| client.get("https://geocoding-api.open-meteo.com/v1/search")
                .query(&[("name", name.as_str()), ("count", "6"), ("language", "zh"), ("format", "json")])
                .send()?.error_for_status()?.json::<serde_json::Value>())
    }).await;
    log(match &result {
        Ok(Ok(_)) => "ok",
        Ok(Err(error)) if error.is_timeout() => "timeout",
        Ok(Err(error)) if error.is_status() => "http_error",
        Ok(Err(_)) => "error",
        Err(_) => "error",
    });
    result
        .map_err(|_| "城市搜索未完成，请重试。".to_string())?
        .map_err(|_| "城市搜索暂不可用，请重试或手填坐标。".to_string())
}
fn main() {
    // 日志核心：进程级 panic 钩子（幂等），崩溃现场写入数据目录 logs/。
    // application 命令每次调用都 Application::open，日志初始化进程内幂等、run_id 每进程仅生成一次。
    wardrobe_core::diagnostics::install_panic_hook();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![application, search_weather_city])
        .run(tauri::generate_context!())
        .expect("无法启动衣橱应用");
}
