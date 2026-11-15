use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::paths::get_opencode_storage_dir;
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{
    log_scan_error, normalize_title_candidate, parse_timestamp_to_ms, path_basename,
    sqlite_source_state, truncate_summary,
};

const PROVIDER_ID: &str = "opencode";

pub(crate) fn get_opencode_data_dir() -> PathBuf {
    get_opencode_storage_dir()
}

pub fn scan_sessions() -> Result<Vec<SessionMeta>, String> {
    let root = get_opencode_data_dir();
    let mut sessions = Vec::new();
    let candidates = if root.is_file() {
        vec![root.clone()]
    } else {
        vec![root.join("opencode.db"), root.join("opencode-next.db")]
    };
    for (rank, database) in candidates.into_iter().enumerate() {
        if database.is_file() {
            sessions.extend(scan_database(
                &database,
                u8::try_from(rank + 1).unwrap_or(u8::MAX),
            )?);
        }
    }

    let storage = if root.join("storage").is_dir() {
        root.join("storage")
    } else {
        root
    };
    let session_dir = storage.join("session");
    if session_dir.is_dir() {
        let mut json_files = Vec::new();
        collect_json_files(&session_dir, &mut json_files);
        sessions.extend(
            json_files
                .into_iter()
                .filter_map(|path| parse_session(&storage, &path)),
        );
    }
    Ok(sessions)
}

#[derive(Debug)]
struct DbSessionRow {
    id: String,
    directory: String,
    title: String,
    created_at: Option<i64>,
    updated_at: Option<i64>,
    model_json: Option<String>,
    tokens_used: Option<i64>,
    archived: bool,
    version: Option<String>,
    parent_id: Option<String>,
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    if let Some((database, session_id)) = split_virtual_source(path) {
        return load_database_messages(&database, &session_id);
    }
    load_legacy_messages(path)
}

fn scan_database(path: &Path, source_rank: u8) -> Result<Vec<SessionMeta>, String> {
    let connection = open_database(path)?;
    let rows = query_database_sessions(&connection)?;
    let (source_mtime, source_size) = sqlite_source_state(path);

    Ok(rows
        .into_iter()
        .map(|row| {
            let title = normalize_title_candidate(&row.title, 160)
                .or_else(|| path_basename(&row.directory));
            let model = row.model_json.as_deref().and_then(parse_model_id);
            let session_id = row.id;
            let preview = row
                .version
                .as_deref()
                .is_some_and(|value| value.starts_with('2'))
                || path.file_name().and_then(|value| value.to_str()) == Some("opencode-next.db");
            SessionMeta {
                provider_id: PROVIDER_ID.to_string(),
                session_id: session_id.clone(),
                title: title.clone(),
                summary: title,
                project_dir: (!row.directory.is_empty()).then(|| row.directory.clone()),
                cwd: (!row.directory.is_empty()).then_some(row.directory),
                model,
                created_at: row.created_at,
                last_active_at: row.updated_at.or(row.created_at).or(source_mtime),
                source_path: Some(virtual_source(path, &session_id)),
                resume_command: Some(format!(
                    "{} session resume {session_id}",
                    if preview { "opencode2" } else { "opencode" }
                )),
                archived: row.archived,
                tokens_used: row.tokens_used.filter(|value| *value > 0),
                parent_session_id: row.parent_id,
                unknown_records: 0,
                source_mtime,
                source_size,
                source_rank,
            }
        })
        .collect())
}

fn open_database(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open OpenCode database {}: {e}", path.display()))
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|e| format!("Failed to inspect OpenCode table {table}: {e}"))
}

fn table_columns(connection: &Connection, table: &str) -> Result<HashSet<String>, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| format!("Failed to inspect OpenCode columns for {table}: {e}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("Failed to query OpenCode columns for {table}: {e}"))?;
    rows.collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(|e| format!("Failed to read OpenCode columns for {table}: {e}"))
}

fn column_or(columns: &HashSet<String>, name: &str, fallback: &str) -> String {
    if columns.contains(name) {
        format!("s.{name}")
    } else {
        fallback.to_string()
    }
}

