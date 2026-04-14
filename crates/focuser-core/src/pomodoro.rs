//! Pomodoro state machine. Transitions phases based on elapsed wall-clock
//! time, persisting updates to the DB. Also provides monotonic-clock tamper
//! detection: if the wall clock jumps forward by a suspicious amount, the
//! session is aborted instead of silently advancing.

use chrono::{DateTime, Utc};
use focuser_common::error::Result;
use focuser_common::pomodoro::{PomodoroConfig, PomodoroPhase, PomodoroSession, PomodoroStatus};
use focuser_common::types::EntityId;
use std::time::Instant;
use tracing::{info, warn};

use crate::Database;
use crate::engine::BlockEngine;

/// Max allowed positive drift between wall-clock and monotonic elapsed
/// before we treat it as a tamper event (seconds).
const MAX_WALL_CLOCK_JUMP_SECS: i64 = 120;

/// Result of evaluating a Pomodoro tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickOutcome {
    /// Nothing happened — session is still running inside the current phase.
    NoChange,
    /// The phase advanced to a new one. The listed phase is the NEW phase.
    PhaseAdvanced {
        from: PomodoroPhase,
        to: PomodoroPhase,
        cycle: u32,
    },
    /// Session is idle / inactive.
    Idle,
    /// Clock tamper detected. Session should be aborted.
    TamperDetected,
}

/// Pomodoro runtime state that lives in the blocker loop.
pub struct PomodoroRuntime {
    /// Anchor for monotonic clock (instant at last tick).
    last_tick_instant: Option<Instant>,
    last_tick_wall: Option<DateTime<Utc>>,
}

impl Default for PomodoroRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl PomodoroRuntime {
    pub fn new() -> Self {
        Self {
            last_tick_instant: None,
            last_tick_wall: None,
        }
    }

    /// Reset the monotonic anchor (e.g., when starting a new session).
    pub fn reset_clock_anchor(&mut self) {
        self.last_tick_instant = Some(Instant::now());
        self.last_tick_wall = Some(Utc::now());
    }

    /// Check for wall-clock tampering. Returns true if a suspicious forward
    /// jump is detected (wall clock advanced much more than monotonic).
    fn detect_tamper(&mut self, now_wall: DateTime<Utc>) -> bool {
        let now_instant = Instant::now();
        let tampered = match (self.last_tick_instant, self.last_tick_wall) {
            (Some(prev_i), Some(prev_w)) => {
                let wall_delta = (now_wall - prev_w).num_seconds();
                let mono_delta = now_instant.saturating_duration_since(prev_i).as_secs() as i64;
                wall_delta - mono_delta > MAX_WALL_CLOCK_JUMP_SECS
            }
            _ => false,
        };
        self.last_tick_instant = Some(now_instant);
        self.last_tick_wall = Some(now_wall);
        tampered
    }
}

/// Advance the phase based on elapsed time. Returns the outcome; caller
/// is responsible for persisting the session and toggling the block list.
///
/// This is pure logic — no DB or side effects — so it is easy to unit-test.
pub fn advance_phase(session: &mut PomodoroSession, now: DateTime<Utc>) -> TickOutcome {
    if session.is_paused() {
        return TickOutcome::NoChange;
    }
    let remaining = session.remaining_secs(now);
    if remaining > 0 {
        return TickOutcome::NoChange;
    }

    let prev_phase = session.current_phase;
    let (next_phase, new_cycle, new_completed) = match session.current_phase {
        PomodoroPhase::Work => {
            let completed = session.completed_cycles + 1;
            // After N work cycles → long break; else short break
            if completed.is_multiple_of(session.config.cycles_until_long_break) {
                (PomodoroPhase::LongBreak, session.current_cycle, completed)
            } else {
                (PomodoroPhase::ShortBreak, session.current_cycle, completed)
            }
        }
        PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => {
            // After any break, next cycle begins
            let next_cycle = session.current_cycle + 1;
            (PomodoroPhase::Work, next_cycle, session.completed_cycles)
        }
    };

    session.current_phase = next_phase;
    session.current_cycle = new_cycle;
    session.completed_cycles = new_completed;
    session.phase_started_at = now;
    session.paused_remaining_secs = None;

    TickOutcome::PhaseAdvanced {
        from: prev_phase,
        to: next_phase,
        cycle: new_cycle,
    }
}

