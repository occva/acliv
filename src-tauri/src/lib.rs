// src-tauri/src/lib.rs（替换全部内容）
mod cmd;
mod paths;           // 新增
mod session_manager; // 新增

use cmd::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            #[cfg(debug_assertions)]
            {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.open_devtools();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_sessions,
            get_session_messages,
            delete_session,
            launch_session_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
