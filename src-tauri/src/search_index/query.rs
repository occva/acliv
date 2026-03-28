use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use super::types::{
    IndexedMessage, IndexedProjectOption, IndexedSession, PagedIndexedSessionsResult,
    SearchContentResult, SearchFragmentHit,
};

pub fn list_sessions(
    connection: &Connection,
    limit: usize,
    provider_id: Option<&str>,
) -> Result<Vec<IndexedSession>, String> {
    let mut sql = String::from(
        r#"
        SELECT
          src.name,
          sess.provider_session_id,
          sess.source_path,
          sess.title,
          sess.summary,
          sess.resume_command,
          sess.cwd,
          sess.model,
          proj.display_path,
          proj.display_name,
          sess.created_at,
          sess.last_active_at,
          sess.message_count,
          sess.has_tool_use
        FROM sessions sess
        JOIN sources src ON src.id = sess.source_id
        JOIN projects proj ON proj.id = sess.project_id
        "#,
    );

    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(provider_id) = provider_id.filter(|value| !value.trim().is_empty()) {
        sql.push_str(" WHERE src.name = ?");
        params.push(provider_id.to_string().into());
    }

    sql.push_str(" ORDER BY sess.last_active_at DESC, sess.created_at DESC LIMIT ?");
    params.push(i64::try_from(limit).unwrap_or(i64::MAX).into());

    let mut stmt = connection
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare indexed session query: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(IndexedSession {
                provider_id: row.get(0)?,
                session_id: row.get(1)?,
                source_path: row.get(2)?,
                title: row.get(3)?,
                summary: row.get(4)?,
                resume_command: row.get(5)?,
                cwd: row.get(6)?,
                model: row.get(7)?,
                project: row.get(8)?,
                project_name: row.get(9)?,
                created_at: row.get(10)?,
                last_active_at: row.get(11)?,
                message_count: row.get(12)?,
                has_tool_use: row.get::<_, bool>(13)?,
            })
        })
        .map_err(|e| format!("Failed to execute indexed session query: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read indexed sessions: {e}"))
}

pub fn list_sessions_page(
    connection: &Connection,
    limit: usize,
    offset: usize,
    provider_id: Option<&str>,
    project_path: Option<&str>,
) -> Result<PagedIndexedSessionsResult, String> {
    let mut conditions: Vec<String> = Vec::new();
    let mut filter_params: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(provider_id) = provider_id.filter(|value| !value.trim().is_empty()) {
        conditions.push("src.name = ?".to_string());
        filter_params.push(provider_id.to_string().into());
    }

    if let Some(project_path) = project_path.filter(|value| !value.trim().is_empty()) {
        conditions.push("proj.display_path = ?".to_string());
        filter_params.push(project_path.to_string().into());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!(
        r#"
        SELECT COUNT(*)
        FROM sessions sess
        JOIN sources src ON src.id = sess.source_id
        JOIN projects proj ON proj.id = sess.project_id
        {where_clause}
        "#
    );
    let total_count = connection
        .query_row(&count_sql, params_from_iter(filter_params.iter()), |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|e| format!("Failed to count paged indexed sessions: {e}"))?;

    let sql = format!(
        r#"
        SELECT
          src.name,
          sess.provider_session_id,
          sess.source_path,
          sess.title,
          sess.summary,
          sess.resume_command,
          sess.cwd,
          sess.model,
          proj.display_path,
          proj.display_name,
          sess.created_at,
          sess.last_active_at,
          sess.message_count,
          sess.has_tool_use
        FROM sessions sess
        JOIN sources src ON src.id = sess.source_id
        JOIN projects proj ON proj.id = sess.project_id
        {where_clause}
        ORDER BY sess.last_active_at DESC, sess.created_at DESC
        LIMIT ? OFFSET ?
        "#
    );

    let mut params = filter_params;
    params.push(i64::try_from(limit).unwrap_or(i64::MAX).into());
    params.push(i64::try_from(offset).unwrap_or(i64::MAX).into());

    let mut stmt = connection
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare paged indexed session query: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(IndexedSession {
                provider_id: row.get(0)?,
                session_id: row.get(1)?,
                source_path: row.get(2)?,
                title: row.get(3)?,
                summary: row.get(4)?,
                resume_command: row.get(5)?,
                cwd: row.get(6)?,
                model: row.get(7)?,
                project: row.get(8)?,
                project_name: row.get(9)?,
                created_at: row.get(10)?,
                last_active_at: row.get(11)?,
                message_count: row.get(12)?,
                has_tool_use: row.get::<_, bool>(13)?,
            })
        })
        .map_err(|e| format!("Failed to execute paged indexed session query: {e}"))?;

    let items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read paged indexed sessions: {e}"))?;

    Ok(PagedIndexedSessionsResult { total_count, items })
}

