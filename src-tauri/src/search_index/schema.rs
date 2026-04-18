use rusqlite::Connection;

use super::tokenizer;

const SEARCH_SCHEMA_VERSION: i32 = 2;

pub fn run_migrations(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sources (
              id         INTEGER PRIMARY KEY,
              name       TEXT NOT NULL UNIQUE,
              base_path  TEXT NOT NULL,
              created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS projects (
              id           INTEGER PRIMARY KEY,
              source_id    INTEGER NOT NULL REFERENCES sources(id),
              slug         TEXT NOT NULL,
              display_path TEXT NOT NULL,
              display_name TEXT NOT NULL,
              last_synced  TEXT,
              UNIQUE(source_id, slug)
            );

            CREATE TABLE IF NOT EXISTS sessions (
              id                  INTEGER PRIMARY KEY,
              project_id          INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
              source_id           INTEGER NOT NULL REFERENCES sources(id),
              provider_session_id TEXT NOT NULL,
              source_path         TEXT NOT NULL,
              title               TEXT,
              summary             TEXT,
              cwd                 TEXT,
              model               TEXT,
              created_at          INTEGER,
              last_active_at      INTEGER,
              message_count       INTEGER NOT NULL DEFAULT 0,
              has_tool_use        INTEGER NOT NULL DEFAULT 0,
              resume_command      TEXT,
              raw_mtime           INTEGER,
              raw_size            INTEGER,
              indexed_at          TEXT NOT NULL DEFAULT (datetime('now')),
              UNIQUE(source_id, source_path)
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_project      ON sessions(project_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_source       ON sessions(source_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_last_active  ON sessions(last_active_at DESC);
            CREATE INDEX IF NOT EXISTS idx_sessions_source_last_active ON sessions(source_id, last_active_at DESC);

            CREATE TABLE IF NOT EXISTS messages (
              id           INTEGER PRIMARY KEY,
              session_id   INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
              source_id    INTEGER NOT NULL REFERENCES sources(id),
              msg_uuid     TEXT,
              parent_uuid  TEXT,
              role         TEXT NOT NULL,
              kind         TEXT NOT NULL DEFAULT 'message',
              name         TEXT,
              call_id      TEXT,
              content_text TEXT NOT NULL DEFAULT '',
              search_text  TEXT NOT NULL DEFAULT '',
              ts           INTEGER,
              is_sidechain INTEGER NOT NULL DEFAULT 0,
              tool_names   TEXT NOT NULL DEFAULT '[]',
              seq          INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session      ON messages(session_id, seq);
            CREATE INDEX IF NOT EXISTS idx_messages_timestamp    ON messages(ts);
            CREATE INDEX IF NOT EXISTS idx_messages_session_sidechain_seq
              ON messages(session_id, is_sidechain, seq);

            CREATE INDEX IF NOT EXISTS idx_projects_source_display_path
              ON projects(source_id, display_path);

            CREATE TABLE IF NOT EXISTS sync_log (
              id          INTEGER PRIMARY KEY,
              source_id   INTEGER NOT NULL REFERENCES sources(id),
              source_path TEXT NOT NULL,
              status      TEXT NOT NULL,
              message     TEXT,
              synced_at   TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_sync_log_synced_at ON sync_log(synced_at DESC);
            "#,
        )
        .map_err(|e| format!("Failed to run search DB migrations: {e}"))?;

    connection
        .execute(
            r#"
            UPDATE sessions
            SET last_active_at = created_at
            WHERE last_active_at IS NULL
              AND created_at IS NOT NULL
            "#,
            [],
        )
        .map_err(|e| format!("Failed to normalize sessions.last_active_at: {e}"))?;

    ensure_column(
        connection,
        "sessions",
        "cwd",
        "ALTER TABLE sessions ADD COLUMN cwd TEXT",
    )?;
    ensure_column(
        connection,
        "sessions",
        "model",
        "ALTER TABLE sessions ADD COLUMN model TEXT",
    )?;
    ensure_column(
        connection,
        "sessions",
        "has_tool_use",
        "ALTER TABLE sessions ADD COLUMN has_tool_use INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "sessions",
        "resume_command",
        "ALTER TABLE sessions ADD COLUMN resume_command TEXT",
    )?;
    ensure_column(
        connection,
        "messages",
        "msg_uuid",
        "ALTER TABLE messages ADD COLUMN msg_uuid TEXT",
    )?;
    ensure_column(
        connection,
        "messages",
        "parent_uuid",
        "ALTER TABLE messages ADD COLUMN parent_uuid TEXT",
    )?;
    ensure_column(
        connection,
        "messages",
        "kind",
        "ALTER TABLE messages ADD COLUMN kind TEXT NOT NULL DEFAULT 'message'",
    )?;
    ensure_column(
        connection,
        "messages",
        "name",
        "ALTER TABLE messages ADD COLUMN name TEXT",
    )?;
    ensure_column(
        connection,
        "messages",
        "call_id",
        "ALTER TABLE messages ADD COLUMN call_id TEXT",
    )?;
    ensure_column(
        connection,
        "messages",
        "search_text",
        "ALTER TABLE messages ADD COLUMN search_text TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "messages",
        "is_sidechain",
        "ALTER TABLE messages ADD COLUMN is_sidechain INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "messages",
        "tool_names",
        "ALTER TABLE messages ADD COLUMN tool_names TEXT NOT NULL DEFAULT '[]'",
    )?;

    backfill_missing_search_text(connection)?;

    if needs_search_text_rewrite(connection)? {
        rewrite_all_search_text(connection)?;
    }

    if needs_search_fts_rebuild(connection)? {
        recreate_search_fts(connection)?;
        set_user_version(connection, SEARCH_SCHEMA_VERSION)?;
    }

    connection
        .execute_batch("PRAGMA optimize;")
        .map_err(|e| format!("Failed to optimize search DB: {e}"))?;

    Ok(())
}

fn needs_search_fts_rebuild(connection: &Connection) -> Result<bool, String> {
    let user_version = get_user_version(connection)?;
    if user_version < SEARCH_SCHEMA_VERSION {
        return Ok(true);
    }

    table_exists(connection, "messages_fts")
        .map(|exists| !exists)
        .map_err(|e| format!("Failed to inspect messages_fts existence: {e}"))
}

fn needs_search_text_rewrite(connection: &Connection) -> Result<bool, String> {
    get_user_version(connection).map(|version| version < SEARCH_SCHEMA_VERSION)
}

fn get_user_version(connection: &Connection) -> Result<i32, String> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
        .map_err(|e| format!("Failed to read PRAGMA user_version: {e}"))
}

fn set_user_version(connection: &Connection, version: i32) -> Result<(), String> {
    connection
        .execute_batch(&format!("PRAGMA user_version = {version};"))
        .map_err(|e| format!("Failed to update PRAGMA user_version: {e}"))
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, rusqlite::Error> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ? LIMIT 1",
            [table],
            |_| Ok(()),
        )
        .map(|_| true)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(false),
            other => Err(other),
        })
}

fn recreate_search_fts(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            r#"
            DROP TRIGGER IF EXISTS messages_fts_insert;
            DROP TRIGGER IF EXISTS messages_fts_update;
            DROP TRIGGER IF EXISTS messages_fts_delete;
            DROP TABLE IF EXISTS messages_fts;

            CREATE VIRTUAL TABLE messages_fts USING fts5(
              search_text,
              content='messages',
              content_rowid='id',
              tokenize='unicode61 remove_diacritics 1'
            );

            CREATE TRIGGER messages_fts_insert
            AFTER INSERT ON messages BEGIN
              INSERT INTO messages_fts(rowid, search_text)
                VALUES (NEW.id, NEW.search_text);
            END;

            CREATE TRIGGER messages_fts_update
            AFTER UPDATE ON messages BEGIN
              INSERT INTO messages_fts(messages_fts, rowid, search_text)
                VALUES ('delete', OLD.id, OLD.search_text);
              INSERT INTO messages_fts(rowid, search_text)
                VALUES (NEW.id, NEW.search_text);
            END;

            CREATE TRIGGER messages_fts_delete
            AFTER DELETE ON messages BEGIN
              INSERT INTO messages_fts(messages_fts, rowid, search_text)
                VALUES ('delete', OLD.id, OLD.search_text);
            END;

            INSERT INTO messages_fts(rowid, search_text)
            SELECT id, search_text FROM messages;
            "#,
        )
        .map_err(|e| format!("Failed to recreate messages FTS: {e}"))?;
    Ok(())
}

