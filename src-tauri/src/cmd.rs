// src-tauri/src/cmd.rs
#![allow(non_snake_case)]

use crate::paths;
use crate::search_index;
use crate::session_manager;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppVersionInfo {
    version: String,
    runtime: String,
    platform: String,
    arch: String,
    update_channel: String,
}

// ==================== 核心命令 ====================

/// 扫描所有 provider 的会话列表
#[tauri::command]
pub async fn list_sessions() -> Result<Vec<session_manager::SessionMeta>, String> {
    let sessions = tauri::async_runtime::spawn_blocking(session_manager::scan_sessions)
        .await
        .map_err(|e| format!("Failed to scan sessions: {e}"))?;
    Ok(sessions)
}

#[tauri::command]
pub async fn list_provider_paths() -> Result<Vec<paths::ProviderPathInfo>, String> {
    tauri::async_runtime::spawn_blocking(paths::list_provider_paths)
        .await
        .map_err(|e| format!("Failed to list provider paths: {e}"))
}

#[tauri::command]
pub async fn set_provider_path(
    providerId: String,
    path: String,
) -> Result<paths::ProviderPathInfo, String> {
    tauri::async_runtime::spawn_blocking(move || paths::set_provider_path(&providerId, &path))
        .await
        .map_err(|e| format!("Failed to save provider path: {e}"))?
}

#[tauri::command]
pub async fn reset_provider_path(providerId: String) -> Result<paths::ProviderPathInfo, String> {
    tauri::async_runtime::spawn_blocking(move || paths::reset_provider_path(&providerId))
        .await
        .map_err(|e| format!("Failed to reset provider path: {e}"))?
}

