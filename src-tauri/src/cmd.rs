// src-tauri/src/cmd.rs
#![allow(non_snake_case)]

use crate::session_manager;

// ==================== 核心命令 ====================

/// 扫描所有 provider 的会话列表
#[tauri::command]
pub async fn list_sessions() -> Result<Vec<session_manager::SessionMeta>, String> {
    let sessions = tauri::async_runtime::spawn_blocking(session_manager::scan_sessions)
        .await
        .map_err(|e| format!("Failed to scan sessions: {e}"))?;
    Ok(sessions)
}

/// 获取指定会话的消息详情
#[tauri::command]
pub async fn get_session_messages(
    providerId: String,
    sourcePath: String,
) -> Result<Vec<session_manager::SessionMessage>, String> {
    let provider_id = providerId.clone();
    let source_path = sourcePath.clone();
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::load_messages(&provider_id, &source_path)
    })
    .await
    .map_err(|e| format!("Failed to load session messages: {e}"))?
}

/// 删除指定会话及其 provider 侧关联资源
#[tauri::command]
pub async fn delete_session(
    providerId: String,
    sessionId: String,
    sourcePath: String,
) -> Result<bool, String> {
    let provider_id = providerId.clone();
    let session_id = sessionId.clone();
    let source_path = sourcePath.clone();

    tauri::async_runtime::spawn_blocking(move || {
        session_manager::delete_session(&provider_id, &session_id, &source_path)
    })
    .await
    .map_err(|e| format!("Failed to delete session: {e}"))?
}

// ==================== Windows 终端启动 ====================

/// 在 Windows 终端中执行命令（仅 Windows 平台）
/// 非 Windows 返回 Err，前端降级为复制到剪贴板
#[tauri::command]
pub async fn launch_session_terminal(
    command: String,
    cwd: Option<String>,
) -> Result<bool, String> {
    #[cfg(not(target_os = "windows"))]
    {
        return Err("Terminal launch is only supported on Windows".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        let full_command = if let Some(ref dir) = cwd {
            // Windows: cd /d 切换驱动器和目录，然后执行命令
            format!("cd /d \"{}\" && {}", dir, command)
        } else {
            command.clone()
        };

        // 优先尝试 Windows Terminal (wt.exe)
        let wt = Command::new("wt.exe")
            .args(["new-tab", "cmd.exe", "/k", &full_command])
            .spawn();

        if wt.is_ok() {
            return Ok(true);
        }

        // 回退：cmd.exe 新窗口
        Command::new("cmd.exe")
            .args(["/c", "start", "cmd.exe", "/k", &full_command])
            .spawn()
            .map_err(|e| format!("Failed to launch terminal: {e}"))?;

        Ok(true)
    }
}