/// Start a new Pomodoro session. Persists the session row and initial phase
/// row, and snapshots the block list's enabled state so we can restore it.
/// Returns an error if a session is already active.
pub fn start_session(
    eng: &mut BlockEngine,
    block_list_id: EntityId,
    config: PomodoroConfig,
) -> Result<PomodoroSession> {
    // Reject if another session is running.
    if eng.db().get_active_pomodoro_session()?.is_some() {
        return Err(focuser_common::error::FocuserError::InvalidConfig(
            "a Pomodoro session is already active".into(),
        ));
    }
    config
        .validate()
        .map_err(focuser_common::error::FocuserError::InvalidConfig)?;

    // Find the block list to snapshot its enabled state.
    let block_list = eng.db().get_block_list(block_list_id).map_err(|_| {
        focuser_common::error::FocuserError::BlockListNotFound(block_list_id.to_string())
    })?;
    let prev_enabled = block_list.enabled;

    // Enable the block list for the work phase (will be toggled per phase).
    let mut updated = block_list.clone();
    updated.enabled = true;
    updated.updated_at = Utc::now();
    eng.db().update_block_list(&updated)?;

    let session = PomodoroSession::new(block_list_id, config, prev_enabled);
    eng.db().create_pomodoro_session(&session)?;
    eng.db().record_pomodoro_phase_start(
        session.id,
        session.current_phase,
        session.current_cycle,
        session.phase_started_at,
    )?;
    let _ = eng.refresh();
    info!(
        session_id = %session.id,
        block_list = %block_list_id,
        "Pomodoro session started"
    );
    Ok(session)
}

/// Stop the active session. Restores the block list's pre-session enabled
/// state. No-op if no session is active. Returns true if a session was ended.
pub fn stop_session(eng: &mut BlockEngine) -> Result<bool> {
    let Some(session) = eng.db().get_active_pomodoro_session()? else {
        return Ok(false);
    };
    let now = Utc::now();
    eng.db().end_pomodoro_session(session.id, now)?;

    // Restore the block list's previous enabled state.
    if let Ok(mut bl) = eng.db().get_block_list(session.block_list_id) {
        bl.enabled = session.prev_enabled;
        bl.updated_at = now;
        let _ = eng.db().update_block_list(&bl);
    }
    let _ = eng.refresh();
    info!(session_id = %session.id, "Pomodoro session stopped");
    Ok(true)
}

/// Pause the active session. If already paused, this is a no-op.
pub fn pause_session(eng: &mut BlockEngine) -> Result<bool> {
    let Some(mut session) = eng.db().get_active_pomodoro_session()? else {
        return Ok(false);
    };
    if session.is_paused() {
        return Ok(true);
    }
    let now = Utc::now();
    let remaining = session.remaining_secs(now);
    session.paused_remaining_secs = Some(remaining);
    eng.db().update_pomodoro_session(&session)?;
    Ok(true)
}

/// Resume a paused session.
pub fn resume_session(eng: &mut BlockEngine) -> Result<bool> {
    let Some(mut session) = eng.db().get_active_pomodoro_session()? else {
        return Ok(false);
    };
    let Some(rem) = session.paused_remaining_secs else {
        return Ok(true);
    };
    let now = Utc::now();
    // Shift phase_started_at backward so that `remaining` equals `rem` from `now`.
    let duration = session.current_phase_duration_secs() as i64;
    let offset = duration - rem as i64;
    session.phase_started_at = now - chrono::Duration::seconds(offset);
    session.paused_remaining_secs = None;
    eng.db().update_pomodoro_session(&session)?;
    Ok(true)
}

/// Skip to the next phase immediately. Used by the "Skip phase" UI button.
pub fn skip_phase(eng: &mut BlockEngine) -> Result<Option<TickOutcome>> {
    let Some(mut session) = eng.db().get_active_pomodoro_session()? else {
        return Ok(None);
    };
    let now = Utc::now();
    // Force remaining to 0 by shifting phase_started_at back
    let duration = session.current_phase_duration_secs() as i64;
    session.phase_started_at = now - chrono::Duration::seconds(duration);
    session.paused_remaining_secs = None;
    let outcome = advance_phase(&mut session, now);
    eng.db().record_pomodoro_phase_end(session.id, now)?;
    eng.db().record_pomodoro_phase_start(
        session.id,
        session.current_phase,
        session.current_cycle,
        session.phase_started_at,
    )?;
    eng.db().update_pomodoro_session(&session)?;
    apply_phase_to_block_list(eng, &session)?;
    let _ = eng.refresh();
    Ok(Some(outcome))
}

/// Called from the blocker loop tick. Evaluates the active session (if any)
/// and applies phase transitions.
pub fn tick(eng: &mut BlockEngine, runtime: &mut PomodoroRuntime) -> Result<TickOutcome> {
    let Some(mut session) = eng.db().get_active_pomodoro_session()? else {
        runtime.last_tick_instant = None;
        runtime.last_tick_wall = None;
        return Ok(TickOutcome::Idle);
    };
    let now = Utc::now();

    // Tamper detection.
    if runtime.detect_tamper(now) {
        warn!(
            session_id = %session.id,
            "Pomodoro: wall-clock tamper detected, aborting session"
        );
        eng.db().end_pomodoro_session(session.id, now)?;
        // Restore prev enabled
        if let Ok(mut bl) = eng.db().get_block_list(session.block_list_id) {
            bl.enabled = session.prev_enabled;
            bl.updated_at = now;
            let _ = eng.db().update_block_list(&bl);
        }
        let _ = eng.refresh();
        return Ok(TickOutcome::TamperDetected);
    }

    let outcome = advance_phase(&mut session, now);
    match outcome {
        TickOutcome::NoChange | TickOutcome::Idle | TickOutcome::TamperDetected => {}
        TickOutcome::PhaseAdvanced { from, to, cycle } => {
            eng.db().record_pomodoro_phase_end(session.id, now)?;
            eng.db().record_pomodoro_phase_start(
                session.id,
                session.current_phase,
                session.current_cycle,
                session.phase_started_at,
            )?;
            eng.db().update_pomodoro_session(&session)?;
            apply_phase_to_block_list(eng, &session)?;
            let _ = eng.refresh();
            info!(
                session_id = %session.id,
                from = ?from, to = ?to, cycle, "Pomodoro phase advanced"
            );
        }
    }
    Ok(outcome)
}

