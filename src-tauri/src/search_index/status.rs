use std::fs;

use rusqlite::{Connection, OptionalExtension};

use super::types::{SearchIndexSourceStat, SearchIndexStatus};

pub fn get_status(connection: &Connection) -> Result<SearchIndexStatus, String> {
    let db_path_buf = crate::paths::get_search_db_path();
    let db_path = db_path_buf.to_string_lossy().to_string();
    let db_size_bytes = fs::metadata(&db_path_buf)
        .ok()
        .and_then(|metadata| i64::try_from(metadata.len()).ok())
        .unwrap_or(0);
    let ready = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'sessions'")
        .and_then(|mut stmt| stmt.exists([]))
        .map_err(|e| format!("Failed to inspect search DB schema: {e}"))?;

    if !ready {
        return Ok(SearchIndexStatus {
            db_path,
            ready: false,
            sources_count: 0,
            projects_count: 0,
            sessions_count: 0,
            messages_count: 0,
            last_indexed_at: None,
            last_successful_sync_at: None,
            last_error_at: None,
            db_size_bytes,
            error_count: 0,
            sources: Vec::new(),
        });
    }

    let sources_count = count_table(connection, "sources")?;
    let projects_count = count_table(connection, "projects")?;
    let sessions_count = count_table(connection, "sessions")?;
    let messages_count = count_table(connection, "messages")?;
    let error_count = connection
        .query_row(
            "SELECT COUNT(*) FROM sync_log WHERE status = 'error'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| format!("Failed to count sync errors: {e}"))?;
    let last_indexed_at = connection
        .query_row("SELECT MAX(indexed_at) FROM sessions", [], |row| {
            row.get::<_, Option<String>>(0)
        })
        .optional()
        .map_err(|e| format!("Failed to read last indexed timestamp: {e}"))?
        .flatten();
    let last_successful_sync_at = connection
        .query_row(
            "SELECT MAX(synced_at) FROM sync_log WHERE status = 'ok'",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|e| format!("Failed to read last successful sync timestamp: {e}"))?
        .flatten();
    let last_error_at = connection
        .query_row(
            "SELECT MAX(synced_at) FROM sync_log WHERE status = 'error'",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|e| format!("Failed to read last error sync timestamp: {e}"))?
        .flatten();
    let sources = list_source_stats(connection)?;

    Ok(SearchIndexStatus {
        db_path,
        ready: true,
        sources_count,
        projects_count,
        sessions_count,
        messages_count,
        last_indexed_at,
        last_successful_sync_at,
        last_error_at,
        db_size_bytes,
        error_count,
        sources,
    })
}

fn count_table(connection: &Connection, table: &str) -> Result<i64, String> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    connection
        .query_row(&sql, [], |row| row.get::<_, i64>(0))
        .map_err(|e| format!("Failed to count {table}: {e}"))
}

fn list_source_stats(connection: &Connection) -> Result<Vec<SearchIndexSourceStat>, String> {
    let mut stmt = connection
        .prepare(
            r#"
            SELECT
              src.name,
              COUNT(DISTINCT sess.project_id) AS projects_count,
              COUNT(DISTINCT sess.id) AS sessions_count,
              COUNT(msg.id) AS messages_count
            FROM sources src
            LEFT JOIN sessions sess ON sess.source_id = src.id
            LEFT JOIN messages msg ON msg.source_id = src.id
            GROUP BY src.id, src.name
            ORDER BY src.name
            "#,
        )
        .map_err(|e| format!("Failed to prepare source stats query: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(SearchIndexSourceStat {
                provider_id: row.get(0)?,
                projects_count: row.get(1)?,
                sessions_count: row.get(2)?,
                messages_count: row.get(3)?,
            })
        })
        .map_err(|e| format!("Failed to execute source stats query: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read source stats: {e}"))
}
