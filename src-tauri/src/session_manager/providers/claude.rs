use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::{
    collections::{HashMap, HashSet},
    mem,
};

use serde_json::Value;

use crate::paths::get_claude_projects_dir;
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{
    extract_text, log_scan_error, normalize_title_candidate, parse_timestamp_to_ms, path_basename,
    read_head_tail_lines, truncate_summary,
};

const PROVIDER_ID: &str = "claude";

pub fn scan_sessions() -> Vec<SessionMeta> {
    let root = get_claude_projects_dir();
    let mut files = Vec::new();
    collect_jsonl_files(&root, &mut files);

    let history_title_index = load_history_title_index(&root);
    let mut title_index_cache: HashMap<PathBuf, HashMap<String, String>> = HashMap::new();
    let mut sessions = Vec::new();
    for path in files {
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let title_index = title_index_cache
            .entry(parent.clone())
            .or_insert_with(|| load_session_title_index(&parent));
        if let Some(meta) = parse_session(&path, title_index, &history_title_index) {
            sessions.push(meta);
        }
    }

    sessions
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open session file: {e}"))?;
    let reader = BufReader::with_capacity(1 << 20, file);
    let mut messages = Vec::new();
    let mut tool_names = HashMap::new();
    let branch = branch_info(path);
    let standalone_agent = is_agent_session(path);
    let mut inherited_prefix = branch.parent_session_id.is_some();

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read Claude session file: {e}"))?;
        let value: Value = match serde_json::from_str(&line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if inherited_prefix {
            let uuid = value.get("uuid").and_then(Value::as_str);
            if uuid.is_none() || uuid.is_some_and(|uuid| branch.inherited_uuids.contains(uuid)) {
                continue;
            }
            inherited_prefix = false;
            messages.push(SessionMessage::structured(
                "system".to_string(),
                "meta",
                None,
                None,
                "Inherited conversation history omitted".to_string(),
                value.get("timestamp").and_then(parse_timestamp_to_ms),
            ));
        }

        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/message/role").and_then(Value::as_str))
            .unwrap_or_default();
        let ts = value.get("timestamp").and_then(parse_timestamp_to_ms);
        if event_type == "system" {
            let kind = if value.get("subtype").and_then(Value::as_str) == Some("compact_boundary") {
                "compact_summary"
            } else {
                "meta"
            };
            let content = value.get("content").and_then(Value::as_str).unwrap_or(
                if kind == "compact_summary" {
                    "Context compacted"
                } else {
                    ""
                },
            );
            if !content.trim().is_empty() {
                messages.push(SessionMessage::structured(
                    "system".to_string(),
                    kind,
                    None,
                    None,
                    content.trim().to_string(),
                    ts,
                ));
            }
            continue;
        }
        if !matches!(event_type, "user" | "assistant") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let msg_uuid = value
            .get("uuid")
            .and_then(Value::as_str)
            .map(str::to_string);
        let parent_uuid = value
            .get("parentUuid")
            .and_then(Value::as_str)
            .map(str::to_string);
        let is_sidechain =
            !standalone_agent && value.get("isSidechain").and_then(Value::as_bool) == Some(true);
        let model = message
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| *value != "<synthetic>")
            .map(str::to_string);
        let kind = if value.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
            "compact_summary"
        } else if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
            "meta"
        } else {
            "message"
        };

        if let Some(content_blocks) = message.get("content").and_then(Value::as_array) {
            append_structured_blocks(
                &mut messages,
                &mut tool_names,
                role,
                ts,
                content_blocks,
                msg_uuid,
                parent_uuid,
                is_sidechain,
                kind,
                model,
            );
            continue;
        }

        let content = message.get("content").map(extract_text).unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let mut parsed =
            SessionMessage::structured(role.to_string(), kind, None, None, content, ts);
        parsed.msg_uuid = msg_uuid;
        parsed.parent_uuid = parent_uuid;
        parsed.is_sidechain = is_sidechain;
        parsed.model = model;
        messages.push(parsed);
    }

    Ok(messages)
}

