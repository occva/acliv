use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::UNIX_EPOCH;

use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::paths::get_codex_sessions_dir;
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{
    extract_text, log_scan_error, normalize_title_candidate, parse_timestamp_to_ms, path_basename,
    truncate_summary,
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

#[derive(Debug, Clone, Default)]
struct StateRow {
    id: String,
    cwd: Option<String>,
    title: Option<String>,
    tokens_used: Option<i64>,
    archived: bool,
    model: Option<String>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct RolloutParse {
    session_id: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    created_at: Option<i64>,
    last_active_at: Option<i64>,
    title: Option<String>,
    summary: Option<String>,
    tokens_used: Option<i64>,
    parent_session_id: Option<String>,
    spawned: bool,
    internal: bool,
    unknown_records: u32,
    messages: Vec<SessionMessage>,
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    let sessions_dir = get_codex_sessions_dir();
    let home = codex_home(&sessions_dir);
    let archived_dir = home.join("archived_sessions");
    let title_index = load_session_title_index(&sessions_dir);
    let history_title_index = load_history_title_index(&sessions_dir);
    let state_index = load_state_index(&home.join("state_5.sqlite"));
    let mut files = Vec::new();
    collect_rollout_files(&sessions_dir, &mut files);
    collect_rollout_files(&archived_dir, &mut files);

    files
        .into_iter()
        .filter_map(|path| {
            let head = read_first_value(&path);
            if head
                .as_ref()
                .and_then(|value| value.get("payload"))
                .is_some_and(|payload| {
                    matches!(thread_disposition(payload), ThreadDisposition::Internal)
                })
            {
                return None;
            }
            let inferred_id = head
                .as_ref()
                .and_then(|value| value.pointer("/payload/id"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| infer_session_id_from_filename(&path));
            let file_key = rollout_file_name(&path).map(str::to_string);
            let state = file_key
                .as_ref()
                .and_then(|key| state_index.get(key))
                .or_else(|| inferred_id.as_ref().and_then(|id| state_index.get(id)));
            parse_session_with_state(
                &path,
                &title_index,
                &history_title_index,
                state,
                path.starts_with(&archived_dir),
            )
        })
        .collect()
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    parse_rollout(path).map(|parsed| parsed.messages)
}

fn parse_rollout(path: &Path) -> Result<RolloutParse, String> {
    let reader = open_rollout(path)?;
    let mut parsed = RolloutParse::default();
    let mut call_names = HashMap::<String, String>::new();
    let mut event_fallback = Vec::new();
    let mut current_model = None;
    let mut inherited_cut = false;
    let mut history_start = None;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read Codex rollout: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                parsed.unknown_records = parsed.unknown_records.saturating_add(1);
                continue;
            }
        };
        let ts = value.get("timestamp").and_then(timestamp_ms);
        if parsed.created_at.is_none() {
            parsed.created_at = ts;
        }
        if ts.is_some() {
            parsed.last_active_at = ts;
        }
        if parsed.spawned && !inherited_cut {
            if let (Some(start), Some(ordinal)) =
                (history_start, value.get("ordinal").and_then(Value::as_i64))
            {
                if ordinal >= start {
                    parsed.messages.clear();
                    event_fallback.clear();
                    call_names.clear();
                    inherited_cut = true;
                }
            }
        }

        let row_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if row_type == "compacted" {
            parsed.messages.push(SessionMessage::structured(
                "system".to_string(),
                "compact_summary",
                None,
                None,
                "Context compacted".to_string(),
                ts,
            ));
            continue;
        }
        let Some(payload) = value.get("payload") else {
            if !matches!(
                row_type,
                "world_state" | "inter_agent_communication_metadata"
            ) {
                parsed.unknown_records = parsed.unknown_records.saturating_add(1);
            }
            continue;
        };

        match row_type {
            "session_meta" => {
                parsed.session_id = payload
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                parsed.cwd = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                match thread_disposition(payload) {
                    ThreadDisposition::User => {}
                    ThreadDisposition::Internal => parsed.internal = true,
                    ThreadDisposition::Spawned(parent) => {
                        parsed.spawned = true;
                        parsed.parent_session_id = parent;
                        history_start = payload
                            .get("subagent_history_start_ordinal")
                            .and_then(Value::as_i64);
                    }
                }
            }
            "turn_context" => {
                current_model = payload
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if current_model.is_some() {
                    parsed.model = current_model.clone();
                }
                if parsed.cwd.is_none() {
                    parsed.cwd = payload
                        .get("cwd")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
            }
            "response_item" => {
                let payload_type = payload
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
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
                        if parsed.title.is_none() && role == "user" {
                            parsed.title = normalize_codex_title_value(&content);
                        }
                        parsed.summary = Some(content.clone());
                        let mut message = SessionMessage::plain(role, content, ts);
                        message.model = current_model.clone();
                        parsed.messages.push(message);
                    }
                    "reasoning" => {
                        let content = payload
                            .get("summary")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|item| item.get("text").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        if !content.trim().is_empty() {
                            let mut message = SessionMessage::structured(
                                "assistant".to_string(),
                                "thinking",
                                None,
                                None,
                                content,
                                ts,
                            );
                            message.model = current_model.clone();
                            parsed.messages.push(message);
                        }
                    }
                    "function_call" | "custom_tool_call" | "local_shell_call" => {
                        let name = payload
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("exec")
                            .to_string();
                        let call_id = payload
                            .get("call_id")
                            .or_else(|| payload.get("id"))
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        if let Some(call_id) = call_id.as_ref() {
                            call_names.insert(call_id.clone(), name.clone());
                        }
                        let content = format_function_call_content(payload);
                        let mut message = SessionMessage::structured(
                            "assistant".to_string(),
                            "function_call",
                            Some(name.clone()),
                            call_id,
                            content,
                            ts,
                        );
                        message.tool_names.push(name);
                        message.model = current_model.clone();
                        parsed.messages.push(message);
                    }
                    "function_call_output" | "custom_tool_call_output" => {
                        let call_id = payload
                            .get("call_id")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        let name = call_id.as_ref().and_then(|id| call_names.get(id)).cloned();
                        let output = payload.get("output").unwrap_or(&Value::Null);
                        let content = output
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| extract_text(output));
                        let mut message = SessionMessage::structured(
                            "tool".to_string(),
                            "function_call_output",
                            name,
                            call_id,
                            content,
                            ts,
                        );
                        message.is_error =
                            payload.get("is_error").and_then(Value::as_bool) == Some(true);
                        parsed.messages.push(message);
                    }
                    "agent_message" => {}
                    _ => parsed.unknown_records = parsed.unknown_records.saturating_add(1),
                }
            }
            "event_msg" => match payload.get("type").and_then(Value::as_str) {
                Some("token_count") => {
                    parsed.tokens_used = payload
                        .pointer("/info/total_token_usage/total_tokens")
                        .and_then(Value::as_i64)
                        .or(parsed.tokens_used);
                }
                Some("user_message") | Some("agent_message") => {
                    let role =
                        if payload.get("type").and_then(Value::as_str) == Some("user_message") {
                            "user"
                        } else {
                            "assistant"
                        };
                    if let Some(content) = payload.get("message").and_then(Value::as_str) {
                        let content = if role == "user" {
                            clean_user_message_text(content)
                        } else {
                            content.trim().to_string()
                        };
                        if !content.is_empty() {
                            event_fallback.push(SessionMessage::plain(
                                role.to_string(),
                                content,
                                ts,
                            ));
                        }
                    }
                }
                Some("item_completed") | Some("turn_started") | Some("turn_complete") => {}
                _ => {}
            },
            "token_usage_record" | "world_state" | "inter_agent_communication_metadata" => {}
            _ => parsed.unknown_records = parsed.unknown_records.saturating_add(1),
        }
    }

    if parsed.messages.is_empty() {
        parsed.messages = event_fallback;
    }
    Ok(parsed)
}

