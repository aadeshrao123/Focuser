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
//! See `internal-docs/tasks/2026-07-26-ui-revamp/02-command-core.md` for the design.

pub mod command;
pub mod context;
pub mod error;
pub mod execute;

pub use command::{Command, CommandResult};
pub use context::{AppContext, NoopSync, PomodoroEvent, SystemSync};
pub use error::{CommandError, CommandOutcome};
pub use execute::execute;