#[allow(clippy::too_many_arguments)]
fn append_structured_blocks(
    messages: &mut Vec<SessionMessage>,
    tool_names: &mut HashMap<String, String>,
    role: &str,
    ts: Option<i64>,
    content_blocks: &[Value],
    msg_uuid: Option<String>,
    parent_uuid: Option<String>,
    is_sidechain: bool,
    base_kind: &str,
    model: Option<String>,
) {
    let mut text_buffer = String::new();
    let flush = |messages: &mut Vec<SessionMessage>, buffer: &mut String| {
        let content = mem::take(buffer);
        if content.trim().is_empty() {
            return;
        }
        let mut message =
            SessionMessage::structured(role.to_string(), base_kind, None, None, content, ts);
        message.msg_uuid = msg_uuid.clone();
        message.parent_uuid = parent_uuid.clone();
        message.is_sidechain = is_sidechain;
        message.model = model.clone();
        messages.push(message);
    };

    for block in content_blocks {
        match block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "text" => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    append_text_buffer(&mut text_buffer, text);
                }
            }
            "thinking" => {
                flush(messages, &mut text_buffer);
                if let Some(content) = block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                {
                    let mut message = SessionMessage::structured(
                        "assistant".to_string(),
                        "thinking",
                        None,
                        None,
                        content.trim().to_string(),
                        ts,
                    );
                    message.msg_uuid = msg_uuid.clone();
                    message.parent_uuid = parent_uuid.clone();
                    message.is_sidechain = is_sidechain;
                    message.model = model.clone();
                    messages.push(message);
                }
            }
            "tool_use" => {
                flush(messages, &mut text_buffer);
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let call_id = block.get("id").and_then(Value::as_str).map(str::to_string);
                if let (Some(call_id), Some(name)) = (call_id.clone(), name.clone()) {
                    tool_names.insert(call_id, name);
                }
                let input = block.get("input").cloned().unwrap_or(Value::Null);
                let mut message = SessionMessage::structured(
                    "assistant".to_string(),
                    "tool_use",
                    name.clone(),
                    call_id,
                    format_json_block(&input),
                    ts,
                );
                message.msg_uuid = msg_uuid.clone();
                message.parent_uuid = parent_uuid.clone();
                message.is_sidechain = is_sidechain;
                message.model = model.clone();
                if let Some(name) = name {
                    message.tool_names.push(name);
                }
                messages.push(message);
            }
            "tool_result" => {
                flush(messages, &mut text_buffer);
                let call_id = block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let name = call_id.as_ref().and_then(|id| tool_names.get(id)).cloned();
                let content = block.get("content").map(extract_text).unwrap_or_default();
                if content.trim().is_empty() {
                    continue;
                }
                let mut message = SessionMessage::structured(
                    "tool".to_string(),
                    "tool_result",
                    name,
                    call_id,
                    content,
                    ts,
                );
                message.msg_uuid = msg_uuid.clone();
                message.parent_uuid = parent_uuid.clone();
                message.is_sidechain = is_sidechain;
                message.is_error = block.get("is_error").and_then(Value::as_bool) == Some(true);
                messages.push(message);
            }
            "image" | "image_url" => {
                append_text_buffer(&mut text_buffer, "[Image]");
            }
            _ => {}
        }
    }
    flush(messages, &mut text_buffer);
}

fn append_text_buffer(buffer: &mut String, next: &str) {
    let trimmed = next.trim();
    if trimmed.is_empty() {
        return;
    }
    if !buffer.is_empty() {
        buffer.push_str("\n\n");
    }
    buffer.push_str(trimmed);
}

fn format_json_block(value: &Value) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| value.to_string())
        .trim()
        .to_string()
}

