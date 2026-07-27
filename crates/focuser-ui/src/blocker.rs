//! Background blocking loop — syncs hosts file, kills blocked processes,
//! and enforces browser extension installation.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use focuser_common::browser::identify_browser;
use focuser_common::extension::BrowserType;
use focuser_common::host::any_host_matches;
use focuser_common::process;
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

    // Cleanup old events on startup (keep 30 days)
    if let Ok(eng) = state.engine.lock() {
        match eng.db().cleanup_old_events(30) {
            Ok(n) if n > 0 => info!(deleted = n, "Cleaned up old blocked events"),
            _ => {}
        }
    }

    // Bootstrap allowance tracker from DB on startup
    if let Ok(eng) = state.engine.lock() {
        let _ = state.allowance_tracker.rebuild_from_db(eng.db());
    }

    let mut pomodoro_runtime = focuser_core::pomodoro::PomodoroRuntime::new();
    let mut heavy_tick_counter: u8 = 0;

    loop {
        thread::sleep(Duration::from_secs(1));
        heavy_tick_counter = heavy_tick_counter.wrapping_add(1);
        let run_heavy = heavy_tick_counter.is_multiple_of(3);

        // Pomodoro tick — advances phase and toggles block list enabled.
        // Evaluated every 1s so phase changes feel responsive.
        if let Ok(mut eng) = state.engine.lock() {
            match focuser_core::pomodoro::tick(&mut eng, &mut pomodoro_runtime) {
                Ok(focuser_core::pomodoro::TickOutcome::PhaseAdvanced { to, cycle, .. }) => {
                    state.push_pomodoro_event(crate::PomodoroEvent::PhaseAdvanced {
                        to: to.as_str().to_string(),
                        cycle,
                    });
                }
                Ok(focuser_core::pomodoro::TickOutcome::TamperDetected) => {
                    state.push_pomodoro_event(crate::PomodoroEvent::TamperDetected);
                }
                _ => {}
            }
        }

        if !run_heavy {
            continue;
        }

        // Refresh engine cache (every ~3s)
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
                let mut domains = eng.collect_blocked_domains();
                // Add allowance-exhausted domains to the hosts set.
                domains.extend(state.allowance_tracker.blocked_domains());
                // Remove domains that currently have an active (non-exhausted)
                // allowance — they should be reachable until the daily quota
                // runs out. Matches any subdomain too.
                let exceptions = state.allowance_exempt_domains(&eng);
                if !exceptions.is_empty() {
                    domains.retain(|d| !any_host_matches(&exceptions, d));
                }
                domains.sort();
                domains.dedup();
                sync_hosts_file(&domains);
            }

            // Kill blocked processes
            kill_blocked_processes(&eng, &state.allowance_tracker);
            // Also kill apps whose allowance is exhausted today.
            kill_allowance_blocked_apps(&state.allowance_tracker);

            // Browser extension enforcement. Settings are read on every heavy
            // tick so a change in the UI applies without restarting the app.
            let (grace_duration, enforce_enabled) = enforcement_settings(eng.db());
            if enforce_enabled {
                let has_active_blocks = eng.block_lists().iter().any(|l| l.is_effectively_active());
                enforce_browser_extension(has_active_blocks, grace_duration, &mut grace_periods);
            }
        }
    }
}

/// Grace period and whether unsupported browsers get closed at all.
fn enforcement_settings(db: &focuser_core::db::Database) -> (Duration, bool) {
    let grace = db
        .get_setting_or_default("extension_grace_period", "60")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_GRACE_PERIOD_SECS);
    let enabled = db
        .get_setting_or_default("block_unsupported_browsers", "true")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);

    (Duration::from_secs(grace), enabled)
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

