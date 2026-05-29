use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::paths::get_codex_sessions_dir;
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{
    extract_text, log_scan_error, normalize_title_candidate, parse_timestamp_to_ms, path_basename,
    read_head_tail_lines, truncate_summary,
};

const PROVIDER_ID: &str = "codex";

static UUID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
        .unwrap()
});
static ENVIRONMENT_CONTEXT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<environment_context>[\s\S]*?</environment_context>").unwrap()
});
static USER_SHELL_COMMAND_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<user_shell_command>[\s\S]*?</user_shell_command>").unwrap()
});
static AGENTS_INSTRUCTIONS_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?im)^\s*#\s*(?:AGENTS|CLAUDE|AGENT)\.md instructions[^\n]*(?:\n|$)").unwrap()
});

pub fn scan_sessions() -> Vec<SessionMeta> {
    let root = get_codex_sessions_dir();
    let title_index = load_session_title_index(&root);
    let history_title_index = load_history_title_index(&root);
    let mut files = Vec::new();
    collect_jsonl_files(&root, &mut files);

    let mut sessions = Vec::new();
    for path in files {
        if let Some(meta) = parse_session(&path, &title_index, &history_title_index) {
            sessions.push(meta);
        }
    }

    sessions
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open session file: {e}"))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut call_names = HashMap::new();

    for line in reader.lines() {
        let line = match line {
            Ok(value) => value,
            Err(_) => continue,
        };
        let value: Value = match serde_json::from_str(&line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };

        if value.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }

        let payload = match value.get("payload") {
            Some(payload) => payload,
            None => continue,
        };
        let payload_type = payload
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let ts = value.get("timestamp").and_then(parse_timestamp_to_ms);

        match payload_type {
            "message" => {
                let role = payload
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let content = extract_message_content(payload.get("content"), &role);
                if content.trim().is_empty() {
                    continue;
                }
                messages.push(SessionMessage::plain(role, content, ts));
            }
            "function_call" => {
                let name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string());
                let call_id = payload
                    .get("call_id")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string());
                if let (Some(call_id), Some(name)) = (call_id.clone(), name.clone()) {
                    call_names.insert(call_id, name);
                }

                let content = format_function_call_content(payload);
                if content.trim().is_empty() {
                    continue;
                }

                messages.push(SessionMessage::structured(
                    "assistant".to_string(),
                    "function_call",
                    name,
                    call_id,
                    content,
                    ts,
                ));
            }
            "function_call_output" => {
                let call_id = payload
                    .get("call_id")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string());
                let name = call_id
                    .as_ref()
                    .and_then(|value| call_names.get(value))
                    .cloned();
                let content = payload
                    .get("output")
                    .and_then(Value::as_str)
                    .map(|value| value.trim().to_string())
                    .unwrap_or_default();
                if content.is_empty() {
                    continue;
                }

                messages.push(SessionMessage::structured(
                    "tool".to_string(),
                    "function_call_output",
                    name,
                    call_id,
                    content,
                    ts,
                ));
            }
            _ => {}
        }
    }

    Ok(messages)
}

fn extract_message_content(content: Option<&Value>, role: &str) -> String {
    if role.eq_ignore_ascii_case("user") || role.eq_ignore_ascii_case("human") {
        content.map(extract_user_message_text).unwrap_or_default()
    } else {
        content.map(extract_text).unwrap_or_default()
    }
}