pub fn delete_session(_root: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    let meta = parse_session(path, &HashMap::new(), &HashMap::new()).ok_or_else(|| {
        format!(
            "Failed to parse Claude session metadata: {}",
            path.display()
        )
    })?;

    if meta.session_id != session_id {
        return Err(format!(
            "Claude session ID mismatch: expected {session_id}, found {}",
            meta.session_id
        ));
    }

    if let Some(stem) = path.file_stem() {
        let sibling = path.parent().unwrap_or_else(|| Path::new("")).join(stem);
        remove_path_if_exists(&sibling).map_err(|e| {
            format!(
                "Failed to delete Claude session sidecar {}: {e}",
                sibling.display()
            )
        })?;
    }

    std::fs::remove_file(path).map_err(|e| {
        format!(
            "Failed to delete Claude session file {}: {e}",
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
    let agent_session = is_agent_session(path);
    let branch = branch_info(path);
    let (head, tail) = read_head_tail_lines(path, 20, 40).ok()?;

    let mut session_id: Option<String> = None;
    let mut project_dir: Option<String> = None;
    let mut model: Option<String> = None;
    let mut created_at: Option<i64> = None;
    let mut fallback_title: Option<String> = None;
    let mut explicit_title: Option<String> = None;
    let mut last_active_at: Option<i64> = None;
    let mut summary: Option<String> = None;

    for line in head.iter().chain(tail.iter()) {
        let value: Value = match serde_json::from_str(line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if event_type == "custom-title" {
            explicit_title = value
                .get("customTitle")
                .and_then(Value::as_str)
                .and_then(|value| normalize_title_candidate(value, 160));
            continue;
        }
        if event_type == "ai-title" {
            explicit_title = value
                .get("aiTitle")
                .and_then(Value::as_str)
                .and_then(|value| normalize_title_candidate(value, 160))
                .or(explicit_title);
            continue;
        }
        if session_id.is_none() {
            session_id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if project_dir.is_none() {
            project_dir = value.get("cwd").and_then(Value::as_str).map(str::to_string);
        }
        let ts = value.get("timestamp").and_then(parse_timestamp_to_ms);
        created_at = created_at.or(ts);
        if ts.is_some() {
            last_active_at = ts;
        }
        if model.is_none() {
            model = value
                .get("message")
                .and_then(|message| message.get("model"))
                .and_then(Value::as_str)
                .filter(|value| *value != "<synthetic>")
                .map(str::to_string);
        }
        if fallback_title.is_none() {
            fallback_title = extract_first_user_title(&value);
        }
        if value.get("isMeta").and_then(Value::as_bool) != Some(true) {
            if let Some(message) = value.get("message") {
                let text = message.get("content").map(extract_text).unwrap_or_default();
                if !text.trim().is_empty() {
                    summary = Some(text);
                }
            }
        }
    }

    let parent_session_id = if agent_session {
        agent_parent_session_id(path)
    } else {
        branch.parent_session_id.clone()
    };
    if agent_session {
        session_id = infer_session_id_from_filename(path);
    }
    let session_id = session_id.or_else(|| infer_session_id_from_filename(path))?;
    if branch.parent_session_id.is_some() || agent_session {
        fallback_title = load_messages(path).ok().and_then(|messages| {
            messages
                .iter()
                .find(|message| message.role == "user" && message.kind == "message")
                .and_then(|message| normalize_claude_prompt_title(&message.content))
        });
    }
    let indexed_title = (!agent_session && branch.parent_session_id.is_none())
        .then(|| {
            title_index
                .get(&session_id)
                .cloned()
                .or_else(|| history_title_index.get(&session_id).cloned())
        })
        .flatten();
    let title = explicit_title
        .filter(|_| branch.parent_session_id.is_none())
        .or(indexed_title)
        .or(fallback_title)
        .or_else(|| project_dir.as_deref().and_then(path_basename));
    let summary = summary.map(|text| truncate_summary(&text, 160));

    if project_dir.is_none()
        && summary.is_none()
        && created_at.is_none()
        && last_active_at.is_none()
    {
        return None;
    }
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
        resume_command: (!agent_session).then(|| format!("claude --resume {session_id}")),
        archived: false,
        tokens_used: None,
        parent_session_id,
        unknown_records: 0,
        source_mtime: None,
        source_size: None,
        source_rank: 1,
    })
}

fn load_session_title_index(project_dir: &Path) -> HashMap<String, String> {
    let index_path = project_dir.join("sessions-index.json");
    let data = match std::fs::read_to_string(index_path) {
        Ok(data) => data,
        Err(_) => return HashMap::new(),
    };
    let value: Value = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(_) => return HashMap::new(),
    };
    let Some(entries) = value.get("entries").and_then(Value::as_array) else {
        return HashMap::new();
    };

    let mut titles = HashMap::new();
    for entry in entries {
        let Some(session_id) = entry.get("sessionId").and_then(Value::as_str) else {
            continue;
        };
        let Some(title) = entry
            .get("firstPrompt")
            .and_then(Value::as_str)
            .and_then(|value| normalize_title_candidate(value, 160))
        else {
            continue;
        };
        titles.insert(session_id.to_string(), title);
    }

    titles
}

fn load_history_title_index(projects_dir: &Path) -> HashMap<String, String> {
    let history_path = projects_dir
        .parent()
        .map(|dir| dir.join("history.jsonl"))
        .filter(|path| path.exists())
        .unwrap_or_else(|| projects_dir.join("history.jsonl"));
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
        let Some(session_id) = value.get("sessionId").and_then(Value::as_str) else {
            continue;
        };
        let Some(title) = value
            .get("display")
            .and_then(Value::as_str)
            .and_then(normalize_history_title)
        else {
            continue;
        };

        titles.entry(session_id.to_string()).or_insert(title);
    }

    titles
}

fn extract_first_user_title(value: &Value) -> Option<String> {
    let message = value.get("message")?;
    if message.get("role").and_then(Value::as_str) != Some("user") {
        return None;
    }

    let content = message.get("content").map(extract_text).unwrap_or_default();
    normalize_claude_prompt_title(&content)
}

fn normalize_history_title(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return None;
    }

    normalize_title_candidate(trimmed, 160)
}

