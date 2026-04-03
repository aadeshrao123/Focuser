use focuser_common::error::{FocuserError, Result};
use rusqlite::Connection;
use tracing::info;

/// Run all database migrations.
pub fn run_all(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        );",
    )
    .map_err(|e| FocuserError::Database(e.to_string()))?;

    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let migrations: &[(&str, &str)] = &[
        (
            "v1: block_lists and statistics",
            "CREATE TABLE IF NOT EXISTS block_lists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                data TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS statistics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                domain_or_app TEXT NOT NULL,
                duration_seconds INTEGER NOT NULL DEFAULT 0,
                blocked_attempts INTEGER NOT NULL DEFAULT 0,
                date TEXT NOT NULL,
                UNIQUE(domain_or_app, date)
            );

            CREATE TABLE IF NOT EXISTS active_blocks (
                block_list_id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL,
                expires_at TEXT,
                lock_type TEXT,
                lock_data TEXT
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        ),
    ];

    for (i, (name, sql)) in migrations.iter().enumerate() {
        let version = (i + 1) as i64;
        if version > current_version {
            info!("Running migration {version}: {name}");
            conn.execute_batch(sql)
                .map_err(|e| FocuserError::Database(format!("Migration {version} failed: {e}")))?;
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                rusqlite::params![version],
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        }
    }

    Ok(())
}
