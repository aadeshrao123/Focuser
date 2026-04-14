use chrono::{DateTime, Utc};
use focuser_common::error::{FocuserError, Result};
use focuser_common::pomodoro::{PomodoroConfig, PomodoroPhase, PomodoroSession};
use focuser_common::types::EntityId;
use rusqlite::{Connection, OptionalExtension, params};

use super::Database;

impl Database {
    /// Insert a freshly created session.
    pub fn create_pomodoro_session(&self, session: &PomodoroSession) -> Result<()> {
        let conn = self.conn_lock()?;
        conn.execute(
            "INSERT INTO pomodoro_sessions (
                id, block_list_id, work_secs, short_break_secs, long_break_secs,
                cycles_until_long_break, started_at, ended_at, completed_cycles,
                current_phase, current_cycle, phase_started_at,
                paused_remaining_secs, prev_enabled
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                session.id.to_string(),
                session.block_list_id.to_string(),
                session.config.work_secs as i64,
                session.config.short_break_secs as i64,
                session.config.long_break_secs as i64,
                session.config.cycles_until_long_break as i64,
                session.started_at.to_rfc3339(),
                session.completed_cycles as i64,
                session.current_phase.as_str(),
                session.current_cycle as i64,
                session.phase_started_at.to_rfc3339(),
                session.paused_remaining_secs.map(|v| v as i64),
                session.prev_enabled as i64,
            ],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    /// Persist an update to an in-progress session.
    pub fn update_pomodoro_session(&self, session: &PomodoroSession) -> Result<()> {
        let conn = self.conn_lock()?;
        conn.execute(
            "UPDATE pomodoro_sessions SET
                completed_cycles = ?1,
                current_phase = ?2,
                current_cycle = ?3,
                phase_started_at = ?4,
                paused_remaining_secs = ?5
             WHERE id = ?6",
            params![
                session.completed_cycles as i64,
                session.current_phase.as_str(),
                session.current_cycle as i64,
                session.phase_started_at.to_rfc3339(),
                session.paused_remaining_secs.map(|v| v as i64),
                session.id.to_string(),
            ],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    /// Mark a session as ended. Also closes the currently-open phase row.
    pub fn end_pomodoro_session(
        &self,
        session_id: EntityId,
        ended_at: DateTime<Utc>,
    ) -> Result<()> {
        let conn = self.conn_lock()?;
        conn.execute(
            "UPDATE pomodoro_sessions SET ended_at = ?1 WHERE id = ?2",
            params![ended_at.to_rfc3339(), session_id.to_string()],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        conn.execute(
            "UPDATE pomodoro_phases SET ended_at = ?1
             WHERE session_id = ?2 AND ended_at IS NULL",
            params![ended_at.to_rfc3339(), session_id.to_string()],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    /// Fetch the single active (not-yet-ended) session, if any.
    pub fn get_active_pomodoro_session(&self) -> Result<Option<PomodoroSession>> {
        let conn = self.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, block_list_id, work_secs, short_break_secs, long_break_secs,
                        cycles_until_long_break, started_at, completed_cycles,
                        current_phase, current_cycle, phase_started_at,
                        paused_remaining_secs, prev_enabled
                 FROM pomodoro_sessions
                 WHERE ended_at IS NULL
                 ORDER BY started_at DESC
                 LIMIT 1",
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        stmt.query_row([], row_to_session)
            .optional()
            .map_err(|e| FocuserError::Database(e.to_string()))
    }

    /// Log a new phase row (called on each phase transition).
    pub fn record_pomodoro_phase_start(
        &self,
        session_id: EntityId,
        phase: PomodoroPhase,
        cycle: u32,
        started_at: DateTime<Utc>,
    ) -> Result<()> {
        let conn = self.conn_lock()?;
        conn.execute(
            "INSERT INTO pomodoro_phases (session_id, phase_type, started_at, cycle_number)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                session_id.to_string(),
                phase.as_str(),
                started_at.to_rfc3339(),
                cycle as i64,
            ],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    /// Close out the most recent open phase row for a session.
    pub fn record_pomodoro_phase_end(
        &self,
        session_id: EntityId,
        ended_at: DateTime<Utc>,
    ) -> Result<()> {
        let conn = self.conn_lock()?;
        conn.execute(
            "UPDATE pomodoro_phases SET ended_at = ?1
             WHERE session_id = ?2 AND ended_at IS NULL",
            params![ended_at.to_rfc3339(), session_id.to_string()],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    /// Aggregate completed-cycle count per session for the last N days (for stats).
    pub fn get_pomodoro_history(&self, days: u32) -> Result<Vec<(DateTime<Utc>, u32, u32)>> {
        // Returns (started_at, completed_cycles, total_work_secs)
        let conn = self.conn_lock()?;
        let cutoff = (Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        let mut stmt = conn
            .prepare(
                "SELECT s.started_at, s.completed_cycles, s.work_secs
                 FROM pomodoro_sessions s
                 WHERE s.started_at >= ?1
                 ORDER BY s.started_at DESC",
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![cutoff], |row| {
                let started_str: String = row.get(0)?;
                let completed: i64 = row.get(1)?;
                let work_secs: i64 = row.get(2)?;
                let started_at = DateTime::parse_from_rfc3339(&started_str)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                Ok((
                    started_at,
                    completed as u32,
                    (completed as u32) * (work_secs as u32),
                ))
            })
            .map_err(|e| FocuserError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub(crate) fn conn_lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| FocuserError::Database(e.to_string()))
    }
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<PomodoroSession> {
    let id_str: String = row.get(0)?;
    let bl_str: String = row.get(1)?;
    let phase_str: String = row.get(8)?;
    let started_str: String = row.get(6)?;
    let phase_started_str: String = row.get(10)?;
    let paused: Option<i64> = row.get(11)?;
    let prev_enabled: i64 = row.get(12)?;

    let id = uuid::Uuid::parse_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let block_list_id = uuid::Uuid::parse_str(&bl_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let phase = PomodoroPhase::parse(&phase_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            8,
            rusqlite::types::Type::Text,
            Box::<dyn std::error::Error + Send + Sync>::from(format!("bad phase: {phase_str}")),
        )
    })?;
    let started_at = DateTime::parse_from_rfc3339(&started_str)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
        })?
        .with_timezone(&Utc);
    let phase_started_at = DateTime::parse_from_rfc3339(&phase_started_str)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e))
        })?
        .with_timezone(&Utc);

    let work_secs: i64 = row.get(2)?;
    let short_break_secs: i64 = row.get(3)?;
    let long_break_secs: i64 = row.get(4)?;
    let cycles: i64 = row.get(5)?;
    let completed_cycles: i64 = row.get(7)?;
    let current_cycle: i64 = row.get(9)?;

    Ok(PomodoroSession {
        id,
        block_list_id,
        config: PomodoroConfig {
            work_secs: work_secs as u32,
            short_break_secs: short_break_secs as u32,
            long_break_secs: long_break_secs as u32,
            cycles_until_long_break: cycles as u32,
        },
        current_phase: phase,
        current_cycle: current_cycle as u32,
        phase_started_at,
        paused_remaining_secs: paused.map(|v| v as u32),
        completed_cycles: completed_cycles as u32,
        started_at,
        prev_enabled: prev_enabled != 0,
    })
}
