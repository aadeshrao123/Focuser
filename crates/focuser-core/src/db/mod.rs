mod migrations;

use focuser_common::error::{FocuserError, Result};
use focuser_common::types::{BlockList, EntityId, UsageStat};
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
    fn test_statistics() {
        let db = Database::open_in_memory().unwrap();
        db.record_blocked_attempt("reddit.com").unwrap();
        db.record_blocked_attempt("reddit.com").unwrap();
        db.record_blocked_attempt("twitter.com").unwrap();

        let total = db.get_total_blocked_today().unwrap();
        assert_eq!(total, 3);
    }
}
