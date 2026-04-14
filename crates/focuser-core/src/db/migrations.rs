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
        (
            "v2: blocked_events for fine-grained timeline",
            "CREATE TABLE IF NOT EXISTS blocked_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                domain_or_app TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_blocked_events_timestamp
                ON blocked_events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_blocked_events_domain
                ON blocked_events(domain_or_app);",
        ),
        (
            "v3: pomodoro sessions and phase log",
            "CREATE TABLE IF NOT EXISTS pomodoro_sessions (
                id TEXT PRIMARY KEY,
                block_list_id TEXT NOT NULL,
                work_secs INTEGER NOT NULL,
                short_break_secs INTEGER NOT NULL,
                long_break_secs INTEGER NOT NULL,
                cycles_until_long_break INTEGER NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                completed_cycles INTEGER NOT NULL DEFAULT 0,
                current_phase TEXT NOT NULL,
                current_cycle INTEGER NOT NULL DEFAULT 1,
                phase_started_at TEXT NOT NULL,
                paused_remaining_secs INTEGER,
                prev_enabled INTEGER NOT NULL DEFAULT 1
            );

            CREATE INDEX IF NOT EXISTS idx_pomodoro_active
                ON pomodoro_sessions(ended_at);

            CREATE TABLE IF NOT EXISTS pomodoro_phases (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                phase_type TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                cycle_number INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_pomodoro_phases_session
                ON pomodoro_phases(session_id);",
        ),
        (
            "v4: allowances and daily usage",
            "CREATE TABLE IF NOT EXISTS allowances (
                id TEXT PRIMARY KEY,
                match_type TEXT NOT NULL,
                match_value TEXT NOT NULL,
                daily_limit_secs INTEGER NOT NULL,
                strict_mode INTEGER NOT NULL DEFAULT 1,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_allowances_value
                ON allowances(match_value);

            CREATE TABLE IF NOT EXISTS allowance_usage (
                allowance_id TEXT NOT NULL,
                usage_date TEXT NOT NULL,
                used_secs INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (allowance_id, usage_date)
            );

            CREATE INDEX IF NOT EXISTS idx_allowance_usage_date
                ON allowance_usage(usage_date);",
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
