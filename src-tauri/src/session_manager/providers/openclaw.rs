use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::paths::get_openclaw_agents_dir;
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{
    extract_text, log_scan_error, normalize_title_candidate, parse_timestamp_to_ms, path_basename,
    sqlite_source_state, truncate_summary,
};

const PROVIDER_ID: &str = "openclaw";
const DB_NAME: &str = "openclaw-agent.sqlite";

#[derive(Clone, Default)]
struct DbRow {
    session_id: String,
    label: Option<String>,
    model: Option<String>,
    tokens_used: Option<i64>,
    parent_session_id: Option<String>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
    event_count: i64,
}

#[derive(Default)]
struct ClawParse {
    session_id: Option<String>,
    cwd: Option<String>,
    title: Option<String>,
    model: Option<String>,
    tokens_used: Option<i64>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
    unknown_records: u32,
    messages: Vec<SessionMessage>,
}

pub fn scan_sessions() -> Result<Vec<SessionMeta>, String> {
    let agents_dir = get_openclaw_agents_dir();
    let agent_entries = match std::fs::read_dir(&agents_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            log_scan_error(PROVIDER_ID, &agents_dir, &err);
            return Err(format!(
                "Failed to scan OpenClaw agents directory {}: {err}",
                agents_dir.display()
            ));
        }
    };
    let mut sessions = Vec::new();
    for agent_entry in agent_entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
    {
        let agent_path = agent_entry.path();
        let database = agent_path.join("agent").join(DB_NAME);
        if database.is_file() {
            sessions.extend(scan_database(&database)?);
        }
        let sessions_dir = agent_path.join("sessions");
        let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if is_legacy_transcript(name) {
                if let Some(meta) = parse_session(&path) {
                    sessions.push(meta);
                }
            }
        }
    }
    Ok(sessions)
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let entries = if let Some((database, session_id)) = split_virtual_source(path) {
        read_database_entries(&database, &session_id)?
    } else {
        read_jsonl_entries(path)?
    };
    Ok(render_entries(linearize(entries)).messages)
}

pub fn delete_session(_root: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    if split_virtual_source(path).is_some() {
        return Err("Deleting SQLite-backed OpenClaw sessions is not supported".to_string());
    }
    let meta = parse_session(path).ok_or_else(|| {
        format!(
            "Failed to parse OpenClaw session metadata: {}",
            path.display()
        )
    })?;
    if meta.session_id != session_id {
        return Err(format!(
            "OpenClaw session ID mismatch: expected {session_id}, found {}",
            meta.session_id
        ));
    }
    let index_path = path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join("sessions.json");
    prune_sessions_index(&index_path, session_id, path)?;
    std::fs::remove_file(path).map_err(|e| {
        format!(
            "Failed to delete OpenClaw session file {}: {e}",
            path.display()
        )
    })?;
    Ok(true)
}

fn scan_database(path: &Path) -> Result<Vec<SessionMeta>, String> {
    let connection = open_database(path)?;
    let rows = query_database_rows(&connection)?;
    let (database_mtime, database_size) = sqlite_source_state(path);
    Ok(rows
        .into_iter()
        .filter(|row| row.event_count > 0)
        .map(|row| {
            let title = row
                .label
                .clone()
                .and_then(|value| normalize_title_candidate(&value, 160));
            SessionMeta {
                provider_id: PROVIDER_ID.to_string(),
                session_id: row.session_id.clone(),
                title: title.clone(),
                summary: title,
                project_dir: None,
                cwd: None,
                model: row.model,
                created_at: row.created_at,
                last_active_at: row.updated_at.or(database_mtime),
                source_path: Some(virtual_source(path, &row.session_id)),
                resume_command: None,
                archived: false,
                tokens_used: row.tokens_used.filter(|value| *value > 0),
                parent_session_id: row.parent_session_id,
                unknown_records: 0,
                source_mtime: database_mtime,
                source_size: database_size,
                source_rank: 2,
            }
        })
        .collect())
}

fn open_database(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open OpenClaw database {}: {e}", path.display()))
}

