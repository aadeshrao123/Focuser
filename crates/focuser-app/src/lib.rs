//! The Focuser command surface.
//!
//! Every action the application can perform is a [`Command`] variant, executed by
//! [`execute`]. The Tauri GUI, the CLI, the service IPC handler, and the extension
//! HTTP API are all thin shims that deserialise into a `Command` and forward it here.
//!
//! **Invariant: no business logic lives outside `execute`.** That is what makes
//! CLI/GUI parity a structural property rather than something to maintain by hand —
//! there is nowhere else for behaviour to diverge.
//!
//! See `docs/COMMAND-CORE.md` for the design rationale.

pub mod context;
pub mod error;

pub use context::{AppContext, PomodoroEvent};
pub use error::{CommandError, CommandOutcome};
