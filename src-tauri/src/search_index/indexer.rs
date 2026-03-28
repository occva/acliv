use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::session_manager::{self, SessionMeta};

use super::{SyncProgress, SyncProgressPhase};
use super::status;
use super::types::{RebuildSearchIndexResult, RefreshSearchIndexResult};

const PROVIDERS: [&str; 5] = ["claude", "codex", "gemini", "openclaw", "opencode"];

pub fn rebuild_index(connection: &mut Connection) -> Result<RebuildSearchIndexResult, String> {
    let sessions = session_manager::scan_sessions();
    let mut error_sessions = 0_i64;

    let transaction = connection
        .transaction()
        .map_err(|e| format!("Failed to start search index transaction: {e}"))?;

    clear_existing_data(&transaction)?;
    let source_ids = seed_sources(&transaction)?;

    for session in &sessions {
        if let Err(err) = upsert_session_index(&transaction, &source_ids, session) {
            error_sessions += 1;
            let source_id = source_ids
                .get(session.provider_id.as_str())
                .copied()
                .unwrap_or_default();
            insert_sync_log(
                &transaction,
                source_id,
                session.source_path.as_deref().unwrap_or_default(),
                "error",
                Some(&err),
            )?;
        }
    }

    cleanup_empty_projects(&transaction)?;
    transaction
        .commit()
        .map_err(|e| format!("Failed to commit search index rebuild: {e}"))?;

    let snapshot = status::get_status(connection)?;
    Ok(RebuildSearchIndexResult {
        db_path: snapshot.db_path,
        sources_count: snapshot.sources_count,
        projects_count: snapshot.projects_count,
        indexed_sessions: snapshot.sessions_count,
        indexed_messages: snapshot.messages_count,
        error_sessions,
    })
}

pub fn refresh_index(connection: &mut Connection) -> Result<RefreshSearchIndexResult, String> {
    refresh_index_inner(connection, Option::<fn(SyncProgress)>::None)
}

#[allow(dead_code)]
pub fn refresh_index_with_progress<F>(
    connection: &mut Connection,
    on_progress: F,
) -> Result<RefreshSearchIndexResult, String>
where
    F: FnMut(SyncProgress),
{
    refresh_index_inner(connection, Some(on_progress))
}

