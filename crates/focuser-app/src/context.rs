//! Shared runtime state every command executes against.
//!
//! Moved out of `focuser-ui`'s `AppState` so the GUI, the CLI, and the service
//! all drive the same engine through the same handle instead of each opening
//! their own `BlockEngine`.

use std::sync::{Arc, Mutex};

use focuser_core::BlockEngine;
use focuser_core::allowance::AllowanceTracker;

/// Events raised by background loops that a frontend should react to
/// (e.g. an OS notification on Pomodoro phase change).
#[derive(Debug, Clone)]
pub enum PomodoroEvent {
    PhaseAdvanced { to: String, cycle: u32 },
    TamperDetected,
}

/// The system-level side effect commands need: pushing the current set of
/// blocked domains into the OS hosts file after a mutation.
///
/// Abstracted as a trait so `focuser-app` does not depend on any particular
/// frontend. The GUI wires in its `blocker::apply_hosts_blocks`, the service
/// wires in its platform blocker, and tests use [`NoopSync`] — which is what
/// makes command behaviour testable without touching a real hosts file.
pub trait SystemSync: Send + Sync {
    fn sync_hosts(&self, domains: &[String]);
}

/// Does nothing. For tests and for headless contexts with no hosts access.
pub struct NoopSync;

impl SystemSync for NoopSync {
    fn sync_hosts(&self, _domains: &[String]) {}
}

/// Everything a command needs in order to run.
pub struct AppContext {
    pub engine: Mutex<BlockEngine>,
    pub allowance_tracker: AllowanceTracker,
    pomodoro_events: Mutex<Vec<PomodoroEvent>>,
    system: Arc<dyn SystemSync>,
}

impl AppContext {
    /// Build a context with a real system-sync implementation.
    pub fn new(engine: BlockEngine, system: Arc<dyn SystemSync>) -> Self {
        Self {
            engine: Mutex::new(engine),
            allowance_tracker: AllowanceTracker::new(),
            pomodoro_events: Mutex::new(Vec::new()),
            system,
        }
    }

    /// Build a context that performs no system-level side effects.
    pub fn new_headless(engine: BlockEngine) -> Self {
        Self::new(engine, Arc::new(NoopSync))
    }

    /// Re-derive the blocked-domain set from the engine and push it to the OS.
    ///
    /// Call after any mutation that can change which domains are blocked.
    pub fn sync_hosts(&self, engine: &BlockEngine) {
        self.system.sync_hosts(&engine.collect_blocked_domains());
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ctx() -> AppContext {
        let db = Database::open_in_memory().unwrap();
        AppContext::new_headless(BlockEngine::new(db).unwrap())
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

    #[derive(Default)]
    struct CountingSync(AtomicUsize);

    impl SystemSync for CountingSync {
        fn sync_hosts(&self, _domains: &[String]) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn sync_hosts_delegates_to_the_injected_impl() {
        let counter = Arc::new(CountingSync::default());
        let db = Database::open_in_memory().unwrap();
        let engine = BlockEngine::new(db).unwrap();
        let ctx = AppContext::new(engine, counter.clone());

        {
            let eng = ctx.engine.lock().unwrap();
            ctx.sync_hosts(&eng);
        }

        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    }
}