fn open_rollout(path: &Path) -> Result<Box<dyn BufRead>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open Codex rollout: {e}"))?;
    if path.extension().and_then(|value| value.to_str()) == Some("zst") {
        let decoder = zstd::stream::read::Decoder::new(file)
            .map_err(|e| format!("Failed to decode Codex rollout: {e}"))?;
        Ok(Box::new(BufReader::with_capacity(1 << 20, decoder)))
    } else {
        Ok(Box::new(BufReader::with_capacity(1 << 20, file)))
    }
}

fn read_first_value(path: &Path) -> Option<Value> {
    let mut reader = open_rollout(path).ok()?;
    let mut line = String::with_capacity(1 << 16);
    reader.read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

#[derive(Debug)]
enum ThreadDisposition {
    User,
    Internal,
    Spawned(Option<String>),
}

fn thread_disposition(payload: &Value) -> ThreadDisposition {
    if let Some(source) = payload.get("source").filter(|source| source.is_object()) {
        if source.pointer("/subagent/thread_spawn").is_some() {
            let parent = source
                .pointer("/subagent/parent_thread_id")
                .or_else(|| source.pointer("/subagent/thread_spawn/parent_thread_id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            return ThreadDisposition::Spawned(parent);
        }
        return ThreadDisposition::Internal;
    }
    if matches!(
        payload.get("thread_source").and_then(Value::as_str),
        Some("subagent" | "guardian_review" | "memory_consolidation")
    ) {
        return ThreadDisposition::Internal;
    }
    ThreadDisposition::User
}

fn timestamp_ms(value: &Value) -> Option<i64> {
    parse_timestamp_to_ms(value).or_else(|| {
        value.as_i64().map(|raw| {
            if raw > 10_000_000_000 {
                raw
            } else {
                raw * 1000
            }
        })
    })
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

    let sibling = if path.to_string_lossy().ends_with(".jsonl.zst") {
        path.to_string_lossy()
            .strip_suffix(".zst")
            .map(PathBuf::from)
    } else {
        Some(PathBuf::from(format!("{}.zst", path.to_string_lossy())))
    };
    fs::remove_file(path).map_err(|e| {
        format!(
            "Failed to delete Codex session file {}: {e}",
            path.display()
        )
    })?;
    if let Some(sibling) = sibling.filter(|candidate| candidate.is_file()) {
        fs::remove_file(&sibling).map_err(|e| {
            format!(
                "Failed to delete Codex session copy {}: {e}",
                sibling.display()
            )
        })?;
    }
    Ok(true)
}

fn parse_session(
    path: &Path,
    title_index: &HashMap<String, String>,
    history_title_index: &HashMap<String, String>,
) -> Option<SessionMeta> {
    parse_session_with_state(path, title_index, history_title_index, None, false)
}

fn parse_session_with_state(
    path: &Path,
    title_index: &HashMap<String, String>,
    history_title_index: &HashMap<String, String>,
    state: Option<&StateRow>,
    archived_by_path: bool,
) -> Option<SessionMeta> {
    let parsed = if state.is_some() {
        let head = read_first_value(path);
        let payload = head.as_ref().and_then(|value| value.get("payload"));
        let disposition = payload
            .map(thread_disposition)
            .unwrap_or(ThreadDisposition::User);
        let spawned = matches!(&disposition, ThreadDisposition::Spawned(_));
        let internal = matches!(&disposition, ThreadDisposition::Internal);
        RolloutParse {
            session_id: payload
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string),
            cwd: payload
                .and_then(|value| value.get("cwd"))
                .and_then(Value::as_str)
                .map(str::to_string),
            parent_session_id: match &disposition {
                ThreadDisposition::Spawned(parent) => parent.clone(),
                _ => None,
            },
            spawned,
            internal,
            ..RolloutParse::default()
        }
    } else {
        parse_rollout(path).ok()?
    };
    if parsed.internal {
        return None;
    }

    let session_id = state
        .map(|row| row.id.clone())
        .filter(|id| !id.is_empty())
        .or(parsed.session_id)
        .or_else(|| infer_session_id_from_filename(path))?;
    let project_dir = state.and_then(|row| row.cwd.clone()).or(parsed.cwd);
    let title = state
        .and_then(|row| row.title.clone())
        .and_then(|value| normalize_codex_title_value(&value))
        .or_else(|| title_index.get(&session_id).cloned())
        .or_else(|| history_title_index.get(&session_id).cloned())
        .or(parsed.title)
        .or_else(|| project_dir.as_deref().and_then(path_basename));
    let metadata = fs::metadata(path).ok();
    let source_mtime = metadata.as_ref().and_then(file_mtime_ms);
    let source_size = metadata
        .as_ref()
        .and_then(|value| i64::try_from(value.len()).ok());
    let created_at = state.and_then(|row| row.created_at).or(parsed.created_at);
    let last_active_at = state
        .and_then(|row| row.updated_at)
        .or(parsed.last_active_at)
        .or(source_mtime);
    let archived = state.map(|row| row.archived).unwrap_or(archived_by_path);

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title: title.clone(),
        summary: parsed
            .summary
            .map(|text| truncate_summary(&text, 160))
            .or(title),
        project_dir: project_dir.clone(),
        cwd: project_dir,
        model: state.and_then(|row| row.model.clone()).or(parsed.model),
        created_at,
        last_active_at,
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: Some(format!("codex resume {session_id}")),
        archived,
        tokens_used: state.and_then(|row| row.tokens_used).or(parsed.tokens_used),
        parent_session_id: parsed.parent_session_id,
        unknown_records: parsed.unknown_records,
        source_mtime,
        source_size,
        source_rank: if archived { 0 } else { 1 },
    })
}

