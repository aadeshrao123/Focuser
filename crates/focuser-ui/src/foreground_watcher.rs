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
    #[cfg(not(windows))]
    {
        None
    }
}

/// Seconds since last user input across the whole system.
/// `None` on platforms without an implementation or on API failure.
fn user_idle_seconds() -> Option<u32> {
    #[cfg(windows)]
    {
        win::user_idle_seconds()
    }
    #[cfg(not(windows))]
    {
        Some(0)
    }
}

/// Strip a full Windows path down to its file name component.
fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
}

#[cfg(windows)]
mod win {
    use super::{ForegroundSample, file_name};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

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