fn refresh_index_inner<F>(
    connection: &mut Connection,
    mut on_progress: Option<F>,
) -> Result<RefreshSearchIndexResult, String>
where
    F: FnMut(SyncProgress),
{
    emit_progress(
        &mut on_progress,
        SyncProgress {
            phase: SyncProgressPhase::Scanning,
            count: 0,
            total: 0,
        },
    );
    let sessions = session_manager::scan_sessions();
    let scanned_sessions = i64::try_from(sessions.len()).unwrap_or(i64::MAX);
    let mut added_sessions = 0_i64;
    let mut updated_sessions = 0_i64;
    let mut removed_sessions = 0_i64;
    let mut skipped_sessions = 0_i64;
    let mut error_sessions = 0_i64;

    let transaction = connection
        .transaction()
        .map_err(|e| format!("Failed to start refresh transaction: {e}"))?;
    let source_ids = seed_sources(&transaction)?;
    let existing = load_existing_sessions(&transaction)?;
    let mut seen_keys = HashSet::with_capacity(sessions.len());
    emit_progress(
        &mut on_progress,
        SyncProgress {
            phase: SyncProgressPhase::Syncing,
            count: 0,
            total: scanned_sessions,
        },
    );

    for (index, session) in sessions.iter().enumerate() {
        let Some(source_path) = session
            .source_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        else {
            error_sessions += 1;
            continue;
        };

        let Some(source_id) = source_ids.get(session.provider_id.as_str()).copied() else {
            error_sessions += 1;
            continue;
        };

        let key = SessionKey::new(source_id, source_path);
        seen_keys.insert(key.clone());
        let file_state = read_source_metadata(source_path);

        match existing.get(&key) {
            Some(record) if record.matches(&file_state) => {
                skipped_sessions += 1;
            }
            Some(_) => match upsert_session_index(&transaction, &source_ids, session) {
                Ok(()) => updated_sessions += 1,
                Err(err) => {
                    error_sessions += 1;
                    insert_sync_log(&transaction, source_id, source_path, "error", Some(&err))?;
                }
            },
            None => match upsert_session_index(&transaction, &source_ids, session) {
                Ok(()) => added_sessions += 1,
                Err(err) => {
                    error_sessions += 1;
                    insert_sync_log(&transaction, source_id, source_path, "error", Some(&err))?;
                }
            },
        }

        let next_count = i64::try_from(index + 1).unwrap_or(i64::MAX);
        if next_count == scanned_sessions || next_count % 25 == 0 {
            emit_progress(
                &mut on_progress,
                SyncProgress {
                    phase: SyncProgressPhase::Syncing,
                    count: next_count,
                    total: scanned_sessions,
                },
            );
        }
    }

    for (key, existing_record) in existing {
        if seen_keys.contains(&key) {
            continue;
        }

        delete_indexed_session_row(&transaction, existing_record.session_row_id)?;
        insert_sync_log(
            &transaction,
            key.source_id,
            &key.source_path,
            "deleted",
            Some("Removed from index because source file no longer exists"),
        )?;
        removed_sessions += 1;
    }

    cleanup_empty_projects(&transaction)?;
    transaction
        .commit()
        .map_err(|e| format!("Failed to commit refresh transaction: {e}"))?;

    let snapshot = status::get_status(connection)?;
    let result = RefreshSearchIndexResult {
        db_path: snapshot.db_path,
        scanned_sessions,
        added_sessions,
        updated_sessions,
        removed_sessions,
        skipped_sessions,
        error_sessions,
        indexed_sessions: snapshot.sessions_count,
        indexed_messages: snapshot.messages_count,
    };
    emit_progress(
        &mut on_progress,
        SyncProgress {
            phase: SyncProgressPhase::Done,
            count: scanned_sessions,
            total: scanned_sessions,
        },
    );
    Ok(result)
}

pub fn delete_indexed_session(
    connection: &mut Connection,
    provider_id: &str,
    source_path: &str,
) -> Result<bool, String> {
    let transaction = connection
        .transaction()
        .map_err(|e| format!("Failed to start delete-index transaction: {e}"))?;
    let source_ids = seed_sources(&transaction)?;
    let Some(source_id) = source_ids.get(provider_id).copied() else {
        return Ok(false);
    };

    let session_row_id = transaction
        .query_row(
            "SELECT id FROM sessions WHERE source_id = ? AND source_path = ?",
            params![source_id, source_path],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| format!("Failed to look up indexed session for deletion: {e}"))?;

    let Some(session_row_id) = session_row_id else {
        transaction
            .commit()
            .map_err(|e| format!("Failed to finalize delete-index transaction: {e}"))?;
        return Ok(false);
    };

    delete_indexed_session_row(&transaction, session_row_id)?;
    cleanup_empty_projects(&transaction)?;
    insert_sync_log(
        &transaction,
        source_id,
        source_path,
        "deleted",
        Some("Deleted after provider-side session deletion"),
    )?;
    transaction
        .commit()
        .map_err(|e| format!("Failed to commit delete-index transaction: {e}"))?;
    Ok(true)
}

fn clear_existing_data(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            r#"
            DELETE FROM sync_log;
            DELETE FROM messages;
            DELETE FROM sessions;
            DELETE FROM projects;
            "#,
        )
        .map_err(|e| format!("Failed to clear previous search index data: {e}"))
}

