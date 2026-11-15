pub mod providers;

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use providers::{claude, codex, dsh, gemini, openclaw, opencode, pi};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub provider_id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_command: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "is_zero_u32")]
    pub unknown_records: u32,
    #[serde(skip)]
    pub source_mtime: Option<i64>,
    #[serde(skip)]
    pub source_size: Option<i64>,
    #[serde(skip)]
    pub source_rank: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_uuid: Option<String>,
    pub role: String,
    #[serde(skip_serializing_if = "is_message_kind")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<i64>,
    #[serde(skip_serializing_if = "is_false")]
    pub is_sidechain: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub is_error: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub truncated: bool,
}

impl SessionMessage {
    pub fn plain(role: String, content: String, ts: Option<i64>) -> Self {
        Self {
            msg_uuid: None,
            parent_uuid: None,
            role,
            kind: "message".to_string(),
            name: None,
            call_id: None,
            content,
            ts,
            is_sidechain: false,
            tool_names: Vec::new(),
            model: None,
            is_error: false,
            truncated: false,
        }
    }

    pub fn structured(
        role: String,
        kind: impl Into<String>,
        name: Option<String>,
        call_id: Option<String>,
        content: String,
        ts: Option<i64>,
    ) -> Self {
        Self {
            msg_uuid: None,
            parent_uuid: None,
            role,
            kind: kind.into(),
            name,
            call_id,
            content,
            ts,
            is_sidechain: false,
            tool_names: Vec::new(),
            model: None,
            is_error: false,
            truncated: false,
        }
    }