#[tauri::command]
pub async fn pick_provider_directory(
    app: tauri::AppHandle,
    providerId: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let info = paths::list_provider_paths()
        .into_iter()
        .find(|item| item.provider_id == providerId)
        .ok_or_else(|| format!("Unsupported provider: {providerId}"))?;
    let initial = info.path;

    let result = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_directory(initial)
            .blocking_pick_folder()
    })
    .await
    .map_err(|e| format!("Failed to open folder picker: {e}"))?;

    match result {
        Some(file_path) => {
            let resolved = file_path
                .simplified()
                .into_path()
                .map_err(|e| format!("Failed to resolve selected folder: {e}"))?;
            Ok(Some(resolved.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn rebuild_search_index() -> Result<search_index::RebuildSearchIndexResult, String> {
    tauri::async_runtime::spawn_blocking(search_index::rebuild_index)
        .await
        .map_err(|e| format!("Failed to rebuild search index: {e}"))?
}

#[tauri::command]
pub async fn refresh_search_index() -> Result<search_index::RefreshSearchIndexResult, String> {
    tauri::async_runtime::spawn_blocking(search_index::refresh_index)
        .await
        .map_err(|e| format!("Failed to refresh search index: {e}"))?
}

#[tauri::command]
pub async fn get_search_index_status() -> Result<search_index::SearchIndexStatus, String> {
    tauri::async_runtime::spawn_blocking(search_index::get_index_status)
        .await
        .map_err(|e| format!("Failed to load search index status: {e}"))?
}

#[tauri::command]
pub async fn search_content(
    query: String,
    limit: Option<u32>,
    providerId: Option<String>,
    sinceTs: Option<i64>,
    projectPath: Option<String>,
    sortBy: Option<String>,
) -> Result<search_index::SearchContentResult, String> {
    let trimmed = query.trim().to_string();
    if trimmed.is_empty() {
        return Ok(search_index::SearchContentResult {
            total_count: 0,
            hits: Vec::new(),
        });
    }

    let limit = usize::try_from(limit.unwrap_or(50)).unwrap_or(50);
    let provider_id = providerId.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let project_path = projectPath.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    tauri::async_runtime::spawn_blocking(move || {
        search_index::search_content(
            &trimmed,
            limit,
            provider_id.as_deref(),
            sinceTs,
            project_path.as_deref(),
            sortBy.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("Failed to search indexed content: {e}"))?
}

#[tauri::command]
pub async fn list_indexed_sessions(
    limit: Option<u32>,
    providerId: Option<String>,
) -> Result<Vec<search_index::IndexedSession>, String> {
    let limit = usize::try_from(limit.unwrap_or(200)).unwrap_or(200);
    let provider_id = providerId.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    tauri::async_runtime::spawn_blocking(move || {
        search_index::list_indexed_sessions(limit, provider_id.as_deref())
    })
    .await
    .map_err(|e| format!("Failed to list indexed sessions: {e}"))?
}

#[tauri::command]
pub async fn list_indexed_sessions_page(
    limit: Option<u32>,
    offset: Option<u32>,
    providerId: Option<String>,
    projectPath: Option<String>,
) -> Result<search_index::PagedIndexedSessionsResult, String> {
    let limit = usize::try_from(limit.unwrap_or(50)).unwrap_or(50);
    let offset = usize::try_from(offset.unwrap_or(0)).unwrap_or(0);
    let provider_id = providerId.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let project_path = projectPath.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    tauri::async_runtime::spawn_blocking(move || {
        search_index::list_indexed_sessions_page(
            limit,
            offset,
            provider_id.as_deref(),
            project_path.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("Failed to list paged indexed sessions: {e}"))?
}

#[tauri::command]
pub async fn list_indexed_projects(
    providerId: Option<String>,
) -> Result<Vec<search_index::IndexedProjectOption>, String> {
    let provider_id = providerId.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    tauri::async_runtime::spawn_blocking(move || {
        search_index::list_indexed_projects(provider_id.as_deref())
    })
    .await
    .map_err(|e| format!("Failed to list indexed projects: {e}"))?
}

#[tauri::command]
pub async fn list_indexed_sessions_by_source_paths(
    providerId: String,
    sourcePaths: Vec<String>,
) -> Result<Vec<search_index::IndexedSession>, String> {
    let provider_id = providerId.trim().to_string();
    if provider_id.is_empty() {
        return Err("providerId is required".to_string());
    }

    tauri::async_runtime::spawn_blocking(move || {
        search_index::list_indexed_sessions_by_source_paths(&provider_id, &sourcePaths)
    })
    .await
    .map_err(|e| format!("Failed to list indexed sessions by source paths: {e}"))?
}

#[tauri::command]
pub async fn get_indexed_session_messages(
    providerId: String,
    sourcePath: String,
) -> Result<Vec<search_index::IndexedMessage>, String> {
    let provider_id = providerId.trim().to_string();
    let source_path = sourcePath.trim().to_string();
    if provider_id.is_empty() || source_path.is_empty() {
        return Err("providerId and sourcePath are required".to_string());
    }

    tauri::async_runtime::spawn_blocking(move || {
        search_index::get_indexed_session_messages(&provider_id, &source_path)
    })
    .await
    .map_err(|e| format!("Failed to load indexed session messages: {e}"))?
    .map_err(|e| format!("Failed to load indexed session messages: {e}"))
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
        let deleted = session_manager::delete_session(&provider_id, &session_id, &source_path)?;
        if deleted {
            let _ = search_index::delete_indexed_session(&provider_id, &source_path);
        }
        Ok(deleted)
    })
    .await
    .map_err(|e| format!("Failed to delete session: {e}"))?
}

#[tauri::command]
pub async fn get_app_version() -> Result<AppVersionInfo, String> {
    Ok(AppVersionInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        runtime: "desktop".to_string(),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        update_channel: "desktop-release".to_string(),
    })
}

// ==================== 系统终端 / 文件管理器 ====================

/// 在桌面系统终端中执行恢复命令。
#[tauri::command]
pub async fn launch_session_terminal(
    command: String,
    cwd: Option<String>,
    terminalKind: Option<String>,
) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        return launch_windows_terminal(&command, cwd.as_deref(), terminalKind.as_deref());
    }

    #[cfg(target_os = "macos")]
    {
        return launch_macos_terminal(&command, cwd.as_deref(), terminalKind.as_deref());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (command, cwd, terminalKind);
        Err("Terminal launch is only supported on Windows and macOS".to_string())
    }
}

#[tauri::command]
pub async fn open_in_file_explorer(path: String) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        return open_windows_file_explorer(&path);
    }

    #[cfg(target_os = "macos")]
    {
        return open_macos_finder(&path);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = path;
        Err("File manager integration is only supported on Windows and macOS".to_string())
    }
}

#[cfg(target_os = "windows")]
fn launch_windows_terminal(
    command: &str,
    cwd: Option<&str>,
    terminal_kind: Option<&str>,
) -> Result<bool, String> {
    use std::process::Command;

    match terminal_kind {
        Some("powershell") => {
            let shell_command = build_powershell_prompt_script(command);
            let binary = resolve_powershell_binary();
            let mut process = Command::new(binary);
            process.args(["-NoExit", "-Command", &shell_command]);
            apply_current_dir(&mut process, cwd);
            process
                .spawn()
                .map_err(|e| format!("Failed to launch {binary}: {e}"))?;
            Ok(true)
        }
        Some("cmd") => {
            let mut process = Command::new("cmd.exe");
            process.args(["/K", command]);
            apply_current_dir(&mut process, cwd);
            process
                .spawn()
                .map_err(|e| format!("Failed to launch cmd.exe: {e}"))?;
            Ok(true)
        }
        Some(other) => Err(format!("Unsupported terminal kind: {other}")),
        None => {
            let mut wt = Command::new("wt.exe");
            wt.arg("new-tab");
            if let Some(dir) = cwd.filter(|dir| !dir.trim().is_empty()) {
                wt.args(["--startingDirectory", dir]);
            }
            let wt = wt.args(["cmd.exe", "/K", command]).spawn();

            if wt.is_ok() {
                return Ok(true);
            }

            let mut process = Command::new("cmd.exe");
            process.args(["/K", command]);
            apply_current_dir(&mut process, cwd);
            process
                .spawn()
                .map_err(|e| format!("Failed to launch terminal: {e}"))?;

            Ok(true)
        }
    }
}

#[cfg(target_os = "windows")]
fn open_windows_file_explorer(path: &str) -> Result<bool, String> {
    use std::path::Path;
    use std::process::Command;

    let target = Path::new(path);
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

#[cfg(target_os = "macos")]
fn launch_macos_terminal(
    command: &str,
    cwd: Option<&str>,
    terminal_kind: Option<&str>,
) -> Result<bool, String> {
    match terminal_kind {
        None | Some("terminal") => launch_macos_terminal_app(command, cwd),
        Some("iterm") | Some("iterm2") => launch_macos_iterm(command, cwd),
        Some("ghostty") => launch_macos_ghostty(command, cwd),
        Some(other) => return Err(format!("Unsupported terminal kind on macOS: {other}")),
    }
}

#[cfg(target_os = "macos")]
fn launch_macos_terminal_app(command: &str, cwd: Option<&str>) -> Result<bool, String> {
    use std::process::Command;

    let shell_command = build_macos_terminal_command(command, cwd)?;
    let do_script = format!("do script {}", quote_applescript_string(&shell_command));

    Command::new("osascript")
        .args([
            "-e",
            "tell application \"Terminal\"",
            "-e",
            "activate",
            "-e",
            &do_script,
            "-e",
            "end tell",
        ])
        .spawn()
        .map_err(|e| format!("Failed to launch Terminal: {e}"))?;

    Ok(true)
}

#[cfg(target_os = "macos")]
fn launch_macos_iterm(command: &str, cwd: Option<&str>) -> Result<bool, String> {
    use std::process::Command;

    let shell_command = build_macos_terminal_command(command, cwd)?;
    let script = format!(
        r#"set launcher_command to {}
set was_running to application "iTerm" is running
tell application "iTerm"
    if was_running then
        activate
        if (count of windows) = 0 then
            create window with default profile
        else
            tell current window
                create tab with default profile
            end tell
        end if
    else
        activate
        set waited to 0
        repeat while (count of windows) = 0
            delay 0.1
            set waited to waited + 1
            if waited >= 30 then exit repeat
        end repeat
        if (count of windows) = 0 then
            create window with default profile
        end if
    end if
    tell current session of current window
        write text launcher_command
    end tell
end tell"#,
        quote_applescript_string(&shell_command),
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| format!("Failed to launch iTerm: {e}"))?;

    Ok(true)
}

#[cfg(target_os = "macos")]
fn launch_macos_ghostty(command: &str, cwd: Option<&str>) -> Result<bool, String> {
    use std::path::Path;
    use std::process::Command;

    let trimmed_command = command.trim();
    if trimmed_command.is_empty() {
        return Err("Command is required".to_string());
    }

    let mut process = Command::new("open");
    process.args([
        "-na",
        "Ghostty",
        "--args",
        "--quit-after-last-window-closed=true",
    ]);
    if let Some(dir) = cwd.filter(|dir| !dir.trim().is_empty()) {
        let target = Path::new(dir);
        if !target.exists() {
            return Err(format!("Working directory not found: {dir}"));
        }
        if !target.is_dir() {
            return Err(format!("Working directory is not a directory: {dir}"));
        }
        process.arg(format!("--working-directory={dir}"));
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    process.args(["-e", &shell, "-l", "-c", trimmed_command]);
    process
        .spawn()
        .map_err(|e| format!("Failed to launch Ghostty: {e}"))?;

    Ok(true)
}

#[cfg(target_os = "macos")]
fn open_macos_finder(path: &str) -> Result<bool, String> {
    use std::path::Path;
    use std::process::Command;

    let target = Path::new(path);
    if !target.exists() {
        return Err(format!("Path not found: {path}"));
    }

    let mut command = Command::new("open");
    if target.is_file() {
        command.args(["-R", path]);
    } else {
        command.arg(path);
    }

    command
        .spawn()
        .map_err(|e| format!("Failed to open Finder: {e}"))?;

    Ok(true)
}

#[cfg(target_os = "macos")]
fn build_macos_terminal_command(command: &str, cwd: Option<&str>) -> Result<String, String> {
    use std::path::Path;

    let trimmed_command = command.trim();
    if trimmed_command.is_empty() {
        return Err("Command is required".to_string());
    }

    if let Some(dir) = cwd.filter(|dir| !dir.trim().is_empty()) {
        let target = Path::new(dir);
        if !target.exists() {
            return Err(format!("Working directory not found: {dir}"));
        }
        if !target.is_dir() {
            return Err(format!("Working directory is not a directory: {dir}"));
        }

        Ok(format!(
            "cd -- {} && {}",
            shell_single_quote(dir),
            trimmed_command
        ))
    } else {
        Ok(trimmed_command.to_string())
    }
}

#[cfg(target_os = "macos")]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "macos")]
fn quote_applescript_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ");
    format!("\"{escaped}\"")
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
        "$Host.UI.RawUI.WindowTitle = 'ACLIV - PowerShell'; \
Write-Host ''; \
Write-Host 'Resume command copied to clipboard.' -ForegroundColor Cyan; \
Write-Host 'Paste and run this command:' -ForegroundColor Cyan; \
Write-Host '{escaped_command}' -ForegroundColor Yellow; \
Write-Host ''"
    )
}