fn extract_user_message_text(content: &Value) -> String {
    match content {
        Value::String(text) => clean_user_message_text(text),
        Value::Array(items) => items
            .iter()
            .filter_map(extract_user_message_text_from_item)
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                return clean_user_message_text(text);
            }
            if let Some(text) = map.get("input_text").and_then(Value::as_str) {
                return clean_user_message_text(text);
            }
            if let Some(text) = map.get("output_text").and_then(Value::as_str) {
                return clean_user_message_text(text);
            }
            map.get("content")
                .map(extract_user_message_text)
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

fn extract_user_message_text_from_item(item: &Value) -> Option<String> {
    let text = match item {
        Value::String(text) => clean_user_message_text(text),
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                clean_user_message_text(text)
            } else if let Some(text) = map.get("input_text").and_then(Value::as_str) {
                clean_user_message_text(text)
            } else if let Some(text) = map.get("output_text").and_then(Value::as_str) {
                clean_user_message_text(text)
            } else if let Some(content) = map.get("content") {
                extract_user_message_text(content)
            } else {
                String::new()
            }
        }
        _ => String::new(),
    };

    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn clean_user_message_text(text: &str) -> String {
    let without_agents = AGENTS_INSTRUCTIONS_LINE_RE.replace_all(text, " ");
    let without_env = ENVIRONMENT_CONTEXT_RE.replace_all(without_agents.as_ref(), " ");
    let without_shell_command = USER_SHELL_COMMAND_RE.replace_all(without_env.as_ref(), " ");
    without_shell_command.trim().to_string()
}

fn format_function_call_content(payload: &Value) -> String {
    let raw_arguments = payload
        .get("arguments")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if raw_arguments.is_empty() {
        return String::new();
    }

    if let Ok(parsed) = serde_json::from_str::<Value>(raw_arguments) {
        if let Ok(pretty) = serde_json::to_string_pretty(&parsed) {
            return pretty;
        }
    }

    raw_arguments.to_string()
}

pub fn delete_session(_root: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    let meta = parse_session(path, &HashMap::new(), &HashMap::new())
        .ok_or_else(|| format!("Failed to parse Codex session metadata: {}", path.display()))?;

    if meta.session_id != session_id {
        return Err(format!(
            "Codex session ID mismatch: expected {session_id}, found {}",
            meta.session_id
        ));
    }

    std::fs::remove_file(path).map_err(|e| {
        format!(
            "Failed to delete Codex session file {}: {e}",
            path.display()
        )
    })?;

    Ok(true)
}

fn parse_session(
    path: &Path,
    title_index: &HashMap<String, String>,
    history_title_index: &HashMap<String, String>,
) -> Option<SessionMeta> {
    let (head, tail) = read_head_tail_lines(path, 40, 30).ok()?;

    let mut session_id: Option<String> = None;
    let mut project_dir: Option<String> = None;
    let mut model: Option<String> = None;
    let mut created_at: Option<i64> = None;
    let mut fallback_title: Option<String> = None;

    // Extract metadata from head lines
    for line in &head {
        let value: Value = match serde_json::from_str(line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if created_at.is_none() {
            created_at = value.get("timestamp").and_then(parse_timestamp_to_ms);
        }
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            if let Some(payload) = value.get("payload") {
                if session_id.is_none() {
                    session_id = payload
                        .get("id")
                        .and_then(Value::as_str)
                        .map(|s| s.to_string());
                }
                if project_dir.is_none() {
                    project_dir = payload
                        .get("cwd")
                        .and_then(Value::as_str)
                        .map(|s| s.to_string());
                }
                if model.is_none() {
                    model = payload
                        .get("model")
                        .and_then(Value::as_str)
                        .or_else(|| payload.get("model_slug").and_then(Value::as_str))
                        .map(|s| s.to_string());
                }
                if let Some(ts) = payload.get("timestamp").and_then(parse_timestamp_to_ms) {
                    created_at.get_or_insert(ts);
                }
            }
        }
        if fallback_title.is_none() {
            fallback_title = extract_first_user_title(&value);
        }
    }

    // Extract last_active_at and summary from tail lines (reverse order)
    let mut last_active_at: Option<i64> = None;
    let mut summary: Option<String> = None;

    for line in tail.iter().rev() {
        let value: Value = match serde_json::from_str(line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if last_active_at.is_none() {
            last_active_at = value.get("timestamp").and_then(parse_timestamp_to_ms);
        }
        if summary.is_none() && value.get("type").and_then(Value::as_str) == Some("response_item") {
            if let Some(payload) = value.get("payload") {
                if payload.get("type").and_then(Value::as_str) == Some("message") {
                    let text = payload.get("content").map(extract_text).unwrap_or_default();
                    if !text.trim().is_empty() {
                        summary = Some(text);
                    }
                }
            }
        }
        if last_active_at.is_some() && summary.is_some() {
            break;
        }
    }

    let session_id = session_id.or_else(|| infer_session_id_from_filename(path));
    let session_id = session_id?;

    let title = title_index
        .get(&session_id)
        .cloned()
        .or_else(|| history_title_index.get(&session_id).cloned())
        .or(fallback_title)
        .or_else(|| {
            project_dir
                .as_deref()
                .and_then(path_basename)
                .map(|value| value.to_string())
        });

    let summary = summary.map(|text| truncate_summary(&text, 160));

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title,
        summary,
        project_dir: project_dir.clone(),
        cwd: project_dir.clone(),
        model,
        created_at,
        last_active_at,
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: Some(format!("codex resume {session_id}")),
    })
}

