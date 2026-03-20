// src-tauri/src/paths.rs
// 简化版路径解析，直接返回各 CLI 工具的默认配置目录
// 不支持用户覆盖（cc-switch 支持，但本项目不需要）

use std::path::PathBuf;

fn home_dir() -> PathBuf {
    if let Some(override_home) = std::env::var_os("AICHV_HOME") {
        let path = PathBuf::from(override_home);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }

    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn resolve_tool_dir(hidden_dir_name: &str, marker_subdir: &str) -> PathBuf {
    let home = home_dir();
    let hidden_dir = home.join(hidden_dir_name);
    if hidden_dir.exists() {
        return hidden_dir;
    }

    // Support AICHV_HOME pointing directly to the tool config root (e.g. ~/.codex).
    if home.join(marker_subdir).exists() {
        return home;
    }

    hidden_dir
}

/// Claude Code: ~/.claude/
pub fn get_claude_config_dir() -> PathBuf {
    resolve_tool_dir(".claude", "projects")
}

/// Codex CLI: ~/.codex/
pub fn get_codex_config_dir() -> PathBuf {
    resolve_tool_dir(".codex", "sessions")
}

/// Gemini CLI: ~/.gemini/
pub fn get_gemini_dir() -> PathBuf {
    resolve_tool_dir(".gemini", "tmp")
}

/// OpenClaw: ~/.openclaw/
pub fn get_openclaw_dir() -> PathBuf {
    resolve_tool_dir(".openclaw", "agents")
}
