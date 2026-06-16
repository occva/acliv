use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::paths::get_pi_sessions_dir;
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{
    extract_text, log_scan_error, normalize_title_candidate, parse_timestamp_to_ms, path_basename,
    read_head_tail_lines, truncate_summary,
};

const PROVIDER_ID: &str = "pi";

pub fn scan_sessions() -> Vec<SessionMeta> {
    let root = get_pi_sessions_dir();
    let mut files = Vec::new();
    collect_jsonl_files(&root, &mut files);

    files
        .into_iter()
        .filter_map(|path| parse_session(&path))
        .collect()
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open Pi session file: {e}"))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut tool_names = HashMap::new();

    for line in reader.lines() {
        let line = match line {
            Ok(value) => value,
            Err(_) => continue,
        };
        let value: Value = match serde_json::from_str(&line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };

        let event_type = value.get("type").and_then(Value::as_str);
        let has_legacy_message = value.get("role").is_some() || value.get("content").is_some();
        if event_type.is_some() && event_type != Some("message") {
            continue;
        }
        if event_type.is_none() && !has_legacy_message {
            continue;
        }

        append_pi_message(&mut messages, &mut tool_names, &value);
    }

    Ok(messages)
}

fn append_pi_message(
    messages: &mut Vec<SessionMessage>,
    tool_names: &mut HashMap<String, String>,
    value: &Value,
) {
    let role = extract_pi_role(value);
    let msg_uuid = value.get("id").and_then(Value::as_str).map(str::to_string);
    let parent_uuid = value
        .get("parentId")
        .or_else(|| value.get("parent_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let ts = extract_pi_timestamp(value);

    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
    else {
        let content = extract_pi_content(value);
        if !content.trim().is_empty() {
            messages.push(pi_message(
                msg_uuid,
                parent_uuid,
                role,
                "message",
                None,
                None,
                content,
                ts,
                Vec::new(),
            ));
        }
        return;
    };

    let Some(items) = content.as_array() else {
        let content = extract_text(content);
        if !content.trim().is_empty() {
            messages.push(pi_message(
                msg_uuid,
                parent_uuid,
                role,
                "message",
                None,
                None,
                content,
                ts,
                Vec::new(),
            ));
        }
        return;
    };

    if role == "tool" {
        let call_id = value
            .get("message")
            .and_then(|message| message.get("toolCallId"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let name = value
            .get("message")
            .and_then(|message| message.get("toolName"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| call_id.as_ref().and_then(|id| tool_names.get(id)).cloned());
        let content = extract_pi_content(value);
        if !content.trim().is_empty() {
            messages.push(pi_message(
                msg_uuid,
                parent_uuid,
                "tool".to_string(),
                "tool_result",
                name.clone(),
                call_id,
                content,
                ts,
                name.into_iter().collect(),
            ));
        }
        return;
    }

    for item in items {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("text");
        match item_type {
            "text" => {
                let content = item
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if content.is_empty() {
                    continue;
                }
                messages.push(pi_message(
                    msg_uuid.clone(),
                    parent_uuid.clone(),
                    role.clone(),
                    "message",
                    None,
                    None,
                    content,
                    ts,
                    Vec::new(),
                ));
            }
            "thinking" => {
                let content = item
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if content.is_empty() {
                    continue;
                }
                messages.push(pi_message(
                    msg_uuid.clone(),
                    parent_uuid.clone(),
                    "assistant".to_string(),
                    "thinking",
                    None,
                    None,
                    content,
                    ts,
                    Vec::new(),
                ));
            }
            "toolCall" => {
                let name = item.get("name").and_then(Value::as_str).map(str::to_string);
                let call_id = item.get("id").and_then(Value::as_str).map(str::to_string);
                if let (Some(call_id), Some(name)) = (call_id.clone(), name.clone()) {
                    tool_names.insert(call_id, name);
                }

                let content = format_pi_arguments(item.get("arguments"));
                messages.push(pi_message(
                    msg_uuid.clone(),
                    parent_uuid.clone(),
                    "assistant".to_string(),
                    "tool_use",
                    name.clone(),
                    call_id,
                    content,
                    ts,
                    name.into_iter().collect(),
                ));
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn pi_message(
    msg_uuid: Option<String>,
    parent_uuid: Option<String>,
    role: String,
    kind: impl Into<String>,
    name: Option<String>,
    call_id: Option<String>,
    content: String,
    ts: Option<i64>,
    tool_names: Vec<String>,
) -> SessionMessage {
    SessionMessage {
        msg_uuid,
        parent_uuid,
        role,
        kind: kind.into(),
        name,
        call_id,
        content,
        ts,
        is_sidechain: false,
        tool_names,
    }
}

fn format_pi_arguments(arguments: Option<&Value>) -> String {
    let Some(arguments) = arguments else {
        return String::new();
    };

    if let Some(text) = arguments.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            return serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| text.to_string());
        }
        return text.to_string();
    }

    serde_json::to_string_pretty(arguments).unwrap_or_default()
}

pub fn delete_session(_root: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    let meta = parse_session(path)
        .ok_or_else(|| format!("Failed to parse Pi session metadata: {}", path.display()))?;

    if meta.session_id != session_id {
        return Err(format!(
            "Pi session ID mismatch: expected {session_id}, found {}",
            meta.session_id
        ));
    }

    std::fs::remove_file(path)
        .map_err(|e| format!("Failed to delete Pi session file {}: {e}", path.display()))?;

    Ok(true)
}

fn parse_session(path: &Path) -> Option<SessionMeta> {
    let (head, tail) = read_head_tail_lines(path, 40, 30).ok()?;

    let mut session_id = None;
    let mut cwd = None;
    let mut model = None;
    let mut created_at = None;
    let mut title = None;

    for line in &head {
        let value: Value = match serde_json::from_str(line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };

        session_id = session_id.or_else(|| extract_pi_session_id(&value));
        cwd = cwd.or_else(|| extract_pi_cwd(&value));
        model = model.or_else(|| extract_pi_model(&value));
        created_at = created_at.or_else(|| extract_pi_timestamp(&value));

        if title.is_none() && extract_pi_role(&value) == "user" {
            title = normalize_title_candidate(&extract_pi_content(&value), 160);
        }
    }

    let mut last_active_at = None;
    let mut summary = None;
    for line in tail.iter().rev() {
        let value: Value = match serde_json::from_str(line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };

        last_active_at = last_active_at.or_else(|| extract_pi_timestamp(&value));
        if summary.is_none() {
            let content = extract_pi_content(&value);
            if !content.trim().is_empty() {
                summary = Some(content);
            }
        }

        if last_active_at.is_some() && summary.is_some() {
            break;
        }
    }

    let session_id = session_id
        .or_else(|| path.file_stem()?.to_str().map(str::to_string))
        .filter(|value| !value.trim().is_empty())?;
    let project_dir = cwd.or_else(|| {
        path.parent()
            .and_then(|parent| parent.to_str())
            .map(str::to_string)
    });
    let title = title.or_else(|| project_dir.as_deref().and_then(path_basename));
    let summary = summary.map(|text| truncate_summary(&text, 160));

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title,
        summary,
        project_dir: project_dir.clone(),
        cwd: project_dir,
        model,
        created_at,
        last_active_at,
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: None,
    })
}

fn extract_pi_session_id(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) == Some("session") {
        return value.get("id").and_then(Value::as_str).map(str::to_string);
    }

    value
        .get("sessionId")
        .or_else(|| value.get("session_id"))
        .or_else(|| value.get("conversationId"))
        .or_else(|| value.get("threadId"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn extract_pi_cwd(value: &Value) -> Option<String> {
    value
        .get("cwd")
        .or_else(|| value.get("projectDir"))
        .or_else(|| value.get("project_dir"))
        .or_else(|| value.get("workspace"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn extract_pi_model(value: &Value) -> Option<String> {
    value
        .get("model")
        .or_else(|| value.get("modelId"))
        .or_else(|| value.get("model_slug"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn extract_pi_role(value: &Value) -> String {
    value
        .get("role")
        .or_else(|| value.get("message").and_then(|message| message.get("role")))
        .and_then(Value::as_str)
        .map(normalize_pi_role)
        .unwrap_or_else(|| "unknown".to_string())
}

fn normalize_pi_role(role: &str) -> String {
    match role {
        "human" => "user".to_string(),
        "ai" | "model" => "assistant".to_string(),
        "toolResult" | "tool_result" => "tool".to_string(),
        other => other.to_string(),
    }
}

fn extract_pi_content(value: &Value) -> String {
    if let Some(content) = value.get("content") {
        return extract_text(content);
    }

    if let Some(text) = value.get("text").and_then(Value::as_str) {
        return text.to_string();
    }

    if let Some(message) = value.get("message") {
        if let Some(content) = message.get("content") {
            return extract_text(content);
        }
        if let Some(text) = message.get("text").and_then(Value::as_str) {
            return text.to_string();
        }
    }

    String::new()
}

fn extract_pi_timestamp(value: &Value) -> Option<i64> {
    value
        .get("timestamp")
        .or_else(|| value.get("createdAt"))
        .or_else(|| value.get("created_at"))
        .or_else(|| value.get("time"))
        .and_then(parse_pi_timestamp_value)
}

fn parse_pi_timestamp_value(value: &Value) -> Option<i64> {
    if let Some(ms) = parse_timestamp_to_ms(value) {
        return Some(ms);
    }

    let raw = value.as_i64()?;
    if raw > 10_000_000_000 {
        Some(raw)
    } else {
        raw.checked_mul(1000)
    }
}

fn collect_jsonl_files(root: &Path, files: &mut Vec<PathBuf>) {
    if !root.exists() {
        return;
    }

    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) => {
            log_scan_error(PROVIDER_ID, root, &err);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn load_messages_preserves_tree_links() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let content = [
            r#"{"id":"root","sessionId":"session-1","parentId":null,"role":"user","content":"start","timestamp":"2026-06-01T00:00:00Z"}"#,
            r#"{"id":"branch-a","parentId":"root","role":"assistant","content":"answer a","timestamp":"2026-06-01T00:00:01Z"}"#,
            r#"{"id":"branch-b","parentId":"root","role":"assistant","content":"answer b","timestamp":"2026-06-01T00:00:02Z"}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write pi fixture");

        let messages = load_messages(&path).expect("load messages");

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1].msg_uuid.as_deref(), Some("branch-a"));
        assert_eq!(messages[1].parent_uuid.as_deref(), Some("root"));
        assert_eq!(messages[2].parent_uuid.as_deref(), Some("root"));
    }

    #[test]
    fn parse_session_uses_session_id_and_cwd() {
        let dir = tempdir().expect("tempdir");
        let project = dir.path().join("D--code");
        fs::create_dir_all(&project).expect("create project");
        let path = project.join("session.jsonl");
        let content = [
            r#"{"type":"session","version":3,"id":"session-1","timestamp":"2026-06-01T00:00:00Z","cwd":"D:\\code"}"#,
            r#"{"type":"model_change","id":"model-1","parentId":null,"timestamp":"2026-06-01T00:00:00Z","provider":"deepseek","modelId":"pi-model"}"#,
            r#"{"id":"msg-1","sessionId":"session-1","cwd":"D:\\code","model":"pi-model","role":"user","content":"build provider","timestamp":1780272000}"#,
            r#"{"id":"msg-2","parentId":"msg-1","role":"assistant","content":"done","timestamp":1780272001000}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write pi fixture");

        let meta = parse_session(&path).expect("parse session");

        assert_eq!(meta.provider_id, "pi");
        assert_eq!(meta.session_id, "session-1");
        assert_eq!(meta.cwd.as_deref(), Some("D:\\code"));
        assert_eq!(meta.model.as_deref(), Some("pi-model"));
        assert_eq!(meta.created_at, Some(1_780_272_000_000));
        assert_eq!(meta.last_active_at, Some(1_780_272_001_000));
        assert_eq!(meta.title.as_deref(), Some("build provider"));
    }

    #[test]
    fn load_messages_expands_tool_calls_and_results() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let content = [
            r#"{"type":"message","id":"user-1","parentId":null,"timestamp":"2026-06-01T00:00:00Z","message":{"role":"user","content":[{"type":"text","text":"read file"}]}}"#,
            r#"{"type":"message","id":"assistant-1","parentId":"user-1","timestamp":"2026-06-01T00:00:01Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Need to read."},{"type":"toolCall","id":"call_1","name":"read","arguments":{"path":"README.md"}}]}}"#,
            r#"{"type":"message","id":"tool-1","parentId":"assistant-1","timestamp":"2026-06-01T00:00:02Z","message":{"role":"toolResult","toolCallId":"call_1","toolName":"read","content":[{"type":"text","text":"hello"}]}}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write pi fixture");

        let messages = load_messages(&path).expect("load messages");

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[1].kind, "thinking");
        assert_eq!(messages[2].kind, "tool_use");
        assert_eq!(messages[2].name.as_deref(), Some("read"));
        assert_eq!(messages[2].call_id.as_deref(), Some("call_1"));
        assert!(messages[2].content.contains("README.md"));
        assert_eq!(messages[3].role, "tool");
        assert_eq!(messages[3].kind, "tool_result");
        assert_eq!(messages[3].call_id.as_deref(), Some("call_1"));
    }
}
