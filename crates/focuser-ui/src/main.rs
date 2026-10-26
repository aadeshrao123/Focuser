#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod command_worker;
#[cfg(not(windows))]
include!("legacy_main.rs");
#[cfg(windows)]
include!("windows_main.rs");
#[cfg(not(windows))]
fn dispatch(
    state: &AppState,
    command: focuser_app::Command,
) -> Result<focuser_app::CommandResult, focuser_app::CommandErrorPayload> {
    focuser_app::execute(state, command).map_err(Into::into)
}