fn normalize_claude_prompt_title(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("<local-command-caveat>")
        || trimmed.starts_with("<local-command-stdout>")
        || trimmed.starts_with("<local-command-stderr>")
        || trimmed.starts_with("<command-message>")
        || trimmed.starts_with("<command-name>")
        || trimmed.starts_with("<command-args>")
        || trimmed.starts_with("<ide_opened_file>")
    {
        return None;
    }

    normalize_title_candidate(trimmed, 160)
}

#[derive(Default)]
struct BranchInfo {
    parent_session_id: Option<String>,
    inherited_uuids: HashSet<String>,
}

fn branch_info(path: &Path) -> BranchInfo {
    if is_agent_session(path) {
        return BranchInfo::default();
    }
    let Some(root_uuid) = first_uuid(path) else {
        return BranchInfo::default();
    };
    let Some(parent) = path.parent() else {
        return BranchInfo::default();
    };
    let Some(current_created) = file_created_key(path) else {
        return BranchInfo::default();
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return BranchInfo::default();
    };
    let mut earlier = Vec::new();
    for entry in entries.flatten() {
        let candidate = entry.path();
        if candidate == path
            || candidate.extension().and_then(|value| value.to_str()) != Some("jsonl")
            || is_agent_session(&candidate)
            || first_uuid(&candidate).as_deref() != Some(root_uuid.as_str())
        {
            continue;
        }
        let Some(created) = file_created_key(&candidate) else {
            continue;
        };
        if created < current_created {
            earlier.push((created, candidate));
        }
    }
    earlier.sort_by_key(|(created, _)| *created);
    let Some((_, original)) = earlier.first() else {
        return BranchInfo::default();
    };
    let mut inherited_uuids = HashSet::new();
    for (_, candidate) in &earlier {
        collect_uuids(candidate, &mut inherited_uuids);
    }
    BranchInfo {
        parent_session_id: infer_session_id_from_filename(original),
        inherited_uuids,
    }
}

