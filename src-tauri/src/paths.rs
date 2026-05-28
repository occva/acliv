use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const PROVIDERS: [ProviderDefinition; 5] = [
    ProviderDefinition {
        id: "claude",
        name: "Claude",
        env_var: "ACLIV_CLAUDE_DIR",
        hidden_dir_name: ".claude",
        data_subdir: "projects",
        default_kind: ProviderDefaultKind::HomeHidden,
    },
    ProviderDefinition {
        id: "codex",
        name: "Codex",
        env_var: "ACLIV_CODEX_DIR",
        hidden_dir_name: ".codex",
        data_subdir: "sessions",
        default_kind: ProviderDefaultKind::HomeHidden,
    },
    ProviderDefinition {
        id: "gemini",
        name: "Gemini",
        env_var: "ACLIV_GEMINI_DIR",
        hidden_dir_name: ".gemini",
        data_subdir: "tmp",
        default_kind: ProviderDefaultKind::HomeHidden,
    },
    ProviderDefinition {
        id: "openclaw",
        name: "OpenClaw",
        env_var: "ACLIV_OPENCLAW_DIR",
        hidden_dir_name: ".openclaw",
        data_subdir: "agents",
        default_kind: ProviderDefaultKind::HomeHidden,
    },
    ProviderDefinition {
        id: "opencode",
        name: "OpenCode",
        env_var: "ACLIV_OPENCODE_DIR",
        hidden_dir_name: "",
        data_subdir: "storage",
        default_kind: ProviderDefaultKind::OpenCode,
    },
];

#[derive(Debug, Clone, Copy)]
enum ProviderDefaultKind {
    HomeHidden,
    OpenCode,
}

#[derive(Debug, Clone, Copy)]
struct ProviderDefinition {
    id: &'static str,
    name: &'static str,
    env_var: &'static str,
    hidden_dir_name: &'static str,
    data_subdir: &'static str,
    default_kind: ProviderDefaultKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPathInfo {
    pub provider_id: String,
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub readable: bool,
    pub env_var: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_path: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PathConfig {
    #[serde(default)]
    provider_overrides: HashMap<String, String>,
}

fn env_dir(name: &str) -> Option<PathBuf> {
    let value = std::env::var_os(name)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

fn home_dir() -> PathBuf {
    if let Some(override_home) = env_dir("ACLIV_HOME") {
        return override_home;
    }

    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn app_data_dir() -> PathBuf {
    if let Some(path) = env_dir("ACLIV_HOME") {
        return path;
    }

    if let Some(path) = dirs::data_local_dir() {
        return path.join("acliv");
    }

    home_dir().join(".acliv")
}

fn config_path() -> PathBuf {
    app_data_dir().join("config.json")
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

    hidden_data_dir
}

fn resolve_provider_dir(env_name: &str, hidden_dir_name: &str, data_subdir: &str) -> PathBuf {
    if let Some(path) = env_dir(env_name) {
        return normalize_data_dir(path, data_subdir);
    }

    resolve_tool_data_dir(hidden_dir_name, data_subdir)
}

fn provider_definition(provider_id: &str) -> Option<&'static ProviderDefinition> {
    PROVIDERS.iter().find(|provider| provider.id == provider_id)
}

fn load_config() -> PathConfig {
    let path = config_path();
    let Ok(raw) = fs::read_to_string(path) else {
        return PathConfig::default();
    };

    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_config(config: &PathConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create ACLIV config directory: {e}"))?;
    }

    let raw = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize ACLIV config: {e}"))?;
    fs::write(&path, format!("{raw}\n"))
        .map_err(|e| format!("Failed to write ACLIV config {}: {e}", path.display()))
}

fn resolve_provider_default(definition: &ProviderDefinition) -> PathBuf {
    match definition.default_kind {
        ProviderDefaultKind::HomeHidden => {
            resolve_tool_data_dir(definition.hidden_dir_name, definition.data_subdir)
        }
        ProviderDefaultKind::OpenCode => {
            if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
                if !xdg.trim().is_empty() {
                    return normalize_data_dir(Path::new(&xdg).join("opencode"), "storage");
                }
            }

            dirs::home_dir()
                .map(|home| home.join(".local/share/opencode/storage"))
                .unwrap_or_else(|| PathBuf::from(".local/share/opencode/storage"))
        }
    }
}

fn resolve_provider_path(definition: &ProviderDefinition) -> (PathBuf, String, Option<PathBuf>) {
    if let Some(path) = env_dir(definition.env_var) {
        return (
            normalize_data_dir(path, definition.data_subdir),
            "env".to_string(),
            None,
        );
    }

    let config = load_config();
    if let Some(path) = config
        .provider_overrides
        .get(definition.id)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let override_path = normalize_data_dir(PathBuf::from(path), definition.data_subdir);
        return (
            override_path.clone(),
            "override".to_string(),
            Some(override_path),
        );
    }

    (
        resolve_provider_default(definition),
        "default".to_string(),
        None,
    )
}

pub fn list_provider_paths() -> Vec<ProviderPathInfo> {
    PROVIDERS
        .iter()
        .map(|definition| {
            let (path, source, override_path) = resolve_provider_path(definition);
            let exists = path.exists();
            let readable = path.read_dir().is_ok();
            ProviderPathInfo {
                provider_id: definition.id.to_string(),
                name: definition.name.to_string(),
                path: path.to_string_lossy().to_string(),
                exists,
                readable,
                env_var: definition.env_var.to_string(),
                source,
                override_path: override_path.map(|path| path.to_string_lossy().to_string()),
            }
        })
        .collect()
}

pub fn set_provider_path(provider_id: &str, path: &str) -> Result<ProviderPathInfo, String> {
    let definition = provider_definition(provider_id)
        .ok_or_else(|| format!("Unsupported provider: {provider_id}"))?;
    if env_dir(definition.env_var).is_some() {
        return Err(format!(
            "{} is set and takes precedence over saved provider paths",
            definition.env_var
        ));
    }

    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Path is required".to_string());
    }

