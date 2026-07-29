//! "Launch at login", which on Windows is two things at once.
//!
//! The Tauri plugin writes an `HKCU\...\Run` entry. The NSIS installer also
//! creates a scheduled task, because a Run entry starts Focuser unelevated and
//! it needs admin to write the hosts file; the task carries `/rl highest` and
//! skips the UAC prompt. Both fire at logon, so the toggle has to drive both or
//! turning it off changes nothing.

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tracing::warn;

/// Set once the very first time the app runs, so the default is applied once
/// rather than fighting the user on every launch.
pub const INITIALISED: &str = "autostart_initialised";

#[tauri::command]
pub fn is_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    let plugin = app.autolaunch().is_enabled().unwrap_or(false);
    Ok(plugin || imp::task_enabled())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    if let Err(e) = result {
        warn!("autostart plugin refused: {e}");
    }

    imp::set_task(enabled);
    Ok(())
}

/// True exactly once per install, and records that it has been asked.
///
/// Separate from [`apply_default_once`] so the part that decides can be tested
/// without a running Tauri app.
pub fn claim_default(db: &focuser_core::db::Database) -> bool {
    if db.get_setting(INITIALISED).ok().flatten().is_some() {
        return false;
    }
    // Written before enabling, so a failure to register still counts as asked.
    // Retrying every launch is the bug this replaced.
    let _ = db.set_setting(INITIALISED, "1");
    true
}

/// The default for a fresh install: on, once, and never re-applied.
pub fn apply_default_once(app: &AppHandle, db: &focuser_core::db::Database) {
    if claim_default(db) {
        let _ = app.autolaunch().enable();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focuser_core::db::Database;

    #[test]
    fn the_default_is_applied_once_and_then_never_again() {
        let db = Database::open_in_memory().expect("in-memory db");

        assert!(claim_default(&db), "a fresh install should get the default");
        assert!(
            !claim_default(&db),
            "#10: re-applying on every launch is what stopped anyone turning it off"
        );
        assert!(!claim_default(&db));
    }

    #[test]
    fn turning_it_off_survives_a_restart() {
        // What the reporter actually did: default applied, user says no, app
        // restarts. The restart must not treat that as a fresh install.
        let db = Database::open_in_memory().expect("in-memory db");
        claim_default(&db);

        assert!(!claim_default(&db));
        assert_eq!(db.get_setting(INITIALISED).unwrap().as_deref(), Some("1"));
    }
}

#[cfg(windows)]
mod imp {
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

    /// Whether the installer's logon task exists *and* is turned on. A missing
    /// task is not an error: portable and dev builds never had one.
    pub fn task_enabled() -> bool {
        let Some(out) = schtasks(&["/query", "/tn", TASK, "/fo", "list"]) else {
            return false;
        };
        if !out.status.success() {
            return false;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        // "Status" is localised, so match on what it is not rather than what it
        // is: a disabled task reports Disabled in English and Ready/Running
        // otherwise. Falling back to "present means on" would be wrong here.
        !text.to_ascii_lowercase().contains("disabled")
    }

    pub fn set_task(enabled: bool) {
        let flag = if enabled { "/enable" } else { "/disable" };
        // Change rather than delete: recreating it needs the install path and
        // would silently drop `/rl highest` if we got that wrong.
        if let Some(out) = schtasks(&["/change", "/tn", TASK, flag])
            && !out.status.success()
        {
            let err = String::from_utf8_lossy(&out.stderr);
            // Nothing to change on a portable install, which is fine.
            if !err.contains("cannot find") {
                warn!("could not {flag} the logon task: {}", err.trim());
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    // macOS uses a LaunchAgent and Linux an autostart .desktop entry, both
    // written by the plugin, so there is no second mechanism to keep in step.
    pub fn task_enabled() -> bool {
        false
    }
    pub fn set_task(_enabled: bool) {}
}
