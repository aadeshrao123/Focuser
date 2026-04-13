//! Background blocking loop — syncs hosts file, kills blocked processes,
//! and enforces browser extension installation.

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use focuser_common::browser::identify_browser;
use focuser_common::extension::BrowserType;
use tracing::{info, warn};

use crate::AppState;

const HOSTS_BEGIN: &str = "# ──── BEGIN FOCUSER BLOCK ────";
const HOSTS_END: &str = "# ──── END FOCUSER BLOCK ────";

/// Default grace period before killing browsers without the extension.
const DEFAULT_GRACE_PERIOD_SECS: u64 = 60;

/// Runs the blocking loop in a background thread.
/// Every 3 seconds: re-sync hosts file, check for blocked processes,
/// and enforce browser extension installation.
pub fn run_blocking_loop(state: Arc<AppState>) {
    info!("Background blocker started");

    // Browser enforcement state
    let mut grace_periods: HashMap<BrowserType, Instant> = HashMap::new();
    let mut was_using_hosts = true;

    // Read settings
    let (grace_secs, enforce_enabled) = {
        let eng = state.engine.lock().unwrap();
        let grace = eng
            .db()
            .get_setting_or_default("extension_grace_period", "60")
            .unwrap_or_else(|_| "60".to_string())
            .parse::<u64>()
            .unwrap_or(DEFAULT_GRACE_PERIOD_SECS);
        let enabled = eng
            .db()
            .get_setting_or_default("block_unsupported_browsers", "true")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true);
        (grace, enabled)
    };
    let grace_duration = Duration::from_secs(grace_secs);

    if enforce_enabled {
        info!(grace_secs, "Browser extension enforcement enabled");
    }

    // Cleanup old events on startup (keep 30 days)
    if let Ok(eng) = state.engine.lock() {
        match eng.db().cleanup_old_events(30) {
            Ok(n) if n > 0 => info!(deleted = n, "Cleaned up old blocked events"),
            _ => {}
        }
    }

    loop {
        thread::sleep(Duration::from_secs(3));

        // Refresh engine cache
        if let Ok(mut eng) = state.engine.lock() {
            let _ = eng.refresh();

            // Note: schedule enforcement is handled at rule compile time via
            // BlockList::is_effectively_active(), which checks both the user's
            // enabled flag AND the schedule. We no longer mutate `enabled` based
            // on the schedule — that would conflict with the user's manual toggle.

            // Sync hosts file — but skip if any browser extension is connected,
            // because the extension provides a better experience (custom block page)
            // while the hosts file just shows a connection error.
            let any_extension_connected = !crate::api::get_connected_browsers(120).is_empty();
            if any_extension_connected {
                // Extension handles blocking — clear hosts file so extension can show block page
                if was_using_hosts {
                    // Just switched from hosts to extension — force clear and flush DNS
                    info!(
                        "Extension connected — switching from hosts file to extension-based blocking"
                    );
                    let _ = remove_hosts_blocks();
                    was_using_hosts = false;
                } else {
                    sync_hosts_file(&[]);
                }
            } else {
                // No extension — use hosts file as fallback
                was_using_hosts = true;
                let domains = eng.collect_blocked_domains();
                sync_hosts_file(&domains);
            }

            // Kill blocked processes
            kill_blocked_processes(&eng);

            // Browser extension enforcement
            if enforce_enabled {
                let has_active_blocks = eng.block_lists().iter().any(|l| l.is_effectively_active());
                enforce_browser_extension(has_active_blocks, grace_duration, &mut grace_periods);
            }
        }
    }
}

/// Apply blocks to the system hosts file.
pub fn apply_hosts_blocks(domains: &[String]) -> Result<(), String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let new_content = replace_section(&content, domains);
    std::fs::write(&path, &new_content)
        .map_err(|e| format!("Cannot write {path}: {e}. Run as administrator."))?;
    flush_dns();
    info!(count = domains.len(), "Hosts file updated");
    Ok(())
}

/// Remove all Focuser entries from hosts file.
pub fn remove_hosts_blocks() -> Result<(), String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let new_content = replace_section(&content, &[]);
    std::fs::write(&path, &new_content)
        .map_err(|e| format!("Cannot write {path}: {e}. Run as administrator."))?;
    flush_dns();
    info!("Hosts file cleaned");
    Ok(())
}

fn sync_hosts_file(domains: &[String]) {
    let path = hosts_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let new_content = replace_section(&content, domains);
    if content != new_content {
        if let Err(e) = std::fs::write(&path, &new_content) {
            // Silently fail if not admin — warn is done once above
            let _ = e;
        } else {
            flush_dns();
        }
    }
}

fn kill_blocked_processes(_eng: &focuser_core::BlockEngine) {
    #[cfg(windows)]
    {
        kill_blocked_processes_windows(_eng);
    }
}

#[cfg(windows)]
fn kill_blocked_processes_windows(eng: &focuser_core::BlockEngine) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::System::Threading::*;

    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return,
        };

        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };

        if Process32First(snapshot, &mut entry).is_ok() {
            loop {
                let name: String = entry
                    .szExeFile
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();

                if let Some(list_name) = eng.check_app(&name, None, None) {
                    let pid = entry.th32ProcessID;
                    // Don't kill ourselves or system processes
                    #[allow(clippy::collapsible_if)]
                    if pid > 4 && pid != std::process::id() {
                        if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                            let _ = TerminateProcess(handle, 1);
                            let _ = CloseHandle(handle);
                            info!(pid, name = %name, list = %list_name, "Killed blocked process");
                            let _ = eng.record_blocked(&name);
                        }
                    }
                }

                if Process32Next(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }
}

