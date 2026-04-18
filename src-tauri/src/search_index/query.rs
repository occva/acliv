use std::collections::HashSet;

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use super::tokenizer;
use super::types::{
    IndexedMessage, IndexedProjectOption, IndexedSession, PagedIndexedSessionsResult,
    SearchContentResult, SearchFragmentHit,
};

#[derive(Debug, Clone)]
struct InternalSearchHit {
    message_id: i64,
    score: f64,
    sort_ts: i64,
    hit: SearchFragmentHit,
}

const VISIBLE_SESSION_CONDITION: &str = r#"
(
  COALESCE(sess.message_count, 0) > 0
)
"#;
const SNIPPET_WINDOW_CHARS: i64 = 120;
const SNIPPET_CONTEXT_CHARS: i64 = 20;

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

    sql.push_str(&format!(" WHERE {VISIBLE_SESSION_CONDITION}"));

    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(provider_id) = provider_id.filter(|value| !value.trim().is_empty()) {
        sql.push_str(" AND src.name = ?");
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
        format!(" WHERE {VISIBLE_SESSION_CONDITION}")
    } else {
        format!(
            " WHERE {VISIBLE_SESSION_CONDITION} AND {}",
            conditions.join(" AND ")
        )
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
          AND "#,
    );
    sql.push_str(VISIBLE_SESSION_CONDITION);

    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(provider_id) = provider_id.filter(|value| !value.trim().is_empty()) {
        sql.push_str(" WHERE src.name = ?");
        params.push(provider_id.to_string().into());
    }

    sql.push_str(
        " GROUP BY proj.id, proj.display_path, proj.display_name HAVING COUNT(sess.id) > 0 ORDER BY sessions_count DESC, proj.display_name ASC",
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
          AND {VISIBLE_SESSION_CONDITION}
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
            SELECT
              msg_uuid,
              parent_uuid,
              role,
              kind,
              name,
              call_id,
              content_text,
              ts,
              is_sidechain,
              tool_names,
              seq
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
                kind: row.get(3)?,
                name: row.get(4)?,
                call_id: row.get(5)?,
                content: row.get(6)?,
                ts: row.get(7)?,
                is_sidechain: row.get::<_, bool>(8)?,
                tool_names: serde_json::from_str::<Vec<String>>(row.get::<_, String>(9)?.as_str())
                    .unwrap_or_default(),
                seq: row.get(10)?,
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

    let fts_result = search_content_fts(
        connection,
        trimmed,
        limit,
        provider_id,
        since_ts,
        project_path,
        sort_by,
    )?;

    if !tokenizer::should_run_substring_fallback(trimmed, fts_result.total_count) {
        return Ok(finalize_search_result(
            fts_result.total_count,
            fts_result.hits,
        ));
    }

    let fallback_result = search_content_substring(
        connection,
        trimmed,
        limit,
        provider_id,
        since_ts,
        project_path,
    )?;

    Ok(finalize_search_result(
        fallback_result.total_count,
        merge_search_hits(fts_result.hits, fallback_result.hits, limit, sort_by),
    ))
}

fn compute_candidate_limit(limit: usize, has_structured_filters: bool) -> usize {
    let floor = if has_structured_filters { 200 } else { 100 };
    let multiplier = if has_structured_filters { 24 } else { 12 };
    limit.saturating_mul(multiplier).max(floor)
}

fn build_snippet_probe_params(query: &str) -> (String, String, i64) {
    let probe = tokenizer::best_snippet_probe(query).unwrap_or_default();
    let folded = if probe.is_ascii() {
        probe.to_ascii_lowercase()
    } else {
        probe.clone()
    };
    let is_ascii = if probe.is_ascii() { 1 } else { 0 };
    (probe, folded, is_ascii)
}

fn build_snippet_source_sql() -> String {
    format!(
        r#"
        CASE
          WHEN snippet_probe = '' THEN
            CASE
              WHEN content_len > {window} THEN substr(content_text, 1, {window}) || '…'
              ELSE content_text
            END
          WHEN snippet_pos > 0 THEN
            (CASE WHEN snippet_pos > {context_plus_one} THEN '…' ELSE '' END) ||
            substr(
              content_text,
              CASE WHEN snippet_pos > {context} THEN snippet_pos - {context} ELSE 1 END,
              {window}
            ) ||
            (CASE
              WHEN content_len > (
                CASE
                  WHEN snippet_pos > {context} THEN snippet_pos - {context} + {window} - 1
                  ELSE {window}
                END
              ) THEN '…'
              ELSE ''
            END)
          ELSE
            CASE
              WHEN content_len > {window} THEN substr(content_text, 1, {window}) || '…'
              ELSE content_text
            END
        END
        "#,
        window = SNIPPET_WINDOW_CHARS,
        context = SNIPPET_CONTEXT_CHARS,
        context_plus_one = SNIPPET_CONTEXT_CHARS + 1,
    )
}

