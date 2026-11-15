use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::paths::get_dsh_sessions_dir;
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{
    extract_text, log_scan_error, normalize_title_candidate, parse_timestamp_to_ms, path_basename,
    truncate_summary,
};

const PROVIDER_ID: &str = "dsh";
const RESUME_COMMAND: &str = "npx @deepseek-ai/dsh web";

#[derive(Default)]
struct ParsedSession {
    session_id: Option<String>,
    cwd: Option<String>,
    title: Option<String>,
    label: Option<String>,
    model: Option<String>,
    tokens_used: Option<i64>,
    parent_session_id: Option<String>,
    subagent: bool,
    seed_length: Option<i64>,
    inherited_cut: Option<usize>,
    unknown_records: u32,
    created_at: Option<i64>,
    last_active_at: Option<i64>,
    summary: Option<String>,
    messages: Vec<SessionMessage>,
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    let root = get_dsh_sessions_dir();
    let mut logs = Vec::new();
    collect_session_logs(&root, &mut logs);
    logs.into_iter()
        .filter_map(|path| parse_session(&path).ok())
        .collect()
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    Ok(parse_dsh_log(path)?.messages)
}

pub fn delete_session(root: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("Invalid DSH session log path: {}", path.display()))?;
    if log_generation(file_name).is_none() {
        return Err(format!("Invalid DSH session log name: {}", path.display()));
    }

    let session_dir = path.parent().ok_or_else(|| {
        format!(
            "DSH session log has no parent directory: {}",
            path.display()
        )
    })?;
    let project_dir = session_dir.parent().ok_or_else(|| {
        format!(
            "DSH session directory has no project parent: {}",
            session_dir.display()
        )
    })?;
    if project_dir.parent() != Some(root) {
        return Err(format!(
            "Unexpected DSH session path layout: {}",
            path.display()
        ));
    }

    let selected_log = select_session_log(session_dir)
        .ok_or_else(|| format!("No DSH session log found in {}", session_dir.display()))?
        .canonicalize()
        .map_err(|e| format!("Failed to resolve DSH session log {}: {e}", path.display()))?;
    if selected_log != path {
        return Err(format!(
            "DSH session source is not the selected log: {}",
            path.display()
        ));
    }

    let meta = parse_session(path)?;
    if meta.session_id != session_id {
        return Err(format!(
            "DSH session ID mismatch: expected {session_id}, found {}",
            meta.session_id
        ));
    }

    fs::remove_dir_all(session_dir).map_err(|e| {
        format!(
            "Failed to delete DSH session directory {}: {e}",
            session_dir.display()
        )
    })?;
    Ok(true)
}

fn parse_session(path: &Path) -> Result<SessionMeta, String> {
    let parsed = parse_dsh_log(path)?;
    let session_id = parsed
        .session_id
        .or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("DSH session ID missing: {}", path.display()))?;

    let project_dir = parsed.cwd.or_else(|| {
        path.parent()
            .and_then(Path::parent)
            .and_then(Path::to_str)
            .map(str::to_string)
    });
    let title = parsed
        .title
        .or(parsed.label)
        .or_else(|| first_user_title(&parsed.messages))
        .or_else(|| project_dir.as_deref().and_then(path_basename));
    let fallback_ts = file_mtime_ms(path);

    Ok(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id,
        title,
        summary: parsed.summary.map(|value| truncate_summary(&value, 160)),
        project_dir: project_dir.clone(),
        cwd: project_dir,
        model: parsed.model,
        created_at: parsed.created_at.or(fallback_ts),
        last_active_at: parsed.last_active_at.or(fallback_ts),
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: Some(RESUME_COMMAND.to_string()),
        archived: false,
        tokens_used: parsed.tokens_used,
        parent_session_id: parsed.parent_session_id,
        unknown_records: parsed.unknown_records,
        source_mtime: fallback_ts,
        source_size: path
            .metadata()
            .ok()
            .and_then(|value| i64::try_from(value.len()).ok()),
        source_rank: path
            .file_name()
            .and_then(|value| value.to_str())
            .and_then(log_generation)
            .and_then(|(generation, _)| u8::try_from(generation).ok())
            .unwrap_or(0),
    })
}