fn query_database_sessions(connection: &Connection) -> Result<Vec<DbSessionRow>, String> {
    let mut sessions = Vec::new();
    let mut seen = HashSet::new();
    for table in ["session_v2", "session"] {
        if !table_exists(connection, table)? {
            continue;
        }
        let columns = table_columns(connection, table)?;
        let tokens = if ["tokens_input", "tokens_output", "tokens_reasoning"]
            .iter()
            .all(|name| columns.contains(*name))
        {
            "COALESCE(s.tokens_input, 0) + COALESCE(s.tokens_output, 0) + COALESCE(s.tokens_reasoning, 0)".to_string()
        } else {
            "NULL".to_string()
        };
        let sql = format!(
            "SELECT s.id, {}, {}, {}, {}, {}, {tokens}, {}, {}, {} FROM {table} s",
            column_or(&columns, "directory", "''"),
            column_or(&columns, "title", "''"),
            column_or(&columns, "time_created", "NULL"),
            column_or(&columns, "time_updated", "NULL"),
            column_or(&columns, "model", "NULL"),
            column_or(&columns, "time_archived", "NULL"),
            column_or(&columns, "version", "NULL"),
            column_or(&columns, "parent_id", "NULL"),
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|e| format!("Failed to query OpenCode {table}: {e}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok(DbSessionRow {
                    id: row.get(0)?,
                    directory: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    title: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    model_json: row.get(5)?,
                    tokens_used: row.get(6)?,
                    archived: row.get::<_, Option<i64>>(7)?.is_some(),
                    version: row.get(8)?,
                    parent_id: row
                        .get::<_, Option<String>>(9)?
                        .filter(|value| !value.is_empty()),
                })
            })
            .map_err(|e| format!("Failed to read OpenCode {table}: {e}"))?;
        for row in rows.flatten() {
            if seen.insert(row.id.clone()) {
                sessions.push(row);
            }
        }
    }
    Ok(sessions)
}

fn virtual_source(database: &Path, session_id: &str) -> String {
    format!("{}#{session_id}", database.to_string_lossy())
}

fn split_virtual_source(path: &Path) -> Option<(PathBuf, String)> {
    let value = path.to_string_lossy();
    let (database, session_id) = value.rsplit_once('#')?;
    let database = PathBuf::from(database);
    (database.is_file() && !session_id.is_empty()).then(|| (database, session_id.to_string()))
}

fn parse_model_id(value: &str) -> Option<String> {
    serde_json::from_str::<Value>(value)
        .ok()
        .and_then(|model| model.get("id").and_then(Value::as_str).map(str::to_string))
        .or_else(|| (!value.trim().is_empty()).then(|| value.to_string()))
}

fn load_database_messages(
    database: &Path,
    session_id: &str,
) -> Result<Vec<SessionMessage>, String> {
    let connection = open_database(database)?;
    let use_v2 = table_exists(&connection, "session_message")?
        && connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM session_message WHERE session_id = ?1 AND type IN ('user', 'assistant'))",
                [session_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|e| format!("Failed to inspect OpenCode 2 messages: {e}"))?;
    if use_v2 {
        parse_v2_messages(&connection, session_id)
    } else {
        parse_v1_messages(&connection, session_id)
    }
}

fn load_legacy_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    // `path` is the message directory: storage/message/{sessionID}/
    if !path.is_dir() {
        return Err(format!("Message directory not found: {}", path.display()));
    }

    let storage = path
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| "Cannot determine storage root from message path".to_string())?;

    let mut msg_files = Vec::new();
    collect_json_files(path, &mut msg_files);

    // Parse all messages and collect (created_ts, message_id, role, parts_text)
    let mut entries: Vec<(i64, String, String, String)> = Vec::new();

    for msg_path in &msg_files {
        let data = match std::fs::read_to_string(msg_path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let value: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_id = match value.get("id").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => continue,
        };

        let role = value
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let created_ts = value
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(parse_timestamp_to_ms)
            .unwrap_or(0);

        // Collect text parts from storage/part/{messageID}/
        let part_dir = storage.join("part").join(&msg_id);
        let text = collect_parts_text(&part_dir);
        if text.trim().is_empty() {
            continue;
        }

        entries.push((created_ts, msg_id, role, text));
    }

    // Sort by created timestamp
    entries.sort_by_key(|(ts, _, _, _)| *ts);

    let messages = entries
        .into_iter()
        .map(|(ts, msg_id, role, content)| SessionMessage {
            msg_uuid: Some(msg_id),
            parent_uuid: None,
            role,
            kind: "message".to_string(),
            name: None,
            call_id: None,
            content,
            ts: if ts > 0 { Some(ts) } else { None },
            is_sidechain: false,
            tool_names: Vec::new(),
            model: None,
            is_error: false,
            truncated: false,
        })
        .collect();

    Ok(messages)
}