pub fn list_projects(
    connection: &Connection,
    provider_id: Option<&str>,
) -> Result<Vec<IndexedProjectOption>, String> {
    let mut sql = String::from(
        r#"
        SELECT
          proj.display_path,
          proj.display_name,
          COUNT(sess.id) AS sessions_count
        FROM projects proj
        JOIN sources src ON src.id = proj.source_id
        LEFT JOIN sessions sess ON sess.project_id = proj.id
        "#,
    );

    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(provider_id) = provider_id.filter(|value| !value.trim().is_empty()) {
        sql.push_str(" WHERE src.name = ?");
        params.push(provider_id.to_string().into());
    }

    sql.push_str(
        " GROUP BY proj.id, proj.display_path, proj.display_name ORDER BY sessions_count DESC, proj.display_name ASC",
    );

    let mut stmt = connection
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare indexed project query: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(IndexedProjectOption {
                project: row.get(0)?,
                project_name: row.get(1)?,
                sessions_count: row.get(2)?,
            })
        })
        .map_err(|e| format!("Failed to execute indexed project query: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read indexed projects: {e}"))
}

pub fn list_sessions_by_source_paths(
    connection: &Connection,
    provider_id: &str,
    source_paths: &[String],
) -> Result<Vec<IndexedSession>, String> {
    let filtered_paths = source_paths
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if filtered_paths.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = std::iter::repeat_n("?", filtered_paths.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        r#"
        SELECT
          src.name,
          sess.provider_session_id,
          sess.source_path,
          sess.title,
          sess.summary,
          sess.resume_command,
          sess.cwd,
          sess.model,
          proj.display_path,
          proj.display_name,
          sess.created_at,
          sess.last_active_at,
          sess.message_count,
          sess.has_tool_use
        FROM sessions sess
        JOIN sources src ON src.id = sess.source_id
        JOIN projects proj ON proj.id = sess.project_id
        WHERE src.name = ?
          AND sess.source_path IN ({placeholders})
        ORDER BY sess.last_active_at DESC, sess.created_at DESC
        "#,
    );

    let mut params: Vec<rusqlite::types::Value> = Vec::with_capacity(filtered_paths.len() + 1);
    params.push(provider_id.to_string().into());
    params.extend(filtered_paths.into_iter().map(Into::into));

    let mut stmt = connection
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare indexed source-path query: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(IndexedSession {
                provider_id: row.get(0)?,
                session_id: row.get(1)?,
                source_path: row.get(2)?,
                title: row.get(3)?,
                summary: row.get(4)?,
                resume_command: row.get(5)?,
                cwd: row.get(6)?,
                model: row.get(7)?,
                project: row.get(8)?,
                project_name: row.get(9)?,
                created_at: row.get(10)?,
                last_active_at: row.get(11)?,
                message_count: row.get(12)?,
                has_tool_use: row.get::<_, bool>(13)?,
            })
        })
        .map_err(|e| format!("Failed to execute indexed source-path query: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read indexed source-path sessions: {e}"))
}

pub fn get_session_messages(
    connection: &Connection,
    provider_id: &str,
    source_path: &str,
) -> Result<Vec<IndexedMessage>, String> {
    let session_id = connection
        .query_row(
            r#"
            SELECT sess.id
            FROM sessions sess
            JOIN sources src ON src.id = sess.source_id
            WHERE src.name = ? AND sess.source_path = ?
            "#,
            params![provider_id, source_path],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| format!("Failed to look up indexed session: {e}"))?
        .ok_or_else(|| format!("Indexed session not found for {provider_id}:{source_path}"))?;

    let mut stmt = connection
        .prepare(
            r#"
            SELECT msg_uuid, parent_uuid, role, content_text, ts, is_sidechain, tool_names, seq
            FROM messages
            WHERE session_id = ?
              AND COALESCE(is_sidechain, 0) = 0
            ORDER BY seq
            "#,
        )
        .map_err(|e| format!("Failed to prepare indexed message query: {e}"))?;
    let rows = stmt
        .query_map([session_id], |row| {
            Ok(IndexedMessage {
                msg_uuid: row.get(0)?,
                parent_uuid: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                ts: row.get(4)?,
                is_sidechain: row.get::<_, bool>(5)?,
                tool_names: serde_json::from_str::<Vec<String>>(
                    row.get::<_, String>(6)?.as_str(),
                )
                .unwrap_or_default(),
                seq: row.get(7)?,
            })
        })
        .map_err(|e| format!("Failed to execute indexed message query: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read indexed messages: {e}"))
}