fn parse_dsh_log(path: &Path) -> Result<ParsedSession, String> {
    let mut parsed = ParsedSession::default();
    let reader = open_log(path)?;
    let mut tool_names = HashMap::<String, String>::new();

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                parsed.unknown_records = parsed.unknown_records.saturating_add(1);
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = match serde_json::from_str(&line) {
            Ok(row) => row,
            Err(_) => {
                parsed.unknown_records = parsed.unknown_records.saturating_add(1);
                continue;
            }
        };
        if row.pointer("/surfaceOp/op").and_then(Value::as_str) == Some("replace") {
            continue;
        }

        let event_type = row.get("type").and_then(Value::as_str).unwrap_or_default();
        let ts = row.get("time").and_then(timestamp_ms);
        if let Some(ts) = ts {
            parsed.last_active_at = Some(parsed.last_active_at.unwrap_or(0).max(ts));
        }
        let data = row.get("data").unwrap_or(&Value::Null);
        if parsed.inherited_cut.is_none()
            && parsed.seed_length.is_some_and(|length| {
                row.get("seq")
                    .and_then(Value::as_i64)
                    .is_some_and(|seq| seq >= length)
            })
        {
            parsed.inherited_cut = Some(parsed.messages.len());
        }

        match event_type {
            "session" => {
                parsed.session_id = row.get("id").and_then(Value::as_str).map(str::to_string);
                parsed.cwd = row.get("cwd").and_then(Value::as_str).map(str::to_string);
                parsed.created_at = row.get("createdAt").and_then(timestamp_ms);
                parsed.seed_length = row.get("seedLength").and_then(Value::as_i64);
                parsed.parent_session_id = row
                    .get("parentSession")
                    .or_else(|| row.get("parentSessionId"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                parsed.subagent = row.get("origin").and_then(Value::as_str) == Some("subagent")
                    || row
                        .get("delegationDepth")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        > 0;
                if row
                    .get("version")
                    .and_then(Value::as_u64)
                    .is_some_and(|version| version > 4)
                {
                    parsed.unknown_records = parsed.unknown_records.saturating_add(1);
                }
            }
            "session/title" => {
                if let Some(title) = data.get("title").and_then(Value::as_str) {
                    parsed.title = normalize_title_candidate(title, 160);
                }
            }
            "request/context" => {
                if parsed.model.is_none() {
                    parsed.model = data
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
            }
            "system/message" | "developer/message" => {
                let content = data
                    .pointer("/message/content")
                    .map(extract_text)
                    .unwrap_or_default();
                push_plain(&mut parsed, "developer", content, ts, false);
            }
            "user/message" => {
                let content = data.get("content").map(extract_text).unwrap_or_default();
                let source_kind = data.pointer("/source/kind").and_then(Value::as_str);
                let is_user = source_kind.is_none() || source_kind == Some("user");
                push_plain(
                    &mut parsed,
                    if is_user { "user" } else { "developer" },
                    content,
                    ts,
                    is_user,
                );
            }
            "assistant/message" => {
                let message = data.get("message").unwrap_or(&Value::Null);
                let message_model = message
                    .pointer("/source/model")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if message_model.is_some() {
                    parsed.model = message_model.clone();
                }
                if let Some(usage) = data.get("usage") {
                    let total = [
                        "inputTokens",
                        "outputTokens",
                        "cacheReadTokens",
                        "cacheWriteTokens",
                    ]
                    .iter()
                    .filter_map(|key| usage.get(*key).and_then(Value::as_i64))
                    .sum::<i64>();
                    if total > 0 {
                        parsed.tokens_used =
                            Some(parsed.tokens_used.unwrap_or(0).saturating_add(total));
                    }
                }
                for block in message
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let content = block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let before = parsed.messages.len();
                            push_plain(&mut parsed, "assistant", content, ts, true);
                            if parsed.messages.len() > before {
                                parsed.messages.last_mut().unwrap().model = message_model.clone();
                            }
                        }
                        Some("reasoning") => {
                            let content = block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .trim()
                                .to_string();
                            if !content.is_empty() {
                                let mut reasoning = SessionMessage::structured(
                                    "assistant".to_string(),
                                    "thinking",
                                    None,
                                    None,
                                    content,
                                    ts,
                                );
                                reasoning.model = message_model.clone();
                                parsed.messages.push(reasoning);
                            }
                        }
                        Some("tool-call") => {
                            let call_id =
                                block.get("id").and_then(Value::as_str).map(str::to_string);
                            let name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            if let (Some(call_id), Some(name)) = (call_id.clone(), name.clone()) {
                                tool_names.insert(call_id, name);
                            }
                            let content = format_arguments(block.get("arguments"));
                            let mut call = SessionMessage::structured(
                                "assistant".to_string(),
                                "tool_use",
                                name.clone(),
                                call_id,
                                content,
                                ts,
                            );
                            call.model = message_model.clone();
                            if let Some(name) = name {
                                call.tool_names.push(name);
                            }
                            parsed.messages.push(call);
                        }
                        Some("image") | Some("file") => {
                            parsed.messages.push(SessionMessage::plain(
                                "assistant".to_string(),
                                "[Image]".to_string(),
                                ts,
                            ));
                        }
                        _ => {}
                    }
                }
            }
            "tool/result" => {
                let message = data.get("message").unwrap_or(&Value::Null);
                let block = if message.get("toolCallId").is_some() {
                    message
                } else {
                    message.pointer("/content/0").unwrap_or(&Value::Null)
                };
                let call_id = block
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let name = call_id
                    .as_ref()
                    .and_then(|value| tool_names.get(value))
                    .cloned();
                let content = block.get("content").map(extract_text).unwrap_or_default();
                if !content.trim().is_empty() {
                    let mut result = SessionMessage::structured(
                        "tool".to_string(),
                        "tool_result",
                        name,
                        call_id,
                        content,
                        ts,
                    );
                    result.is_error = block.get("isError").and_then(Value::as_bool) == Some(true)
                        || data.get("error").is_some_and(|value| !value.is_null());
                    parsed.messages.push(result);
                }
            }
            "session/end-seed" => {
                if data.get("inherited").and_then(Value::as_bool) == Some(true) {
                    parsed.inherited_cut = Some(parsed.messages.len());
                }
            }
            "subagent/descriptor" => {
                if parsed.label.is_none() {
                    parsed.label = data
                        .get("label")
                        .and_then(Value::as_str)
                        .and_then(|value| normalize_title_candidate(value, 160));
                }
            }
            _ => {}
        }
    }

    if let Some(cut) = parsed.inherited_cut.filter(|cut| *cut > 0) {
        parsed.messages.drain(..cut);
        parsed.messages.insert(
            0,
            SessionMessage::structured(
                "system".to_string(),
                "meta",
                None,
                None,
                "Inherited session history omitted".to_string(),
                parsed.created_at,
            ),
        );
    }
    Ok(parsed)
}