fn load_session_title_index(root: &Path) -> HashMap<String, String> {
    let Some(index_path) = root.parent().map(|path| path.join("session_index.jsonl")) else {
        return HashMap::new();
    };
    let file = match File::open(index_path) {
        Ok(file) => file,
        Err(_) => return HashMap::new(),
    };

    let mut titles = HashMap::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(session_id) = value.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(title) = value
            .get("thread_name")
            .and_then(Value::as_str)
            .and_then(normalize_codex_title_value)
        else {
            continue;
        };
        titles.insert(session_id.to_string(), title);
    }

    titles
}

fn load_history_title_index(root: &Path) -> HashMap<String, String> {
    let Some(history_path) = root.parent().map(|path| path.join("history.jsonl")) else {
        return HashMap::new();
    };
    let file = match File::open(history_path) {
        Ok(file) => file,
        Err(_) => return HashMap::new(),
    };

    let mut titles = HashMap::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(session_id) = value.get("session_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(title) = value
            .get("text")
            .and_then(Value::as_str)
            .and_then(normalize_codex_title_value)
        else {
            continue;
        };

        titles.entry(session_id.to_string()).or_insert(title);
    }

    titles
}

fn extract_first_user_title(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return None;
    }

    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    if payload.get("role").and_then(Value::as_str) != Some("user") {
        return None;
    }

    payload.get("content").and_then(extract_first_content_title)
}

fn extract_first_content_title(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => normalize_codex_title_value(text),
        Value::Array(items) => items.iter().find_map(extract_first_content_title),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .and_then(normalize_codex_title_value)
            .or_else(|| {
                map.get("input_text")
                    .and_then(Value::as_str)
                    .and_then(normalize_codex_title_value)
            })
            .or_else(|| {
                map.get("output_text")
                    .and_then(Value::as_str)
                    .and_then(normalize_codex_title_value)
            })
            .or_else(|| map.get("content").and_then(extract_first_content_title)),
        _ => None,
    }
}

fn normalize_codex_title_value(value: &str) -> Option<String> {
    if let Some(request) = extract_marked_user_request(value) {
        if let Some(title) = normalize_codex_title_candidate(request) {
            return Some(title);
        }
    }

    normalize_codex_title_candidate(value)
}

fn extract_marked_user_request(value: &str) -> Option<&str> {
    let lowered = value.to_ascii_lowercase();
    for marker in [
        "## my request for codex:",
        "# my request for codex:",
        "my request for codex:",
    ] {
        if let Some(index) = lowered.find(marker) {
            let request = value[index + marker.len()..].trim();
            if !request.is_empty() {
                return Some(request);
            }
        }
    }

    None
}

fn normalize_codex_title_candidate(value: &str) -> Option<String> {
    let normalized = normalize_title_candidate(value, 160)?;
    let lowered = normalized.to_ascii_lowercase();

    if lowered.starts_with("## code review guidelines:")
        || lowered.starts_with("# review guidelines:")
        || lowered.starts_with("# agents.md instructions")
        || lowered.starts_with("<environment_context>")
        || lowered.starts_with("<user_shell_command>")
        || lowered.starts_with("codex resume ")
        || lowered == "q"
        || lowered == "hi"
        || lowered == "hey"
        || lowered == "ping"
        || lowered == "exit"
    {
        return None;
    }

    if looks_like_shell_transcript(&lowered) {
        return None;
    }

    if normalized.chars().count() == 1 && normalized.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    Some(normalized)
}