fn seed_sources(transaction: &Transaction<'_>) -> Result<HashMap<String, i64>, String> {
    let mut source_ids = HashMap::new();

    for provider in PROVIDERS {
        let base_path = crate::paths::get_provider_base_dir(provider)?;
        transaction
            .execute(
                r#"
                INSERT INTO sources (name, base_path)
                VALUES (?, ?)
                ON CONFLICT(name) DO UPDATE SET base_path = excluded.base_path
                "#,
                params![provider, base_path.to_string_lossy().to_string()],
            )
            .map_err(|e| format!("Failed to upsert source {provider}: {e}"))?;

        let source_id = transaction
            .query_row("SELECT id FROM sources WHERE name = ?", [provider], |row| {
                row.get(0)
            })
            .map_err(|e| format!("Failed to fetch source id for {provider}: {e}"))?;
        source_ids.insert(provider.to_string(), source_id);
    }

    Ok(source_ids)
}

fn upsert_session_index(
    transaction: &Transaction<'_>,
    source_ids: &HashMap<String, i64>,
    session: &SessionMeta,
) -> Result<(), String> {
    let source_id = source_ids
        .get(session.provider_id.as_str())
        .copied()
        .ok_or_else(|| format!("Missing source id for provider {}", session.provider_id))?;
    let source_path = session
        .source_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Session {} has no source_path", session.session_id))?;

    let messages = session_manager::load_messages(&session.provider_id, source_path)
        .map_err(|e| format!("Failed to load session messages: {e}"))?;

    let project = resolve_project(session, source_path);
    let project_id = get_or_create_project(transaction, source_id, &project)?;
    let file_state = read_source_metadata(source_path);
    let message_count = i64::try_from(messages.iter().filter(|message| !message.is_sidechain).count())
        .unwrap_or(i64::MAX);
    let has_tool_use = messages
        .iter()
        .any(|message| !message.tool_names.is_empty() || message.role == "tool");

    let existing_session_id = transaction
        .query_row(
            "SELECT id FROM sessions WHERE source_id = ? AND source_path = ?",
            params![source_id, source_path],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| {
            format!(
                "Failed to query existing indexed session {}: {e}",
                source_path
            )
        })?;

    let session_row_id = if let Some(existing_session_id) = existing_session_id {
        transaction
            .execute(
                r#"
                UPDATE sessions SET
                  project_id = ?,
                  provider_session_id = ?,
                  title = ?,
                  summary = ?,
                  cwd = ?,
                  model = ?,
                  created_at = ?,
                  last_active_at = ?,
                  message_count = ?,
                  has_tool_use = ?,
                  resume_command = ?,
                  raw_mtime = ?,
                  raw_size = ?,
                  indexed_at = datetime('now')
                WHERE id = ?
                "#,
                params![
                    project_id,
                    session.session_id,
                    session.title.as_deref(),
                    session.summary.as_deref(),
                    session.cwd.as_deref(),
                    session.model.as_deref(),
                    session.created_at,
                    session.last_active_at.or(session.created_at),
                    message_count,
                    has_tool_use,
                    session.resume_command.as_deref(),
                    file_state.raw_mtime,
                    file_state.raw_size,
                    existing_session_id,
                ],
            )
            .map_err(|e| format!("Failed to update session {}: {e}", session.session_id))?;
        transaction
            .execute(
                "DELETE FROM messages WHERE session_id = ?",
                params![existing_session_id],
            )
            .map_err(|e| {
                format!(
                    "Failed to clear old messages for {}: {e}",
                    session.session_id
                )
            })?;
        existing_session_id
    } else {
        transaction
            .execute(
                r#"
                INSERT INTO sessions (
                  project_id,
                  source_id,
                  provider_session_id,
                  source_path,
                  title,
                  summary,
                  cwd,
                  model,
                  created_at,
                  last_active_at,
                  message_count,
                  has_tool_use,
                  resume_command,
                  raw_mtime,
                  raw_size
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                params![
                    project_id,
                    source_id,
                    session.session_id,
                    source_path,
                    session.title.as_deref(),
                    session.summary.as_deref(),
                    session.cwd.as_deref(),
                    session.model.as_deref(),
                    session.created_at,
                    session.last_active_at.or(session.created_at),
                    message_count,
                    has_tool_use,
                    session.resume_command.as_deref(),
                    file_state.raw_mtime,
                    file_state.raw_size,
                ],
            )
            .map_err(|e| format!("Failed to insert session {}: {e}", session.session_id))?;
        transaction.last_insert_rowid()
    };

    let mut insert_message = transaction
        .prepare_cached(
            r#"
            INSERT INTO messages (
              session_id,
              source_id,
              msg_uuid,
              parent_uuid,
              role,
              content_text,
              ts,
              is_sidechain,
              tool_names,
              seq
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .map_err(|e| format!("Failed to prepare message insert statement: {e}"))?;

    for (index, message) in messages.iter().enumerate() {
        let tool_names = serde_json::to_string(&message.tool_names)
            .map_err(|e| format!("Failed to encode tool names for {}: {e}", session.session_id))?;
        insert_message
            .execute(params![
                session_row_id,
                source_id,
                message.msg_uuid.as_deref(),
                message.parent_uuid.as_deref(),
                message.role.as_str(),
                message.content.as_str(),
                message.ts,
                message.is_sidechain,
                tool_names,
                i64::try_from(index).unwrap_or(i64::MAX),
            ])
            .map_err(|e| {
                format!(
                    "Failed to insert message for session {}: {e}",
                    session.session_id
                )
            })?;
    }

    insert_sync_log(transaction, source_id, source_path, "ok", None)
}

fn insert_sync_log(
    transaction: &Transaction<'_>,
    source_id: i64,
    source_path: &str,
    status: &str,
    message: Option<&str>,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO sync_log (source_id, source_path, status, message) VALUES (?, ?, ?, ?)",
            params![source_id, source_path, status, message],
        )
        .map_err(|e| format!("Failed to insert sync log for {source_path}: {e}"))?;
    Ok(())
}

fn get_or_create_project(
    transaction: &Transaction<'_>,
    source_id: i64,
    project: &ResolvedProject,
) -> Result<i64, String> {
    let existing = transaction
        .query_row(
            "SELECT id FROM projects WHERE source_id = ? AND slug = ?",
            params![source_id, project.slug],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| format!("Failed to look up project {}: {e}", project.slug))?;

    if let Some(project_id) = existing {
        transaction
            .execute(
                "UPDATE projects SET display_path = ?, display_name = ?, last_synced = datetime('now') WHERE id = ?",
                params![project.display_path, project.display_name, project_id],
            )
            .map_err(|e| format!("Failed to update project {}: {e}", project.slug))?;
        return Ok(project_id);
    }

    transaction
        .execute(
            "INSERT INTO projects (source_id, slug, display_path, display_name, last_synced) VALUES (?, ?, ?, ?, datetime('now'))",
            params![
                source_id,
                project.slug,
                project.display_path,
                project.display_name
            ],
        )
        .map_err(|e| format!("Failed to insert project {}: {e}", project.slug))?;

    Ok(transaction.last_insert_rowid())
}

fn delete_indexed_session_row(
    transaction: &Transaction<'_>,
    session_row_id: i64,
) -> Result<(), String> {
    transaction
        .execute(
            "DELETE FROM messages WHERE session_id = ?",
            params![session_row_id],
        )
        .map_err(|e| {
            format!("Failed to delete indexed messages for session {session_row_id}: {e}")
        })?;
    transaction
        .execute("DELETE FROM sessions WHERE id = ?", params![session_row_id])
        .map_err(|e| format!("Failed to delete indexed session {session_row_id}: {e}"))?;
    Ok(())
}

fn cleanup_empty_projects(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute(
            "DELETE FROM projects WHERE id NOT IN (SELECT DISTINCT project_id FROM sessions)",
            [],
        )
        .map_err(|e| format!("Failed to cleanup empty projects: {e}"))?;
    Ok(())
}

fn load_existing_sessions(
    transaction: &Transaction<'_>,
) -> Result<HashMap<SessionKey, IndexedSessionRecord>, String> {
    let mut stmt = transaction
        .prepare(
            r#"
            SELECT source_id, source_path, id, raw_mtime, raw_size
            FROM sessions
            "#,
        )
        .map_err(|e| format!("Failed to prepare existing session query: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                SessionKey::new(row.get(0)?, row.get::<_, String>(1)?),
                IndexedSessionRecord {
                    session_row_id: row.get(2)?,
                    raw_mtime: row.get(3)?,
                    raw_size: row.get(4)?,
                },
            ))
        })
        .map_err(|e| format!("Failed to query existing sessions: {e}"))?;

    let mut existing = HashMap::new();
    for row in rows {
        let (key, record) = row.map_err(|e| format!("Failed to read existing session row: {e}"))?;
        existing.insert(key, record);
    }
    Ok(existing)
}

