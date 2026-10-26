//! Hosts/process blocking and a separate, faster browser-extension guard.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use focuser_common::browser::identify_browser;
use focuser_common::extension::BrowserType;
use focuser_common::host::any_host_matches;
use focuser_common::process;
use focuser_common::uninstall;
use tracing::{info, warn};

use crate::AppState;

const HOSTS_BEGIN: &str = "# ──── BEGIN FOCUSER BLOCK ────";
const HOSTS_END: &str = "# ──── END FOCUSER BLOCK ────";

/// Runs the blocking loop in a background thread.
/// Every 3 seconds: re-sync hosts file and check for blocked processes.
/// Browser coverage is checked independently by `run_browser_guard`.
pub fn run_blocking_loop(state: Arc<AppState>) {
    info!("Background blocker started");

    let mut was_using_hosts = true;
    let mut cleared_processes = HashSet::new();

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
                    crate::sound::phase_changed(eng.db(), to);
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
        let mut watch_uninstalls = false;
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
            kill_allowance_blocked_apps(&state.allowance_tracker, &eng);

            // Uninstall protection. Only while a lock actually asks for it.
            watch_uninstalls = eng.has_uninstall_protection();
        }

        // After the engine lock is released, never inside it. Every command
        // from the window and every request from the extension waits for that
        // lock, and reading command lines is the one step here whose cost
        // depends on what else the machine is running.
        if watch_uninstalls {
            block_uninstall_attempts(&mut cleared_processes);
        }
    }
}

/// Grace period and whether unsupported browsers get closed at all.
fn enforcement_settings(db: &focuser_core::db::Database) -> (Duration, bool) {
    use focuser_app::execute::{
        DEFAULT_GRACE_PERIOD_SECS, SETTING_CLOSE_BROWSERS, SETTING_GRACE_PERIOD,
    };

    let grace = db
        .get_setting(SETTING_GRACE_PERIOD)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_GRACE_PERIOD_SECS);
    let enabled = db
        .get_setting_or_default(SETTING_CLOSE_BROWSERS, "true")
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

/// Whether the OS hosts file can be written right now.
///
/// Opened for append and immediately dropped: that needs the same privileges
/// as a real write but changes nothing, so it is safe to call on a timer.
pub fn hosts_writable() -> bool {
    std::fs::OpenOptions::new()
        .append(true)
        .open(hosts_path())
        .is_ok()
}

/// Whether the last hosts write failed. Read by the blocking-health command so
/// the UI can say that rules are armed but not actually in force.
static HOSTS_WRITE_FAILED: AtomicBool = AtomicBool::new(false);

fn sync_hosts_file(domains: &[String]) {
    let path = hosts_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let new_content = replace_section(&content, domains);
    if content == new_content {
        return;
    }

    match std::fs::write(&path, &new_content) {
        Ok(()) => {
            // Only log the recovery, not every successful write.
            if HOSTS_WRITE_FAILED.swap(false, Ordering::Relaxed) {
                info!("Hosts file is writable again");
            }
            flush_dns();
        }
        Err(e) => {
            // Almost always "not running as administrator". Logged once per
            // spell rather than every three seconds, but no longer swallowed —
            // this is the difference between blocking and only looking like it.
            if !HOSTS_WRITE_FAILED.swap(true, Ordering::Relaxed) {
                warn!(
                    path = %path,
                    error = %e,
                    "Cannot write the hosts file — website blocking will not take effect                      without the browser extension. Run Focuser as administrator."
                );
            }
        }
    }
}

/// Close anything that looks like it is uninstalling Focuser.
///
/// Only reached while a lock has `prevent_uninstall` set. The detection is
/// deliberately narrow — see [`focuser_common::uninstall`] — because the
/// alternative is killing an uninstaller the user aimed at other software.
fn block_uninstall_attempts(cleared: &mut HashSet<(u32, String)>) {
    for pid in uninstall_attempts(&process::list(), cleared, process::cmdline) {
        if process::terminate(pid) {
            warn!(pid, "Closed an uninstall attempt — a lock is active");
        }
    }
}

