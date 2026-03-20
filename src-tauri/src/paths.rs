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

/// Claude Code: ~/.claude/
pub fn get_claude_config_dir() -> PathBuf {
    home_dir().join(".claude")
}

/// Codex CLI: ~/.codex/
pub fn get_codex_config_dir() -> PathBuf {
    home_dir().join(".codex")
}

/// Gemini CLI: ~/.gemini/
pub fn get_gemini_dir() -> PathBuf {
    home_dir().join(".gemini")
}

/// OpenClaw: ~/.openclaw/
pub fn get_openclaw_dir() -> PathBuf {
    home_dir().join(".openclaw")
}