    let normalized = normalize_data_dir(PathBuf::from(trimmed), definition.data_subdir);
    if !normalized.exists() {
        return Err(format!("Path not found: {}", normalized.display()));
    }
    if !normalized.is_dir() {
        return Err(format!("Path is not a directory: {}", normalized.display()));
    }

    let mut config = load_config();
    config.provider_overrides.insert(
        definition.id.to_string(),
        normalized.to_string_lossy().to_string(),
    );
    save_config(&config)?;

    Ok(list_provider_paths()
        .into_iter()
        .find(|info| info.provider_id == definition.id)
        .expect("saved provider should be listed"))
}

pub fn reset_provider_path(provider_id: &str) -> Result<ProviderPathInfo, String> {
    let definition = provider_definition(provider_id)
        .ok_or_else(|| format!("Unsupported provider: {provider_id}"))?;
    let mut config = load_config();
    config.provider_overrides.remove(definition.id);
    save_config(&config)?;

    Ok(list_provider_paths()
        .into_iter()
        .find(|info| info.provider_id == definition.id)
        .expect("reset provider should be listed"))
}

pub fn get_claude_projects_dir() -> PathBuf {
    get_provider_base_dir("claude")
        .unwrap_or_else(|_| resolve_provider_dir("ACLIV_CLAUDE_DIR", ".claude", "projects"))
}

pub fn get_codex_sessions_dir() -> PathBuf {
    get_provider_base_dir("codex")
        .unwrap_or_else(|_| resolve_provider_dir("ACLIV_CODEX_DIR", ".codex", "sessions"))
}

pub fn get_gemini_tmp_dir() -> PathBuf {
    get_provider_base_dir("gemini")
        .unwrap_or_else(|_| resolve_provider_dir("ACLIV_GEMINI_DIR", ".gemini", "tmp"))
}

pub fn get_openclaw_agents_dir() -> PathBuf {
    get_provider_base_dir("openclaw")
        .unwrap_or_else(|_| resolve_provider_dir("ACLIV_OPENCLAW_DIR", ".openclaw", "agents"))
}

pub fn get_opencode_storage_dir() -> PathBuf {
    get_provider_base_dir("opencode").unwrap_or_else(|_| {
        resolve_provider_default(provider_definition("opencode").expect("opencode provider"))
    })
}

pub fn get_provider_base_dir(provider_id: &str) -> Result<PathBuf, String> {
    let definition = provider_definition(provider_id)
        .ok_or_else(|| format!("Unsupported provider: {provider_id}"))?;
    Ok(resolve_provider_path(definition).0)
}

pub fn get_search_index_dir() -> PathBuf {
    if let Some(path) = env_dir("ACLIV_INDEX_DIR") {
        return path;
    }

    app_data_dir().join("search-index")
}

pub fn get_search_db_path() -> PathBuf {
    get_search_index_dir().join("search.db")
}