fn codex_home(sessions_dir: &Path) -> PathBuf {
    if sessions_dir.file_name().and_then(|value| value.to_str()) == Some("sessions") {
        sessions_dir.parent().unwrap_or(sessions_dir).to_path_buf()
    } else {
        sessions_dir.to_path_buf()
    }
}

fn load_state_index(path: &Path) -> HashMap<String, StateRow> {
    let Ok(connection) = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return HashMap::new();
    };
    let Ok(mut statement) = connection.prepare(
        "SELECT id, rollout_path, cwd, title, name, tokens_used, archived, model, created_at_ms, updated_at_ms FROM threads",
    ) else {
        return HashMap::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let rollout_path: Option<String> = row.get(1)?;
        let title: Option<String> = row.get(3)?;
        let name: Option<String> = row.get(4)?;
        Ok((
            rollout_path,
            StateRow {
                id,
                cwd: row.get(2)?,
                title: name.filter(|value| !value.trim().is_empty()).or(title),
                tokens_used: row.get(5)?,
                archived: row.get::<_, Option<i64>>(6)?.unwrap_or(0) != 0,
                model: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            },
        ))
    }) else {
        return HashMap::new();
    };
    let mut index = HashMap::new();
    for (rollout_path, state) in rows.flatten() {
        index.insert(state.id.clone(), state.clone());
        if let Some(name) = rollout_path
            .as_deref()
            .and_then(|value| Path::new(value).file_name())
            .and_then(|value| value.to_str())
            .map(|value| value.strip_suffix(".zst").unwrap_or(value).to_string())
        {
            index.insert(name, state);
        }
    }
    index
}

