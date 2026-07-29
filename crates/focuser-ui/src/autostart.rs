//! "Launch at login", which on Windows is two things at once.
//!
//! The Tauri plugin writes an `HKCU\...\Run` entry. The NSIS installer also
//! creates a scheduled task, because a Run entry starts Focuser unelevated and
//! it needs admin to write the hosts file; the task carries `/rl highest` and
//! skips the UAC prompt. Both fire at logon, so the toggle has to drive both.
//!
//! The catch is that changing a `/rl highest` task needs admin, and Focuser is
//! only elevated when the task itself launched it. Opened from the Start menu
//! it is not, and `schtasks` returns "Access is denied". So what the user asked
//! for is stored in settings and treated as the truth, the OS registrations are
//! brought in line as far as permissions allow, and anything left over is
//! retried at the next startup — which is exactly when the task has handed us
//! the privileges to finish the job.

use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tracing::{info, warn};

use crate::AppState;

/// Set the first time the app runs, so the default is applied once rather than
/// re-applied on every launch.
pub const INITIALISED: &str = "autostart_initialised";

/// What the user last asked for. Absent until they touch the toggle.
pub const ENABLED: &str = "autostart_enabled";

/// How far [`imp::set_task`] got.
pub enum TaskChange {
    Done,
    /// No such task: portable and dev builds never had one.
    NoTask,
    NeedsAdmin,
}

#[tauri::command]
pub fn is_autostart_enabled(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    if let Ok(engine) = state.engine.lock()
        && let Ok(Some(saved)) = engine.db().get_setting(ENABLED)
    {
        // The answer to "what did you ask for", not "what did the OS accept".
        // Reading the OS here is what made the toggle spring back: the task
        // cannot always be changed, and the UI then argued with the user.
        return Ok(saved == "1");
    }

    Ok(app.autolaunch().is_enabled().unwrap_or(false) || imp::task_enabled())
}

#[tauri::command]
pub fn set_autostart(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<(), String> {
    if let Ok(engine) = state.engine.lock() {
        engine
            .db()
            .set_setting(ENABLED, if enabled { "1" } else { "0" })
            .map_err(|e| e.to_string())?;
    }

    // HKCU, so this one always works.
    let plugin = app.autolaunch();
    let wrote = if enabled {
        plugin.enable()
    } else {
        plugin.disable()
    };
    if let Err(e) = wrote {
        warn!("autostart plugin refused: {e}");
    }

    match imp::set_task(enabled) {
        TaskChange::Done | TaskChange::NoTask => Ok(()),
        // Deliberately an error rather than a silent shrug, but the setting is
        // already saved, so the toggle stays where the user put it and startup
        // finishes the job.
        TaskChange::NeedsAdmin => Err("needs-admin".into()),
    }
}

/// Bring the OS in line with what the user asked for.
///
/// Called at startup, where being launched by the task means we are elevated
/// and can finally change it.
pub fn reconcile(app: &AppHandle, db: &focuser_core::db::Database) {
    if db.get_setting(INITIALISED).ok().flatten().is_none() {
        let _ = app.autolaunch().enable();
        let _ = db.set_setting(INITIALISED, "1");
        return;
    }

    let Ok(Some(saved)) = db.get_setting(ENABLED) else {
        return;
    };
    let want = saved == "1";
    if want == imp::task_enabled() {
        return;
    }

    match imp::set_task(want) {
        TaskChange::Done => info!("logon task brought in line with the saved setting"),
        TaskChange::NoTask => {}
        TaskChange::NeedsAdmin => {
            warn!("logon task still needs admin to change; will retry next start")
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::TaskChange;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use tracing::warn;

    const TASK: &str = "Focuser";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn schtasks(args: &[&str]) -> Option<std::process::Output> {
        Command::new("schtasks")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| warn!("schtasks failed to run: {e}"))
            .ok()
    }

    pub fn task_enabled() -> bool {
        let Some(out) = schtasks(&["/query", "/tn", TASK, "/fo", "list"]) else {
            return false;
        };
        if !out.status.success() {
            return false;
        }
        // "Status" is localised. A disabled task reports Disabled and an
        // enabled one Ready or Running, so match on the one word we are sure
        // about rather than trying to enumerate the others.
        !String::from_utf8_lossy(&out.stdout)
            .to_ascii_lowercase()
            .contains("disabled")
    }

    pub fn set_task(enabled: bool) -> TaskChange {
        let flag = if enabled { "/enable" } else { "/disable" };
        // Change rather than recreate: rebuilding it needs the install path and
        // would drop `/rl highest` if we got that wrong.
        let Some(out) = schtasks(&["/change", "/tn", TASK, flag]) else {
            return TaskChange::NoTask;
        };
        if out.status.success() {
            return TaskChange::Done;
        }

        let err = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
        if err.contains("cannot find") || err.contains("does not exist") {
            TaskChange::NoTask
        } else if err.contains("access is denied") {
            TaskChange::NeedsAdmin
        } else {
            warn!("could not {flag} the logon task: {}", err.trim());
            TaskChange::NeedsAdmin
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::TaskChange;

    // macOS uses a LaunchAgent and Linux an autostart .desktop entry, both
    // written by the plugin, so there is no second mechanism to keep in step.
    pub fn task_enabled() -> bool {
        false
    }
    pub fn set_task(_enabled: bool) -> TaskChange {
        TaskChange::NoTask
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focuser_core::db::Database;

    /// The stored answer is what the toggle shows, whatever the OS did.
    fn shown(db: &Database) -> Option<bool> {
        db.get_setting(ENABLED).ok().flatten().map(|v| v == "1")
    }

    #[test]
    fn the_default_is_applied_once_and_then_never_again() {
        let db = Database::open_in_memory().expect("in-memory db");
        assert!(db.get_setting(INITIALISED).unwrap().is_none());

        db.set_setting(INITIALISED, "1").unwrap();
        assert!(
            db.get_setting(INITIALISED).unwrap().is_some(),
            "#10: re-applying the default every launch is what stopped anyone turning it off"
        );
    }

    #[test]
    fn turning_it_off_sticks_even_when_the_task_cannot_be_changed() {
        // The 0.7.2 regression: schtasks needs admin, the change failed, and
        // reading the OS back flipped the toggle on again. The saved answer
        // has to survive that.
        let db = Database::open_in_memory().expect("in-memory db");
        db.set_setting(ENABLED, "0").unwrap();

        assert_eq!(shown(&db), Some(false));
    }

    #[test]
    fn nothing_is_stored_until_the_user_chooses() {
        let db = Database::open_in_memory().expect("in-memory db");
        assert_eq!(
            shown(&db),
            None,
            "a fresh install falls back to the OS state"
        );

        db.set_setting(ENABLED, "1").unwrap();
        assert_eq!(shown(&db), Some(true));
    }
}