fn backfill_missing_search_text(connection: &Connection) -> Result<(), String> {
    let mut stmt = connection
        .prepare(
            r#"
            SELECT id, content_text
            FROM messages
            WHERE COALESCE(search_text, '') = ''
              AND COALESCE(kind, 'message') = 'message'
            "#,
        )
        .map_err(|e| format!("Failed to prepare missing search_text query: {e}"))?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| format!("Failed to query missing search_text rows: {e}"))?;

    let pending = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read missing search_text rows: {e}"))?;

    for (id, content_text) in pending {
        let search_text = tokenizer::normalize_search_text(&content_text);
        connection
            .execute(
                "UPDATE messages SET search_text = ? WHERE id = ?",
                rusqlite::params![search_text, id],
            )
            .map_err(|e| format!("Failed to backfill messages.search_text for row {id}: {e}"))?;
    }

    Ok(())
}

fn rewrite_all_search_text(connection: &Connection) -> Result<(), String> {
    let mut stmt = connection
        .prepare("SELECT id, kind, content_text FROM messages")
        .map_err(|e| format!("Failed to prepare search_text rewrite query: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("Failed to query messages for search_text rewrite: {e}"))?;

    let messages = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read messages for search_text rewrite: {e}"))?;

    for (id, kind, content_text) in messages {
        let search_text = if kind == "message" {
            tokenizer::normalize_search_text(&content_text)
        } else {
            String::new()
        };
        connection
            .execute(
                "UPDATE messages SET search_text = ? WHERE id = ?",
                rusqlite::params![search_text, id],
            )
            .map_err(|e| format!("Failed to rewrite messages.search_text for row {id}: {e}"))?;
    }

    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<(), String> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = connection
        .prepare(&pragma)
        .map_err(|e| format!("Failed to inspect table {table}: {e}"))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| format!("Failed to query table info for {table}: {e}"))?;

    while let Some(row) = rows
        .next()
        .map_err(|e| format!("Failed to read table info for {table}: {e}"))?
    {
        let current: String = row
            .get(1)
            .map_err(|e| format!("Failed to read column name for {table}: {e}"))?;
        if current == column {
            return Ok(());
        }
    }

    connection
        .execute(alter_sql, [])
        .map_err(|e| format!("Failed to alter table {table} add {column}: {e}"))?;
    Ok(())
}
