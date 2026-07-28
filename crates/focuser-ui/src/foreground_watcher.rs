//! Foreground-window watcher → app allowance ticks.
//!
//! The browser extension feeds *domain* ticks (active tab hostname) into
//! the AllowanceTracker. Apps don't have a corresponding feeder — without
//! this watcher, app allowances can never count down or trigger blocking.
//!
//! Strategy: every `TICK_INTERVAL` seconds resolve the OS-level foreground
//! window's owning process executable and submit it as an `AllowanceTick`
//! with `app_exe = Some(name)`. Idle users (no input for IDLE_THRESHOLD)
//! are skipped so the quota doesn't drain on an unattended machine.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use focuser_common::allowance::AllowanceTick;
use tracing::{debug, info};

use crate::AppState;

const TICK_INTERVAL: Duration = Duration::from_secs(5);
const IDLE_THRESHOLD_SECS: u32 = 60;
const TICK_SOURCE: &str = "foreground-watcher";

/// Spawn the watcher loop. Runs forever; never returns under normal use.
pub fn run_foreground_watcher(state: Arc<AppState>) {
    info!("Foreground app watcher started");
    let mut last_tick = Instant::now();

    loop {
        thread::sleep(TICK_INTERVAL);

        let now = Instant::now();
        let elapsed = now.duration_since(last_tick).as_secs() as u32;
        last_tick = now;

        // Skip idle users — quota only counts when they're actually present.
        if user_idle_seconds().unwrap_or(0) >= IDLE_THRESHOLD_SECS {
            continue;
        }

        let Some(sample) = foreground_app() else {
            continue;
        };

        // Don't count the Focuser UI itself.
        if sample.is_self {
            continue;
        }

        let tick = AllowanceTick {
            hostname: None,
            app_exe: Some(sample.exe_name.clone()),
            active: true,
            source: TICK_SOURCE.into(),
            increment_secs: Some(elapsed.clamp(1, 60)),
        };

        if let Ok(eng) = state.engine.lock()
            && let Err(e) = state.allowance_tracker.ingest_tick(eng.db(), &tick)
        {
            debug!(error = %e, app = %sample.exe_name, "ingest_tick failed");
        }
    }
}

/// One sample of the foreground process: the executable file name and a
/// flag for "this is us".
struct ForegroundSample {
    exe_name: String,
    is_self: bool,
}

/// Active foreground window's executable file name (e.g. "discord.exe").
/// `None` on platforms without an implementation or on API failure.
fn foreground_app() -> Option<ForegroundSample> {
    #[cfg(windows)]
    {
        win::foreground_app()
    }
    #[cfg(target_os = "macos")]
    {
        mac::foreground_app()
    }
    #[cfg(target_os = "linux")]
    {
        linux::foreground_app()
    }
}

/// Seconds since last user input across the whole system.
/// `None` on platforms without an implementation or on API failure.
fn user_idle_seconds() -> Option<u32> {
    #[cfg(windows)]
    {
        win::user_idle_seconds()
    }
    #[cfg(target_os = "macos")]
    {
        mac::user_idle_seconds()
    }
    #[cfg(target_os = "linux")]
    {
        linux::user_idle_seconds()
    }
}

#[cfg(windows)]
mod win {
    use super::ForegroundSample;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    /// Strip a full path down to its file name component.
    ///
    /// Lives inside the Windows module because that is its only caller — on
    /// other platforms `foreground_app` returns `None` and this would be dead
    /// code that fails a `-D warnings` build.
    fn file_name(path: &str) -> String {
        path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
    }

    pub fn foreground_app() -> Option<ForegroundSample> {
        let pid = foreground_pid()?;
        if pid == 0 {
            return None;
        }
        let path = process_image_path(pid)?;
        let exe_name = file_name(&path).to_ascii_lowercase();
        let is_self = unsafe { pid == GetCurrentProcessId() };
        Some(ForegroundSample { exe_name, is_self })
    }

