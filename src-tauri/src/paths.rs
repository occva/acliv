use std::ffi::OsStr;
use std::path::{Path, PathBuf};

fn env_dir(name: &str) -> Option<PathBuf> {
    let value = std::env::var_os(name)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

fn home_dir() -> PathBuf {
    if let Some(override_home) = env_dir("AICHV_HOME") {
        return override_home;
    }

    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn app_data_dir() -> PathBuf {
    if let Some(path) = env_dir("AICHV_HOME") {
        return path;
    }

    if let Some(path) = dirs::data_local_dir() {
        return path.join("ai-cli-history-viewer");
    }

    home_dir().join(".ai-cli-history-viewer")
}

fn normalize_data_dir(path: PathBuf, leaf: &str) -> PathBuf {
    if path
        .file_name()
        .map(|value| value == OsStr::new(leaf))
        .unwrap_or(false)
    {
        return path;
    }

    let nested = path.join(leaf);
    if nested.exists() {
        return nested;
    }

    path
}

fn resolve_tool_data_dir(hidden_dir_name: &str, data_subdir: &str) -> PathBuf {
    let home = home_dir();
    let hidden_dir = home.join(hidden_dir_name);
    let hidden_data_dir = hidden_dir.join(data_subdir);
    if hidden_data_dir.exists() {
        return hidden_data_dir;
    }

    normalize_data_dir(home, data_subdir)
}

fn resolve_provider_dir(env_name: &str, hidden_dir_name: &str, data_subdir: &str) -> PathBuf {
    if let Some(path) = env_dir(env_name) {
        return normalize_data_dir(path, data_subdir);
    }

    resolve_tool_data_dir(hidden_dir_name, data_subdir)
}

pub fn get_claude_projects_dir() -> PathBuf {
    resolve_provider_dir("AICHV_CLAUDE_DIR", ".claude", "projects")
}

pub fn get_codex_sessions_dir() -> PathBuf {
    resolve_provider_dir("AICHV_CODEX_DIR", ".codex", "sessions")
}

pub fn get_gemini_tmp_dir() -> PathBuf {
    resolve_provider_dir("AICHV_GEMINI_DIR", ".gemini", "tmp")
}

pub fn get_openclaw_agents_dir() -> PathBuf {
    resolve_provider_dir("AICHV_OPENCLAW_DIR", ".openclaw", "agents")
}

pub fn get_opencode_storage_dir() -> PathBuf {
    if let Some(path) = env_dir("AICHV_OPENCODE_DIR") {
        return normalize_data_dir(path, "storage");
    }

    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.trim().is_empty() {
            return normalize_data_dir(Path::new(&xdg).join("opencode"), "storage");
        }
    }

    dirs::home_dir()
        .map(|home| home.join(".local/share/opencode/storage"))
        .unwrap_or_else(|| PathBuf::from(".local/share/opencode/storage"))
}

pub fn get_provider_base_dir(provider_id: &str) -> Result<PathBuf, String> {
    let root = match provider_id {
        "claude" => get_claude_projects_dir(),
        "codex" => get_codex_sessions_dir(),
        "gemini" => get_gemini_tmp_dir(),
        "openclaw" => get_openclaw_agents_dir(),
        "opencode" => get_opencode_storage_dir(),
        _ => return Err(format!("Unsupported provider: {provider_id}")),
    };

    Ok(root)
}

pub fn get_search_index_dir() -> PathBuf {
    if let Some(path) = env_dir("AICHV_INDEX_DIR") {
        return path;
    }

    app_data_dir().join("search-index")
}

pub fn get_search_db_path() -> PathBuf {
    get_search_index_dir().join("search.db")
}