/// Toggle the linked block list's enabled flag to match the current phase.
fn apply_phase_to_block_list(eng: &mut BlockEngine, session: &PomodoroSession) -> Result<()> {
    let Ok(mut bl) = eng.db().get_block_list(session.block_list_id) else {
        return Ok(());
    };
    let should_be_enabled = session.current_phase.is_work();
    if bl.enabled != should_be_enabled {
        bl.enabled = should_be_enabled;
        bl.updated_at = Utc::now();
        eng.db().update_block_list(&bl)?;
    }
    Ok(())
}

/// Build a status snapshot for the UI.
pub fn build_status(db: &Database) -> Result<Option<PomodoroStatus>> {
    let Some(session) = db.get_active_pomodoro_session()? else {
        return Ok(None);
    };
    let block_list_name = db
        .get_block_list(session.block_list_id)
        .map(|bl| bl.name)
        .unwrap_or_else(|_| "(unknown)".into());
    let now = Utc::now();
    let remaining = session.remaining_secs(now);
    Ok(Some(PomodoroStatus {
        session_id: session.id,
        block_list_id: session.block_list_id,
        block_list_name,
        config: session.config,
        current_phase: session.current_phase,
        current_cycle: session.current_cycle,
        completed_cycles: session.completed_cycles,
        remaining_secs: remaining,
        phase_duration_secs: session.current_phase_duration_secs(),
        paused: session.is_paused(),
        started_at: session.started_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use focuser_common::pomodoro::PomodoroConfig;

    fn mk_session() -> PomodoroSession {
        PomodoroSession::new(
            uuid::Uuid::new_v4(),
            PomodoroConfig {
                work_secs: 60,
                short_break_secs: 10,
                long_break_secs: 30,
                cycles_until_long_break: 3,
            },
            true,
        )
    }

    #[test]
    fn no_advance_before_timer_elapses() {
        let mut s = mk_session();
        let now = s.phase_started_at + chrono::Duration::seconds(30);
        let out = advance_phase(&mut s, now);
        assert_eq!(out, TickOutcome::NoChange);
        assert_eq!(s.current_phase, PomodoroPhase::Work);
    }

    #[test]
    fn work_to_short_break_transition() {
        let mut s = mk_session();
        let now = s.phase_started_at + chrono::Duration::seconds(60);
        let out = advance_phase(&mut s, now);
        assert!(matches!(
            out,
            TickOutcome::PhaseAdvanced {
                to: PomodoroPhase::ShortBreak,
                ..
            }
        ));
        assert_eq!(s.current_phase, PomodoroPhase::ShortBreak);
        assert_eq!(s.completed_cycles, 1);
    }

    #[test]
    fn long_break_after_n_cycles() {
        let mut s = mk_session(); // cycles_until_long_break = 3
        s.completed_cycles = 2; // next work completion = 3
        s.current_phase = PomodoroPhase::Work;
        s.phase_started_at = Utc::now();
        let now = s.phase_started_at + chrono::Duration::seconds(60);
        let out = advance_phase(&mut s, now);
        assert!(matches!(
            out,
            TickOutcome::PhaseAdvanced {
                to: PomodoroPhase::LongBreak,
                ..
            }
        ));
    }

    #[test]
    fn break_back_to_work_increments_cycle() {
        let mut s = mk_session();
        s.current_phase = PomodoroPhase::ShortBreak;
        s.current_cycle = 1;
        s.phase_started_at = Utc::now();
        let now = s.phase_started_at + chrono::Duration::seconds(10);
        advance_phase(&mut s, now);
        assert_eq!(s.current_phase, PomodoroPhase::Work);
        assert_eq!(s.current_cycle, 2);
    }

    #[test]
    fn paused_session_does_not_advance() {
        let mut s = mk_session();
        s.paused_remaining_secs = Some(10);
        let now = s.phase_started_at + chrono::Duration::seconds(120);
        let out = advance_phase(&mut s, now);
        assert_eq!(out, TickOutcome::NoChange);
        assert_eq!(s.current_phase, PomodoroPhase::Work);
    }

    #[test]
    fn remaining_secs_when_paused() {
        let mut s = mk_session();
        s.paused_remaining_secs = Some(42);
        assert_eq!(s.remaining_secs(Utc::now()), 42);
    }

    #[test]
    fn config_validation() {
        assert!(PomodoroConfig::CLASSIC.validate().is_ok());
        let bad = PomodoroConfig {
            work_secs: 0,
            short_break_secs: 60,
            long_break_secs: 60,
            cycles_until_long_break: 4,
        };
        assert!(bad.validate().is_err());
    }
}
