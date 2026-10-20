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
//!
//! On Linux the .deb ships a systemd user unit that also respawns Focuser if
//! it is killed. When that unit is installed and the session runs
//! `graphical-session.target`, the unit *is* the login launcher: the app
//! enables it itself, the toggle enables and disables it, and the plugin's XDG
//! autostart entry is kept off, since with both, login launched Focuser twice.
//!
//! Some desktops (Cinnamon, for one) never start that target. There the XDG
//! entry stays the launcher, and the login launch hands itself over to the
//! unit (see [`hand_off_to_unit`]), so a kill still brings Focuser back.

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
///
/// Only Windows has a task to change, so everywhere else two of these are
/// unreachable by construction rather than merely unused.
#[cfg_attr(not(windows), allow(dead_code))]
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
        // Off, then a reboot, would end a lock that keeps Focuser running.
        if !enabled && engine.has_service_protection() {
            return Err("protected".into());
        }
        engine
            .db()
            .set_setting(ENABLED, if enabled { "1" } else { "0" })
            .map_err(|e| e.to_string())?;
    }

    // HKCU, so this one always works.
    set_plugin(&app, enabled);

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
        set_plugin(app, true);
        enable_launcher();
        let _ = db.set_setting(INITIALISED, "1");
        return;
    }

    let Ok(Some(saved)) = db.get_setting(ENABLED) else {
        // Never touched, so the default (on) stands. Still clear an XDG entry
        // left over from before the systemd unit was installed, and enable
        // the unit in its place.
        if imp::replaces_plugin() {
            set_plugin(app, true);
            enable_launcher();
        }
        return;
    };
    let want = saved == "1";
    set_plugin(app, want);
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

/// Turn on a platform launcher that replaces the plugin.
///
/// Nothing else would: a package installed through a software center runs no
/// script as the user, so the app has to enable its own unit.
fn enable_launcher() {
    if imp::replaces_plugin() && !imp::task_enabled() {
        imp::set_task(true);
    }
}

/// Move a login launch into the systemd unit, on desktops that never start it.
///
/// True when the unit took over and this process should exit. Called before
/// anything else opens, so the two copies never both hold the database or the
/// single-instance lock. When the hand-off fails, Focuser just runs as it is,
/// without the restart on kill.
pub fn hand_off_to_unit() -> bool {
    imp::hand_off_to_unit()
}

/// Write or remove the plugin's own registration, unless a platform launcher
/// replaces it, in which case it is always removed.
fn set_plugin(app: &AppHandle, enabled: bool) {
    let plugin = app.autolaunch();
    let wrote = if enabled && !imp::replaces_plugin() {
        plugin.enable()
    } else {
        plugin.disable()
    };
    if let Err(e) = wrote {
        warn!("autostart plugin refused: {e}");
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

    /// The Run entry and the task both fire; the single-instance handler
    /// ignores whichever login launch comes second.
    pub fn replaces_plugin() -> bool {
        false
    }

    pub fn hand_off_to_unit() -> bool {
        false
    }

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

#[cfg(target_os = "linux")]
mod imp {
    use super::TaskChange;
    use std::path::Path;
    use std::process::Command;
    use std::sync::OnceLock;
    use tracing::{info, warn};

    /// Where the .deb puts the unit. Tied to the packaging in tauri.conf.json.
    const SYSTEMD_UNIT: &str = "/usr/lib/systemd/user/focuser.service";
    const UNIT: &str = "focuser.service";

    /// Set by the unit, so a copy it started never tries to hand off again.
    const SUPERVISED: &str = "FOCUSER_SUPERVISED";

    /// What a window needs to open, which the systemd user manager does not
    /// have unless the desktop hands it over.
    const DISPLAY_VARS: &[&str] = &[
        "DISPLAY",
        "XAUTHORITY",
        "WAYLAND_DISPLAY",
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
    ];

    fn systemctl(args: &[&str]) -> bool {
        Command::new("systemctl")
            .arg("--user")
            .args(args)
            .output()
            .is_ok_and(|out| out.status.success())
    }

    fn unit_installed() -> bool {
        Path::new(SYSTEMD_UNIT).exists()
    }

    /// The unit is installed and this session starts it.
    ///
    /// The unit hangs off `graphical-session.target`, which some desktops
    /// never start. There the XDG entry stays the launcher and hands off to
    /// the unit instead. Checked once per run: it does not change mid-session.
    fn has_unit() -> bool {
        static USABLE: OnceLock<bool> = OnceLock::new();
        *USABLE.get_or_init(|| {
            unit_installed() && systemctl(&["is-active", "--quiet", "graphical-session.target"])
        })
    }

    pub fn hand_off_to_unit() -> bool {
        if std::env::var_os(SUPERVISED).is_some() || !unit_installed() || has_unit() {
            return false;
        }
        // Already running under the unit: carry on, and the single-instance
        // handler drops this second login launch as usual.
        if systemctl(&["is-active", "--quiet", UNIT]) {
            return false;
        }
        let vars: Vec<&str> = DISPLAY_VARS
            .iter()
            .copied()
            .filter(|v| std::env::var_os(v).is_some())
            .collect();
        if vars.is_empty() {
            return false;
        }

        let mut import = vec!["import-environment"];
        import.extend(vars);
        if !systemctl(&import) {
            warn!("could not pass the display to systemd; running without restart on kill");
            return false;
        }
        // A unit that used up its restarts at the last logout would otherwise
        // refuse to start. Fails harmlessly when there is nothing to reset.
        systemctl(&["reset-failed", UNIT]);
        let started = systemctl(&["start", UNIT]);
        if started {
            info!("handed over to {UNIT}, which restarts Focuser if it is killed");
        } else {
            warn!("could not start {UNIT}; running without restart on kill");
        }
        started
    }

    pub fn replaces_plugin() -> bool {
        has_unit()
    }

    pub fn task_enabled() -> bool {
        has_unit()
            && Command::new("systemctl")
                .args(["--user", "is-enabled", "--quiet", UNIT])
                .status()
                .is_ok_and(|s| s.success())
    }

    pub fn set_task(enabled: bool) -> TaskChange {
        if !has_unit() {
            return TaskChange::NoTask;
        }
        // Enable or disable only: stopping it here would quit the app that is
        // asking. A user unit needs no admin, so this cannot be refused for
        // permissions; any failure is logged and retried at the next start.
        let verb = if enabled { "enable" } else { "disable" };
        match Command::new("systemctl")
            .args(["--user", verb, UNIT])
            .output()
        {
            Ok(out) if out.status.success() => {}
            Ok(out) => warn!(
                "could not {verb} {UNIT}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            Err(e) => warn!("systemctl failed to run: {e}"),
        }
        TaskChange::Done
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::TaskChange;

    // The plugin's LaunchAgent is the only launcher, so there is no second
    // mechanism to keep in step.
    pub fn replaces_plugin() -> bool {
        false
    }
    pub fn task_enabled() -> bool {
        false
    }
    pub fn set_task(_enabled: bool) -> TaskChange {
        TaskChange::NoTask
    }
    pub fn hand_off_to_unit() -> bool {
        false
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