fn query_database_rows(connection: &Connection) -> Result<Vec<DbRow>, String> {
    let mut statement = connection
        .prepare(
            "SELECT w.session_id, w.spawned_by, w.created_at, w.updated_at,
                    w.transcript_updated_at, w.started_at, w.model, w.display_name,
                    n.label, n.display_name, n.entry_json,
                    COUNT(e.seq), COALESCE(SUM(LENGTH(e.event_json)), 0),
                    COALESCE(MAX(e.created_at), 0), w.parent_session_key
             FROM session_windows w
             LEFT JOIN session_nodes n ON n.session_key = w.session_key
             LEFT JOIN transcript_events e ON e.session_id = w.session_id
             GROUP BY w.session_id",
        )
        .map_err(|e| format!("Failed to prepare OpenClaw session query: {e}"))?;
    let rows = statement
        .query_map([], |row| {
            let session_id: String = row.get(0)?;
            let entry: Value = row
                .get::<_, Option<String>>(10)?
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or(Value::Null);
            let label = row
                .get::<_, Option<String>>(8)?
                .or(row.get::<_, Option<String>>(9)?)
                .or(row.get::<_, Option<String>>(7)?)
                .or_else(|| {
                    entry
                        .get("label")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .or_else(|| {
                    entry
                        .get("displayName")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            let exact_parent = entry
                .get("parentSessionId")
                .or_else(|| entry.get("spawnedBySessionId"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let parent_session_id =
                exact_parent
                    .or(row.get::<_, Option<String>>(1)?)
                    .or(row.get::<_, Option<String>>(14)?);
            let created_at = [row.get::<_, Option<i64>>(5)?, row.get::<_, Option<i64>>(2)?]
                .into_iter()
                .flatten()
                .map(normalize_epoch_ms)
                .find(|value| *value > 0);
            let updated_at = [
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(13)?,
                row.get::<_, Option<i64>>(3)?,
            ]
            .into_iter()
            .flatten()
            .map(normalize_epoch_ms)
            .max();
            let is_current = entry.get("sessionId").and_then(Value::as_str) == Some(&session_id);
            Ok(DbRow {
                session_id,
                label,
                model: row.get::<_, Option<String>>(6)?.or_else(|| {
                    entry
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }),
                tokens_used: is_current
                    .then(|| entry.get("totalTokens").and_then(Value::as_i64))
                    .flatten(),
                parent_session_id,
                created_at,
                updated_at,
                event_count: row.get(11)?,
            })
        })
        .map_err(|e| format!("Failed to query OpenClaw sessions: {e}"))?;
    Ok(rows.flatten().collect())
}

fn read_database_entries(database: &Path, session_id: &str) -> Result<Vec<Value>, String> {
    let connection = open_database(database)?;
    let mut statement = connection
        .prepare("SELECT seq, event_json FROM transcript_events WHERE session_id = ?1 ORDER BY seq")
        .map_err(|e| format!("Failed to prepare OpenClaw transcript query: {e}"))?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query OpenClaw transcript: {e}"))?;
    let mut entries = Vec::new();
    for (seq, data) in rows.flatten() {
        if let Ok(value) = serde_json::from_str::<Value>(&data) {
            entries.push((seq, value));
        }
    }
    let active = connection
        .prepare(
            "SELECT event_seq FROM session_transcript_active_events
             WHERE session_id = ?1 ORDER BY active_position",
        )
        .and_then(|mut statement| {
            statement
                .query_map([session_id], |row| row.get::<_, i64>(0))
                .map(|rows| rows.flatten().collect::<Vec<_>>())
        })
        .unwrap_or_default();
    if active.is_empty() {
        return Ok(entries.into_iter().map(|(_, value)| value).collect());
    }
    let active: HashSet<i64> = active.into_iter().collect();
    Ok(entries
        .into_iter()
        .filter_map(|(seq, value)| active.contains(&seq).then_some(value))
        .collect())
}

fn read_jsonl_entries(path: &Path) -> Result<Vec<Value>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open OpenClaw session: {e}"))?;
    let mut entries = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| format!("Failed to read OpenClaw session: {e}"))?;
        if let Ok(value) = serde_json::from_str(&line) {
            entries.push(value);
        }
    }
    Ok(entries)
}

fn linearize(mut entries: Vec<Value>) -> Vec<Value> {
    let header = entries
        .iter()
        .position(|value| value.get("type").and_then(Value::as_str) == Some("session"));
    let by_id: HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            value
                .get("id")
                .and_then(Value::as_str)
                .map(|id| (id.to_string(), index))
        })
        .collect();
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = entries
        .iter()
        .rposition(|value| value.get("id").and_then(Value::as_str).is_some());
    while let Some(index) = cursor {
        if !seen.insert(index) {
            break;
        }
        chain.push(index);
        cursor = entries[index]
            .get("parentId")
            .and_then(Value::as_str)
            .and_then(|parent| by_id.get(parent).copied());
    }
    chain.reverse();
    let mut result = Vec::with_capacity(chain.len() + 1);
    if let Some(index) = header {
        result.push(std::mem::take(&mut entries[index]));
    }
    for index in chain {
        if Some(index) != header {
            result.push(std::mem::take(&mut entries[index]));
        }
    }
    result
}

fn render_entries(entries: Vec<Value>) -> ClawParse {
    let mut parsed = ClawParse::default();
    let mut tool_names = HashMap::<String, String>::new();
    for value in entries {
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let ts = value
            .get("timestamp")
            .and_then(claw_timestamp_ms)
            .or_else(|| {
                value
                    .pointer("/message/timestamp")
                    .and_then(claw_timestamp_ms)
            });
        if parsed.created_at.is_none() {
            parsed.created_at = ts;
        }
        if ts.is_some() {
            parsed.updated_at = ts;
        }
        match event_type {
            "session" => {
                parsed.session_id = value.get("id").and_then(Value::as_str).map(str::to_string);
                parsed.cwd = value.get("cwd").and_then(Value::as_str).map(str::to_string);
            }
            "message" => {
                let Some(message) = value.get("message") else {
                    parsed.unknown_records = parsed.unknown_records.saturating_add(1);
                    continue;
                };
                let role = message
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if let Some(model) = message.get("model").and_then(Value::as_str) {
                    parsed.model = Some(model.to_string());
                }
                if let Some(usage) = message.get("usage") {
                    let total = ["input", "output", "cacheRead", "cacheWrite"]
                        .iter()
                        .filter_map(|key| usage.get(*key).and_then(Value::as_i64))
                        .sum::<i64>();
                    if total > 0 {
                        parsed.tokens_used =
                            Some(parsed.tokens_used.unwrap_or(0).saturating_add(total));
                    }
                }
                if role == "toolResult" {
                    let call_id = message
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let name = call_id.as_ref().and_then(|id| tool_names.get(id)).cloned();
                    let content = message.get("content").map(extract_text).unwrap_or_default();
                    if !content.trim().is_empty() {
                        let mut result = SessionMessage::structured(
                            "tool".to_string(),
                            "tool_result",
                            name,
                            call_id,
                            content,
                            ts,
                        );
                        result.is_error =
                            message.get("isError").and_then(Value::as_bool) == Some(true);
                        parsed.messages.push(result);
                    }
                    continue;
                }
                let blocks = message.get("content").and_then(Value::as_array);
                if let Some(blocks) = blocks {
                    for block in blocks {
                        match block.get("type").and_then(Value::as_str) {
                            Some("text") => {
                                if let Some(text) = block
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .filter(|text| !text.trim().is_empty())
                                {
                                    parsed.messages.push(SessionMessage::plain(
                                        role.to_string(),
                                        text.trim().to_string(),
                                        ts,
                                    ));
                                }
                            }
                            Some("thinking") => {
                                if let Some(text) = block
                                    .get("thinking")
                                    .and_then(Value::as_str)
                                    .filter(|text| !text.trim().is_empty())
                                {
                                    parsed.messages.push(SessionMessage::structured(
                                        "assistant".to_string(),
                                        "thinking",
                                        None,
                                        None,
                                        text.trim().to_string(),
                                        ts,
                                    ));
                                }
                            }
                            Some("toolCall") => {
                                let name = block
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("tool")
                                    .to_string();
                                let call_id =
                                    block.get("id").and_then(Value::as_str).map(str::to_string);
                                if let Some(id) = call_id.as_ref() {
                                    tool_names.insert(id.clone(), name.clone());
                                }
                                let input = block.get("arguments").cloned().unwrap_or(Value::Null);
                                let input = serde_json::to_string_pretty(&input)
                                    .unwrap_or_else(|_| input.to_string());
                                let mut call = SessionMessage::structured(
                                    "assistant".to_string(),
                                    "tool_use",
                                    Some(name.clone()),
                                    call_id,
                                    input,
                                    ts,
                                );
                                call.tool_names.push(name);
                                parsed.messages.push(call);
                            }
                            Some("image") => parsed.messages.push(SessionMessage::plain(
                                role.to_string(),
                                "[Image]".to_string(),
                                ts,
                            )),
                            _ => {}
                        }
                    }
                } else {
                    let content = message.get("content").map(extract_text).unwrap_or_default();
                    if !content.trim().is_empty() {
                        parsed
                            .messages
                            .push(SessionMessage::plain(role.to_string(), content, ts));
                    }
                }
            }
            "model_change" => {
                parsed.model = value
                    .get("modelId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            "compaction" | "branch_summary" => {
                if let Some(summary) = value
                    .get("summary")
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                {
                    parsed.messages.push(SessionMessage::structured(
                        "system".to_string(),
                        "compact_summary",
                        None,
                        None,
                        summary.trim().to_string(),
                        ts,
                    ));
                }
            }
            "custom_message" => {
                if value.get("display").and_then(Value::as_bool) == Some(true) {
                    let content = value.get("content").map(extract_text).unwrap_or_default();
                    if !content.trim().is_empty() {
                        parsed.messages.push(SessionMessage::structured(
                            "system".to_string(),
                            "meta",
                            None,
                            None,
                            content,
                            ts,
                        ));
                    }
                }
            }
            "session_info" => {
                parsed.title = value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            "thinking_level_change" | "label" | "custom" | "reset" => {}
            _ => parsed.unknown_records = parsed.unknown_records.saturating_add(1),
        }
    }
    parsed
}

fn parse_session(path: &Path) -> Option<SessionMeta> {
    let parsed = render_entries(linearize(read_jsonl_entries(path).ok()?));
    let session_id = parsed
        .session_id
        .or_else(|| path.file_stem()?.to_str().map(str::to_string))?;
    let first_user = parsed
        .messages
        .iter()
        .find(|message| message.role == "user" && message.kind == "message")
        .and_then(|message| normalize_title_candidate(&message.content, 160));
    let title = parsed
        .title
        .and_then(|value| normalize_title_candidate(&value, 160))
        .or(first_user)
        .or_else(|| parsed.cwd.as_deref().and_then(path_basename));
    let summary = parsed
        .messages
        .iter()
        .rev()
        .find(|message| message.kind == "message")
        .map(|message| truncate_summary(&message.content, 160));
    let metadata = path.metadata().ok();
    let source_mtime = metadata
        .as_ref()
        .and_then(|value| value.modified().ok())
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|value| i64::try_from(value.as_millis()).ok());
    let source_size = metadata.and_then(|value| i64::try_from(value.len()).ok());
    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id,
        title,
        summary,
        project_dir: parsed.cwd.clone(),
        cwd: parsed.cwd,
        model: parsed.model,
        created_at: parsed.created_at,
        last_active_at: parsed.updated_at.or(source_mtime),
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: None,
        archived: false,
        tokens_used: parsed.tokens_used,
        parent_session_id: None,
        unknown_records: parsed.unknown_records,
        source_mtime,
        source_size,
        source_rank: 1,
    })
}

