#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use tauri::Manager;
#[tauri::command]
fn application(app:tauri::AppHandle,operation:String,input:serde_json::Value)->Result<serde_json::Value,wardrobe_core::ApiError> {
    let root=std::env::var_os("WARDROBE_DATA_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| {
        // Explicit shared directory: CLI and GUI use the same app identifier.
        let home=std::env::var_os("HOME").or_else(||std::env::var_os("USERPROFILE")).map(std::path::PathBuf::from).unwrap_or_default();
        #[cfg(target_os="macos")] { home.join("Library/Application Support/wardrobe-agent") }
        #[cfg(target_os="windows")] { std::env::var_os("APPDATA").map(std::path::PathBuf::from).unwrap_or(home).join("wardrobe-agent") }
        #[cfg(not(any(target_os="macos",target_os="windows")))] { std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).unwrap_or_else(||home.join(".local/share")).join("wardrobe-agent") }
    });
    let _=app.path();
    wardrobe_core::Application::open(&root.join("wardrobe.sqlite"))?.execute(&operation,input)
}
fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![application]).run(tauri::generate_context!()).expect("无法启动衣橱应用"); }