fn read_source_metadata(source_path: &str) -> FileState {
    let metadata = match fs::metadata(source_path) {
        Ok(metadata) => metadata,
        Err(_) => return FileState::default(),
    };

    let raw_mtime = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok());
    let raw_size = i64::try_from(metadata.len()).ok();

    FileState {
        raw_mtime,
        raw_size,
    }
}

fn resolve_project(session: &SessionMeta, source_path: &str) -> ResolvedProject {
    let display_path = session
        .project_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| provider_bucket(session.provider_id.as_str()).to_string());

    let display_name = path_basename(&display_path)
        .or_else(|| {
            Path::new(source_path)
                .parent()
                .and_then(|path| path.file_name())
                .and_then(|value| value.to_str())
                .map(|value| value.to_string())
        })
        .unwrap_or_else(|| provider_bucket(session.provider_id.as_str()).to_string());

    let slug_base = if session
        .project_dir
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        display_path.clone()
    } else {
        format!("{}-{display_name}", session.provider_id)
    };

    ResolvedProject {
        slug: slugify(&slug_base),
        display_path,
        display_name,
    }
}

fn provider_bucket(provider_id: &str) -> &'static str {
    match provider_id {
        "claude" => "Claude Sessions",
        "codex" => "Codex Sessions",
        "gemini" => "Gemini Sessions",
        "openclaw" => "OpenClaw Sessions",
        "opencode" => "OpenCode Sessions",
        _ => "Unknown Sessions",
    }
}

