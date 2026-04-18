mod db;
mod indexer;
mod query;
mod schema;
mod status;
mod tokenizer;
pub mod types;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum SyncProgressPhase {
    Scanning,
    Syncing,
    Done,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct SyncProgress {
    pub phase: SyncProgressPhase,
    pub count: i64,
    pub total: i64,
}

pub use types::{
    IndexedMessage, IndexedProjectOption, IndexedSession, PagedIndexedSessionsResult,
    RebuildSearchIndexResult, RefreshSearchIndexResult, SearchContentResult, SearchIndexStatus,
};

pub fn rebuild_index() -> Result<RebuildSearchIndexResult, String> {
    let mut connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    indexer::rebuild_index(&mut connection)
}

pub fn get_index_status() -> Result<SearchIndexStatus, String> {
    let connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    status::get_status(&connection)
}

pub fn refresh_index() -> Result<RefreshSearchIndexResult, String> {
    let mut connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    indexer::refresh_index(&mut connection)
}

#[allow(dead_code)]
pub(crate) fn refresh_index_with_progress<F>(
    on_progress: F,
) -> Result<RefreshSearchIndexResult, String>
where
    F: FnMut(SyncProgress),
{
    let mut connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    indexer::refresh_index_with_progress(&mut connection, on_progress)
}

pub fn list_indexed_sessions(
    limit: usize,
    provider_id: Option<&str>,
) -> Result<Vec<IndexedSession>, String> {
    let connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    query::list_sessions(&connection, limit, provider_id)
}

pub fn list_indexed_sessions_page(
    limit: usize,
    offset: usize,
    provider_id: Option<&str>,
    project_path: Option<&str>,
) -> Result<PagedIndexedSessionsResult, String> {
    let connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    query::list_sessions_page(&connection, limit, offset, provider_id, project_path)
}

pub fn list_indexed_projects(
    provider_id: Option<&str>,
) -> Result<Vec<IndexedProjectOption>, String> {
    let connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    query::list_projects(&connection, provider_id)
}

pub fn list_indexed_sessions_by_source_paths(
    provider_id: &str,
    source_paths: &[String],
) -> Result<Vec<IndexedSession>, String> {
    let connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    query::list_sessions_by_source_paths(&connection, provider_id, source_paths)
}

pub fn get_indexed_session_messages(
    provider_id: &str,
    source_path: &str,
) -> Result<Vec<IndexedMessage>, String> {
    let connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    query::get_session_messages(&connection, provider_id, source_path)
}

pub fn search_content(
    query: &str,
    limit: usize,
    provider_id: Option<&str>,
    since_ts: Option<i64>,
    project_path: Option<&str>,
    sort_by: Option<&str>,
) -> Result<SearchContentResult, String> {
    let connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    query::search_content(
        &connection,
        query,
        limit,
        provider_id,
        since_ts,
        project_path,
        sort_by,
    )
}

pub fn delete_indexed_session(provider_id: &str, source_path: &str) -> Result<bool, String> {
    let mut connection = db::open_connection()?;
    schema::run_migrations(&connection)?;
    indexer::delete_indexed_session(&mut connection, provider_id, source_path)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn rebuild_and_search_roundtrip() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SearchFixture::new();

        let rebuild = rebuild_index().expect("rebuild search index");
        assert_eq!(rebuild.indexed_sessions, 1);
        assert_eq!(rebuild.indexed_messages, 2);
        assert_eq!(rebuild.error_sessions, 0);

        let status = get_index_status().expect("status");
        assert!(status.ready);
        assert_eq!(status.sessions_count, 1);
        assert_eq!(status.messages_count, 2);

        let sessions = list_indexed_sessions(10, Some("claude")).expect("indexed sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "session-claude-1");
        assert_eq!(sessions[0].message_count, 2);

        let messages =
            get_indexed_session_messages("claude", &fixture.source_path).expect("indexed messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert!(messages[0].content.contains("XYLOPHONE_CANARY_42"));

        let result = search_content(
            "XYLOPHONE_CANARY_42",
            10,
            Some("claude"),
            None,
            None,
            Some("relevance"),
        )
        .expect("search content");
        let hits = result.hits;
        assert_eq!(result.total_count, 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].provider_id, "claude");
        assert_eq!(hits[0].session_id, "session-claude-1");
        assert!(
            hits[0].snippet.contains("mark"),
            "unexpected snippet: {}",
            hits[0].snippet
        );
    }

    #[test]
    fn search_content_finds_cjk_substrings() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SearchFixture::new();

        rebuild_index().expect("rebuild search index");

        let result = search_content("删除", 10, Some("claude"), None, None, Some("relevance"))
            .expect("search chinese substring");
        assert!(result.total_count >= 1);
        assert!(!result.hits.is_empty());
        assert!(result.hits[0].snippet.contains("<mark>删除</mark>"));
        assert_eq!(result.hits[0].source_path, fixture.source_path);
    }