/// Enforce browser extension installation.
///
/// If active blocks exist and a browser is running without the Focuser extension
/// connected, start a grace period. After the grace period expires, kill the browser.
///
/// Note: Since the Tauri app communicates with the extension via HTTP API (port 17549),
/// we track connected extensions via a simple "has the extension polled recently" check.
/// The extension polls /api/rules every 2 seconds. If no poll in 10 seconds, it's gone.
fn enforce_browser_extension(
    has_active_blocks: bool,
    grace_duration: Duration,
    grace_periods: &mut HashMap<BrowserType, Instant>,
) {
    if !has_active_blocks {
        grace_periods.clear();
        return;
    }

    // For now, the extension connects via HTTP — we can't easily distinguish which
    // browser's extension is connected. So we check if ANY extension is connected
    // by seeing if the API server has been polled recently.
    // The native messaging host (focuser-native) will send Connected events in the future.
    //
    // Current strategy: detect running browsers and enforce after grace period.
    // Extension connection tracking will be enhanced when native messaging is active.

    #[cfg(windows)]
    enforce_browser_extension_windows(has_active_blocks, grace_duration, grace_periods);
}

#[cfg(windows)]
fn enforce_browser_extension_windows(
    _has_active_blocks: bool,
    grace_duration: Duration,
    grace_periods: &mut HashMap<BrowserType, Instant>,
) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::System::Threading::*;

    let now = Instant::now();

    // Enumerate processes and find browsers
    let mut running_browsers: HashMap<BrowserType, Vec<u32>> = HashMap::new();

    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return,
        };

        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };

        if Process32First(snapshot, &mut entry).is_ok() {
            loop {
                let name: String = entry
                    .szExeFile
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();

                if let Some(browser_info) = identify_browser(&name) {
                    let pid = entry.th32ProcessID;
                    if pid > 4 && pid != std::process::id() {
                        running_browsers
                            .entry(browser_info.browser_type.clone())
                            .or_default()
                            .push(pid);
                    }
                }

                if Process32Next(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }

    // Check which browsers have a connected extension.
    // Generous 2-minute window: extensions use chrome.alarms which fires
    // every 30s when the service worker is asleep. We need at least 2x the
    // alarm period plus margin for slow startup, suspended SW, or hiccups.
    let connected_extensions = crate::api::get_connected_browsers(120);

    // Check each running browser
    for (browser_type, pids) in &running_browsers {
        if connected_extensions.contains(browser_type) {
            grace_periods.remove(browser_type);
            continue;
        }

        match grace_periods.get(browser_type) {
            None => {
                // Start grace period
                warn!(
                    browser = ?browser_type,
                    grace_secs = grace_duration.as_secs(),
                    "Browser running without Focuser extension — grace period started"
                );
                grace_periods.insert(browser_type.clone(), now);
            }
            Some(started_at) => {
                if now.duration_since(*started_at) >= grace_duration {
                    // Grace expired — final safety net with an even wider 3-minute window.
                    // This accounts for the case where the extension was just installed
                    // mid-grace and is still warming up its alarms.
                    let fresh_check = crate::api::get_connected_browsers(180);
                    if fresh_check.contains(browser_type) {
                        // Extension connected just in time — cancel kill
                        info!(
                            browser = ?browser_type,
                            "Extension connected during grace period — cancelling termination"
                        );
                        grace_periods.remove(browser_type);
                    } else {
                        // Confirmed: no extension — kill browser
                        info!(
                            browser = ?browser_type,
                            pid_count = pids.len(),
                            "Grace period expired — terminating browser without extension"
                        );

                        for &pid in pids {
                            unsafe {
                                if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                                    let _ = TerminateProcess(handle, 1);
                                    let _ = CloseHandle(handle);
                                }
                            }
                        }

                        // Show the Focuser app with "install extension" prompt
                        let browser_name = focuser_common::browser::KNOWN_BROWSERS
                            .iter()
                            .find(|b| b.browser_type == *browser_type)
                            .map(|b| b.display_name)
                            .unwrap_or("your browser");
                        crate::api::set_killed_browser(browser_name);
                        crate::api::SHOW_WINDOW_REQUESTED
                            .store(true, std::sync::atomic::Ordering::Relaxed);

                        // Reset so grace restarts if browser is relaunched
                        grace_periods.remove(browser_type);
                    }
                }
            }
        }
    }

    // Clean up grace periods for browsers no longer running
    grace_periods.retain(|bt, _| running_browsers.contains_key(bt));
}

fn hosts_path() -> String {
    #[cfg(windows)]
    {
        r"C:\Windows\System32\drivers\etc\hosts".into()
    }
    #[cfg(target_os = "macos")]
    {
        "/etc/hosts".into()
    }
    #[cfg(target_os = "linux")]
    {
        "/etc/hosts".into()
    }
}

fn replace_section(content: &str, domains: &[String]) -> String {
    let mut result = String::with_capacity(content.len() + domains.len() * 30);
    let mut in_section = false;

    for line in content.lines() {
        if line.trim() == HOSTS_BEGIN {
            in_section = true;
            continue;
        }
        if line.trim() == HOSTS_END {
            in_section = false;
            continue;
        }
        if !in_section {
            result.push_str(line);
            result.push('\n');
        }
    }

    if !domains.is_empty() {
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(HOSTS_BEGIN);
        result.push('\n');
        for domain in domains {
            result.push_str(&format!("127.0.0.1 {domain}\n"));
            result.push_str(&format!("::1 {domain}\n"));
        }
        result.push_str(HOSTS_END);
        result.push('\n');
    }

    result
}

fn flush_dns() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("ipconfig")
            .args(["/flushdns"])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("dscacheutil")
            .args(["-flushcache"])
            .output();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("systemd-resolve")
            .args(["--flush-caches"])
            .output();
    }
}