/// `cleared` remembers processes already read and found harmless: a command
/// line never changes, and on macOS each read starts `ps` (#12).
fn uninstall_attempts(
    procs: &[process::Process],
    cleared: &mut HashSet<(u32, String)>,
    mut read_cmdline: impl FnMut(u32) -> Option<String>,
) -> Vec<u32> {
    cleared.retain(|(pid, name)| procs.iter().any(|p| p.pid == *pid && p.name == *name));

    uninstall::detect(procs, |pid| {
        let key = (pid, procs.iter().find(|p| p.pid == pid)?.name.clone());
        if cleared.contains(&key) {
            return None;
        }
        let line = read_cmdline(pid);
        if !uninstall::is_uninstall_attempt(&key.1, line.as_deref()) {
            cleared.insert(key);
        }
        line
    })
}

/// Kill processes matching an app rule on an active block list.
fn kill_blocked_processes(
    eng: &focuser_core::BlockEngine,
    tracker: &focuser_core::allowance::AllowanceTracker,
) {
    // An app still inside its daily quota is exempt. Blocking it here would
    // defeat the allowance, which exists precisely to permit some use.
    let exempt: HashSet<String> = tracker
        .active_allowance_apps(eng)
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
fn kill_allowance_blocked_apps(
    tracker: &focuser_core::allowance::AllowanceTracker,
    eng: &focuser_core::BlockEngine,
) {
    let exhausted: HashSet<String> = tracker
        .blocked_apps()
        .into_iter()
        .filter(|exe| !eng.shared_covers_app(exe))
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

/// Check browser-owned state twice a second, independently of the heavy loop.
/// Heartbeats remain the fallback for unreadable/unsupported profiles; a sleepy
/// MV3 worker alone must not be mistaken for an uninstall.
pub fn run_browser_guard(state: Arc<AppState>) {
    let mut guard = BrowserGuard::new();
    loop {
        // A failed policy restore cancels quitting. Stay alive so enforcement
        // resumes in that case instead of silently losing the guard thread.
        if crate::browser_recovery::is_stopping() {
            thread::sleep(Duration::from_millis(500));
            continue;
        }
        let settings = state.engine.lock().ok().map(|engine| {
            let (grace, enabled) = enforcement_settings(engine.db());
            (
                enabled
                    && engine
                        .block_lists()
                        .iter()
                        .any(|l| l.is_effectively_active()),
                grace,
            )
        });
        if let Some((active, grace)) = settings {
            guard.enforce(active, grace);
        }
        thread::sleep(Duration::from_millis(500));
    }
}

struct BrowserGuard {
    monitor: focuser_common::extension_watch::ProfileMonitor,
    profiles: HashMap<BrowserType, Vec<focuser_common::extension_watch::BrowserProfile>>,
    grace_periods: HashMap<BrowserType, Grace>,
    recovery: HashSet<BrowserType>,
    needs_restart: HashSet<BrowserType>,
    last_failure: HashMap<BrowserType, Instant>,
}

impl BrowserGuard {
    fn new() -> Self {
        let recovery: HashSet<_> = crate::browser_recovery::pending().into_iter().collect();
        // Journals survive a crash/restart, heartbeats do not. Require a new
        // check-in before releasing any previously installed restriction.
        for browser in &recovery {
            crate::api::forget_browser(browser);
        }
        Self {
            monitor: Default::default(),
            profiles: HashMap::new(),
            grace_periods: HashMap::new(),
            needs_restart: recovery.clone(),
            recovery,
            last_failure: HashMap::new(),
        }
    }

    fn enforce(&mut self, active: bool, grace_duration: Duration) {
        if crate::browser_recovery::is_stopping() {
            return;
        }
        use focuser_common::extension_watch::Coverage;
        let running = browser_main_processes(&process::list(), process::cmdline);
        if !active {
            // Restore policies when the schedule ends or enforcement is
            // disabled, even if the user has already closed the browser.
            for browser in crate::browser_recovery::pending() {
                if let Some(profile) = crate::browser_recovery::saved_profile(&browser) {
                    self.profiles
                        .entry(browser.clone())
                        .or_insert_with(|| vec![profile]);
                }
                if let Err(error) = crate::browser_recovery::restore(&browser) {
                    warn!(%error, ?browser, "Could not restore browser policies");
                    continue;
                }
                self.recovery.remove(&browser);
                self.needs_restart.remove(&browser);
                if let Some(pids) = running.get(&browser) {
                    self.restart(&browser, pids, "about:blank");
                }
            }
            self.grace_periods.clear();
            self.profiles.clear();
            return;
        }
        let safely_connected = crate::api::get_safely_connected_browsers(120);
        let connected = crate::api::get_connected_browsers(120);
        let recovery_ready = crate::api::get_recovery_ready_browsers();
        for (browser, pids) in &running {
            let commands: Vec<_> = pids
                .iter()
                .filter_map(|pid| process::cmdline(*pid))
                .collect();
            let discovered = self.monitor.profiles(browser, &commands);
            let watched = self.profiles.entry(browser.clone()).or_default();
            for profile in discovered {
                if !watched.contains(&profile) {
                    watched.push(profile);
                }
            }
            if let Some(saved) = crate::browser_recovery::saved_profile(browser)
                && !watched.contains(&saved)
            {
                watched.push(saved);
            }
            if self.needs_restart.contains(browser) {
                if let Err(error) = crate::browser_recovery::install(browser, watched.first()) {
                    for &pid in pids {
                        process::terminate(pid);
                    }
                    warn!(?browser, %error, "Could not reapply interrupted browser recovery");
                    continue;
                }
                self.needs_restart.remove(browser);
                let store = focuser_common::browser::KNOWN_BROWSERS
                    .iter()
                    .find(|b| b.browser_type == *browser)
                    .map(|b| b.store_url())
                    .unwrap_or("");
                if !self.restart(browser, pids, store) {
                    self.needs_restart.insert(browser.clone());
                }
                continue;
            }
            let missing_index = watched.iter().position(|profile| {
                self.monitor
                    .coverage(profile, *browser == BrowserType::Firefox)
                    == Coverage::Missing
            });
            let missing = missing_index.is_some();
            // Install into the profile that lost the extension, not whichever
            // profile happens to sort first (which might already be covered).
            if let Some(index) = missing_index {
                watched.swap(0, index);
            }
            let recovering = self.recovery.contains(browser);
            if recovering {
                if !missing && recovery_ready.contains(browser) {
                    if let Err(error) = crate::browser_recovery::restore(browser) {
                        warn!(%error, ?browser, "Could not leave installation-only recovery");
                        continue;
                    }
                    self.recovery.remove(browser);
                    self.grace_periods.remove(browser);
                    self.restart(browser, pids, "about:blank");
                    info!(?browser, "Extension reconnected; normal browsing restored");
                }
                continue;
            }
            if !missing && safely_connected.contains(browser) {
                self.grace_periods.remove(browser);
                continue;
            }
            let name = focuser_common::browser::KNOWN_BROWSERS
                .iter()
                .find(|b| b.browser_type == *browser)
                .map(|b| b.display_name)
                .unwrap_or("your browser");
            let reason = if !missing && connected.contains(browser) {
                crate::api::ClosedReason::IncognitoNotAllowed
            } else {
                crate::api::ClosedReason::NotInstalled
            };
            let grace = self.grace_periods.entry(browser.clone()).or_insert(Grace {
                started: Instant::now(),
                warned: false,
            });
            let waited = grace.started.elapsed();
            match enforcement_step(missing, waited, grace_duration, grace.warned) {
                GraceStep::Wait => continue,
                GraceStep::Warn => {
                    grace.warned = true;
                    crate::api::prompt_about_browser(crate::api::BrowserPrompt {
                        browser: name.into(),
                        reason,
                        closing_in_secs: Some(grace_duration.saturating_sub(waited).as_secs()),
                    });
                    continue;
                }
                GraceStep::Close => {}
            }
            let profile = watched.first().cloned();
            // Never reopen an unrestricted browser if installing the policy
            // failed (for example on an unsupported/enterprise-managed browser).
            match crate::browser_recovery::install(browser, profile.as_ref()) {
                Ok(()) => {
                    self.recovery.insert(browser.clone());
                    let store = focuser_common::browser::KNOWN_BROWSERS
                        .iter()
                        .find(|b| b.browser_type == *browser)
                        .map(|b| b.store_url())
                        .unwrap_or("");
                    let destination = if reason == crate::api::ClosedReason::IncognitoNotAllowed {
                        crate::extension_settings_url(name)
                    } else {
                        store
                    };
                    if !self.restart(browser, pids, destination) {
                        self.needs_restart.insert(browser.clone());
                    }
                    info!(
                        ?browser,
                        "Extension missing; browser restricted to installation"
                    );
                }
                Err(error) => {
                    if crate::browser_recovery::is_stopping() {
                        return;
                    }
                    if crate::browser_recovery::pending().contains(browser) {
                        self.recovery.insert(browser.clone());
                        self.needs_restart.insert(browser.clone());
                    }
                    for &pid in pids {
                        process::terminate(pid);
                    }
                    crate::api::forget_browser(browser);
                    if self
                        .last_failure
                        .get(browser)
                        .is_none_or(|at| at.elapsed() >= Duration::from_secs(30))
                    {
                        warn!(?browser, %error, "Recovery unavailable; uncovered browser kept closed");
                        crate::api::prompt_about_browser(crate::api::BrowserPrompt {
                            browser: name.into(),
                            reason,
                            closing_in_secs: None,
                        });
                        self.last_failure.insert(browser.clone(), Instant::now());
                    }
                }
            }
        }
        self.grace_periods
            .retain(|browser, _| running.contains_key(browser));
        self.profiles
            .retain(|browser, _| running.contains_key(browser) || self.recovery.contains(browser));
    }

    fn restart(&self, browser: &BrowserType, pids: &[u32], url: &str) -> bool {
        if crate::browser_recovery::is_stopping() {
            return false;
        }
        for &pid in pids {
            process::terminate(pid);
        }
        // A launch while the old main process still owns the profile would
        // forward to that process, leaving its old policy cache in force.
        for _ in 0..20 {
            if process::list().iter().all(|p| !pids.contains(&p.pid)) {
                crate::api::forget_browser(browser);
                let profile = self
                    .profiles
                    .get(browser)
                    .and_then(|p| p.first())
                    .cloned()
                    .or_else(|| crate::browser_recovery::saved_profile(browser));
                let args = profile
                    .map(|p| p.launch_args(*browser == BrowserType::Firefox))
                    .unwrap_or_default();
                if let Some(info) = focuser_common::browser::KNOWN_BROWSERS
                    .iter()
                    .find(|b| b.browser_type == *browser)
                    && let Err(error) =
                        crate::native::launch_browser(info.launch_name(), url, &args)
                {
                    warn!(%error, ?browser, "Could not reopen browser");
                    return false;
                }
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        warn!(
            ?browser,
            "Browser did not exit; refused an unrestricted relaunch"
        );
        false
    }
}

fn enforcement_step(
    confirmed_missing: bool,
    waited: Duration,
    grace: Duration,
    warned: bool,
) -> GraceStep {
    if confirmed_missing {
        GraceStep::Close
    } else {
        grace_step(waited, grace, warned)
    }
}

/// How long a browser has been running without cover, and whether the user
/// has been told.
struct Grace {
    started: Instant,
    warned: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum GraceStep {
    Wait,
    Warn,
    Close,
}

/// A browser that has only just started has not heard from its extension yet,
/// so its first seconds say nothing. Warning then would be a false alarm on
/// every start.
const SETTLE: Duration = Duration::from_secs(15);

/// What to do about a browser that has run without cover for `waited`.
///
/// It is closed when the grace period ends. Before that the user is told
/// once, with time left to fix it: closing a browser loses work, and nobody
/// should learn why only afterwards.
fn grace_step(waited: Duration, grace: Duration, warned: bool) -> GraceStep {
    if waited >= grace {
        GraceStep::Close
    } else if !warned && waited >= SETTLE {
        GraceStep::Warn
    } else {
        GraceStep::Wait
    }
}

/// The main process of every running browser.
///
/// Chromium's renderer, GPU, zygote, and utility subprocesses exec the same
/// binary as the browser itself, so on Linux and Windows they report the same
/// name as the top-level process. Killing them individually bypasses the
/// browser's own shutdown path entirely and reads as a crash, not a close.
/// Every subprocess carries `--type=...`; the top-level process never does.
/// Firefox's equivalent is `-contentproc`: its children usually rename
/// themselves ("Web Content", "forkserver"), but not reliably before we look.
fn browser_main_processes(
    procs: &[process::Process],
    mut read_cmdline: impl FnMut(u32) -> Option<String>,
) -> HashMap<BrowserType, Vec<u32>> {
    // On macOS a helper runs from its own bundle under its own name ("Google
    // Chrome Helper (Renderer)", "plugin-container"), so the name already
    // picks out the main process, and every command line read starts `ps`.
    let helpers_share_the_name = !cfg!(target_os = "macos");

    let mut running: HashMap<BrowserType, Vec<u32>> = HashMap::new();
    for proc in procs.iter().filter(|p| p.is_killable()) {
        let Some(info) = identify_browser(&proc.name) else {
            continue;
        };
        if helpers_share_the_name
            && read_cmdline(proc.pid).is_some_and(|cmd| is_browser_subprocess(&cmd))
        {
            continue;
        }
        running
            .entry(info.browser_type.clone())
            .or_default()
            .push(proc.pid);
    }
    running
}

/// Whether a browser process command line belongs to a child process
/// (Chromium `--type=...`, Firefox `-contentproc`) rather than the browser
/// itself.
fn is_browser_subprocess(cmdline: &str) -> bool {
    cmdline.contains("--type=") || cmdline.contains("-contentproc")
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

#[cfg(test)]
mod tests {
    use super::is_browser_subprocess;

    #[test]
    fn a_browser_main_process_is_not_a_subprocess() {
        assert!(!is_browser_subprocess(
            "/app/brave/brave --no-default-browser-check"
        ));
        assert!(!is_browser_subprocess("/usr/lib/firefox/firefox-bin"));
    }

    #[test]
    fn chromium_and_firefox_children_are_subprocesses() {
        assert!(is_browser_subprocess(
            "/app/brave/brave --type=zygote --no-zygote-sandbox"
        ));
        assert!(is_browser_subprocess(
            "/opt/google/chrome/chrome --type=renderer"
        ));
        assert!(is_browser_subprocess(
            "/usr/lib/firefox/firefox-bin -contentproc -childID 3 tab"
        ));
    }

    use super::*;
    use focuser_common::process::Process;

    const MINUTE: Duration = Duration::from_secs(60);
    const fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    /// A browser that has only just started has not heard from its extension
    /// yet. Telling the user then would be a false alarm on every start.
    #[test]
    fn a_browser_that_has_just_started_is_given_time_to_report() {
        assert_eq!(grace_step(secs(0), MINUTE, false), GraceStep::Wait);
        assert_eq!(grace_step(secs(9), MINUTE, false), GraceStep::Wait);
    }

    #[test]
    fn the_user_is_warned_once_while_there_is_still_time_to_fix_it() {
        assert_eq!(grace_step(SETTLE, MINUTE, false), GraceStep::Warn);
        assert_eq!(grace_step(secs(30), MINUTE, false), GraceStep::Warn);
        assert_eq!(grace_step(secs(30), MINUTE, true), GraceStep::Wait);
    }

    #[test]
    fn the_browser_is_closed_when_the_grace_period_is_over() {
        assert_eq!(grace_step(MINUTE, MINUTE, true), GraceStep::Close);
        assert_eq!(grace_step(secs(61), MINUTE, false), GraceStep::Close);
    }

    /// Someone who set the grace period this short asked for a quick close.
    #[test]
    fn a_grace_period_shorter_than_the_settle_time_closes_with_no_warning() {
        assert_eq!(grace_step(secs(3), secs(5), false), GraceStep::Wait);
        assert_eq!(grace_step(secs(6), secs(5), false), GraceStep::Close);
    }

    #[test]
    fn confirmed_removal_bypasses_even_a_long_grace_period() {
        assert_eq!(
            enforcement_step(true, secs(0), secs(3600), false),
            GraceStep::Close
        );
        assert_eq!(
            enforcement_step(false, secs(0), secs(3600), false),
            GraceStep::Wait
        );
    }

    #[test]
    fn a_browser_is_closed_through_its_main_process_only() {
        let name = focuser_common::browser::KNOWN_BROWSERS[0].exe_names[0];
        let procs = [10, 11].map(|pid| Process {
            pid,
            name: name.into(),
        });
        let mut reads = 0;
        let running = browser_main_processes(&procs, |pid| {
            reads += 1;
            Some(
                if pid == 11 {
                    "browser --type=renderer"
                } else {
                    "browser"
                }
                .to_string(),
            )
        });
        let pids = running.into_values().next().unwrap_or_default();

        if cfg!(target_os = "macos") {
            // A helper has its own name there, so both of these are main
            // processes, and nothing had to start `ps` to find that out.
            assert_eq!((pids, reads), (vec![10, 11], 0));
        } else {
            assert_eq!(pids, [10]);
        }
    }

    #[test]
    fn a_harmless_shell_is_read_once_and_an_uninstall_every_time() {
        let (shell, removal) = if cfg!(windows) {
            ("cmd.exe", r"cmd.exe /c rd /s /q %LOCALAPPDATA%\Focuser")
        } else if cfg!(target_os = "macos") {
            ("bash", "bash -c rm -rf /Applications/Focuser.app")
        } else {
            ("bash", "bash -c rm -f /usr/bin/focuser-ui")
        };
        let procs = [7, 9].map(|pid| Process {
            pid,
            name: shell.into(),
        });
        let mut cleared = HashSet::new();
        let mut reads = Vec::new();
        let mut read = |pid| {
            reads.push(pid);
            Some(if pid == 9 { removal } else { "dir" }.to_string())
        };

        assert_eq!(uninstall_attempts(&procs, &mut cleared, &mut read), [9]);
        assert_eq!(uninstall_attempts(&procs, &mut cleared, &mut read), [9]);

        // Gone, so a new process reusing pid 7 gets read afresh.
        uninstall_attempts(&procs[1..], &mut cleared, &mut read);
        assert!(cleared.is_empty());

        assert_eq!(reads, [7, 9, 9, 9], "the harmless shell was read again");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_script_that_only_mentions_uninstalling_focuser_is_left_running() {
        // Killed with exit 144 on 2026-10-03 while a list was locked.
        let procs = [Process {
            pid: 7,
            name: "bash".into(),
        }];
        let line = "bash -c python3 - <<'EOF'\nimport sqlite3\n\
                    db = sqlite3.connect('/home/u/.local/share/focuser/focuser.db')\n\
                    db.execute('UPDATE block_lists SET prevent_uninstall = 0')\n\
                    db.execute('DELETE FROM blocks')\nEOF";
        let mut cleared = HashSet::new();
        let found = uninstall_attempts(&procs, &mut cleared, |_| Some(line.to_string()));
        assert!(found.is_empty());
        assert!(cleared.contains(&(7, "bash".to_string())));
    }
}
