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
    terminalKind: Option<String>,
) -> Result<bool, String> {
    #[cfg(not(target_os = "windows"))]
    {
        return Err("Terminal launch is only supported on Windows".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        match terminalKind.as_deref() {
            Some("powershell") => {
                let shell_command = build_powershell_prompt_script(&command);
                let binary = resolve_powershell_binary();
                let mut process = Command::new(binary);
                process.args(["-NoExit", "-Command", &shell_command]);
                apply_current_dir(&mut process, cwd.as_deref());
                process
                    .spawn()
                    .map_err(|e| format!("Failed to launch {binary}: {e}"))?;
                Ok(true)
            }
            Some("cmd") => {
                let mut process = Command::new("cmd.exe");
                process.args(["/K", &command]);
                apply_current_dir(&mut process, cwd.as_deref());
                process
                    .spawn()
                    .map_err(|e| format!("Failed to launch cmd.exe: {e}"))?;
                Ok(true)
            }
            Some(other) => Err(format!("Unsupported terminal kind: {other}")),
            None => {
                let mut wt = Command::new("wt.exe");
                wt.arg("new-tab");
                if let Some(dir) = cwd.as_deref().filter(|dir| !dir.trim().is_empty()) {
                    wt.args(["--startingDirectory", dir]);
                }
                let wt = wt.args(["cmd.exe", "/K", &command]).spawn();

                if wt.is_ok() {
                    return Ok(true);
                }

                let mut process = Command::new("cmd.exe");
                process.args(["/K", &command]);
                apply_current_dir(&mut process, cwd.as_deref());
                process
                    .spawn()
                    .map_err(|e| format!("Failed to launch terminal: {e}"))?;

                Ok(true)
            }
        }
    }
}

#[tauri::command]
pub async fn open_in_file_explorer(path: String) -> Result<bool, String> {
    #[cfg(not(target_os = "windows"))]
    {
        return Err("File Explorer integration is only supported on Windows".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use std::path::Path;
        use std::process::Command;

        let target = Path::new(&path);
        if !target.exists() {
            return Err(format!("Path not found: {path}"));
        }

        let mut command = Command::new("explorer.exe");
        if target.is_file() {
            command.arg(format!("/select,{}", target.display()));
        } else {
            command.arg(target);
        }

        command
            .spawn()
            .map_err(|e| format!("Failed to open File Explorer: {e}"))?;

        Ok(true)
    }
}

#[cfg(target_os = "windows")]
fn apply_current_dir(command: &mut std::process::Command, cwd: Option<&str>) {
    if let Some(dir) = cwd.filter(|dir| !dir.trim().is_empty()) {
        command.current_dir(dir);
    }
}

#[cfg(target_os = "windows")]
fn resolve_powershell_binary() -> &'static str {
    if command_exists_on_path("pwsh.exe") {
        "pwsh.exe"
    } else {
        "powershell.exe"
    }
}

#[cfg(target_os = "windows")]
fn command_exists_on_path(executable: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(executable).is_file()))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn build_powershell_prompt_script(command: &str) -> String {
    let escaped_command = command.replace('\'', "''");
    format!(
        "$Host.UI.RawUI.WindowTitle = 'AI CLI History Viewer - PowerShell'; \
Write-Host ''; \
Write-Host 'Resume command copied to clipboard.' -ForegroundColor Cyan; \
Write-Host 'Paste and run this command:' -ForegroundColor Cyan; \
Write-Host '{escaped_command}' -ForegroundColor Yellow; \
Write-Host ''"
    )
}
