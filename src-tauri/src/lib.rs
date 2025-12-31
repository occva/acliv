// ===============================
// AI CLI History Viewer - Tauri Application
// ===============================

mod cmd;
mod loader;
mod models;

use cmd::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 日志插件（仅 debug 模式）
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            
            // 启动时预加载 Claude 数据
            log::info!("Pre-loading Claude data...");
            let stats = loader::get_stats("claude");
            log::info!(
                "Loaded {} projects, {} conversations in {:.2}s",
                stats.projects_count,
                stats.conversations_count,
                stats.load_time
            );

            // 打开开发者工具（仅 debug 模式）
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
            // API Commands
            get_stats,
            get_projects,
            get_conversations,
            get_conversation_detail,
            search,
            reload_data,
            list_sources,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
