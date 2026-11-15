use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde_json::Value;

use super::utils::{
    extract_text, log_scan_error, normalize_title_candidate, parse_timestamp_to_ms,
};
use crate::paths::get_gemini_tmp_dir;
use crate::session_manager::{SessionMessage, SessionMeta};
const PROVIDER_ID: &str = "gemini";

#[derive(Default)]
struct GeminiParse {
    session_id: Option<String>,
    summary: Option<String>,
    model: Option<String>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
    unknown_records: u32,
    messages: Vec<SessionMessage>,
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    let tmp_dir = get_gemini_tmp_dir();
    if !tmp_dir.exists() {
        return Vec::new();
    }
    let projects = load_project_map(&tmp_dir);
    let project_dirs = match std::fs::read_dir(&tmp_dir) {
        Ok(entries) => entries,
        Err(err) => {
            log_scan_error(PROVIDER_ID, &tmp_dir, &err);
            return Vec::new();
        }
    };

    let mut sessions = Vec::new();
    for entry in project_dirs.flatten() {
        let chats_dir = entry.path().join("chats");
        let Ok(chat_files) = std::fs::read_dir(&chats_dir) else {
            continue;
        };
        for file_entry in chat_files.flatten() {
            let path = file_entry.path();
            let extension = path.extension().and_then(|value| value.to_str());
            if !matches!(extension, Some("json" | "jsonl")) {
                continue;
            }
            let cwd = entry
                .file_name()
                .to_str()
                .and_then(|slug| projects.get(slug))
                .cloned();
            if let Some(meta) = parse_session_with_cwd(&path, cwd) {
                sessions.push(meta);
            }
        }
    }
    sessions
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    parse_gemini(path).map(|parsed| parsed.messages)
}

pub fn delete_session(_root: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    let meta = parse_session(path).ok_or_else(|| {
        format!(
            "Failed to parse Gemini session metadata: {}",
            path.display()
        )
    })?;
    if meta.session_id != session_id {
        return Err(format!(
            "Gemini session ID mismatch: expected {session_id}, found {}",
            meta.session_id
        ));
    }
    std::fs::remove_file(path).map_err(|e| {
        format!(
            "Failed to delete Gemini session file {}: {e}",
            path.display()
        )
    })?;
    Ok(true)
}

fn parse_gemini(path: &Path) -> Result<GeminiParse, String> {
    if path.extension().and_then(|value| value.to_str()) == Some("json") {
        return parse_legacy_json(path);
    }
    let file = File::open(path).map_err(|e| format!("Failed to open Gemini session: {e}"))?;
    let reader = BufReader::with_capacity(1 << 20, file);
    let mut parsed = GeminiParse::default();
    let mut last_messages = None;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read Gemini session: {e}"))?;
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
        if let Some(set) = value.get("$set") {
            if let Some(messages) = set.get("messages").and_then(Value::as_array) {
                last_messages = Some(messages.clone());
            }
            continue;
        }
        if value.get("sessionId").and_then(Value::as_str).is_some() {
            parsed.session_id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string);
            parsed.created_at = value.get("startTime").and_then(parse_timestamp_to_ms);
            parsed.updated_at = value.get("lastUpdated").and_then(parse_timestamp_to_ms);
            parsed.summary = value
                .get("summary")
                .and_then(Value::as_str)
                .map(str::to_string);
            parsed.model = value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string);
        } else {
            parsed.unknown_records = parsed.unknown_records.saturating_add(1);
        }
    }
    parsed.messages = parse_messages(last_messages.as_deref().unwrap_or(&[]), &mut parsed);
    Ok(parsed)
}

fn parse_legacy_json(path: &Path) -> Result<GeminiParse, String> {
    let data =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read Gemini session: {e}"))?;
    let value: Value = serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse Gemini session JSON: {e}"))?;
    let mut parsed = GeminiParse {
        session_id: value
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string),
        summary: value
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_string),
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string),
        created_at: value.get("startTime").and_then(parse_timestamp_to_ms),
        updated_at: value.get("lastUpdated").and_then(parse_timestamp_to_ms),
        ..GeminiParse::default()
    };
    let messages = value
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    parsed.messages = parse_messages(&messages, &mut parsed);
    Ok(parsed)
}