pub fn search_content(
    connection: &Connection,
    query: &str,
    limit: usize,
    provider_id: Option<&str>,
    since_ts: Option<i64>,
    project_path: Option<&str>,
    sort_by: Option<&str>,
) -> Result<SearchContentResult, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(SearchContentResult {
            total_count: 0,
            hits: Vec::new(),
        });
    }

    let fts_query = build_fts_query(trimmed);
    let has_structured_filters = provider_id
        .filter(|value| !value.trim().is_empty())
        .is_some()
        || since_ts.is_some()
        || project_path
            .filter(|value| !value.trim().is_empty())
            .is_some();
    let candidate_limit = compute_candidate_limit(limit, has_structured_filters);
    let mut conditions = vec![
        "messages_fts MATCH ?".to_string(),
        "COALESCE(msg.is_sidechain, 0) = 0".to_string(),
    ];
    let mut filter_params: Vec<rusqlite::types::Value> = vec![fts_query.clone().into()];

    if let Some(provider_id) = provider_id.filter(|value| !value.trim().is_empty()) {
        conditions.push("src.name = ?".to_string());
        filter_params.push(provider_id.to_string().into());
    }

    if let Some(since_ts) = since_ts {
        conditions.push("COALESCE(msg.ts, sess.last_active_at, sess.created_at, 0) >= ?".to_string());
        filter_params.push(since_ts.into());
    }

    if let Some(project_path) = project_path.filter(|value| !value.trim().is_empty()) {
        conditions.push("proj.display_path = ?".to_string());
        filter_params.push(project_path.to_string().into());
    }

    let mut params: Vec<rusqlite::types::Value> = vec![
        fts_query.into(),
        i64::try_from(candidate_limit).unwrap_or(i64::MAX).into(),
    ];
    params.extend(filter_params.iter().cloned());

    let count_sql = format!(
        r#"
        SELECT COUNT(*)
        FROM messages_fts
        JOIN messages msg ON msg.id = messages_fts.rowid
        JOIN sessions sess ON sess.id = msg.session_id
        JOIN projects proj ON proj.id = sess.project_id
        JOIN sources src ON src.id = sess.source_id
        WHERE {}
        "#,
        conditions.join(" AND ")
    );
    let total_count = connection
        .query_row(&count_sql, params_from_iter(filter_params.iter()), |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|e| format!("Failed to count content search results: {e}"))?;

    let order_by = if matches!(sort_by, Some("recent")) {
        "COALESCE(msg.ts, sess.last_active_at, sess.created_at, 0) DESC, matched.score ASC"
    } else {
        "matched.score ASC, COALESCE(msg.ts, sess.last_active_at, sess.created_at, 0) DESC"
    };

    let sql = format!(
        r#"
        WITH matched_messages AS (
          SELECT
            rowid AS message_id,
            bm25(messages_fts) AS score
          FROM messages_fts
          WHERE messages_fts MATCH ?
          ORDER BY score ASC
          LIMIT ?
        )
        SELECT
          matched.score,
          src.name,
          sess.provider_session_id,
          sess.source_path,
          COALESCE(sess.title, sess.summary, sess.provider_session_id),
          proj.display_path,
          sess.last_active_at,
          snippet(messages_fts, 0, '<mark>', '</mark>', '…', 20),
          msg.role,
          msg.ts,
          msg.seq
        FROM matched_messages matched
        JOIN messages_fts ON messages_fts.rowid = matched.message_id
        JOIN messages msg ON msg.id = matched.message_id
        JOIN sessions sess ON sess.id = msg.session_id
        JOIN projects proj ON proj.id = sess.project_id
        JOIN sources src ON src.id = sess.source_id
        WHERE {}
        ORDER BY {order_by}
        LIMIT ?
        "#,
        conditions.join(" AND ")
    );

    params.push(i64::try_from(limit).unwrap_or(i64::MAX).into());

    let mut stmt = connection
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare content search query: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(SearchFragmentHit {
                rank: 0,
                provider_id: row.get(1)?,
                session_id: row.get(2)?,
                source_path: row.get(3)?,
                session_title: row.get(4)?,
                project: row.get(5)?,
                last_active_at: row.get(6)?,
                snippet: row.get(7)?,
                message_role: row.get(8)?,
                message_timestamp: row.get(9)?,
                seq: row.get(10)?,
            })
        })
        .map_err(|e| format!("Failed to execute content search query: {e}"))?;

    let mut hits = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read content search results: {e}"))?;

    for (index, hit) in hits.iter_mut().enumerate() {
        hit.rank = i64::try_from(index + 1).unwrap_or(i64::MAX);
    }

    Ok(SearchContentResult { total_count, hits })
}

fn compute_candidate_limit(limit: usize, has_structured_filters: bool) -> usize {
    let floor = if has_structured_filters { 200 } else { 100 };
    let multiplier = if has_structured_filters { 24 } else { 12 };
    limit.saturating_mul(multiplier).max(floor)
}

fn build_fts_query(query: &str) -> String {
    if query.contains('"') || query.contains('*') || query.contains(" OR ") {
        query.to_string()
    } else {
        format!("\"{}\"", query.replace('"', "\"\""))
    }
}
