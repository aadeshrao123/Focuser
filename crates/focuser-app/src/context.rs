//! Shared runtime state every command executes against.
//!
//! Moved out of `focuser-ui`'s `AppState` so the GUI, the CLI, and the service
//! all drive the same engine through the same handle instead of each opening
//! their own `BlockEngine`.

use std::sync::Mutex;

use focuser_core::BlockEngine;
use focuser_core::allowance::AllowanceTracker;

/// Events raised by background loops that a frontend should react to
/// (e.g. an OS notification on Pomodoro phase change).
#[derive(Debug, Clone)]
pub enum PomodoroEvent {
    PhaseAdvanced { to: String, cycle: u32 },
    TamperDetected,
}

/// Everything a command needs in order to run.
pub struct AppContext {
    pub engine: Mutex<BlockEngine>,
    pub allowance_tracker: AllowanceTracker,
    pomodoro_events: Mutex<Vec<PomodoroEvent>>,
}

impl AppContext {
    pub fn new(engine: BlockEngine) -> Self {
        Self {
            engine: Mutex::new(engine),
            allowance_tracker: AllowanceTracker::new(),
            pomodoro_events: Mutex::new(Vec::new()),
        }
    }

    pub fn push_pomodoro_event(&self, event: PomodoroEvent) {
        if let Ok(mut buf) = self.pomodoro_events.lock() {
            buf.push(event);
        }
    }

    pub fn drain_pomodoro_events(&self) -> Vec<PomodoroEvent> {
        self.pomodoro_events
            .lock()
            .map(|mut b| std::mem::take(&mut *b))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focuser_core::Database;

    fn ctx() -> AppContext {
        let db = Database::open_in_memory().unwrap();
        AppContext::new(BlockEngine::new(db).unwrap())
    }

    #[test]
    fn drain_returns_pushed_events_then_empties() {
        let ctx = ctx();
        ctx.push_pomodoro_event(PomodoroEvent::TamperDetected);
        ctx.push_pomodoro_event(PomodoroEvent::PhaseAdvanced {
            to: "work".into(),
            cycle: 2,
        });

        assert_eq!(ctx.drain_pomodoro_events().len(), 2);
        assert!(
            ctx.drain_pomodoro_events().is_empty(),
            "drain must consume the buffer"
        );
    }
}