fn parse_v1_messages(
    connection: &Connection,
    session_id: &str,
) -> Result<Vec<SessionMessage>, String> {
    if !table_exists(connection, "message")? || !table_exists(connection, "part")? {
        return Ok(Vec::new());
    }
    let mut parts_by_message = HashMap::<String, Vec<Value>>::new();
    let mut part_statement = connection
        .prepare("SELECT message_id, data FROM part WHERE session_id = ?1 ORDER BY message_id, id")
        .map_err(|e| format!("Failed to prepare OpenCode part query: {e}"))?;
    let part_rows = part_statement
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query OpenCode parts: {e}"))?;
    for (message_id, data) in part_rows.flatten() {
        if let Ok(value) = serde_json::from_str(&data) {
            parts_by_message.entry(message_id).or_default().push(value);
        }
    }

    let mut messages = Vec::new();
    let mut message_statement = connection
        .prepare("SELECT id, data FROM message WHERE session_id = ?1 ORDER BY time_created, id")
        .map_err(|e| format!("Failed to prepare OpenCode message query: {e}"))?;
    let message_rows = message_statement
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query OpenCode messages: {e}"))?;
    for (message_id, data) in message_rows.flatten() {
        let Ok(message_data) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let role = message_data
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("system");
        let ts = message_data
            .pointer("/time/created")
            .and_then(opencode_timestamp_ms);
        let model = message_data
            .pointer("/model/id")
            .and_then(Value::as_str)
            .map(str::to_string);
        for part in parts_by_message.remove(&message_id).unwrap_or_default() {
            push_opencode_part(&mut messages, role, ts, model.clone(), &part);
        }
    }
    Ok(messages)
}

fn parse_v2_messages(
    connection: &Connection,
    session_id: &str,
) -> Result<Vec<SessionMessage>, String> {
    let mut statement = connection
        .prepare("SELECT type, data FROM session_message WHERE session_id = ?1 ORDER BY seq")
        .map_err(|e| format!("Failed to prepare OpenCode 2 message query: {e}"))?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query OpenCode 2 messages: {e}"))?;
    let mut messages = Vec::new();
    for (message_type, data) in rows.flatten() {
        let Ok(message_data) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let ts = message_data
            .pointer("/time/created")
            .and_then(opencode_timestamp_ms);
        match message_type.as_str() {
            "user" => {
                let text = message_data
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or_default();
                let mut content_messages = Vec::new();
                for part in message_data
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    push_opencode_part(&mut content_messages, "user", ts, None, part);
                }
                let text_repeated = content_messages
                    .iter()
                    .any(|message| message.kind == "message" && message.content.trim() == text);
                if !text.is_empty() && !text_repeated {
                    messages.push(SessionMessage::plain(
                        "user".to_string(),
                        text.to_string(),
                        ts,
                    ));
                }
                messages.extend(content_messages);
            }
            "assistant" => {
                let model = message_data
                    .pointer("/model/id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                for part in message_data
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    push_opencode_part(&mut messages, "assistant", ts, model.clone(), part);
                }
            }
            "synthetic" | "system" => {
                if let Some(text) = message_data.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        messages.push(SessionMessage::structured(
                            "system".to_string(),
                            "meta",
                            None,
                            None,
                            text.trim().to_string(),
                            ts,
                        ));
                    }
                }
            }
            "shell" => {
                let call_id = message_data
                    .get("callID")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let command = message_data
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let mut call = SessionMessage::structured(
                    "assistant".to_string(),
                    "tool_use",
                    Some("shell".to_string()),
                    call_id.clone(),
                    serde_json::json!({ "command": command }).to_string(),
                    ts,
                );
                call.tool_names.push("shell".to_string());
                messages.push(call);
                if let Some(output) = message_data.get("output").and_then(Value::as_str) {
                    if !output.is_empty() {
                        messages.push(SessionMessage::structured(
                            "tool".to_string(),
                            "tool_result",
                            Some("shell".to_string()),
                            call_id,
                            output.to_string(),
                            ts,
                        ));
                    }
                }
            }
            "compaction" => {
                if let Some(summary) = message_data.get("summary").and_then(Value::as_str) {
                    if !summary.trim().is_empty() {
                        messages.push(SessionMessage::structured(
                            "system".to_string(),
                            "compact_summary",
                            None,
                            None,
                            summary.trim().to_string(),
                            ts,
                        ));
                    }
                }
            }
            "agent-switched" | "model-switched" => {}
            _ => {}
        }
    }
    Ok(messages)
}