    #[test]
    fn refresh_detects_add_update_remove() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SearchFixture::new();

        let initial = rebuild_index().expect("initial rebuild");
        assert_eq!(initial.indexed_sessions, 1);

        fixture.write_session(
            "session-claude-1",
            &[
                ("user", "Refresh canary ALPHA_1 updated"),
                ("assistant", "Assistant mentions ALPHA_1"),
            ],
        );
        fixture.write_session(
            "session-claude-2",
            &[
                ("user", "Brand new BRAVO_2 session"),
                ("assistant", "BRAVO_2 reply"),
            ],
        );

        let refreshed = refresh_index().expect("refresh");
        assert_eq!(refreshed.added_sessions, 1);
        assert_eq!(refreshed.updated_sessions, 1);
        assert_eq!(refreshed.removed_sessions, 0);

        let hits = search_content("BRAVO_2", 10, Some("claude"), None, None, Some("relevance"))
            .expect("search new session")
            .hits;
        assert_eq!(hits.len(), 2);

        fixture.remove_session("session-claude-1");
        let refreshed = refresh_index().expect("refresh after delete");
        assert_eq!(refreshed.removed_sessions, 1);

        let hits = search_content("ALPHA_1", 10, Some("claude"), None, None, Some("relevance"))
            .expect("search removed session")
            .hits;
        assert!(hits.is_empty());
    }

    #[test]
    fn refresh_updates_title_when_claude_sidecar_changes() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SearchFixture::new();

        rebuild_index().expect("initial rebuild");
        let sessions = list_indexed_sessions(10, Some("claude")).expect("indexed sessions");
        assert_eq!(
            sessions[0].title.as_deref(),
            Some("Investigate XYLOPHONE_CANARY_42 in the search index. 请删除旧逻辑。")
        );

        fixture.write_claude_sessions_index(&[(
            "session-claude-1",
            "[@%E7%A4%BE%E5%9B%A2%E9%A6%96%E9%A1%B5.htm](file:///D:/code/demo-script/web-tools/%E7%A4%BE%E5%9B%A2%E9%A6%96%E9%A1%B5.htm)",
        )]);

        let refreshed = refresh_index().expect("refresh after claude sidecar change");
        assert_eq!(refreshed.updated_sessions, 1);

        let sessions = list_indexed_sessions(10, Some("claude")).expect("indexed sessions");
        assert_eq!(sessions[0].title.as_deref(), Some("@社团首页.htm"));
    }

    #[test]
    fn delete_indexed_session_removes_index_rows() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SearchFixture::new();

        rebuild_index().expect("rebuild");
        let deleted =
            delete_indexed_session("claude", &fixture.source_path).expect("delete indexed session");
        assert!(deleted);

        let sessions = list_indexed_sessions(10, Some("claude")).expect("indexed sessions");
        assert!(sessions.is_empty());
        let hits = search_content(
            "XYLOPHONE_CANARY_42",
            10,
            Some("claude"),
            None,
            None,
            Some("relevance"),
        )
        .expect("search deleted index")
        .hits;
        assert!(hits.is_empty());
    }

    #[test]
    fn search_content_respects_project_and_since_filters() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SearchFixture::new();

        rebuild_index().expect("rebuild");

        let hits = search_content(
            "XYLOPHONE_CANARY_42",
            10,
            Some("claude"),
            Some(1_000),
            Some(&fixture.claude_project_path),
            Some("recent"),
        )
        .expect("search with filters")
        .hits;
        assert_eq!(hits.len(), 2);

        let misses = search_content(
            "XYLOPHONE_CANARY_42",
            10,
            Some("claude"),
            Some(9_999_999_999_999),
            Some(&fixture.claude_project_path),
            Some("recent"),
        )
        .expect("search with strict since filter")
        .hits;
        assert!(misses.is_empty());
    }

    #[test]
    fn list_queries_hide_empty_placeholder_sessions() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SearchFixture::new();

        fixture.write_empty_codex_session("empty-codex-session");
        fixture.write_metadata_only_codex_session("metadata-only-codex-session");
        rebuild_index().expect("rebuild");

        let sessions = list_indexed_sessions(20, Some("codex")).expect("codex indexed sessions");
        assert!(sessions.is_empty());

        let paged = list_indexed_sessions_page(20, 0, Some("codex"), None)
            .expect("paged codex indexed sessions");
        assert_eq!(paged.total_count, 0);
        assert!(paged.items.is_empty());

        let projects = list_indexed_projects(Some("codex")).expect("codex indexed projects");
        assert!(projects.is_empty());

        let by_paths = list_indexed_sessions_by_source_paths(
            "codex",
            &[
                fixture.empty_codex_source_path.clone(),
                fixture.metadata_only_codex_source_path.clone(),
            ],
        )
        .expect("codex by-path sessions");
        assert!(by_paths.is_empty());
    }

    #[test]
    fn indexed_messages_keep_tool_calls_but_search_ignores_them() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SearchFixture::new();

        fixture.write_codex_session_with_tooling("codex-tool-session");
        rebuild_index().expect("rebuild");

        let messages = get_indexed_session_messages("codex", &fixture.codex_tool_source_path)
            .expect("indexed codex tool messages");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[1].kind, "function_call");
        assert_eq!(messages[1].name.as_deref(), Some("shell_command"));
        assert_eq!(messages[2].kind, "function_call_output");
        assert_eq!(messages[2].call_id.as_deref(), Some("call_900"));

        let search = search_content(
            "TOOL_ARGUMENT_CANARY_77",
            10,
            Some("codex"),
            None,
            None,
            Some("relevance"),
        )
        .expect("search codex tool payload");
        assert_eq!(search.total_count, 0);
        assert!(search.hits.is_empty());
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct SearchFixture {
        _tempdir: tempfile::TempDir,
        _env: EnvGuard,
        source_path: String,
        claude_project_path: String,
        codex_sessions_path: String,
        empty_codex_source_path: String,
        metadata_only_codex_source_path: String,
        codex_tool_source_path: String,
    }

    impl SearchFixture {
        fn new() -> Self {
            let tempdir = tempdir().expect("tempdir");
            let base = tempdir.path();

            let index_dir = base.join("index");
            let claude_projects = base.join(".claude").join("projects");
            let claude_project = claude_projects.join("demo-project");
            let codex_sessions = base.join(".codex").join("sessions");
            let gemini_tmp = base.join(".gemini").join("tmp");
            let openclaw_agents = base.join(".openclaw").join("agents");
            let opencode_storage = base
                .join(".local")
                .join("share")
                .join("opencode")
                .join("storage");

            fs::create_dir_all(&claude_project).expect("create claude project");
            fs::create_dir_all(&codex_sessions).expect("create codex root");
            fs::create_dir_all(&gemini_tmp).expect("create gemini root");
            fs::create_dir_all(&openclaw_agents).expect("create openclaw root");
            fs::create_dir_all(&opencode_storage).expect("create opencode root");

            let source_path = claude_project.join("session-claude-1.jsonl");

            let env = EnvGuard::set_many(&[
                ("ACLIV_INDEX_DIR", index_dir.to_string_lossy().as_ref()),
                (
                    "ACLIV_CLAUDE_DIR",
                    claude_projects.to_string_lossy().as_ref(),
                ),
                ("ACLIV_CODEX_DIR", codex_sessions.to_string_lossy().as_ref()),
                ("ACLIV_GEMINI_DIR", gemini_tmp.to_string_lossy().as_ref()),
                (
                    "ACLIV_OPENCLAW_DIR",
                    openclaw_agents.to_string_lossy().as_ref(),
                ),
                (
                    "ACLIV_OPENCODE_DIR",
                    opencode_storage.to_string_lossy().as_ref(),
                ),
            ]);

            let fixture = Self {
                _tempdir: tempdir,
                _env: env,
                source_path: source_path.to_string_lossy().to_string(),
                claude_project_path: claude_project.to_string_lossy().to_string(),
                codex_sessions_path: codex_sessions.to_string_lossy().to_string(),
                empty_codex_source_path: codex_sessions
                    .join("empty-codex-session.jsonl")
                    .to_string_lossy()
                    .to_string(),
                metadata_only_codex_source_path: codex_sessions
                    .join("metadata-only-codex-session.jsonl")
                    .to_string_lossy()
                    .to_string(),
                codex_tool_source_path: codex_sessions
                    .join("codex-tool-session.jsonl")
                    .to_string_lossy()
                    .to_string(),
            };
            fixture.write_session(
                "session-claude-1",
                &[
                    (
                        "user",
                        "Investigate XYLOPHONE_CANARY_42 in the search index. 请删除旧逻辑。",
                    ),
                    (
                        "assistant",
                        "XYLOPHONE_CANARY_42 is now indexed and searchable.",
                    ),
                ],
            );
            fixture
        }

        fn write_session(&self, session_id: &str, messages: &[(&str, &str)]) {
            let session_path =
                PathBuf::from(&self.claude_project_path).join(format!("{session_id}.jsonl"));
            let lines = messages
                .iter()
                .enumerate()
                .map(|(index, (role, content))| {
                    let timestamp = format!("2026-03-28T12:{index:02}:00Z");
                    json!({
                        "sessionId": session_id,
                        "cwd": self.claude_project_path,
                        "timestamp": timestamp,
                        "message": {
                            "role": role,
                            "content": content,
                        }
                    })
                })
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(session_path, format!("{lines}\n")).expect("write fixture session");
        }

        fn write_claude_sessions_index(&self, entries: &[(&str, &str)]) {
            let index_path = PathBuf::from(&self.claude_project_path).join("sessions-index.json");
            let entries = entries
                .iter()
                .map(|(session_id, first_prompt)| {
                    json!({
                        "sessionId": session_id,
                        "firstPrompt": first_prompt,
                    })
                })
                .collect::<Vec<_>>();
            let payload = json!({
                "version": 1,
                "entries": entries,
            });
            fs::write(index_path, payload.to_string()).expect("write claude sessions index");
        }

        fn write_empty_codex_session(&self, session_id: &str) {
            let session_path =
                PathBuf::from(&self.codex_sessions_path).join(format!("{session_id}.jsonl"));
            fs::write(session_path, "").expect("write empty codex session");
        }

        fn write_metadata_only_codex_session(&self, session_id: &str) {
            let session_path =
                PathBuf::from(&self.codex_sessions_path).join(format!("{session_id}.jsonl"));
            let line = json!({
                "timestamp": "2026-03-29T11:59:59Z",
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "cwd": self.claude_project_path,
                    "model": "gpt-5-codex"
                }
            });
            fs::write(session_path, format!("{line}\n"))
                .expect("write metadata-only codex session");
        }

        fn write_codex_session_with_tooling(&self, session_id: &str) {
            let session_path =
                PathBuf::from(&self.codex_sessions_path).join(format!("{session_id}.jsonl"));
            let lines = [
                json!({
                    "timestamp": "2026-03-29T12:00:00Z",
                    "type": "session_meta",
                    "payload": {
                        "id": session_id,
                        "cwd": self.claude_project_path,
                        "model": "gpt-5-codex"
                    }
                }),
                json!({
                    "timestamp": "2026-03-29T12:00:01Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{"type": "input_text", "text": "请检查目录"}]
                    }
                }),
                json!({
                    "timestamp": "2026-03-29T12:00:02Z",
                    "type": "response_item",
                    "payload": {
                        "type": "function_call",
                        "name": "shell_command",
                        "arguments": "{\"command\":\"Write-Output TOOL_ARGUMENT_CANARY_77\"}",
                        "call_id": "call_900"
                    }
                }),
                json!({
                    "timestamp": "2026-03-29T12:00:03Z",
                    "type": "response_item",
                    "payload": {
                        "type": "function_call_output",
                        "call_id": "call_900",
                        "output": "Exit code: 0\nOutput:\nTOOL_ARGUMENT_CANARY_77"
                    }
                }),
                json!({
                    "timestamp": "2026-03-29T12:00:04Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": "目录检查完成"}]
                    }
                }),
            ]
            .into_iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join("\n");
            fs::write(session_path, format!("{lines}\n")).expect("write codex tool session");
        }

        fn remove_session(&self, session_id: &str) {
            let session_path =
                PathBuf::from(&self.claude_project_path).join(format!("{session_id}.jsonl"));
            if session_path.exists() {
                fs::remove_file(session_path).expect("remove fixture session");
            }
        }
    }

    struct EnvGuard {
        saved: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn set_many(entries: &[(&str, &str)]) -> Self {
            let mut saved = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                saved.push(((*key).to_string(), std::env::var(key).ok()));
                std::env::set_var(key, value);
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.saved.iter().rev() {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}