fn push_plain(
    parsed: &mut ParsedSession,
    role: &str,
    content: String,
    ts: Option<i64>,
    summarize: bool,
) {
    let content = content.trim().to_string();
    if content.is_empty() {
        return;
    }
    if summarize {
        parsed.summary = Some(content.clone());
    }
    parsed
        .messages
        .push(SessionMessage::plain(role.to_string(), content, ts));
}

fn first_user_title(messages: &[SessionMessage]) -> Option<String> {
    messages
        .iter()
        .find(|message| message.role == "user" && message.kind == "message")
        .and_then(|message| normalize_title_candidate(&message.content, 160))
}

fn format_arguments(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if let Some(raw) = value.as_str() {
        return serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|parsed| serde_json::to_string_pretty(&parsed).ok())
            .unwrap_or_else(|| raw.to_string());
    }
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn open_log(path: &Path) -> Result<Box<dyn BufRead>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open DSH session log: {e}"))?;
    if path.extension().and_then(|value| value.to_str()) == Some("zstd") {
        let decoder = zstd::stream::read::Decoder::new(file)
            .map_err(|e| format!("Failed to decode DSH session log: {e}"))?;
        Ok(Box::new(BufReader::new(decoder)))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

fn collect_session_logs(root: &Path, logs: &mut Vec<PathBuf>) {
    if !root.exists() {
        return;
    }
    let projects = match fs::read_dir(root) {
        Ok(projects) => projects,
        Err(err) => {
            log_scan_error(PROVIDER_ID, root, &err);
            return;
        }
    };

    for project in projects.flatten().filter(|entry| entry.path().is_dir()) {
        let Ok(sessions) = fs::read_dir(project.path()) else {
            continue;
        };
        for session in sessions.flatten().filter(|entry| entry.path().is_dir()) {
            if let Some(log) = select_session_log(&session.path()) {
                logs.push(log);
            }
        }
    }
}

fn select_session_log(session_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(session_dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let (generation, compressed) = log_generation(path.file_name()?.to_str()?)?;
            let modified = file_mtime_ms(&path).unwrap_or(0);
            Some(((generation, modified, compressed), path))
        })
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, path)| path)
}