fn parse_messages(messages: &[Value], parsed: &mut GeminiParse) -> Vec<SessionMessage> {
    let mut result = Vec::new();
    for message in messages {
        let Some(raw_role) = message.get("type").and_then(Value::as_str) else {
            parsed.unknown_records = parsed.unknown_records.saturating_add(1);
            continue;
        };
        let role = if raw_role == "user" {
            "user"
        } else {
            "assistant"
        };
        let content = message.get("content").map(extract_text).unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let ts = message.get("timestamp").and_then(parse_timestamp_to_ms);
        if parsed.created_at.is_none() {
            parsed.created_at = ts;
        }
        if ts.is_some() {
            parsed.updated_at = ts;
        }
        result.push(SessionMessage::plain(role.to_string(), content, ts));
    }
    result
}

fn parse_session(path: &Path) -> Option<SessionMeta> {
    parse_session_with_cwd(path, None)
}

fn parse_session_with_cwd(path: &Path, cwd: Option<String>) -> Option<SessionMeta> {
    let parsed = parse_gemini(path).ok()?;
    let session_id = parsed
        .session_id
        .or_else(|| path.file_stem()?.to_str().map(str::to_string))?;
    let first_user = parsed
        .messages
        .iter()
        .find(|message| message.role == "user")
        .and_then(|message| normalize_title_candidate(&message.content, 160));
    let title = parsed
        .summary
        .as_deref()
        .and_then(|value| normalize_title_candidate(value, 160))
        .or(first_user);
    let metadata = path.metadata().ok();
    let source_mtime = metadata
        .as_ref()
        .and_then(|value| value.modified().ok())
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|value| i64::try_from(value.as_millis()).ok());
    let source_size = metadata.and_then(|value| i64::try_from(value.len()).ok());

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title: title.clone(),
        summary: title,
        project_dir: cwd.clone(),
        cwd,
        model: parsed.model,
        created_at: parsed.created_at,
        last_active_at: parsed.updated_at.or(parsed.created_at).or(source_mtime),
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: Some(format!("gemini --resume {session_id}")),
        archived: false,
        tokens_used: None,
        parent_session_id: None,
        unknown_records: parsed.unknown_records,
        source_mtime,
        source_size,
        source_rank: if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            1
        } else {
            0
        },
    })
}

fn load_project_map(tmp_dir: &Path) -> HashMap<String, String> {
    let path = tmp_dir.parent().unwrap_or(tmp_dir).join("projects.json");
    let Ok(data) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return HashMap::new();
    };
    value
        .get("projects")
        .and_then(Value::as_object)
        .map(|projects| {
            projects
                .iter()
                .filter_map(|(path, slug)| Some((slug.as_str()?.to_string(), path.clone())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn parse_session_prefers_summary_for_title() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.json");
        fs::write(
            &path,
            r#"{
  "sessionId": "827eede4-2d4d-4d9b-900b-ddea7069ffb1",
  "startTime": "2025-12-20T10:07:54.366Z",
  "lastUpdated": "2025-12-20T13:48:46.055Z",
  "summary": "Summarize video transcript and save to file.",
  "messages": [
    {
      "type": "user",
      "timestamp": "2025-12-20T10:07:54.366Z",
      "content": "总结 https://www.youtube.com/watch?v=Hzl4WtUKg-U"
    }
  ]
}"#,
        )
        .expect("write gemini fixture");

        let meta = parse_session(&path).expect("parse session");

        assert_eq!(
            meta.title.as_deref(),
            Some("Summarize video transcript and save to file.")
        );
    }
    #[test]
    fn parses_latest_jsonl_snapshot_and_counts_bad_records() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session-current.jsonl");
        fs::write(
            &path,
            [
                r#"{"sessionId":"gem-current","startTime":"2026-11-01T00:00:00Z","lastUpdated":"2026-11-01T00:00:03Z"}"#,
                r#"{"$set":{"messages":[{"type":"user","content":"old","timestamp":"2026-11-01T00:00:01Z"}]}}"#,
                "not-json",
                r#"{"$set":{"messages":[{"type":"user","content":"new request","timestamp":"2026-11-01T00:00:02Z"},{"type":"gemini","content":"new answer","timestamp":"2026-11-01T00:00:03Z"}]}}"#,
            ]
            .join("\n"),
        )
        .expect("write Gemini JSONL fixture");

        let parsed = parse_gemini(&path).expect("parse Gemini JSONL");
        assert_eq!(parsed.messages.len(), 2);
        assert_eq!(parsed.messages[0].content, "new request");
        assert_eq!(parsed.unknown_records, 1);
    }
}
