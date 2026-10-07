use serde_json::{json, Value};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
pub fn native_schedule(app: tauri::AppHandle, action: String) -> Result<Value, String> {
    match action.as_str() {
        "status" => {}
        "enable_autostart" => app.autolaunch().enable().map_err(|e| e.to_string())?,
        "disable_autostart" => app.autolaunch().disable().map_err(|e| e.to_string())?,
        _ => return Err("不支持的本机调度设置".into()),
    }
    let state = app.autolaunch().is_enabled();
    Ok(match state {
        Ok(enabled) => {
            json!({"available":true,"autostart_enabled":enabled,"notification_permission":"managed_by_os_not_queryable"})
        }
        Err(error) => {
            json!({"available":true,"autostart_enabled":null,"autostart_error":error.to_string(),"notification_permission":"managed_by_os_not_queryable"})
        }
    })
}
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    app.handle().plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        Some(vec!["--background"]),
    ))?;
    let open = MenuItem::with_id(app, "open", "打开衣橱", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出并停止定时执行", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("衣橱 · 定时计划")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    if std::env::args().any(|a| a == "--background") && background_enabled() {
        if let Some(window) = app.get_webview_window("main") {
            window.hide()?;
        }
    }
    let handle = app.handle().clone();
    std::thread::spawn(move || loop {
        if let Err(error) = step(&handle) {
            if let Ok(app) =
                wardrobe_core::Application::open(&wardrobe_core::default_database_path())
            {
                let _ = app.schedule_report_runtime_error(&error.code);
            }
            let _ = handle.emit(
                "scheduler-error",
                json!({"code":error.code,"message":"定时执行未完成，请查看计划记录或重新打开衣橱"}),
            );
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    });
    Ok(())
}
pub fn background_enabled() -> bool {
    wardrobe_core::Application::open(&wardrobe_core::default_database_path())
        .and_then(|app| app.execute("schedule.runtime.get", json!({})))
        .is_ok_and(|v| v["background_enabled"] == true)
}
fn step(handle: &tauri::AppHandle) -> Result<(), wardrobe_core::ApiError> {
    let app = wardrobe_core::Application::open(&wardrobe_core::default_database_path())?;
    let foreground = || {
        handle.get_webview_window("main").is_some_and(|window| {
            window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true)
        })
    };
    if !app.schedule_host_should_run(foreground())? {
        return Ok(());
    }
    app.schedule_tick_at(chrono::Utc::now())?;
    for job in app.schedule_pending_jobs()? {
        if !app.schedule_host_should_run(foreground())? {
            return Ok(());
        }
        let result = app.execute(
            "schedule.worker.run",
            json!({"member_id":job["member_id"],"id":job["id"]}),
        )?;
        if matches!(result["state"].as_str(), Some("failed" | "waiting_input")) {
            app.schedule_report_runtime_error(
                result["error"]["code"]
                    .as_str()
                    .unwrap_or("scheduler_failed"),
            )?;
        }
    }
    // Claim reconciles stale interrupted submissions even when notifications are
    // disabled, then checks the current preference under its write transaction.
    if !app.schedule_host_should_run(foreground())? {
        return Ok(());
    }
    let members = app.execute("member.list", json!({}))?;
    for member in members["items"].as_array().unwrap() {
        let member = member["member_id"].as_str().unwrap();
        for _ in 0..16 {
            if !app.schedule_host_should_run(foreground())? {
                return Ok(());
            }
            let Some(notice) = app.schedule_claim_notification(member)? else {
                break;
            };
            let mut native = notify_rust::Notification::new();
            native
                .summary(notice["title"].as_str().unwrap_or("衣橱计划"))
                .body(notice["body"].as_str().unwrap_or("请打开衣橱查看"));
            #[cfg(target_os = "windows")]
            native.app_id(&handle.config().identifier);
            #[cfg(target_os = "macos")]
            let configured = notify_rust::set_application(&handle.config().identifier);
            #[cfg(target_os = "macos")]
            if let Err(_error) = configured {
                app.schedule_finish_notification(
                    member,
                    notice["id"].as_str().unwrap(),
                    notice["claim_token"].as_str().unwrap(),
                    "failed",
                    Some("系统通知配置失败，尚未提交，请核对安装与系统设置"),
                )?;
                continue;
            }
            #[cfg(target_os = "linux")]
            native.appname(
                &handle
                    .config()
                    .product_name
                    .clone()
                    .unwrap_or_else(|| "衣橱".into()),
            );
            let outcome = native.show();
            match outcome {
                Ok(_) => {
                    app.schedule_finish_notification(
                        member,
                        notice["id"].as_str().unwrap(),
                        notice["claim_token"].as_str().unwrap(),
                        "submitted",
                        None,
                    )?;
                }
                Err(_error) => {
                    app.schedule_finish_notification(
                        member,
                        notice["id"].as_str().unwrap(),
                        notice["claim_token"].as_str().unwrap(),
                        "unknown",
                        Some("系统通知接口返回错误，无法确定是否接收，请核对系统设置"),
                    )?;
                }
            }
        }
    }
    Ok(())
}
