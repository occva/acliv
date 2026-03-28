use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{
    event::{CreateKind, ModifyKind, RemoveKind},
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::search_index;
use crate::search_index::types::IndexedSourceRef;

const SEARCH_INDEX_SYNC_EVENT: &str = "search-index-sync";
const PROVIDERS: [&str; 5] = ["claude", "codex", "gemini", "openclaw", "opencode"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexSyncEvent {
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_sessions: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_messages: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_sessions: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_sessions: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed_sessions: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_sessions: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_sessions: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_sources: Option<Vec<IndexedSourceRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<search_index::SearchIndexStatus>,
}

impl SearchIndexSyncEvent {
    fn refreshing(message: impl Into<String>) -> Self {
        Self {
            phase: "refreshing".to_string(),
            message: Some(message.into()),
            count: None,
            total: None,
            indexed_sessions: None,
            indexed_messages: None,
            added_sessions: None,
            updated_sessions: None,
            removed_sessions: None,
            skipped_sessions: None,
            error_sessions: None,
            changed_sources: None,
            status: None,
        }
    }

    fn progress(phase: &str, count: i64, total: i64, message: impl Into<String>) -> Self {
        Self {
            phase: phase.to_string(),
            message: Some(message.into()),
            count: Some(count),
            total: Some(total),
            indexed_sessions: None,
            indexed_messages: None,
            added_sessions: None,
            updated_sessions: None,
            removed_sessions: None,
            skipped_sessions: None,
            error_sessions: None,
            changed_sources: None,
            status: None,
        }
    }

    fn updated(
        result: &search_index::RefreshSearchIndexResult,
        changed_sources: Vec<IndexedSourceRef>,
        status: search_index::SearchIndexStatus,
    ) -> Self {
        Self {
            phase: "updated".to_string(),
            message: None,
            count: Some(result.scanned_sessions),
            total: Some(result.scanned_sessions),
            indexed_sessions: Some(result.indexed_sessions),
            indexed_messages: Some(result.indexed_messages),
            added_sessions: Some(result.added_sessions),
            updated_sessions: Some(result.updated_sessions),
            removed_sessions: Some(result.removed_sessions),
            skipped_sessions: Some(result.skipped_sessions),
            error_sessions: Some(result.error_sessions),
            changed_sources: Some(changed_sources),
            status: Some(status),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            phase: "error".to_string(),
            message: Some(message.into()),
            count: None,
            total: None,
            indexed_sessions: None,
            indexed_messages: None,
            added_sessions: None,
            updated_sessions: None,
            removed_sessions: None,
            skipped_sessions: None,
            error_sessions: None,
            changed_sources: None,
            status: None,
        }
    }
}

pub fn start_search_index_watcher(app_handle: AppHandle) -> Result<(), String> {
    let provider_roots = collect_watch_roots()?;
    if provider_roots.is_empty() {
        log::info!("Search index watcher skipped: no provider roots found");
        return Ok(());
    }

    thread::spawn(move || {
        if let Err(err) = watch_loop(app_handle, provider_roots) {
            log::error!("Search index watcher stopped: {err}");
        }
    });

    Ok(())
}

fn watch_loop(app_handle: AppHandle, provider_roots: Vec<(String, PathBuf)>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<Vec<PendingWatchChange>>();
    let callback_handle = app_handle.clone();
    let callback_tx = tx.clone();
    let callback_roots = provider_roots.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<notify::Event>| match result {
            Ok(event) if should_refresh(&event.kind) => {
                let changed_sources = extract_changed_sources(&callback_roots, &event.paths);
                if !changed_sources.is_empty() {
                    let _ = callback_tx.send(changed_sources);
                }
            }
            Ok(_) => {}
            Err(err) => {
                log::error!("Search index watcher notify error: {err}");
                emit_event(
                    &callback_handle,
                    SearchIndexSyncEvent::error(format!("Watcher error: {err}")),
                );
            }
        },
        notify::Config::default(),
    )
    .map_err(|e| format!("Failed to create search index watcher: {e}"))?;

    for (_, root) in &provider_roots {
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch {}: {e}", root.display()))?;
        log::info!("Watching provider root: {}", root.display());
    }

    loop {
        let mut pending_changes = match rx.recv() {
            Ok(changed_sources) => changed_sources,
            Err(_) => return Ok(()),
        };

        emit_event(&app_handle, SearchIndexSyncEvent::refreshing("Detected session file changes"));

        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(800)) {
            merge_changed_sources(&mut pending_changes, batch);
        }

        wait_for_stable_paths(&mut pending_changes, Duration::from_millis(1800), Duration::from_millis(250));
        let changed_sources = pending_changes
            .iter()
            .map(|change| change.source.clone())
            .collect::<Vec<_>>();

        match search_index::refresh_index_with_progress(|progress| {
            let (phase, label) = match progress.phase {
                search_index::SyncProgressPhase::Scanning => ("scanning", "Scanning sessions"),
                search_index::SyncProgressPhase::Syncing => ("syncing", "Updating local index"),
                search_index::SyncProgressPhase::Done => ("done", "Sync complete"),
            };
            emit_event(
                &app_handle,
                SearchIndexSyncEvent::progress(phase, progress.count, progress.total, label),
            );
        }) {
            Ok(result) => {
                let status = match search_index::get_index_status() {
                    Ok(status) => status,
                    Err(err) => {
                        log::error!("Failed to load search index status after watcher refresh: {err}");
                        search_index::SearchIndexStatus {
                            db_path: crate::paths::get_search_db_path().to_string_lossy().to_string(),
                            ready: result.indexed_sessions > 0,
                            sources_count: 0,
                            projects_count: 0,
                            sessions_count: result.indexed_sessions,
                            messages_count: result.indexed_messages,
                            last_indexed_at: None,
                            last_successful_sync_at: None,
                            last_error_at: None,
                            db_size_bytes: 0,
                            error_count: result.error_sessions,
                            sources: Vec::new(),
                        }
                    }
                };
                emit_event(
                    &app_handle,
                    SearchIndexSyncEvent::updated(&result, changed_sources, status),
                );
            }
            Err(err) => {
                log::error!("Search index refresh from watcher failed: {err}");
                emit_event(&app_handle, SearchIndexSyncEvent::error(err));
            }
        }
    }
}

