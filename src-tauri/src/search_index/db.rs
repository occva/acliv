use std::fs;

use rusqlite::Connection;

pub fn open_connection() -> Result<Connection, String> {
    let db_path = crate::paths::get_search_db_path();
    let parent = db_path
        .parent()
        .ok_or_else(|| format!("Invalid search DB path: {}", db_path.display()))?;

    fs::create_dir_all(parent).map_err(|e| {
        format!(
            "Failed to create search index dir {}: {e}",
            parent.display()
        )
    })?;

    let connection = Connection::open(&db_path)
        .map_err(|e| format!("Failed to open search DB {}: {e}", db_path.display()))?;

    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("Failed to enable WAL mode: {e}"))?;
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| format!("Failed to set synchronous=NORMAL: {e}"))?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| format!("Failed to enable foreign_keys: {e}"))?;
    connection
        .pragma_update(None, "busy_timeout", 5_000)
        .map_err(|e| format!("Failed to set busy_timeout: {e}"))?;
    connection
        .pragma_update(None, "temp_store", "MEMORY")
        .map_err(|e| format!("Failed to set temp_store=MEMORY: {e}"))?;
    connection
        .pragma_update(None, "cache_size", -20_000)
        .map_err(|e| format!("Failed to set cache_size: {e}"))?;

    Ok(connection)
}