fn path_basename(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return None;
    }

    trimmed
        .split(['/', '\\'])
        .next_back()
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
}

fn slugify(input: &str) -> String {
    let mut slug = String::with_capacity(input.len());
    let mut prev_dash = false;

    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            slug.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "default".to_string()
    } else {
        slug
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionKey {
    source_id: i64,
    source_path: String,
}

impl SessionKey {
    fn new(source_id: i64, source_path: impl Into<String>) -> Self {
        Self {
            source_id,
            source_path: source_path.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct FileState {
    raw_mtime: Option<i64>,
    raw_size: Option<i64>,
}

#[derive(Debug, Clone)]
struct IndexedSessionRecord {
    session_row_id: i64,
    raw_mtime: Option<i64>,
    raw_size: Option<i64>,
}

impl IndexedSessionRecord {
    fn matches(&self, file_state: &FileState) -> bool {
        self.raw_mtime == file_state.raw_mtime && self.raw_size == file_state.raw_size
    }
}

fn emit_progress<F>(on_progress: &mut Option<F>, progress: SyncProgress)
where
    F: FnMut(SyncProgress),
{
    if let Some(callback) = on_progress.as_mut() {
        callback(progress);
    }
}

struct ResolvedProject {
    slug: String,
    display_path: String,
    display_name: String,
}
