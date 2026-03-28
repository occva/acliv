use rusqlite::Connection;

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
              content_text TEXT NOT NULL DEFAULT '',
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

            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
              content_text,
              content='messages',
              content_rowid='id',
              tokenize='unicode61 remove_diacritics 1'
            );

            CREATE TRIGGER IF NOT EXISTS messages_fts_insert
            AFTER INSERT ON messages BEGIN
              INSERT INTO messages_fts(rowid, content_text)
                VALUES (NEW.id, NEW.content_text);
            END;

            CREATE TRIGGER IF NOT EXISTS messages_fts_update
            AFTER UPDATE ON messages BEGIN
              INSERT INTO messages_fts(messages_fts, rowid, content_text)
                VALUES ('delete', OLD.id, OLD.content_text);
              INSERT INTO messages_fts(rowid, content_text)
                VALUES (NEW.id, NEW.content_text);
            END;

            CREATE TRIGGER IF NOT EXISTS messages_fts_delete
            AFTER DELETE ON messages BEGIN
              INSERT INTO messages_fts(messages_fts, rowid, content_text)
                VALUES ('delete', OLD.id, OLD.content_text);
            END;

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
        "is_sidechain",
        "ALTER TABLE messages ADD COLUMN is_sidechain INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "messages",
        "tool_names",
        "ALTER TABLE messages ADD COLUMN tool_names TEXT NOT NULL DEFAULT '[]'",
    )?;

    connection
        .execute_batch("PRAGMA optimize;")
        .map_err(|e| format!("Failed to optimize search DB: {e}"))?;

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