fn push_opencode_part(
    messages: &mut Vec<SessionMessage>,
    role: &str,
    ts: Option<i64>,
    model: Option<String>,
    part: &Value,
) {
    match part.get("type").and_then(Value::as_str) {
        Some("text") => {
            let Some(text) = part
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
            else {
                return;
            };
            let kind = if part.get("synthetic").and_then(Value::as_bool) == Some(true) {
                "meta"
            } else {
                "message"
            };
            let mut message = SessionMessage::structured(
                role.to_string(),
                kind,
                None,
                None,
                text.trim().to_string(),
                ts,
            );
            message.model = model;
            messages.push(message);
        }
        Some("reasoning") => {
            if let Some(text) = part
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
            {
                let mut message = SessionMessage::structured(
                    "assistant".to_string(),
                    "thinking",
                    None,
                    None,
                    text.trim().to_string(),
                    ts,
                );
                message.model = model;
                messages.push(message);
            }
        }
        Some("tool") => {
            let state = part.get("state").unwrap_or(&Value::Null);
            let name = part
                .get("tool")
                .or_else(|| part.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let call_id = part
                .get("callID")
                .or_else(|| part.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let input = state.get("input").cloned().unwrap_or(Value::Null);
            let input = serde_json::to_string_pretty(&input).unwrap_or_else(|_| input.to_string());
            let mut call = SessionMessage::structured(
                "assistant".to_string(),
                "tool_use",
                Some(name.clone()),
                call_id.clone(),
                input,
                ts,
            );
            call.tool_names.push(name.clone());
            call.model = model;
            messages.push(call);

            let output = ["output", "result", "error"]
                .iter()
                .find_map(|key| state.get(*key))
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string())
                });
            if let Some(output) = output.filter(|value| !value.trim().is_empty()) {
                let mut result = SessionMessage::structured(
                    "tool".to_string(),
                    "tool_result",
                    Some(name),
                    call_id,
                    output,
                    ts,
                );
                result.is_error = state.get("status").and_then(Value::as_str) == Some("error")
                    || state.get("error").is_some_and(|value| !value.is_null());
                messages.push(result);
            }
        }
        Some("subtask") => {
            let text = part
                .get("command")
                .and_then(Value::as_str)
                .map(|value| format!("/{value}"))
                .or_else(|| {
                    part.get("description")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .or_else(|| {
                    part.get("prompt")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            if let Some(text) = text.filter(|value| !value.trim().is_empty()) {
                messages.push(SessionMessage::plain(role.to_string(), text, ts));
            }
        }
        Some("image") | Some("file") => {
            let media_type = part
                .get("mime")
                .or_else(|| part.get("mediaType"))
                .and_then(Value::as_str)
                .unwrap_or("image");
            messages.push(SessionMessage::plain(
                role.to_string(),
                format!("[Image: {media_type}]"),
                ts,
            ));
        }
        Some(
            "step-start" | "step-finish" | "snapshot" | "patch" | "agent" | "retry" | "compaction"
            | "timeline" | "checkpoint",
        ) => {}
        _ => {}
    }
}

fn opencode_timestamp_ms(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .map(|raw| {
            if raw > 10_000_000_000 {
                raw
            } else {
                raw * 1000
            }
        })
        .or_else(|| parse_timestamp_to_ms(value))
}

pub fn delete_session(storage: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    if split_virtual_source(path).is_some() {
        return Err("Deleting SQLite-backed OpenCode sessions is not supported".to_string());
    }
    if path.file_name().and_then(|name| name.to_str()) != Some(session_id) {
        return Err(format!(
            "OpenCode session path does not match session ID: expected {session_id}, found {}",
            path.display()
        ));
    }

    let mut message_files = Vec::new();
    collect_json_files(path, &mut message_files);

    let mut message_ids = Vec::new();
    for message_path in &message_files {
        let data = match std::fs::read_to_string(message_path) {
            Ok(data) => data,
            Err(_) => continue,
        };
        let value: Value = match serde_json::from_str(&data) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if let Some(message_id) = value.get("id").and_then(Value::as_str) {
            message_ids.push(message_id.to_string());
        }
    }

    for message_id in &message_ids {
        let part_dir = storage.join("part").join(message_id);
        remove_dir_all_if_exists(&part_dir).map_err(|e| {
            format!(
                "Failed to delete OpenCode part directory {}: {e}",
                part_dir.display()
            )
        })?;
    }

    let session_diff_path = storage
        .join("session_diff")
        .join(format!("{session_id}.json"));
    remove_file_if_exists(&session_diff_path).map_err(|e| {
        format!(
            "Failed to delete OpenCode session diff {}: {e}",
            session_diff_path.display()
        )
    })?;

    remove_dir_all_if_exists(path).map_err(|e| {
        format!(
            "Failed to delete OpenCode message directory {}: {e}",
            path.display()
        )
    })?;

    if let Some(session_file) = find_session_file(storage, session_id) {
        remove_file_if_exists(&session_file).map_err(|e| {
            format!(
                "Failed to delete OpenCode session file {}: {e}",
                session_file.display()
            )
        })?;
    }

    Ok(true)
}

fn parse_session(storage: &Path, path: &Path) -> Option<SessionMeta> {
    let data = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&data).ok()?;

    let session_id = value.get("id").and_then(Value::as_str)?.to_string();
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let directory = value
        .get("directory")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .or_else(|| value.get("modelName").and_then(Value::as_str))
        .or_else(|| {
            value
                .get("config")
                .and_then(|config| config.get("model"))
                .and_then(Value::as_str)
        })
        .map(|s| s.to_string());

    let created_at = value
        .get("time")
        .and_then(|t| t.get("created"))
        .and_then(parse_timestamp_to_ms);
    let updated_at = value
        .get("time")
        .and_then(|t| t.get("updated"))
        .and_then(parse_timestamp_to_ms);

    // Derive title from directory basename if no explicit title
    let has_title = title.is_some();
    let display_title = title.or_else(|| {
        directory
            .as_deref()
            .and_then(path_basename)
            .map(|s| s.to_string())
    });

    // Build source_path = message directory for this session
    let msg_dir = storage.join("message").join(&session_id);
    let source_path = msg_dir.to_string_lossy().to_string();

    // Skip expensive I/O if title already available from session JSON
    let summary = if has_title {
        display_title.clone()
    } else {
        get_first_user_summary(storage, &session_id)
    };

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title: display_title,
        summary,
        project_dir: directory,
        cwd: value
            .get("directory")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        model,
        created_at,
        last_active_at: updated_at.or(created_at),
        source_path: Some(source_path),
        resume_command: Some(format!("opencode session resume {session_id}")),
        archived: false,
        tokens_used: None,
        parent_session_id: None,
        unknown_records: 0,
        source_mtime: None,
        source_size: None,
        source_rank: 0,
    })
}