fn log_generation(name: &str) -> Option<(u32, bool)> {
    let (stem, compressed) = match name.strip_suffix(".zstd") {
        Some(stem) => (stem, true),
        None => (name, false),
    };
    let version = stem.strip_suffix(".jsonl")?.strip_prefix("session")?;
    if version.is_empty() {
        return Some((0, compressed));
    }
    let digits = version.strip_prefix(".v")?;
    if digits.starts_with('0') || !digits.bytes().all(|value| value.is_ascii_digit()) {
        return None;
    }
    Some((digits.parse().ok()?, compressed))
}

fn timestamp_ms(value: &Value) -> Option<i64> {
    if let Some(value) = value.as_i64() {
        return Some(if value > 10_000_000_000 {
            value
        } else {
            value.checked_mul(1000)?
        });
    }
    parse_timestamp_to_ms(value)
}

fn file_mtime_ms(path: &Path) -> Option<i64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|value| value.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn fixture_lines() -> Vec<String> {
        vec![
            r#"{"type":"session","version":4,"id":"session-dsh-1","createdAt":1770000000000,"cwd":"/tmp/dsh-project"}"#.to_string(),
            r#"{"type":"user/message","seq":1,"time":1770000001000,"data":{"source":{"kind":"user"},"content":[{"type":"text","text":"检查接口"}]}}"#.to_string(),
            r#"{"type":"assistant/message","seq":2,"time":1770000002000,"data":{"message":{"source":{"model":"deepseek-test"},"content":[{"type":"reasoning","text":"先检查"},{"type":"tool-call","id":"call-1","name":"read","arguments":"{\"path\":\"README.md\"}"}]}}}"#.to_string(),
            r#"{"type":"tool/result","seq":3,"time":1770000003000,"data":{"message":{"toolCallId":"call-1","content":[{"type":"text","text":"ok"}]}}}"#.to_string(),
            r#"{"type":"assistant/message","seq":4,"time":1770000004000,"data":{"message":{"source":{"model":"deepseek-test"},"content":[{"type":"text","text":"检查完成"}]}}}"#.to_string(),
            r#"{"type":"session/title","seq":5,"time":1770000005000,"data":{"title":"DSH 检查"}}"#.to_string(),
        ]
    }

    #[test]
    fn parses_dsh_messages_and_metadata() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.v4.jsonl");
        fs::write(&path, fixture_lines().join("\n") + "\n").expect("write fixture");

        let meta = parse_session(&path).expect("parse session");
        assert_eq!(meta.provider_id, "dsh");
        assert_eq!(meta.session_id, "session-dsh-1");
        assert_eq!(meta.title.as_deref(), Some("DSH 检查"));
        assert_eq!(meta.cwd.as_deref(), Some("/tmp/dsh-project"));
        assert_eq!(meta.model.as_deref(), Some("deepseek-test"));
        assert_eq!(meta.resume_command.as_deref(), Some(RESUME_COMMAND));

        let messages = load_messages(&path).expect("load messages");
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.kind == "thinking")
                .count(),
            1
        );
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.kind == "tool_use")
                .count(),
            1
        );
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.kind == "tool_result")
                .count(),
            1
        );
        assert_eq!(
            messages.last().map(|message| message.content.as_str()),
            Some("检查完成")
        );
    }

    #[test]
    fn reads_concatenated_zstd_frames() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.v4.jsonl.zstd");
        let mut file = File::create(&path).expect("create compressed fixture");
        for line in fixture_lines() {
            let frame =
                zstd::stream::encode_all(format!("{line}\n").as_bytes(), 1).expect("encode frame");
            file.write_all(&frame).expect("write frame");
        }

        let messages = load_messages(&path).expect("load compressed messages");
        assert!(messages.iter().any(|message| message.kind == "tool_use"));
        assert_eq!(
            parse_session(&path)
                .expect("parse compressed session")
                .session_id,
            "session-dsh-1"
        );
    }

    #[test]
    fn selects_newest_log_generation() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("session.jsonl"), "old").expect("write v0");
        fs::write(dir.path().join("session.v3.jsonl"), "v3").expect("write v3");
        fs::write(dir.path().join("session.v4.jsonl"), "v4").expect("write v4");

        let selected = select_session_log(dir.path()).expect("selected log");
        assert_eq!(
            selected.file_name().and_then(|value| value.to_str()),
            Some("session.v4.jsonl")
        );
    }

    #[test]
    fn deletes_only_exact_session_directory_layout() {
        let root = tempdir().expect("tempdir");
        let project_dir = root.path().join("project");
        let session_dir = project_dir.join("session-dsh-1");
        fs::create_dir_all(&session_dir).expect("create session dir");
        let path = session_dir.join("session.v4.jsonl");
        fs::write(&path, fixture_lines().join("\n") + "\n").expect("write fixture");

        let canonical_root = root.path().canonicalize().expect("canonical root");
        let canonical_path = path.canonicalize().expect("canonical path");
        assert!(
            delete_session(&canonical_root, &canonical_path, "session-dsh-1")
                .expect("delete session")
        );
        assert!(!session_dir.exists());
        assert!(project_dir.exists());
    }

    #[test]
    fn rejects_shallow_log_without_deleting_project_directory() {
        let root = tempdir().expect("tempdir");
        let project_dir = root.path().join("project");
        fs::create_dir_all(&project_dir).expect("create project dir");
        let path = project_dir.join("session.v4.jsonl");
        fs::write(&path, fixture_lines().join("\n") + "\n").expect("write fixture");

        let canonical_root = root.path().canonicalize().expect("canonical root");
        let canonical_path = path.canonicalize().expect("canonical path");
        let error = delete_session(&canonical_root, &canonical_path, "session-dsh-1")
            .expect_err("reject shallow log");

        assert!(error.contains("Unexpected DSH session path layout"));
        assert!(project_dir.exists());
        assert!(path.exists());
    }
    #[test]
    fn folds_fork_seed_and_collects_usage_and_parent() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.v4.jsonl");
        let rows = [
            r#"{"type":"session","version":4,"id":"child","createdAt":1761955200000,"cwd":"/work","origin":"subagent","parentSessionId":"parent"}"#,
            r#"{"type":"user/message","seq":1,"time":1761955200001,"data":{"content":"inherited"}}"#,
            r#"{"type":"session/end-seed","seq":2,"time":1761955200002,"data":{"inherited":true}}"#,
            r#"{"type":"user/message","seq":3,"time":1761955200003,"data":{"content":"own request","source":{"kind":"user"}}}"#,
            r#"{"type":"assistant/message","seq":4,"time":1761955200004,"data":{"message":{"source":{"model":"deepseek-v4"},"content":[{"type":"text","text":"own answer"}]},"usage":{"inputTokens":10,"outputTokens":4,"cacheReadTokens":2,"cacheWriteTokens":1}}}"#,
        ];
        fs::write(&path, rows.join("\n") + "\n").expect("write fork fixture");

        let parsed = parse_dsh_log(&path).expect("parse fork fixture");
        assert_eq!(parsed.parent_session_id.as_deref(), Some("parent"));
        assert_eq!(parsed.tokens_used, Some(17));
        assert!(!parsed
            .messages
            .iter()
            .any(|message| message.content == "inherited"));
        assert!(parsed
            .messages
            .iter()
            .any(|message| message.content == "own request"));
    }
}