fn search_content_fts(
    connection: &Connection,
    query: &str,
    limit: usize,
    provider_id: Option<&str>,
    since_ts: Option<i64>,
    project_path: Option<&str>,
    sort_by: Option<&str>,
) -> Result<SearchContentResultInternal, String> {
    let fts_query = tokenizer::build_fts_query(query);
    let (snippet_probe, snippet_probe_folded, snippet_probe_is_ascii) =
        build_snippet_probe_params(query);
    let snippet_source_sql = build_snippet_source_sql();
    let has_structured_filters = provider_id
        .filter(|value| !value.trim().is_empty())
        .is_some()
        || since_ts.is_some()
        || project_path
            .filter(|value| !value.trim().is_empty())
            .is_some();
    let candidate_limit = compute_candidate_limit(limit, has_structured_filters);
    let (structured_conditions, structured_params) =
        build_structured_conditions(provider_id, since_ts, project_path);

    let mut count_conditions = vec!["messages_fts MATCH ?".to_string()];
    count_conditions.extend(structured_conditions.iter().cloned());

    let mut count_params: Vec<rusqlite::types::Value> = vec![fts_query.clone().into()];
    count_params.extend(structured_params.iter().cloned());

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
        count_conditions.join(" AND ")
    );
    let total_count = connection
        .query_row(&count_sql, params_from_iter(count_params.iter()), |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|e| format!("Failed to count content search results: {e}"))?;

    let ranked_order_by = if matches!(sort_by, Some("recent")) {
        "sort_ts DESC, matched.score ASC"
    } else {
        "matched.score ASC, sort_ts DESC"
    };
    let final_order_by = if matches!(sort_by, Some("recent")) {
        "sort_ts DESC, score ASC"
    } else {
        "score ASC, sort_ts DESC"
    };
    let structured_clause = structured_conditions.join(" AND ");

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
        ),
        snippet_config AS (
          SELECT
            ? AS snippet_probe,
            ? AS snippet_probe_folded,
            ? AS snippet_probe_is_ascii
        ),
        ranked_messages AS (
          SELECT
            matched.message_id,
            matched.score,
            COALESCE(msg.ts, sess.last_active_at, sess.created_at, 0) AS sort_ts,
            src.name AS provider_id,
            sess.provider_session_id AS session_id,
            sess.source_path,
            COALESCE(
              NULLIF(sess.title, ''),
              NULLIF(proj.display_name, ''),
              proj.display_path
            ) AS session_title,
            proj.display_path AS project,
            sess.last_active_at,
            msg.content_text,
            msg.role,
            msg.ts,
            msg.seq,
            length(msg.content_text) AS content_len,
            snippet_config.snippet_probe AS snippet_probe,
            instr(
              CASE
                WHEN snippet_config.snippet_probe_is_ascii = 1 THEN lower(msg.content_text)
                ELSE msg.content_text
              END,
              CASE
                WHEN snippet_config.snippet_probe_is_ascii = 1 THEN snippet_config.snippet_probe_folded
                ELSE snippet_config.snippet_probe
              END
            ) AS snippet_pos
          FROM matched_messages matched
          JOIN messages_fts ON messages_fts.rowid = matched.message_id
          JOIN messages msg ON msg.id = matched.message_id
          JOIN sessions sess ON sess.id = msg.session_id
          JOIN projects proj ON proj.id = sess.project_id
          JOIN sources src ON src.id = sess.source_id
          CROSS JOIN snippet_config
          WHERE messages_fts MATCH ?
            AND {structured_clause}
          ORDER BY {ranked_order_by}
          LIMIT ?
        )
        SELECT
          message_id,
          score,
          sort_ts,
          provider_id,
          session_id,
          source_path,
          session_title,
          project,
          last_active_at,
          {snippet_source_sql} AS snippet_source,
          role,
          ts,
          seq
        FROM ranked_messages
        ORDER BY {final_order_by}
        "#
    );

    let mut params: Vec<rusqlite::types::Value> = vec![
        fts_query.clone().into(),
        i64::try_from(candidate_limit).unwrap_or(i64::MAX).into(),
        snippet_probe.into(),
        snippet_probe_folded.into(),
        snippet_probe_is_ascii.into(),
        fts_query.into(),
    ];
    params.extend(structured_params.iter().cloned());
    params.push(i64::try_from(limit).unwrap_or(i64::MAX).into());

    let mut stmt = connection
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare content search query: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            let snippet_source: String = row.get(9)?;
            Ok(InternalSearchHit {
                message_id: row.get(0)?,
                score: row.get(1)?,
                sort_ts: row.get(2)?,
                hit: SearchFragmentHit {
                    rank: 0,
                    provider_id: row.get(3)?,
                    session_id: row.get(4)?,
                    source_path: row.get(5)?,
                    session_title: row.get(6)?,
                    project: row.get(7)?,
                    last_active_at: row.get(8)?,
                    snippet: tokenizer::build_highlight_snippet(&snippet_source, query),
                    message_role: row.get(10)?,
                    message_timestamp: row.get(11)?,
                    seq: row.get(12)?,
                },
            })
        })
        .map_err(|e| format!("Failed to execute content search query: {e}"))?;

    let hits = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read content search results: {e}"))?;

    Ok(SearchContentResultInternal { total_count, hits })
}