    pub fn searchable_text(&self) -> &str {
        if self.kind == "message" {
            self.content.as_str()
        } else {
            ""
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}
fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn is_message_kind(value: &String) -> bool {
    value == "message"
}

#[derive(Debug)]
pub(crate) struct SessionScan {
    pub sessions: Vec<SessionMeta>,
    pub failed_providers: Vec<String>,
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    scan_sessions_with_status().sessions
}

pub(crate) fn scan_sessions_with_status() -> SessionScan {
    let joined = std::thread::scope(|s| {
        let h1 = s.spawn(|| Ok::<_, String>(codex::scan_sessions()));
        let h2 = s.spawn(|| Ok::<_, String>(claude::scan_sessions()));
        let h3 = s.spawn(opencode::scan_sessions);
        let h4 = s.spawn(openclaw::scan_sessions);
        let h5 = s.spawn(|| Ok::<_, String>(gemini::scan_sessions()));
        let h6 = s.spawn(|| Ok::<_, String>(pi::scan_sessions()));
        let h7 = s.spawn(|| Ok::<_, String>(pi::scan_omp_sessions()));
        let h8 = s.spawn(|| Ok::<_, String>(dsh::scan_sessions()));
        (
            h1.join(),
            h2.join(),
            h3.join(),
            h4.join(),
            h5.join(),
            h6.join(),
            h7.join(),
            h8.join(),
        )
    });

    let mut failed_providers = Vec::new();
    let mut groups = Vec::new();
    for (provider, result) in [
        ("codex", joined.0),
        ("claude", joined.1),
        ("opencode", joined.2),
        ("openclaw", joined.3),
        ("gemini", joined.4),
        ("pi", joined.5),
        ("omp", joined.6),
        ("dsh", joined.7),
    ] {
        match result {
            Ok(Ok(sessions)) => groups.push(sessions),
            Ok(Err(error)) => {
                eprintln!("[WARN] provider={provider} scan failed: {error}");
                failed_providers.push(provider.to_string());
            }
            Err(_) => {
                eprintln!("[WARN] provider={provider} scan panicked");
                failed_providers.push(provider.to_string());
            }
        }
    }

    let mut by_native_id = HashMap::<(String, String), SessionMeta>::new();
    for mut session in groups.into_iter().flatten() {
        hydrate_source_fingerprint(&mut session);
        let key = (session.provider_id.clone(), session.session_id.clone());
        match by_native_id.get_mut(&key) {
            Some(current) if should_replace_copy(current, &session) => *current = session,
            Some(_) => {}
            None => {
                by_native_id.insert(key, session);
            }
        }
    }

    let mut sessions: Vec<_> = by_native_id.into_values().collect();
    sessions.sort_by(|a, b| {
        let a_ts = a.last_active_at.or(a.created_at).unwrap_or(0);
        let b_ts = b.last_active_at.or(b.created_at).unwrap_or(0);
        b_ts.cmp(&a_ts)
    });
    SessionScan {
        sessions,
        failed_providers,
    }
}

fn hydrate_source_fingerprint(session: &mut SessionMeta) {
    if session.source_mtime.is_some() && session.source_size.is_some() {
        return;
    }
    let Some(source_path) = session.source_path.as_deref() else {
        return;
    };
    let backing_path = source_path
        .rsplit_once('#')
        .map_or(source_path, |(path, _)| path);
    let Ok(metadata) = std::fs::metadata(backing_path) else {
        return;
    };
    session.source_mtime.get_or_insert_with(|| {
        metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|value| i64::try_from(value.as_millis()).ok())
            .unwrap_or(0)
    });
    session
        .source_size
        .get_or_insert_with(|| i64::try_from(metadata.len()).unwrap_or(i64::MAX));
}

fn should_replace_copy(current: &SessionMeta, candidate: &SessionMeta) -> bool {
    (
        candidate.source_rank,
        candidate.source_mtime.unwrap_or(0),
        candidate.source_size.unwrap_or(0),
    ) > (
        current.source_rank,
        current.source_mtime.unwrap_or(0),
        current.source_size.unwrap_or(0),
    )
}

pub fn load_messages(provider_id: &str, source_path: &str) -> Result<Vec<SessionMessage>, String> {
    let path = Path::new(source_path);
    match provider_id {
        "codex" => codex::load_messages(path),
        "claude" => claude::load_messages(path),
        "opencode" => opencode::load_messages(path),
        "openclaw" => openclaw::load_messages(path),
        "gemini" => gemini::load_messages(path),
        "pi" => pi::load_messages(path),
        "omp" => pi::load_messages(path),
        "dsh" => dsh::load_messages(path),
        _ => Err(format!("Unsupported provider: {provider_id}")),
    }
}

pub fn delete_session(
    provider_id: &str,
    session_id: &str,
    source_path: &str,
) -> Result<bool, String> {
    let root = crate::paths::get_provider_base_dir(provider_id)?;
    delete_session_with_root(provider_id, session_id, Path::new(source_path), &root)
}

fn delete_session_with_root(
    provider_id: &str,
    session_id: &str,
    source_path: &Path,
    root: &Path,
) -> Result<bool, String> {
    let validated_root = canonicalize_existing_path(root, "session root")?;
    let source_text = source_path.to_string_lossy();
    let virtual_backing = source_text
        .rsplit_once('#')
        .filter(|(path, id)| !path.is_empty() && !id.is_empty())
        .map(|(path, _)| PathBuf::from(path));
    let validated_source = canonicalize_existing_path(
        virtual_backing.as_deref().unwrap_or(source_path),
        "session source",
    )?;
    let inside_root = validated_source.starts_with(&validated_root);
    let inside_codex_archive = provider_id == "codex"
        && validated_root
            .parent()
            .map(|home| validated_source.starts_with(home.join("archived_sessions")))
            .unwrap_or(false);
    if !inside_root && !inside_codex_archive {
        return Err(format!(
            "Session source path is outside provider root: {}",
            source_path.display()
        ));
    }
    let dispatch_source = if virtual_backing.is_some() {
        source_path
    } else {
        validated_source.as_path()
    };

    match provider_id {
        "codex" => codex::delete_session(&validated_root, dispatch_source, session_id),
        "claude" => claude::delete_session(&validated_root, dispatch_source, session_id),
        "opencode" => opencode::delete_session(&validated_root, dispatch_source, session_id),
        "openclaw" => openclaw::delete_session(&validated_root, dispatch_source, session_id),
        "gemini" => gemini::delete_session(&validated_root, dispatch_source, session_id),
        "pi" => pi::delete_session(&validated_root, dispatch_source, session_id),
        "omp" => pi::delete_omp_session(&validated_root, dispatch_source, session_id),
        "dsh" => dsh::delete_session(&validated_root, dispatch_source, session_id),
        _ => Err(format!("Unsupported provider: {provider_id}")),
    }
}

fn canonicalize_existing_path(path: &Path, label: &str) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("{label} not found: {}", path.display()));
    }

    path.canonicalize()
        .map_err(|e| format!("Failed to resolve {label} {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_source_path_outside_provider_root() {
        let root = tempdir().expect("tempdir");
        let outside = tempdir().expect("tempdir");
        let source = outside.path().join("session.jsonl");
        std::fs::write(&source, "{}").expect("write source");

        let err = delete_session_with_root("codex", "session-1", &source, root.path())
            .expect_err("expected outside-root path to be rejected");

        assert!(err.contains("outside provider root"));
    }

    #[test]
    fn rejects_missing_source_path() {
        let root = tempdir().expect("tempdir");
        let missing = root.path().join("missing.jsonl");

        let err = delete_session_with_root("codex", "session-1", &missing, root.path())
            .expect_err("expected missing source path to fail");

        assert!(err.contains("session source not found"));
    }
}