fn file_mtime_ms(metadata: &fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
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

fn rollout_file_name(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    let name = name.strip_suffix(".zst").unwrap_or(name);
    name.ends_with(".jsonl").then_some(name)
}

fn collect_rollout_files(root: &Path, files: &mut Vec<PathBuf>) {
    if !root.exists() {
        return;
    }

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) => {
            log_scan_error(PROVIDER_ID, root, &err);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rollout_files(&path, files);
        } else if rollout_file_name(&path).is_some() {
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
    #[test]
    fn reads_compressed_rollout_extended_records_and_archived_state() {
        let dir = tempdir().expect("tempdir");
        let path = dir
            .path()
            .join("rollout-2026-11-01T00-00-00-11111111-2222-4333-8444-555555555555.jsonl.zst");
        let content = [
            r#"{"timestamp":"2026-11-01T00:00:00Z","type":"session_meta","payload":{"id":"11111111-2222-4333-8444-555555555555","cwd":"/work"}}"#,
            r#"{"timestamp":"2026-11-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.6"}}"#,
            r#"{"timestamp":"2026-11-01T00:00:02Z","type":"response_item","payload":{"type":"reasoning","summary":[{"text":"inspect first"}]}}"#,
            r#"{"timestamp":"2026-11-01T00:00:03Z","type":"response_item","payload":{"type":"local_shell_call","id":"call-1","action":{"command":"pwd"}}}"#,
            r#"{"timestamp":"2026-11-01T00:00:04Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":"/work"}}"#,
            r#"{"timestamp":"2026-11-01T00:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":321}}}}"#,
        ]
        .join("\n");
        let compressed = zstd::stream::encode_all(content.as_bytes(), 1).expect("compress fixture");
        fs::write(&path, compressed).expect("write compressed rollout");

        let messages = load_messages(&path).expect("load compressed rollout");
        assert!(messages.iter().any(|message| message.kind == "thinking"));
        assert!(messages
            .iter()
            .any(|message| message.kind == "function_call"));
        assert!(messages
            .iter()
            .any(|message| message.kind == "function_call_output"));
        assert!(messages
            .iter()
            .any(|message| message.model.as_deref() == Some("gpt-5.6")));

        let meta = parse_session_with_state(&path, &HashMap::new(), &HashMap::new(), None, true)
            .expect("parse archived compressed rollout");
        assert!(meta.archived);
        assert_eq!(meta.tokens_used, Some(321));
    }

    #[test]
    fn classifies_internal_and_spawned_threads() {
        let internal = serde_json::json!({"source":{"subagent":{"review":{}}}});
        assert!(matches!(
            thread_disposition(&internal),
            ThreadDisposition::Internal
        ));
        let spawned = serde_json::json!({
            "source":{"subagent":{"thread_spawn":{"agent_path":"/root/task"},"parent_thread_id":"parent-1"}}
        });
        assert!(matches!(
            thread_disposition(&spawned),
            ThreadDisposition::Spawned(Some(parent)) if parent == "parent-1"
        ));
    }
}
