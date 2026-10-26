//! The UI login preference does not control SCM or the independent session agent.
use crate::AppState;
use focuser_app::{Command, CommandResult};
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
#[tauri::command]
pub async fn is_autostart_enabled(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    if state.development {
        return Ok(false);
    }
    match crate::typed_commands::dispatch_worker(
        state.inner().clone(),
        Command::GetSetting {
            key: "autostart_enabled".into(),
            default: None,
        },
    )
    .await
    .map_err(|e| e.message)?
    {
        CommandResult::Setting(Some(value)) => Ok(value == "1" || value == "true"),
        _ => Ok(app.autolaunch().is_enabled().unwrap_or(false)),
    }
}
#[tauri::command]
pub async fn set_autostart(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<(), String> {
    if state.development {
        return Err("Development clients cannot change Windows startup registrations".into());
    }
    crate::typed_commands::dispatch_worker(
        state.inner().clone(),
        Command::SetSetting {
            key: "autostart_enabled".into(),
            value: if enabled { "1" } else { "0" }.into(),
        },
    )
    .await
    .map_err(|e| e.code)?;
    if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    }
    .map_err(|e| e.to_string())
}