fn is_legacy_transcript(name: &str) -> bool {
    name.ends_with(".jsonl")
        && !name.starts_with('.')
        && !name.contains(".deleted.")
        && !name.contains(".reset.")
        && !name.contains(".checkpoint.")
}

fn virtual_source(database: &Path, session_id: &str) -> String {
    format!("{}#{session_id}", database.to_string_lossy())
}

fn split_virtual_source(path: &Path) -> Option<(PathBuf, String)> {
    let source = path.to_string_lossy();
    let (database, session_id) = source.rsplit_once('#')?;
    let database = PathBuf::from(database);
    (database.is_file() && !session_id.is_empty()).then(|| (database, session_id.to_string()))
}

fn claw_timestamp_ms(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .map(normalize_epoch_ms)
        .or_else(|| parse_timestamp_to_ms(value))
}

fn normalize_epoch_ms(value: i64) -> i64 {
    if value > 10_000_000_000 {
        value
    } else {
        value.saturating_mul(1000)
    }
}

fn prune_sessions_index(
    index_path: &Path,
    session_id: &str,
    source_path: &Path,
) -> Result<(), String> {
    if !index_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(index_path).map_err(|e| {
        format!(
            "Failed to read OpenClaw sessions index {}: {e}",
            index_path.display()
        )
    })?;
    let mut index: serde_json::Map<String, Value> =
        serde_json::from_str(&content).map_err(|e| {
            format!(
                "Failed to parse OpenClaw sessions index {}: {e}",
                index_path.display()
            )
        })?;

    let source = source_path.to_string_lossy();
    index.retain(|_, entry| {
        let same_id = entry.get("sessionId").and_then(Value::as_str) == Some(session_id);
        let same_file = entry.get("sessionFile").and_then(Value::as_str) == Some(source.as_ref());
        !(same_id || same_file)
    });

    let serialized = serde_json::to_string_pretty(&index).map_err(|e| {
        format!(
            "Failed to serialize OpenClaw sessions index {}: {e}",
            index_path.display()
        )
    })?;

    std::fs::write(index_path, format!("{serialized}\n")).map_err(|e| {
        format!(
            "Failed to update OpenClaw sessions index {}: {e}",
            index_path.display()
        )
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn linearizes_legacy_tree_to_latest_leaf() {
        let entries = vec![
            serde_json::json!({"type":"session","id":"s","cwd":"/work"}),
            serde_json::json!({"type":"message","id":"root","message":{"role":"user","content":[{"type":"text","text":"root"}]}}),
            serde_json::json!({"type":"message","id":"dead","parentId":"root","message":{"role":"assistant","content":[{"type":"text","text":"dead"}]}}),
            serde_json::json!({"type":"message","id":"live","parentId":"root","message":{"role":"assistant","content":[{"type":"text","text":"live"}]}}),
        ];
        let parsed = render_entries(linearize(entries));
        assert!(parsed
            .messages
            .iter()
            .any(|message| message.content == "live"));
        assert!(!parsed
            .messages
            .iter()
            .any(|message| message.content == "dead"));
    }

    #[test]
    fn parses_sqlite_window_and_active_events() {
        let dir = tempdir().expect("tempdir");
        let database = dir.path().join(DB_NAME);
        let connection = Connection::open(&database).expect("open fixture db");
        connection.execute_batch(
            r#"
            CREATE TABLE session_windows (
              session_id TEXT, spawned_by TEXT, created_at INTEGER, updated_at INTEGER,
              transcript_updated_at INTEGER, started_at INTEGER, model TEXT,
              display_name TEXT, session_key TEXT, parent_session_key TEXT
            );
            CREATE TABLE session_nodes (
              session_key TEXT, label TEXT, display_name TEXT, entry_json TEXT
            );
            CREATE TABLE transcript_events (
              session_id TEXT, seq INTEGER, event_json TEXT, created_at INTEGER
            );
            CREATE TABLE session_transcript_active_events (
              session_id TEXT, event_seq INTEGER, active_position INTEGER
            );
            INSERT INTO session_windows VALUES ('claw-1', NULL, 1000, 4000, 4000, 1000, 'model-claw', 'Window', 'key-1', NULL);
            INSERT INTO session_windows VALUES ('claw-empty', NULL, 5000, 5000, 5000, 5000, 'model-claw', 'Empty', 'key-empty', NULL);
            INSERT INTO session_nodes VALUES ('key-1', 'Claw title', NULL, '{"sessionId":"claw-1","totalTokens":99}');
            INSERT INTO transcript_events VALUES
              ('claw-1', 0, '{"type":"session","id":"claw-1","cwd":"/work"}', 1000),
              ('claw-1', 1, '{"type":"message","id":"u1","message":{"role":"user","content":[{"type":"text","text":"hello"}]}}', 2000),
              ('claw-1', 2, '{"type":"message","id":"a1","parentId":"u1","message":{"role":"assistant","content":[{"type":"text","text":"world"}]}}', 3000);
            INSERT INTO session_transcript_active_events VALUES ('claw-1', 0, 0), ('claw-1', 1, 1), ('claw-1', 2, 2);
            "#,
        ).expect("build OpenClaw db");
        drop(connection);

        let sessions = scan_database(&database).expect("scan database");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title.as_deref(), Some("Claw title"));
        assert_eq!(sessions[0].tokens_used, Some(99));
        let messages = load_messages(Path::new(&virtual_source(&database, "claw-1")))
            .expect("load db transcript");
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.kind == "message")
                .count(),
            2
        );
    }
}