fn search_content_substring(
    connection: &Connection,
    query: &str,
    limit: usize,
    provider_id: Option<&str>,
    since_ts: Option<i64>,
    project_path: Option<&str>,
) -> Result<SearchContentResultInternal, String> {
    let (snippet_probe, snippet_probe_folded, snippet_probe_is_ascii) =
        build_snippet_probe_params(query);
    let snippet_source_sql = build_snippet_source_sql();
    let (mut conditions, mut params): (Vec<String>, Vec<rusqlite::types::Value>) =
        build_structured_conditions(provider_id, since_ts, project_path);
    conditions.push("instr(msg.content_text, ?) > 0".to_string());
    params.push(query.to_string().into());

    let count_sql = format!(
        r#"
        SELECT COUNT(*)
        FROM messages msg
        JOIN sessions sess ON sess.id = msg.session_id
        JOIN projects proj ON proj.id = sess.project_id
        JOIN sources src ON src.id = sess.source_id
        WHERE {}
        "#,
        conditions.join(" AND ")
    );
    let total_count = connection
        .query_row(&count_sql, params_from_iter(params.iter()), |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|e| format!("Failed to count substring search results: {e}"))?;

    let sql = format!(
        r#"
        WITH snippet_config AS (
          SELECT
            ? AS snippet_probe,
            ? AS snippet_probe_folded,
            ? AS snippet_probe_is_ascii
        ),
        ranked_messages AS (
          SELECT
            msg.id AS message_id,
            1000000.0 AS score,
            COALESCE(msg.ts, sess.last_active_at, sess.created_at, 0) AS sort_ts,
            src.name AS provider_id,
            sess.provider_session_id AS session_id,
            sess.source_path,
            COALESCE(
              NULLIF(sess.title, ''),
              NULLIF(proj.display_name, ''),
              proj.display_path
            ) AS session_title,
            proj.display_path AS project,
            sess.last_active_at,
            msg.content_text,
            msg.role,
            msg.ts,
            msg.seq,
            length(msg.content_text) AS content_len,
            snippet_config.snippet_probe AS snippet_probe,
            instr(
              CASE
                WHEN snippet_config.snippet_probe_is_ascii = 1 THEN lower(msg.content_text)
                ELSE msg.content_text
              END,
              CASE
                WHEN snippet_config.snippet_probe_is_ascii = 1 THEN snippet_config.snippet_probe_folded
                ELSE snippet_config.snippet_probe
              END
            ) AS snippet_pos
          FROM messages msg
          JOIN sessions sess ON sess.id = msg.session_id
          JOIN projects proj ON proj.id = sess.project_id
          JOIN sources src ON src.id = sess.source_id
          CROSS JOIN snippet_config
          WHERE {}
          ORDER BY sort_ts DESC, msg.seq DESC
          LIMIT ?
        )
        SELECT
          message_id,
          score,
          sort_ts,
          provider_id,
          session_id,
          source_path,
          session_title,
          project,
          last_active_at,
          {snippet_source_sql} AS snippet_source,
          role,
          ts,
          seq
        FROM ranked_messages
        ORDER BY sort_ts DESC, seq DESC
        "#,
        conditions.join(" AND ")
    );

    let mut list_params: Vec<rusqlite::types::Value> = vec![
        snippet_probe.into(),
        snippet_probe_folded.into(),
        snippet_probe_is_ascii.into(),
    ];
    list_params.extend(params);
    list_params.push(i64::try_from(limit).unwrap_or(i64::MAX).into());

    let mut stmt = connection
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare substring search query: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(list_params.iter()), |row| {
            let snippet_source: String = row.get(9)?;
            Ok(InternalSearchHit {
                message_id: row.get(0)?,
                score: row.get(1)?,
                sort_ts: row.get(2)?,
                hit: SearchFragmentHit {
                    rank: 0,
                    provider_id: row.get(3)?,
                    session_id: row.get(4)?,
                    source_path: row.get(5)?,
                    session_title: row.get(6)?,
                    project: row.get(7)?,
                    last_active_at: row.get(8)?,
                    snippet: tokenizer::build_highlight_snippet(&snippet_source, query),
                    message_role: row.get(10)?,
                    message_timestamp: row.get(11)?,
                    seq: row.get(12)?,
                },
            })
        })
        .map_err(|e| format!("Failed to execute substring search query: {e}"))?;

    let hits = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read substring search results: {e}"))?;

    Ok(SearchContentResultInternal { total_count, hits })
}

