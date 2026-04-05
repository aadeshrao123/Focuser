mod migrations;

use focuser_common::error::{FocuserError, Result};
use focuser_common::types::{BlockList, BlockedEvent, EntityId, UsageStat};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;
use tracing::info;

/// Main database handle. Thread-safe via internal Mutex.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (or create) the database at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| FocuserError::Database(e.to_string()))?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        info!("Database initialized");
        Ok(db)
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn =
            Connection::open_in_memory().map_err(|e| FocuserError::Database(e.to_string()))?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        migrations::run_all(&conn)
    }

    // ─── Block List CRUD ────────────────────────────────────

    pub fn create_block_list(&self, list: &BlockList) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let json = serde_json::to_string(list)?;
        conn.execute(
            "INSERT INTO block_lists (id, name, data, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                list.id.to_string(),
                list.name,
                json,
                list.enabled,
                list.created_at.to_rfc3339(),
                list.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn update_block_list(&self, list: &BlockList) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let json = serde_json::to_string(list)?;
        let rows = conn
            .execute(
                "UPDATE block_lists SET name = ?1, data = ?2, enabled = ?3, updated_at = ?4
                 WHERE id = ?5",
                rusqlite::params![
                    list.name,
                    json,
                    list.enabled,
                    list.updated_at.to_rfc3339(),
                    list.id.to_string(),
                ],
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        if rows == 0 {
            return Err(FocuserError::BlockListNotFound(list.id.to_string()));
        }
        Ok(())
    }

    pub fn delete_block_list(&self, id: EntityId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let rows = conn
            .execute(
                "DELETE FROM block_lists WHERE id = ?1",
                rusqlite::params![id.to_string()],
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        if rows == 0 {
            return Err(FocuserError::BlockListNotFound(id.to_string()));
        }
        Ok(())
    }

    pub fn get_block_list(&self, id: EntityId) -> Result<BlockList> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let json: String = conn
            .query_row(
                "SELECT data FROM block_lists WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    FocuserError::BlockListNotFound(id.to_string())
                }
                _ => FocuserError::Database(e.to_string()),
            })?;
        let list: BlockList = serde_json::from_str(&json)?;
        Ok(list)
    }

    pub fn list_block_lists(&self) -> Result<Vec<BlockList>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT data FROM block_lists ORDER BY created_at")
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let lists = stmt
            .query_map([], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .map_err(|e| FocuserError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str::<BlockList>(&json).ok())
            .collect();
        Ok(lists)
    }

    // ─── Settings ───────────────────────────────────────────

    /// Get a setting value by key.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        match conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(FocuserError::Database(e.to_string())),
        }
    }

    /// Set a setting value (upsert).
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![key, value],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    /// Get a setting value, returning a default if not set.
    pub fn get_setting_or_default(&self, key: &str, default: &str) -> Result<String> {
        Ok(self
            .get_setting(key)?
            .unwrap_or_else(|| default.to_string()))
    }

    // ─── Statistics ─────────────────────────────────────────

    pub fn record_blocked_attempt(&self, domain_or_app: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let today = chrono::Utc::now().date_naive().to_string();
        conn.execute(
            "INSERT INTO statistics (domain_or_app, blocked_attempts, date)
             VALUES (?1, 1, ?2)
             ON CONFLICT(domain_or_app, date)
             DO UPDATE SET blocked_attempts = blocked_attempts + 1",
            rusqlite::params![domain_or_app, today],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    // ─── Blocked Events (fine-grained timeline) ────────────

    /// Record an individual block event with a precise timestamp.
    pub fn record_blocked_event(&self, domain_or_app: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO blocked_events (domain_or_app, timestamp) VALUES (?1, ?2)",
            rusqlite::params![domain_or_app, now],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    /// Get blocked events in a time range (ISO 8601 timestamps).
    pub fn get_blocked_events(&self, from: &str, to: &str) -> Result<Vec<BlockedEvent>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT domain_or_app, timestamp FROM blocked_events
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let events = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok(BlockedEvent {
                    domain_or_app: row.get(0)?,
                    timestamp: row.get(1)?,
                })
            })
            .map_err(|e| FocuserError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(events)
    }

    /// Delete blocked events older than the given number of days.
    pub fn cleanup_old_events(&self, keep_days: u32) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(keep_days as i64)).to_rfc3339();
        let deleted = conn
            .execute(
                "DELETE FROM blocked_events WHERE timestamp < ?1",
                rusqlite::params![cutoff],
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(deleted as u64)
    }

    pub fn get_stats(
        &self,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> Result<Vec<UsageStat>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT domain_or_app, duration_seconds, blocked_attempts, date
                 FROM statistics
                 WHERE date >= ?1 AND date <= ?2
                 ORDER BY blocked_attempts DESC",
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let stats = stmt
            .query_map(rusqlite::params![from.to_string(), to.to_string()], |row| {
                Ok(UsageStat {
                    domain_or_app: row.get(0)?,
                    duration_seconds: row.get(1)?,
                    blocked_attempts: row.get(2)?,
                    date: row
                        .get::<_, String>(3)?
                        .parse()
                        .unwrap_or(chrono::NaiveDate::default()),
                })
            })
            .map_err(|e| FocuserError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(stats)
    }

    pub fn get_total_blocked_today(&self) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let today = chrono::Utc::now().date_naive().to_string();
        let count: u64 = conn
            .query_row(
                "SELECT COALESCE(SUM(blocked_attempts), 0) FROM statistics WHERE date = ?1",
                rusqlite::params![today],
                |row| row.get(0),
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focuser_common::types::BlockList;

    #[test]
    fn test_crud_block_list() {
        let db = Database::open_in_memory().unwrap();
        let list = BlockList::new("Social Media");

        db.create_block_list(&list).unwrap();
        let fetched = db.get_block_list(list.id).unwrap();
        assert_eq!(fetched.name, "Social Media");

        let all = db.list_block_lists().unwrap();
        assert_eq!(all.len(), 1);

        db.delete_block_list(list.id).unwrap();
        let all = db.list_block_lists().unwrap();
        assert_eq!(all.len(), 0);
    }

    #[test]
    fn test_settings() {
        let db = Database::open_in_memory().unwrap();

        // Not set → None
        assert_eq!(db.get_setting("missing").unwrap(), None);

        // Default fallback
        assert_eq!(db.get_setting_or_default("missing", "42").unwrap(), "42");

        // Set and get
        db.set_setting("grace_period", "60").unwrap();
        assert_eq!(db.get_setting("grace_period").unwrap(), Some("60".into()));

        // Upsert
        db.set_setting("grace_period", "120").unwrap();
        assert_eq!(db.get_setting("grace_period").unwrap(), Some("120".into()));
    }

    #[test]
    fn test_statistics() {
        let db = Database::open_in_memory().unwrap();
        db.record_blocked_attempt("reddit.com").unwrap();
        db.record_blocked_attempt("reddit.com").unwrap();
        db.record_blocked_attempt("twitter.com").unwrap();

        let total = db.get_total_blocked_today().unwrap();
        assert_eq!(total, 3);
    }
}
