//! Read-only examination for explicit, cold administrator recovery. This never
//! starts an authority, grants a maintenance permit, or deletes/updates a DB.
use focuser_common::types::BlockList;
use focuser_ipc::IpcError;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct Inspection {
    pub installation: String,
    pub database_sha256: String,
    pub lists: usize,
    pub decision: focuser_ipc::RemovalDecision,
    pub browser_cleanup_required: bool,
}
fn fail(error: impl std::fmt::Display) -> IpcError {
    IpcError::new("recovery_unverified", error.to_string())
}
pub fn inspect(path: &Path) -> Result<Inspection, IpcError> {
    for suffix in ["-wal", "-shm", "-journal"] {
        if path
            .with_file_name(format!(
                "{}{suffix}",
                path.file_name()
                    .ok_or_else(|| fail("Missing filename"))?
                    .to_string_lossy()
            ))
            .try_exists()
            .map_err(fail)?
        {
            return Err(fail(
                "SQLite sidecars require a verified authority restore/checkpoint before cold recovery; no files were removed",
            ));
        }
    }
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(fail)?;
    db.busy_timeout(std::time::Duration::from_secs(3))
        .map_err(fail)?;
    db.execute_batch("PRAGMA query_only=ON; BEGIN;")
        .map_err(fail)?;
    let integrity: String = db
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(fail)?;
    if integrity != "ok" {
        return Err(fail("Database integrity failed"));
    }
    let versions = db
        .prepare("SELECT version FROM schema_version ORDER BY version")
        .map_err(fail)?
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(fail)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(fail)?;
    if versions != (1..=8).collect::<Vec<_>>() {
        return Err(fail("Unsupported schema; use manual recovery"));
    }
    let installation: String = db
        .query_row(
            "SELECT value FROM settings WHERE key='_authority.installation'",
            [],
            |r| r.get(0),
        )
        .map_err(fail)?;
    uuid::Uuid::parse_str(&installation).map_err(fail)?;
    let mut lists = Vec::new();
    let mut stmt = db.prepare("SELECT data FROM block_lists").map_err(fail)?;
    for json in stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(fail)?
    {
        let json = json.map_err(fail)?;
        if lists.len() >= 5000 || json.len() > focuser_ipc::MAX_MESSAGE_BYTES {
            return Err(fail("Policy inventory exceeds recovery limits"));
        }
        lists.push(serde_json::from_str::<BlockList>(&json).map_err(fail)?);
    }
    // A missing/stopped runtime has dormant rules, not a running engine. Still
    // honor every currently stored manual/scheduled commitment through breaks.
    let now = chrono::Local::now();
    let commitments: Vec<_> = lists
        .iter()
        .filter(|l| {
            l.manual_protection_at(now.with_timezone(&chrono::Utc))
                .is_some()
                || l.scheduled_protection_at(now).is_some()
        })
        .cloned()
        .collect();
    let decision = crate::removal::complete_decision_at(&commitments, now);
    if !decision.allowed {
        return Err(IpcError::new(
            "removal_protected",
            crate::removal::explanation(&decision),
        ));
    }
    let sessions: i64 = db
        .query_row(
            "SELECT count(*) FROM pomodoro_sessions WHERE ended_at IS NULL",
            [],
            |r| r.get(0),
        )
        .map_err(fail)?;
    if sessions != 0 {
        return Err(IpcError::new(
            "removal_protected",
            "An ongoing Pomodoro session remains, including any pause/break. Restore the authority and end it legitimately before recovery.",
        ));
    }
    let coverage: Option<String> = {
        use rusqlite::OptionalExtension;
        db.query_row(
            "SELECT value FROM settings WHERE key='_authority.browser_cleanup_required'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(fail)?
    };
    Ok(Inspection {
        installation,
        database_sha256: format!("{:X}", Sha256::digest(std::fs::read(path).map_err(fail)?)),
        lists: lists.len(),
        decision,
        browser_cleanup_required: coverage.as_deref() != Some("false"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority.db");
        let db = focuser_core::Database::open(&path).unwrap();
        db.set_setting("_authority.installation", &uuid::Uuid::new_v4().to_string())
            .unwrap();
        db.set_setting("_authority.browser_cleanup_required", "false")
            .unwrap();
        drop(db);
        (dir, path)
    }
    #[test]
    fn cold_idle_history_and_expired_commitments_are_read_only_and_recoverable() {
        let (_dir, path) = fixture();
        let db = focuser_core::Database::open(&path).unwrap();
        let mut list = BlockList::new("Old rules");
        list.websites
            .push(focuser_common::types::WebsiteRule::domain("example.com"));
        list.protection = Some(focuser_common::types::Protection::for_duration(1));
        list.protection.as_mut().unwrap().expires_at =
            Some(chrono::Utc::now() - chrono::Duration::hours(1));
        db.create_block_list(&list).unwrap();
        drop(db);
        let original = std::fs::read(&path).unwrap();
        assert_eq!(inspect(&path).unwrap().lists, 1);
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    #[test]
    fn cold_recovery_never_releases_an_active_disabled_commitment() {
        let (_dir, path) = fixture();
        let db = focuser_core::Database::open(&path).unwrap();
        let mut list = BlockList::new("Break");
        list.enabled = false;
        list.protection = Some(focuser_common::types::Protection::for_duration(60));
        db.create_block_list(&list).unwrap();
        drop(db);
        assert_eq!(inspect(&path).unwrap_err().code, "removal_protected");
    }
    #[test]
    fn unknown_state_corruption_and_journals_are_never_permission() {
        let (_dir, path) = fixture();
        std::fs::write(path.with_extension("db-wal"), b"preserve").unwrap();
        assert!(inspect(&path).is_err());
        std::fs::remove_file(path.with_extension("db-wal")).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "UPDATE settings SET value='true' WHERE key='_authority.browser_cleanup_required'",
            [],
        )
        .unwrap();
        drop(db);
        assert!(inspect(&path).unwrap().browser_cleanup_required);
        std::fs::write(&path, b"bad").unwrap();
        assert!(inspect(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(inspect(&path).is_err());
        assert!(!path.exists());
    }
    #[test]
    fn paused_pomodoro_survives_missing_runtime_until_legitimately_ended() {
        let (_dir, path) = fixture();
        let db = focuser_core::Database::open(&path).unwrap();
        let list = BlockList::new("Pomodoro break");
        db.create_block_list(&list).unwrap();
        let mut session = focuser_common::pomodoro::PomodoroSession::new(
            list.id,
            focuser_common::pomodoro::PomodoroConfig::CLASSIC,
            false,
        );
        session.paused_remaining_secs = Some(30);
        db.create_pomodoro_session(&session).unwrap();
        drop(db);
        assert_eq!(inspect(&path).unwrap_err().code, "removal_protected");
        let db = focuser_core::Database::open(&path).unwrap();
        db.end_pomodoro_session(session.id, chrono::Utc::now())
            .unwrap();
        drop(db);
        assert!(inspect(&path).unwrap().decision.allowed);
    }
    #[test]
    fn malformed_policy_rows_and_future_schema_do_not_disappear_from_inspection() {
        let (_dir, path) = fixture();
        let db = focuser_core::Database::open(&path).unwrap();
        db.create_block_list(&BlockList::new("Keep")).unwrap();
        drop(db);
        let db = Connection::open(&path).unwrap();
        db.execute("UPDATE block_lists SET data='{'", []).unwrap();
        drop(db);
        assert!(inspect(&path).is_err());
        let db = Connection::open(&path).unwrap();
        db.execute("DELETE FROM block_lists", []).unwrap();
        db.execute("INSERT INTO schema_version(version) VALUES (9)", [])
            .unwrap();
        drop(db);
        assert!(inspect(&path).is_err());
    }
}