fn build_structured_conditions(
    provider_id: Option<&str>,
    since_ts: Option<i64>,
    project_path: Option<&str>,
) -> (Vec<String>, Vec<rusqlite::types::Value>) {
    let mut conditions = vec!["COALESCE(msg.is_sidechain, 0) = 0".to_string()];
    let mut params: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(provider_id) = provider_id.filter(|value| !value.trim().is_empty()) {
        conditions.push("src.name = ?".to_string());
        params.push(provider_id.to_string().into());
    }

    if let Some(since_ts) = since_ts {
        conditions
            .push("COALESCE(msg.ts, sess.last_active_at, sess.created_at, 0) >= ?".to_string());
        params.push(since_ts.into());
    }

    if let Some(project_path) = project_path.filter(|value| !value.trim().is_empty()) {
        conditions.push("proj.display_path = ?".to_string());
        params.push(project_path.to_string().into());
    }

    (conditions, params)
}

fn merge_search_hits(
    fts_hits: Vec<InternalSearchHit>,
    fallback_hits: Vec<InternalSearchHit>,
    limit: usize,
    sort_by: Option<&str>,
) -> Vec<InternalSearchHit> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();

    if matches!(sort_by, Some("recent")) {
        for hit in fts_hits.into_iter().chain(fallback_hits) {
            if seen.insert(hit.message_id) {
                merged.push(hit);
            }
        }
        merged.sort_by(|left, right| {
            right
                .sort_ts
                .cmp(&left.sort_ts)
                .then_with(|| left.score.total_cmp(&right.score))
        });
    } else {
        for hit in fts_hits {
            if seen.insert(hit.message_id) {
                merged.push(hit);
            }
        }
        let mut fallback_only = Vec::new();
        for hit in fallback_hits {
            if seen.insert(hit.message_id) {
                fallback_only.push(hit);
            }
        }
        fallback_only.sort_by(|left, right| {
            right
                .sort_ts
                .cmp(&left.sort_ts)
                .then_with(|| left.score.total_cmp(&right.score))
        });
        merged.extend(fallback_only);
    }

    merged.truncate(limit);
    merged
}

fn finalize_search_result(total_count: i64, hits: Vec<InternalSearchHit>) -> SearchContentResult {
    let hits = hits
        .into_iter()
        .enumerate()
        .map(|(index, mut hit)| {
            hit.hit.rank = i64::try_from(index + 1).unwrap_or(i64::MAX);
            hit.hit
        })
        .collect::<Vec<_>>();
    SearchContentResult { total_count, hits }
}

#[derive(Debug, Clone)]
struct SearchContentResultInternal {
    total_count: i64,
    hits: Vec<InternalSearchHit>,
}