fn looks_like_shell_transcript(lowered: &str) -> bool {
    lowered.contains("exit code:")
        || lowered.contains("categoryinfo")
        || lowered.contains("fullyqualifiederrorid")
        || lowered.contains("service_name:")
        || lowered.contains("win32_exit_code")
        || lowered.contains("service_exit_code")
        || lowered.contains("status name displayname")
}

fn infer_session_id_from_filename(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_string_lossy();
    UUID_RE.find(&file_name).map(|mat| mat.as_str().to_string())
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
    fn load_messages_keeps_function_calls_and_outputs() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let content = [
            r#"{"timestamp":"2026-03-29T06:00:00Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"请检查项目"}]}}"#,
            r#"{"timestamp":"2026-03-29T06:00:01Z","type":"response_item","payload":{"type":"function_call","name":"shell_command","arguments":"{\"command\":\"Get-ChildItem\"}","call_id":"call_123"}}"#,
            r#"{"timestamp":"2026-03-29T06:00:02Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_123","output":"Exit code: 0\nOutput:\nfile-a"}}"#,
            r#"{"timestamp":"2026-03-29T06:00:03Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"已完成检查"}]}}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");

        let messages = load_messages(&path).expect("load messages");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[1].kind, "function_call");
        assert_eq!(messages[1].name.as_deref(), Some("shell_command"));
        assert_eq!(messages[1].call_id.as_deref(), Some("call_123"));
        assert!(messages[1].content.contains("Get-ChildItem"));
        assert_eq!(messages[2].kind, "function_call_output");
        assert_eq!(messages[2].role, "tool");
        assert_eq!(messages[2].name.as_deref(), Some("shell_command"));
        assert_eq!(messages[2].searchable_text(), "");
    }

    #[test]
    fn load_messages_strips_injected_user_context_fragments() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let content = [
            r##"{"timestamp":"2026-04-10T18:55:08.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for d:\\code"},{"type":"input_text","text":"<environment_context>\n  <cwd>d:\\code</cwd>\n  <shell>powershell</shell>\n</environment_context>"},{"type":"input_text","text":"请改界面"}]}}"##,
            r#"{"timestamp":"2026-03-29T06:00:03Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"已完成检查"}]}}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");

        let messages = load_messages(&path).expect("load messages");

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "请改界面");
    }

    #[test]
    fn load_messages_skips_context_only_user_messages() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let content = [
            r##"{"timestamp":"2026-04-03T08:44:03.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for D:\\code"}]}}"##,
            r##"{"timestamp":"2026-04-03T08:44:04.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>D:\\code</cwd>\n</environment_context>"}]}}"##,
            r##"{"timestamp":"2026-04-03T08:44:05.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"请检查发布流程哪里有问题"}]}}"##,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");

        let messages = load_messages(&path).expect("load messages");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "请检查发布流程哪里有问题");
    }

    #[test]
    fn parse_session_prefers_thread_name_from_session_index() {
        let dir = tempdir().expect("tempdir");
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");

        let path = sessions_dir
            .join("rollout-2026-04-11T02-51-56-019d78bc-8caa-7a40-a847-e6d1feb2f991.jsonl");
        let content = [
            r#"{"timestamp":"2026-04-10T18:55:06.911Z","type":"session_meta","payload":{"id":"019d78bc-8caa-7a40-a847-e6d1feb2f991","timestamp":"2026-04-10T18:51:56.025Z","cwd":"d:\\code"}}"#,
            r#"{"timestamp":"2026-04-10T18:55:08.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"请改界面"}]}}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");
        fs::write(
            dir.path().join("session_index.jsonl"),
            "{\"id\":\"019d78bc-8caa-7a40-a847-e6d1feb2f991\",\"thread_name\":\"重设计 toolcalling 命令包裹样式\",\"updated_at\":\"2026-04-10T18:55:14.8801488Z\"}\n",
        )
        .expect("write codex index");

        let title_index = load_session_title_index(&sessions_dir);
        let meta = parse_session(&path, &title_index, &HashMap::new()).expect("parse session");

        assert_eq!(
            meta.title.as_deref(),
            Some("重设计 toolcalling 命令包裹样式")
        );
    }

    #[test]
    fn parse_session_falls_back_to_history_when_index_missing() {
        let dir = tempdir().expect("tempdir");
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");

        let path = sessions_dir
            .join("rollout-2026-04-03T16-44-02-019d5283-7e20-7893-b29a-0939c460dbda.jsonl");
        let content = [
            r##"{"timestamp":"2026-04-03T08:44:02.472Z","type":"session_meta","payload":{"id":"019d5283-7e20-7893-b29a-0939c460dbda","timestamp":"2026-04-03T08:44:02.472Z","cwd":"D:\\code"}}"##,
            r##"{"timestamp":"2026-04-03T08:44:16.969Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"opencode -version"}]}}"##,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");
        fs::write(
            dir.path().join("history.jsonl"),
            [
                "{\"session_id\":\"019d5283-7e20-7893-b29a-0939c460dbda\",\"ts\":1775205842,\"text\":\"q\"}",
                "{\"session_id\":\"019d5283-7e20-7893-b29a-0939c460dbda\",\"ts\":1775205856,\"text\":\"检查一下登录逻辑\"}",
            ]
            .join("\n"),
        )
        .expect("write codex history");

        let history_title_index = load_history_title_index(&sessions_dir);
        let meta =
            parse_session(&path, &HashMap::new(), &history_title_index).expect("parse session");

        assert_eq!(meta.title.as_deref(), Some("检查一下登录逻辑"));
    }

    #[test]
    fn parse_session_falls_back_to_first_meaningful_user_message() {
        let dir = tempdir().expect("tempdir");
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");

        let path = sessions_dir
            .join("rollout-2026-04-03T16-44-02-019d5283-7e20-7893-b29a-0939c460dbda.jsonl");
        let content = [
            r##"{"timestamp":"2026-04-03T08:44:02.472Z","type":"session_meta","payload":{"id":"019d5283-7e20-7893-b29a-0939c460dbda","timestamp":"2026-04-03T08:44:02.472Z","cwd":"D:\\code"}}"##,
            r##"{"timestamp":"2026-04-03T08:44:03.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for D:\\code"}]}}"##,
            r##"{"timestamp":"2026-04-03T08:44:04.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>D:\\code</cwd>\n</environment_context>"}]}}"##,
            r##"{"timestamp":"2026-04-03T08:44:05.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"请检查发布流程哪里有问题"}]}}"##,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");

        let meta = parse_session(&path, &HashMap::new(), &HashMap::new()).expect("parse session");

        assert_eq!(meta.title.as_deref(), Some("请检查发布流程哪里有问题"));
    }

    #[test]
    fn parse_session_extracts_request_from_review_wrapper() {
        let dir = tempdir().expect("tempdir");
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");

        let path = sessions_dir
            .join("rollout-2026-03-20T19-03-08-019d0ae9-cfef-79a2-ba8f-5f4e00fcb031.jsonl");
        let content = [
            r##"{"timestamp":"2026-03-20T11:03:58.561Z","type":"session_meta","payload":{"id":"019d0ae9-cfef-79a2-ba8f-5f4e00fcb031","timestamp":"2026-03-20T11:03:08.530Z","cwd":"D:\\new\\atguigu\\Association"}}"##,
            r###"{"timestamp":"2026-03-20T11:03:58.563Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"## Code review guidelines:\n# Review guidelines:\nYou are acting as a reviewer for a proposed code change made by another engineer.\nReview the current code changes (staged, unstaged, and untracked files) and provide prioritized findings.\n## My request for Codex:\n请审查我未提交的更改"}]}}"###,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");

        let meta = parse_session(&path, &HashMap::new(), &HashMap::new()).expect("parse session");

        assert_eq!(meta.title.as_deref(), Some("请审查我未提交的更改"));
    }

    #[test]
    fn parse_session_skips_injected_preamble_inside_same_user_message() {
        let dir = tempdir().expect("tempdir");
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");

        let path = sessions_dir
            .join("rollout-2026-04-10T18-51-56-019d78bc-8caa-7a40-a847-e6d1feb2f991.jsonl");
        let content = [
            r##"{"timestamp":"2026-04-10T18:55:06.911Z","type":"session_meta","payload":{"id":"019d78bc-8caa-7a40-a847-e6d1feb2f991","timestamp":"2026-04-10T18:51:56.025Z","cwd":"d:\\code"}}"##,
            r##"{"timestamp":"2026-04-10T18:55:08.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for d:\\code"},{"type":"input_text","text":"<environment_context>\n  <cwd>d:\\code</cwd>\n</environment_context>"},{"type":"input_text","text":"请改界面"}]}}"##,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");

        let meta = parse_session(&path, &HashMap::new(), &HashMap::new()).expect("parse session");

        assert_eq!(meta.title.as_deref(), Some("请改界面"));
    }

    #[test]
    fn parse_session_skips_shell_output_in_history_titles() {
        let dir = tempdir().expect("tempdir");
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");

        let path = sessions_dir
            .join("rollout-2026-04-03T16-44-02-019d5283-7e20-7893-b29a-0939c460dbdb.jsonl");
        let content = [
            r##"{"timestamp":"2026-04-03T08:44:02.472Z","type":"session_meta","payload":{"id":"019d5283-7e20-7893-b29a-0939c460dbdb","timestamp":"2026-04-03T08:44:02.472Z","cwd":"D:\\code"}}"##,
            r##"{"timestamp":"2026-04-03T08:44:03.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"q"}]}}"##,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");
        fs::write(
            dir.path().join("history.jsonl"),
            [
                "{\"session_id\":\"019d5283-7e20-7893-b29a-0939c460dbdb\",\"ts\":1775205842,\"text\":\"Get-Service -Name MongoDB\\n\\nStatus   Name               DisplayName\\n------   ----               -----------\\nStopped  MongoDB            MongoDB Server (MongoDB)\"}",
                "{\"session_id\":\"019d5283-7e20-7893-b29a-0939c460dbdb\",\"ts\":1775205856,\"text\":\"启动mongodb服务失败，帮我排查\"}",
            ]
            .join("\n"),
        )
        .expect("write codex history");

        let history_title_index = load_history_title_index(&sessions_dir);
        let meta =
            parse_session(&path, &HashMap::new(), &history_title_index).expect("parse session");

        assert_eq!(meta.title.as_deref(), Some("启动mongodb服务失败，帮我排查"));
    }

    #[test]
    fn parse_session_falls_back_to_project_directory_when_no_meaningful_title_exists() {
        let dir = tempdir().expect("tempdir");
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");

        let path = sessions_dir
            .join("rollout-2026-04-03T16-44-02-019d5283-7e20-7893-b29a-0939c460dbda.jsonl");
        let content = [
            r##"{"timestamp":"2026-04-03T08:44:02.472Z","type":"session_meta","payload":{"id":"019d5283-7e20-7893-b29a-0939c460dbda","timestamp":"2026-04-03T08:44:02.472Z","cwd":"D:\\code"}}"##,
            r##"{"timestamp":"2026-04-03T08:44:03.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for D:\\code"}]}}"##,
            r##"{"timestamp":"2026-04-03T08:44:04.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>D:\\code</cwd>\n</environment_context>"}]}}"##,
            r##"{"timestamp":"2026-04-03T08:44:05.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"q"}]}}"##,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write codex fixture");

        let meta = parse_session(&path, &HashMap::new(), &HashMap::new()).expect("parse session");

        assert_eq!(meta.title.as_deref(), Some("code"));
    }
}