fn first_uuid(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let value: Value = serde_json::from_str(&line).ok()?;
        if let Some(uuid) = value.get("uuid").and_then(Value::as_str) {
            return Some(uuid.to_string());
        }
    }
    None
}

fn collect_uuids(path: &Path, target: &mut HashSet<String>) {
    let Ok(file) = File::open(path) else {
        return;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            if let Some(uuid) = value.get("uuid").and_then(Value::as_str) {
                target.insert(uuid.to_string());
            }
        }
    }
}

fn file_created_key(path: &Path) -> Option<u128> {
    let metadata = path.metadata().ok()?;
    metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|value| value.as_nanos())
}

fn agent_parent_session_id(path: &Path) -> Option<String> {
    let subagents = path.parent()?;
    if subagents.file_name()?.to_str()? != "subagents" {
        return None;
    }
    subagents
        .parent()?
        .file_name()?
        .to_str()
        .map(str::to_string)
}

fn is_agent_session(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with("agent-"))
        .unwrap_or(false)
}

fn infer_session_id_from_filename(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_string())
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

fn remove_path_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::metadata(path) {
        Ok(meta) => {
            if meta.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn load_messages_keeps_tool_use_and_result_blocks() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        let content = [
            r#"{"sessionId":"claude-tool-1","cwd":"D:\\code","timestamp":"2026-03-29T06:00:00Z","message":{"role":"user","content":"请读取文件"}}"#,
            r#"{"sessionId":"claude-tool-1","cwd":"D:\\code","timestamp":"2026-03-29T06:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"先读取文件。"},{"type":"tool_use","id":"tooluse_1","name":"Read","input":{"file_path":"D:\\code\\README.md"}}]}}"#,
            r#"{"sessionId":"claude-tool-1","cwd":"D:\\code","timestamp":"2026-03-29T06:00:02Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tooluse_1","content":"line 1\nline 2"}]}}"#,
            r#"{"sessionId":"claude-tool-1","cwd":"D:\\code","timestamp":"2026-03-29T06:00:03Z","message":{"role":"assistant","content":[{"type":"text","text":"读取完成。"}]}}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write claude fixture");

        let messages = load_messages(&path).expect("load messages");
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[1].kind, "message");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[2].kind, "tool_use");
        assert_eq!(messages[2].name.as_deref(), Some("Read"));
        assert_eq!(messages[2].call_id.as_deref(), Some("tooluse_1"));
        assert!(messages[2].content.contains("file_path"));
        assert_eq!(messages[3].kind, "tool_result");
        assert_eq!(messages[3].role, "tool");
        assert_eq!(messages[3].name.as_deref(), Some("Read"));
        assert_eq!(messages[3].searchable_text(), "");
    }

    #[test]
    fn parse_session_prefers_prompt_from_sessions_index() {
        let dir = tempdir().expect("tempdir");
        let project_dir = dir.path().join("D--code");
        fs::create_dir_all(&project_dir).expect("create claude project dir");

        let path = project_dir.join("3f942f3b-b4c2-4898-8d15-29644c395384.jsonl");
        let content = [
            r#"{"sessionId":"3f942f3b-b4c2-4898-8d15-29644c395384","cwd":"D:\\code","timestamp":"2026-03-29T06:00:00Z","message":{"role":"user","content":"请读取文件"}}"#,
            r#"{"sessionId":"3f942f3b-b4c2-4898-8d15-29644c395384","cwd":"D:\\code","timestamp":"2026-03-29T06:00:03Z","message":{"role":"assistant","content":"读取完成。"}}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write claude fixture");
        fs::write(
            project_dir.join("sessions-index.json"),
            r#"{"version":1,"entries":[{"sessionId":"3f942f3b-b4c2-4898-8d15-29644c395384","firstPrompt":"[@%E7%A4%BE%E5%9B%A2%E9%A6%96%E9%A1%B5.htm](file:///D:/code/demo-script/web-tools/%E7%A4%BE%E5%9B%A2%E9%A6%96%E9%A1%B5.htm)"}]}"#,
        )
        .expect("write claude session index");

        let title_index = load_session_title_index(&project_dir);
        let meta = parse_session(&path, &title_index, &HashMap::new()).expect("parse session");

        assert_eq!(meta.title.as_deref(), Some("@社团首页.htm"));
    }

    #[test]
    fn parse_session_falls_back_to_history_display() {
        let dir = tempdir().expect("tempdir");
        let claude_root = dir.path().join(".claude");
        let project_dir = claude_root.join("projects").join("D--code");
        fs::create_dir_all(&project_dir).expect("create claude project dir");

        let path = project_dir.join("99604d52-7774-4136-a4fe-ab3943d8114f.jsonl");
        let content = [
            r#"{"sessionId":"99604d52-7774-4136-a4fe-ab3943d8114f","cwd":"D:\\code","timestamp":"2026-03-29T06:00:00Z","message":{"role":"user","content":"检查更新"}}"#,
            r#"{"sessionId":"99604d52-7774-4136-a4fe-ab3943d8114f","cwd":"D:\\code","timestamp":"2026-03-29T06:00:03Z","message":{"role":"assistant","content":"读取完成。"}}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write claude fixture");
        fs::write(
            claude_root.join("history.jsonl"),
            r#"{"display":"/status","timestamp":1775206131867,"project":"D:\\code","sessionId":"99604d52-7774-4136-a4fe-ab3943d8114f"}
{"display":"检查更新","timestamp":1775206177291,"project":"D:\\code","sessionId":"99604d52-7774-4136-a4fe-ab3943d8114f"}"#,
        )
        .expect("write claude history");

        let history_index = load_history_title_index(&claude_root.join("projects"));
        let meta = parse_session(&path, &HashMap::new(), &history_index).expect("parse session");

        assert_eq!(meta.title.as_deref(), Some("检查更新"));
    }

    #[test]
    fn parse_session_falls_back_to_first_meaningful_user_prompt() {
        let dir = tempdir().expect("tempdir");
        let project_dir = dir.path().join("D--code");
        fs::create_dir_all(&project_dir).expect("create claude project dir");

        let path = project_dir.join("b42cc271-933e-439c-87f4-9c0d8f0fa948.jsonl");
        let content = [
            r#"{"sessionId":"b42cc271-933e-439c-87f4-9c0d8f0fa948","cwd":"D:\\code","timestamp":"2026-03-28T11:12:11Z","message":{"role":"user","content":"<command-message>init</command-message>\n<command-name>/init</command-name>"}}"#,
            r#"{"sessionId":"b42cc271-933e-439c-87f4-9c0d8f0fa948","cwd":"D:\\code","timestamp":"2026-03-28T11:12:12Z","isMeta":true,"message":{"role":"user","content":[{"type":"text","text":"Please analyze this codebase and create a CLAUDE.md file, which will be given to future instances of Claude Code to operate in this repository."}]}}"#,
            r#"{"sessionId":"b42cc271-933e-439c-87f4-9c0d8f0fa948","cwd":"D:\\code","timestamp":"2026-03-28T11:12:13Z","message":{"role":"assistant","content":"好的。"}}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{content}\n")).expect("write claude fixture");

        let meta = parse_session(&path, &HashMap::new(), &HashMap::new()).expect("parse session");

        assert_eq!(
            meta.title.as_deref(),
            Some("Please analyze this codebase and create a CLAUDE.md file, which will be given to future instances of Claude Code to operate in this repository.")
        );
    }
    #[test]
    fn standalone_agent_transcript_promotes_sidechain_messages_to_mainline() {
        let dir = tempdir().expect("tempdir");
        let subagents = dir.path().join("parent-session/subagents");
        fs::create_dir_all(&subagents).expect("create subagents dir");
        let path = subagents.join("agent-child.jsonl");
        fs::write(
            &path,
            r#"{"type":"user","uuid":"u1","isSidechain":true,"timestamp":"2026-11-01T00:00:00Z","message":{"role":"user","content":"delegated task"}}
"#,
        )
        .expect("write agent transcript");

        let messages = load_messages(&path).expect("load agent transcript");
        assert_eq!(messages.len(), 1);
        assert!(!messages[0].is_sidechain);
        assert_eq!(messages[0].content, "delegated task");
    }

    #[test]
    fn folds_forked_history_and_links_branch_to_original() {
        let dir = tempdir().expect("tempdir");
        let project = dir.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let original = project.join("11111111-1111-4111-8111-111111111111.jsonl");
        fs::write(
            &original,
            r#"{"type":"user","uuid":"root","sessionId":"11111111-1111-4111-8111-111111111111","cwd":"/work","timestamp":"2026-11-01T00:00:00Z","message":{"role":"user","content":"parent request"}}
"#,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let branch = project.join("22222222-2222-4222-8222-222222222222.jsonl");
        fs::write(
            &branch,
            [
                r#"{"type":"user","uuid":"root","sessionId":"22222222-2222-4222-8222-222222222222","cwd":"/work","timestamp":"2026-11-01T00:00:00Z","message":{"role":"user","content":"parent request"}}"#,
                r#"{"type":"user","uuid":"own","parentUuid":"root","sessionId":"22222222-2222-4222-8222-222222222222","cwd":"/work","timestamp":"2026-11-01T00:01:00Z","message":{"role":"user","content":"branch request"}}"#,
            ]
            .join("\n"),
        )
        .unwrap();

        let info = branch_info(&branch);
        assert_eq!(
            info.parent_session_id.as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        let messages = load_messages(&branch).expect("load branch");
        assert!(!messages
            .iter()
            .any(|message| message.content == "parent request"));
        assert!(messages
            .iter()
            .any(|message| message.content == "branch request"));
        let meta = parse_session(&branch, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(meta.title.as_deref(), Some("branch request"));
        assert_eq!(meta.parent_session_id, info.parent_session_id);
    }

    #[test]
    fn keeps_titles_compaction_sidechains_and_tool_errors() {
        let dir = tempdir().expect("tempdir");
        let path = dir
            .path()
            .join("33333333-3333-4333-8333-333333333333.jsonl");
        fs::write(
            &path,
            [
                r#"{"type":"custom-title","customTitle":"Named session","sessionId":"33333333-3333-4333-8333-333333333333"}"#,
                r#"{"type":"system","subtype":"compact_boundary","timestamp":"2026-11-01T00:00:00Z"}"#,
                r#"{"type":"user","uuid":"u1","isSidechain":true,"sessionId":"33333333-3333-4333-8333-333333333333","cwd":"/work","timestamp":"2026-11-01T00:00:01Z","message":{"role":"user","content":"side task"}}"#,
                r#"{"type":"user","uuid":"u2","parentUuid":"u1","sessionId":"33333333-3333-4333-8333-333333333333","cwd":"/work","timestamp":"2026-11-01T00:00:02Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call","is_error":true,"content":"failed"}]}}"#,
            ]
            .join("\n"),
        )
        .unwrap();

        let messages = load_messages(&path).unwrap();
        assert!(messages
            .iter()
            .any(|message| message.kind == "compact_summary"));
        assert!(messages.iter().any(|message| message.is_sidechain));
        assert!(messages
            .iter()
            .any(|message| message.kind == "tool_result" && message.is_error));
        let meta = parse_session(&path, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Named session"));
    }
}
