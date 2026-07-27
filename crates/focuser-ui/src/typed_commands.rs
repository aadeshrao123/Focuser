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

/// Where generated bindings are written, relative to the crate root.
const BINDINGS_PATH: &str = "frontend/src/bindings.ts";

/// Write `bindings.ts` next to the frontend source.
///
/// Invoked via `--export-bindings`, which exits before the database is opened or
/// a window is created. An earlier version did this during normal startup, but
/// that made regeneration depend on successfully launching a GUI app — which
/// fails silently when another instance holds the database, leaving stale
/// bindings behind with no error. A dedicated flag is deterministic and
/// scriptable:
///
/// ```text
/// cargo run -p focuser-ui -- --export-bindings
/// ```
///
/// Returns whether the export succeeded, so the caller can set an exit code.
pub fn export_bindings() -> bool {
    use specta_typescript::Typescript;

    // Note: 64-bit fields opt into `number` individually with
    // `#[specta(type = specta_typescript::Number)]` at their definition, rather
    // than being waved through globally here — so each one is a deliberate,
    // reviewable decision about precision.
    match specta_builder().export(Typescript::default(), BINDINGS_PATH) {
        Ok(()) => {
            println!("exported TypeScript bindings to {BINDINGS_PATH}");
            true
        }
        Err(e) => {
            eprintln!("failed to export bindings to {BINDINGS_PATH}: {e}");
            false
        }
    }
}
