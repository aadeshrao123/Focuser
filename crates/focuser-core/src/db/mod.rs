mod allowance;
mod migrations;
mod pomodoro;

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
        self.update_block_list_at(list, chrono::Local::now())
    }

    pub(crate) fn update_block_list_at<T: chrono::TimeZone>(
        &self,
        list: &BlockList,
        now: chrono::DateTime<T>,
    ) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let old_json: String = tx
            .query_row(
                "SELECT data FROM block_lists WHERE id = ?1",
                [list.id.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    FocuserError::BlockListNotFound(list.id.to_string())
                }
                _ => FocuserError::Database(e.to_string()),
            })?;
        let old: BlockList = serde_json::from_str(&old_json)?;
        crate::shared_allowance::schedule_edited(&tx, &old, list, now)?;
        let json = serde_json::to_string(list)?;
        let rows = tx
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
        tx.commit()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
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

    // ─── Unlock challenges ──────────────────────────────────

    /// The outstanding random-text challenge for a block list, stored as
    /// `fresh` if there is none yet. Asking twice returns the same text, so a
    /// second request cannot swap it out from under the one on screen.
    ///
    /// A list can carry two locks, a manual one and a scheduled one, each with
    /// its own length. A text left over from the other lock is replaced, so the
    /// short one cannot stand in for the long one.
    pub fn issue_unlock_challenge(&self, list_id: EntityId, fresh: &str) -> Result<String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        conn.query_row(
            "INSERT INTO unlock_challenges (block_list_id, challenge) VALUES (?1, ?2)
             ON CONFLICT(block_list_id) DO UPDATE SET challenge = CASE
                 WHEN length(challenge) = length(excluded.challenge) THEN challenge
                 ELSE excluded.challenge END
             RETURNING challenge",
            rusqlite::params![list_id.to_string(), fresh],
            |row| row.get(0),
        )
        .map_err(|e| FocuserError::Database(e.to_string()))
    }

    /// Remove and return the outstanding challenge for a block list, if any.
    ///
    /// Single-use by construction: whether the caller's answer turns out
    /// right or wrong, the challenge is gone afterwards, so a wrong attempt
    /// cannot be retried against the same string and a right one cannot be
    /// replayed.
    pub fn take_unlock_challenge(&self, list_id: EntityId) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        match conn.query_row(
            "DELETE FROM unlock_challenges WHERE block_list_id = ?1 RETURNING challenge",
            rusqlite::params![list_id.to_string()],
            |row| row.get(0),
        ) {
            Ok(challenge) => Ok(Some(challenge)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(FocuserError::Database(e.to_string())),
        }
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

    /// Write a statistics row for a given day. Exists for the dev server's
    /// `--seed`, which needs history that `record_blocked_attempt` cannot make.
    pub fn record_usage_on(
        &self,
        domain_or_app: &str,
        date: &str,
        attempts: i64,
        seconds: i64,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO statistics (domain_or_app, blocked_attempts, duration_seconds, date)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(domain_or_app, date)
             DO UPDATE SET blocked_attempts = ?2, duration_seconds = ?3",
            rusqlite::params![domain_or_app, attempts, seconds, date],
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

    /// Delete statistics older than `keep_days` days. Returns the number
    /// of rows deleted.
    pub fn cleanup_old_statistics(&self, keep_days: u32) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let cutoff = (chrono::Utc::now().date_naive() - chrono::Duration::days(keep_days as i64))
            .to_string();
        let stats_deleted = conn
            .execute(
                "DELETE FROM statistics WHERE date < ?1",
                rusqlite::params![cutoff],
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        // Also cleanup blocked events (timestamped, not date-based)
        let cutoff_ts =
            (chrono::Utc::now() - chrono::Duration::days(keep_days as i64)).to_rfc3339();
        let _ = conn.execute(
            "DELETE FROM blocked_events WHERE timestamp < ?1",
            rusqlite::params![cutoff_ts],
        );
        Ok(stats_deleted as u64)
    }

    /// Clear all settings (resets preferences to defaults).
    pub fn clear_settings(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        conn.execute("DELETE FROM settings", [])
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    /// Clear all statistics and blocked events.
    pub fn clear_all_statistics(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        conn.execute("DELETE FROM statistics", [])
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        conn.execute("DELETE FROM blocked_events", [])
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    /// Delete EVERYTHING: block lists, rules, schedules, exceptions,
    /// statistics, blocked events, settings. Full reset.
    pub fn delete_all_data(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        for table in [
            "statistics",
            "blocked_events",
            "active_blocks",
            "block_lists",
            "settings",
            "unlock_challenges",
            "shared_allowance_usage",
            "shared_allowance_occurrences",
        ] {
            let _ = conn.execute(&format!("DELETE FROM {table}"), []);
        }
        Ok(())
    }

    /// Get blocked attempt count for a specific domain/app today.
    pub fn get_blocked_count_today(&self, domain_or_app: &str) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let today = chrono::Utc::now().date_naive().to_string();
        let count: u64 = conn
            .query_row(
                "SELECT COALESCE(blocked_attempts, 0) FROM statistics
                 WHERE domain_or_app = ?1 AND date = ?2",
                rusqlite::params![domain_or_app, today],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focuser_common::types::BlockList;

    #[test]
    fn scheduled_protection_and_bypass_survive_reopening_database() {
        use chrono::Datelike;
        use focuser_common::types::{Schedule, ScheduledProtection, TimeSlot, new_id};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scheduled.db");
        let mut list = BlockList::new("Scheduled");
        list.schedule = Some(Schedule {
            id: new_id(),
            name: "Today".into(),
            enabled: true,
            time_slots: vec![TimeSlot::new(
                chrono::Local::now().weekday(),
                chrono::NaiveTime::MIN,
                chrono::NaiveTime::MIN,
            )],
        });
        list.scheduled_protection = Some(ScheduledProtection { lock: None });
        {
            let db = Database::open(&path).unwrap();
            db.create_block_list(&list).unwrap();
        }
        {
            let engine = crate::BlockEngine::new(Database::open(&path).unwrap()).unwrap();
            assert!(engine.is_block_list_protected(list.id));
            assert!(engine.has_service_protection());
            assert_eq!(engine.active_protection_info().len(), 1);
            list.schedule_unlocked_until = list.effective_protection().unwrap().expires_at;
            engine.db().update_block_list(&list).unwrap();
        }
        {
            let engine = crate::BlockEngine::new(Database::open(&path).unwrap()).unwrap();
            assert!(!engine.is_block_list_protected(list.id));
            assert!(!engine.has_service_protection());
            assert!(engine.active_protection_info().is_empty());
        }
    }

    #[test]
    fn existing_database_records_default_to_no_scheduled_protection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let list = BlockList::new("Legacy");
        {
            let db = Database::open(&path).unwrap();
            db.create_block_list(&list).unwrap();
            let mut legacy = serde_json::to_value(&list).unwrap();
            legacy
                .as_object_mut()
                .unwrap()
                .remove("scheduled_protection");
            legacy
                .as_object_mut()
                .unwrap()
                .remove("schedule_unlocked_until");
            db.conn
                .lock()
                .unwrap()
                .execute(
                    "UPDATE block_lists SET data = ?1 WHERE id = ?2",
                    rusqlite::params![legacy.to_string(), list.id.to_string()],
                )
                .unwrap();
        }
        let db = Database::open(&path).unwrap();
        let loaded = db.get_block_list(list.id).unwrap();
        assert!(loaded.scheduled_protection.is_none());
        assert!(loaded.schedule_unlocked_until.is_none());
        assert!(!loaded.is_modification_protected());
        db.update_block_list(&loaded).unwrap();
        assert_eq!(db.get_block_list(list.id).unwrap().name, "Legacy");
    }

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

    #[test]
    fn unlock_challenge_is_single_use_and_per_list() {
        let db = Database::open_in_memory().unwrap();
        let a = focuser_common::types::new_id();
        let b = focuser_common::types::new_id();

        assert_eq!(db.take_unlock_challenge(a).unwrap(), None);

        db.issue_unlock_challenge(a, "abc123").unwrap();
        db.issue_unlock_challenge(b, "xyz789").unwrap();

        // Taking it once returns the value...
        assert_eq!(db.take_unlock_challenge(a).unwrap(), Some("abc123".into()));
        // ...and a second take finds nothing, whether the first answer was
        // right or wrong — this is what makes a challenge single-use.
        assert_eq!(db.take_unlock_challenge(a).unwrap(), None);

        // A separate list's challenge is unaffected.
        assert_eq!(db.take_unlock_challenge(b).unwrap(), Some("xyz789".into()));
    }

    #[test]
    fn asking_again_returns_the_challenge_already_out() {
        // The screen asked twice (React runs effects twice in development)
        // and showed the first answer while the second replaced it here.
        let db = Database::open_in_memory().unwrap();
        let id = focuser_common::types::new_id();

        assert_eq!(db.issue_unlock_challenge(id, "first").unwrap(), "first");
        assert_eq!(db.issue_unlock_challenge(id, "again").unwrap(), "first");
        assert_eq!(db.take_unlock_challenge(id).unwrap(), Some("first".into()));

        // Once answered, the next one is fresh.
        assert_eq!(db.issue_unlock_challenge(id, "third").unwrap(), "third");

        // Another length means the list's other lock is asking. The short text
        // left over must not be the answer to the long one.
        let longer = "a much longer text";
        assert_eq!(db.issue_unlock_challenge(id, longer).unwrap(), longer);
    }

    #[test]
    fn unlock_challenge_survives_a_fresh_connection_to_the_same_file() {
        // `protect unlock` runs in its own process, so the challenge the app
        // issued must live in the file, not in any in-process state.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("focuser.db");
        let id = focuser_common::types::new_id();

        {
            let db = Database::open(&path).unwrap();
            db.issue_unlock_challenge(id, "persisted").unwrap();
        }
        {
            let db = Database::open(&path).unwrap();
            assert_eq!(
                db.take_unlock_challenge(id).unwrap(),
                Some("persisted".into())
            );
        }
    }
}