/// Kill processes matching an app rule on an active block list.
fn kill_blocked_processes(
    eng: &focuser_core::BlockEngine,
    tracker: &focuser_core::allowance::AllowanceTracker,
) {
    // An app still inside its daily quota is exempt. Blocking it here would
    // defeat the allowance, which exists precisely to permit some use.
    let exempt: HashSet<String> = tracker
        .active_allowance_apps(eng.db())
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();

    for proc in process::list() {
        if !proc.is_killable() || exempt.contains(&proc.name.to_ascii_lowercase()) {
            continue;
        }
        let Some(list_name) = eng.check_app(&proc.name, None, None) else {
            continue;
        };
        if process::terminate(proc.pid) {
            info!(pid = proc.pid, name = %proc.name, list = %list_name, "Killed blocked process");
            let _ = eng.record_blocked(&proc.name);
        }
    }
}

/// Kill processes whose executable name matches an allowance that is
/// exhausted for today.
fn kill_allowance_blocked_apps(tracker: &focuser_core::allowance::AllowanceTracker) {
    let exhausted: HashSet<String> = tracker
        .blocked_apps()
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if exhausted.is_empty() {
        return;
    }

    for proc in process::list() {
        if !proc.is_killable() || !exhausted.contains(&proc.name.to_ascii_lowercase()) {
            continue;
        }
        if process::terminate(proc.pid) {
            info!(pid = proc.pid, name = %proc.name, "Killed app over allowance quota");
        }
    }
}

/// Enforce browser extension installation.
///
/// If active blocks exist and a browser is running without the Focuser extension
/// connected, start a grace period. After the grace period expires, kill the browser.
///
/// "Connected" means the extension polled `/api/rules` recently — that HTTP
/// call is the only signal we get, so a browser is judged by whether its
/// extension has been in touch, not by asking the browser anything.
fn enforce_browser_extension(
    has_active_blocks: bool,
    grace_duration: Duration,
    grace_periods: &mut HashMap<BrowserType, Instant>,
) {
    if !has_active_blocks {
        grace_periods.clear();
        return;
    }

    let now = Instant::now();

    let mut running: HashMap<BrowserType, Vec<u32>> = HashMap::new();
    for proc in process::list() {
        if !proc.is_killable() {
            continue;
        }
        if let Some(info) = identify_browser(&proc.name) {
            running
                .entry(info.browser_type.clone())
                .or_default()
                .push(proc.pid);
        }
    }

    // Generous 2-minute window: extensions use chrome.alarms, which fires
    // every 30s once the service worker sleeps. Anything tighter would call a
    // healthy extension dead during a slow startup or a suspended worker.
    let connected = crate::api::get_connected_browsers(120);

    for (browser, pids) in &running {
        if connected.contains(browser) {
            grace_periods.remove(browser);
            continue;
        }

        let Some(started_at) = grace_periods.get(browser) else {
            warn!(
                browser = ?browser,
                grace_secs = grace_duration.as_secs(),
                "Browser running without Focuser extension — grace period started"
            );
            grace_periods.insert(browser.clone(), now);
            continue;
        };

        if now.duration_since(*started_at) < grace_duration {
            continue;
        }

        // Last look before closing anything, with a wider window still: the
        // extension may have been installed mid-grace and be warming up.
        if crate::api::get_connected_browsers(180).contains(browser) {
            info!(
                browser = ?browser,
                "Extension connected during grace period — cancelling termination"
            );
            grace_periods.remove(browser);
            continue;
        }

        info!(
            browser = ?browser,
            pid_count = pids.len(),
            "Grace period expired — terminating browser without extension"
        );
        for &pid in pids {
            process::terminate(pid);
        }

        let name = focuser_common::browser::KNOWN_BROWSERS
            .iter()
            .find(|b| b.browser_type == *browser)
            .map(|b| b.display_name)
            .unwrap_or("your browser");
        crate::api::set_killed_browser(name);
        crate::api::SHOW_WINDOW_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);

        // Reset so the grace period restarts if the browser is relaunched.
        grace_periods.remove(browser);
    }

    grace_periods.retain(|browser, _| running.contains_key(browser));
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