    fn foreground_pid() -> Option<u32> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid: u32 = 0;
            let _tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            Some(pid)
        }
    }

    fn process_image_path(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            // MAX_PATH is enough for the vast majority — we only need the
            // file name component anyway. On overflow the API returns Err.
            let mut buf = [0u16; 260];
            let mut len = buf.len() as u32;
            let res = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(handle);
            if res.is_err() || len == 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    pub fn user_idle_seconds() -> Option<u32> {
        unsafe {
            let mut info = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if !GetLastInputInfo(&mut info).as_bool() {
                return None;
            }
            let now = GetTickCount();
            // GetTickCount wraps every ~49.7 days — saturating_sub handles it.
            Some(now.saturating_sub(info.dwTime) / 1000)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::file_name;

        #[test]
        fn file_name_strips_windows_path() {
            assert_eq!(
                file_name(r"C:\Program Files\Discord\Discord.exe"),
                "Discord.exe"
            );
        }

        #[test]
        fn file_name_strips_unix_path() {
            assert_eq!(file_name("/usr/bin/firefox"), "firefox");
        }

        #[test]
        fn file_name_handles_bare_name() {
            assert_eq!(file_name("notepad.exe"), "notepad.exe");
        }

        #[test]
        fn file_name_handles_empty() {
            assert_eq!(file_name(""), "");
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::ForegroundSample;
    use objc2_app_kit::NSWorkspace;
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};

    pub fn foreground_app() -> Option<ForegroundSample> {
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        let pid = app.processIdentifier();
        // AppKit reports -1 for an application that has no process.
        if pid <= 0 {
            return None;
        }
        let pid = pid as u32;

        // Resolved through `process`, not from the bundle name, so the string
        // matches what a rule is compared against.
        let exe_name = focuser_common::process::name_for_pid(pid)?;
        Some(ForegroundSample {
            exe_name,
            is_self: pid == std::process::id(),
        })
    }

    /// `kCGAnyInputEventType` is `~0` and has no generated constant, so the
    /// newtype is built from the value the header defines.
    const ANY_INPUT_EVENT: CGEventType = CGEventType(u32::MAX);

    pub fn user_idle_seconds() -> Option<u32> {
        let seconds = CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::CombinedSessionState,
            ANY_INPUT_EVENT,
        );
        // A negative or absurd reading means the API could not answer.
        (seconds.is_finite() && seconds >= 0.0).then_some(seconds as u32)
    }
}

/// X11 only, and deliberately so.
///
/// Wayland has no protocol for "which window is focused" — it was left out on
/// purpose, so that one app cannot watch another. GNOME needs a shell
/// extension and KDE offers nothing public, so there is no portable answer to
/// implement. Under Wayland this reports nothing rather than reporting
/// something wrong, and `session::app_usage_measurable` lets the app say so
/// instead of leaving the user wondering why a timer never moves.
#[cfg(target_os = "linux")]
mod linux {
    use super::ForegroundSample;
    use x11rb::connection::Connection;
    use x11rb::protocol::screensaver::ConnectionExt as _;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
    use x11rb::rust_connection::RustConnection;

    /// A connection per tick. The handshake is a local socket round trip every
    /// five seconds, which is cheaper than holding one open across a suspend
    /// or an X server restart and having to notice it died.
    fn connect() -> Option<(RustConnection, u32)> {
        if !focuser_common::session::app_usage_measurable() {
            return None;
        }
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        Some((conn, root))
    }

    fn atom(conn: &RustConnection, name: &[u8]) -> Option<u32> {
        Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom)
    }

    /// First 32-bit word of a window property, which is all these two hold.
    fn first_word(
        conn: &RustConnection,
        window: u32,
        property: u32,
        kind: AtomEnum,
    ) -> Option<u32> {
        conn.get_property(false, window, property, kind, 0, 1)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()
    }

    pub fn foreground_app() -> Option<ForegroundSample> {
        let (conn, root) = connect()?;

        let active = atom(&conn, b"_NET_ACTIVE_WINDOW")?;
        let window = first_word(&conn, root, active, AtomEnum::WINDOW)?;
        if window == 0 {
            return None;
        }

        // _NET_WM_PID is a convention, not a guarantee. A window without one
        // simply does not get counted.
        let wm_pid = atom(&conn, b"_NET_WM_PID")?;
        let pid = first_word(&conn, window, wm_pid, AtomEnum::CARDINAL)?;

        let exe_name = focuser_common::process::name_for_pid(pid)?;
        Some(ForegroundSample {
            exe_name,
            is_self: pid == std::process::id(),
        })
    }

    pub fn user_idle_seconds() -> Option<u32> {
        let (conn, root) = connect()?;
        let info = conn.screensaver_query_info(root).ok()?.reply().ok()?;
        Some(info.ms_since_user_input / 1000)
    }
}