fn collect_watch_roots() -> Result<Vec<(String, PathBuf)>, String> {
    let mut roots = Vec::new();
    for provider in PROVIDERS {
        let root = crate::paths::get_provider_base_dir(provider)?;
        if root.exists() {
            roots.push((provider.to_string(), root));
        }
    }
    Ok(roots)
}

fn should_refresh(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(CreateKind::File)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Modify(ModifyKind::Any)
            | EventKind::Remove(RemoveKind::File)
            | EventKind::Remove(RemoveKind::Any)
    )
}

fn extract_changed_sources(
    provider_roots: &[(String, PathBuf)],
    paths: &[PathBuf],
) -> Vec<PendingWatchChange> {
    let mut unique = HashMap::<String, PendingWatchChange>::new();
    for path in paths {
        for (provider_id, root) in provider_roots {
            if !path.starts_with(root) {
                continue;
            }

            let source_path = path.to_string_lossy().to_string();
            let key = format!("{provider_id}:{source_path}");
            unique.entry(key).or_insert_with(|| PendingWatchChange {
                source: IndexedSourceRef {
                    provider_id: provider_id.clone(),
                    source_path,
                },
                fs_path: path.clone(),
            });
            break;
        }
    }

    unique.into_values().collect()
}

fn merge_changed_sources(target: &mut Vec<PendingWatchChange>, incoming: Vec<PendingWatchChange>) {
    let mut unique = target
        .drain(..)
        .map(|item| {
            (
                format!("{}:{}", item.source.provider_id, item.source.source_path),
                item,
            )
        })
        .collect::<HashMap<_, _>>();
    for item in incoming {
        unique.insert(
            format!("{}:{}", item.source.provider_id, item.source.source_path),
            item,
        );
    }
    *target = unique.into_values().collect();
}

fn wait_for_stable_paths(
    pending_changes: &mut Vec<PendingWatchChange>,
    stability_threshold: Duration,
    poll_interval: Duration,
) {
    if pending_changes.is_empty() {
        return;
    }

    let mut last_seen = pending_changes
        .iter()
        .map(|change| (change.fs_path.clone(), read_path_signature(&change.fs_path)))
        .collect::<HashMap<_, _>>();
    let mut stable_since = SystemTime::now();

    loop {
        thread::sleep(poll_interval);

        let current = pending_changes
            .iter()
            .map(|change| (change.fs_path.clone(), read_path_signature(&change.fs_path)))
            .collect::<HashMap<_, _>>();

        if current == last_seen {
            if stable_since.elapsed().unwrap_or_default() >= stability_threshold {
                return;
            }
        } else {
            last_seen = current;
            stable_since = SystemTime::now();
        }
    }
}

fn read_path_signature(path: &PathBuf) -> PathSignature {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return PathSignature::Missing,
    };

    let len = metadata.len();
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok());

    PathSignature::Present {
        len,
        modified_millis: modified,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSignature {
    Missing,
    Present {
        len: u64,
        modified_millis: Option<i64>,
    },
}

#[derive(Debug, Clone)]
struct PendingWatchChange {
    source: IndexedSourceRef,
    fs_path: PathBuf,
}

fn emit_event(app_handle: &AppHandle, payload: SearchIndexSyncEvent) {
    if let Err(err) = app_handle.emit(SEARCH_INDEX_SYNC_EVENT, payload) {
        log::error!("Failed to emit search index sync event: {err}");
    }
}