/// Read the first user message's first text part to use as summary.
fn get_first_user_summary(storage: &Path, session_id: &str) -> Option<String> {
    let msg_dir = storage.join("message").join(session_id);
    if !msg_dir.is_dir() {
        return None;
    }

    let mut msg_files = Vec::new();
    collect_json_files(&msg_dir, &mut msg_files);

    // Collect user messages with timestamps for ordering
    let mut user_msgs: Vec<(i64, String)> = Vec::new();
    for msg_path in &msg_files {
        let data = match std::fs::read_to_string(msg_path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let value: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if value.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }

        let msg_id = match value.get("id").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => continue,
        };

        let ts = value
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(parse_timestamp_to_ms)
            .unwrap_or(0);

        user_msgs.push((ts, msg_id));
    }

    user_msgs.sort_by_key(|(ts, _)| *ts);

    // Take first user message and get its parts
    let (_, first_id) = user_msgs.first()?;
    let part_dir = storage.join("part").join(first_id);
    let text = collect_parts_text(&part_dir);
    if text.trim().is_empty() {
        return None;
    }
    Some(truncate_summary(&text, 160))
}

/// Collect text content from all parts in a part directory.
fn collect_parts_text(part_dir: &Path) -> String {
    if !part_dir.is_dir() {
        return String::new();
    }

    let mut parts = Vec::new();
    collect_json_files(part_dir, &mut parts);

    let mut texts = Vec::new();
    for part_path in &parts {
        let data = match std::fs::read_to_string(part_path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let value: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Only include text-type parts
        if value.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }

        if let Some(text) = value.get("text").and_then(Value::as_str) {
            if !text.trim().is_empty() {
                texts.push(text.to_string());
            }
        }
    }

    texts.join("\n")
}

fn collect_json_files(root: &Path, files: &mut Vec<PathBuf>) {
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
            collect_json_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
}

fn find_session_file(storage: &Path, session_id: &str) -> Option<PathBuf> {
    let session_root = storage.join("session");
    let mut files = Vec::new();
    collect_json_files(&session_root, &mut files);
    let expected = format!("{session_id}.json");

    files
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(expected.as_str()))
}

fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn remove_dir_all_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn build_database(path: &Path) {
        let connection = Connection::open(path).expect("open fixture db");
        connection
            .execute_batch(
                r#"
                CREATE TABLE session (
                  id TEXT PRIMARY KEY, directory TEXT, title TEXT,
                  time_created INTEGER, time_updated INTEGER, model TEXT,
                  tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER,
                  time_archived INTEGER, version TEXT, parent_id TEXT
                );
                CREATE TABLE message (
                  id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT
                );
                CREATE TABLE part (
                  id TEXT PRIMARY KEY, session_id TEXT, message_id TEXT, data TEXT
                );
                CREATE TABLE session_message (
                  session_id TEXT, seq INTEGER, type TEXT, data TEXT
                );
                INSERT INTO session VALUES
                  ('v1', '/work/v1', 'Legacy session', 1000, 2000, '{"id":"model-v1"}', 10, 5, 2, NULL, '1.0', NULL),
                  ('v2', '/work/v2', 'Current session', 3000, 4000, '{"id":"model-v2"}', 20, 8, 3, NULL, '2.0', 'parent');
                INSERT INTO message VALUES
                  ('m1', 'v1', 1000, '{"role":"user","time":{"created":1000}}'),
                  ('m2', 'v1', 2000, '{"role":"assistant","time":{"created":2000},"model":{"id":"model-v1"}}');
                INSERT INTO part VALUES
                  ('p1', 'v1', 'm1', '{"type":"text","text":"legacy request"}'),
                  ('p2', 'v1', 'm2', '{"type":"tool","callID":"call-1","tool":"shell","state":{"input":{"command":"pwd"},"output":"/work"}}');
                INSERT INTO session_message VALUES
                  ('v2', 1, 'user', '{"text":"current request","content":[{"type":"text","text":"current request"}],"time":{"created":3000}}'),
                  ('v2', 2, 'assistant', '{"model":{"id":"model-v2"},"time":{"created":4000},"content":[{"type":"reasoning","text":"think"},{"type":"text","text":"current answer"}]}');
                "#,
            )
            .expect("build fixture db");
    }

    #[test]
    fn parses_v1_and_v2_sqlite_sessions() {
        let dir = tempdir().expect("tempdir");
        let database = dir.path().join("opencode.db");
        build_database(&database);

        let sessions = scan_database(&database, 1).expect("scan database");
        assert_eq!(sessions.len(), 2);
        assert_eq!(
            sessions
                .iter()
                .find(|session| session.session_id == "v2")
                .unwrap()
                .parent_session_id
                .as_deref(),
            Some("parent")
        );
        assert_eq!(
            sessions
                .iter()
                .find(|session| session.session_id == "v1")
                .unwrap()
                .tokens_used,
            Some(17)
        );

        let v1 = load_database_messages(&database, "v1").expect("load v1");
        assert!(v1.iter().any(|message| message.content == "legacy request"));
        assert!(v1.iter().any(|message| message.kind == "tool_use"));
        assert!(v1.iter().any(|message| message.kind == "tool_result"));

        let v2 = load_database_messages(&database, "v2").expect("load v2");
        assert!(v2.iter().any(|message| message.kind == "thinking"));
        assert!(v2.iter().any(|message| message.content == "current answer"));
        assert_eq!(
            v2.iter()
                .filter(|message| message.content == "current request")
                .count(),
            1
        );
    }

    #[test]
    fn legacy_storage_layout_still_loads() {
        let dir = tempdir().expect("tempdir");
        let storage = dir.path();
        let session_dir = storage.join("session/project");
        let message_dir = storage.join("message/legacy");
        let part_dir = storage.join("part/m1");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::create_dir_all(&message_dir).unwrap();
        std::fs::create_dir_all(&part_dir).unwrap();
        let session_file = session_dir.join("legacy.json");
        std::fs::write(&session_file, r#"{"id":"legacy","title":"Legacy","directory":"/work","time":{"created":1000,"updated":2000}}"#).unwrap();
        std::fs::write(
            message_dir.join("m1.json"),
            r#"{"id":"m1","role":"user","time":{"created":1000}}"#,
        )
        .unwrap();
        std::fs::write(
            part_dir.join("p1.json"),
            r#"{"type":"text","text":"legacy body"}"#,
        )
        .unwrap();

        assert!(parse_session(storage, &session_file).is_some());
        let messages = load_legacy_messages(&message_dir).expect("load legacy storage");
        assert_eq!(messages[0].content, "legacy body");
    }
}
