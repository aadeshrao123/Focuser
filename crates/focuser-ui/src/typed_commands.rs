//! The typed Tauri entry point into the command core.
//!
//! One command — [`run_command`] — takes a [`Command`] and returns a
//! [`CommandResult`]. Every transport therefore speaks the *same* shape:
//!
//! ```text
//!   Tauri webview  invoke("run_command", { command })  ─┐
//!   browser        POST /command                       ─┼─> execute()
//!   terminal       focuser <subcommand>                 ─┘
//! ```
//!
//! An earlier draft of this used 55 individual `#[tauri::command]` shims, which
//! is what tauri-specta is optimised for (per-command functions, JSDoc lifted
//! from Rust docs, per-command query keys). That was dropped: the dev server and
//! the CLI cannot use per-command Tauri bindings — they need a serialisable
//! envelope — so per-command shims would have meant maintaining two different
//! wire shapes for the same operations. One shape across all three transports is
//! worth more than the generated ergonomics, especially since the query hooks are
//! hand-written either way.
//!
//! specta still earns its place: it generates `Command`, `CommandResult`, and
//! `CommandError` as TypeScript, so the payload is fully checked and the
//! hand-maintained mirror in the frontend can be deleted.

use std::sync::Arc;

use focuser_app::{Command, CommandErrorPayload, CommandResult, execute};
use tauri::State;

use crate::AppState;

/// Execute one command against the shared context.
///
/// Errors are flattened to [`CommandErrorPayload`] (`{ code, message }`) because
/// `CommandError` wraps `FocuserError`, which has no TypeScript representation.
/// The frontend branches on `code`, never on `message`.
#[tauri::command]
#[specta::specta]
pub fn run_command(
    state: State<'_, Arc<AppState>>,
    command: Command,
) -> Result<CommandResult, CommandErrorPayload> {
    execute(&state, command).map_err(CommandErrorPayload::from)
}

/// The specta builder, used both to export bindings and to document the surface.
///
/// Kept in one place so the export path and the registered commands cannot drift.
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![run_command])
}

/// Write `bindings.ts` next to the frontend source.
///
/// Debug builds only: bindings are committed, and a release build should never
/// depend on writing into the source tree.
#[cfg(debug_assertions)]
pub fn export_bindings() {
    use specta_typescript::Typescript;

    const OUT: &str = "frontend/src/bindings.ts";

    match specta_builder().export(Typescript::default(), OUT) {
        Ok(()) => tracing::info!(path = OUT, "exported TypeScript bindings"),
        // Not fatal: the app must still run from a directory where the frontend
        // source isn't present (a packaged debug build, say).
        Err(e) => tracing::warn!(error = %e, path = OUT, "could not export bindings"),
    }
}
