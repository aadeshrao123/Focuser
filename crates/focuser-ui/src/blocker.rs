//! Background blocking loop — syncs hosts file, closes blocked processes,
//! and enforces browser extension installation.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use focuser_common::browser::identify_browser;
use focuser_common::extension::BrowserType;
use focuser_common::host::any_host_matches;
use focuser_common::process;
use focuser_common::types::AppMatchType;
use focuser_common::uninstall;
use tracing::{info, warn};

use crate::AppState;

const HOSTS_BEGIN: &str = "# ──── BEGIN FOCUSER BLOCK ────";
const HOSTS_END: &str = "# ──── END FOCUSER BLOCK ────";
/// For whoever opens the file after Focuser went without cleaning up, for
/// instance when an antivirus removed it (#54).
const HOSTS_NOTE: &str =
    "# Added by Focuser to block sites. If Focuser is gone, delete this block from BEGIN to END.";

/// Runs the blocking loop in a background thread.
/// Every 3 seconds: re-sync hosts file, check for blocked processes,
/// and enforce browser extension installation.
pub fn run_blocking_loop(state: Arc<AppState>) {
    info!("Background blocker started");

    // Browser enforcement state
    let mut grace_periods: HashMap<BrowserType, Grace> = HashMap::new();
    let mut was_using_hosts = true;
    let mut cleared_processes = HashSet::new();
    let mut closer = crate::closing::Closer::default();
    let mut warner = crate::closing::Warner::default();

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
        let mut enforce_browsers = None;
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

            // Close blocked apps, and those over today's allowance.
            let procs = process::list();
            let by_lists = blocked_by_lists(&eng, &state.allowance_tracker, &procs);
            let mut blocked: Vec<(u32, String)> = by_lists
                .iter()
                .map(|(pid, name, _)| (*pid, name.clone()))
                .collect();
            blocked.extend(over_allowance(&state.allowance_tracker, &eng, &procs));
            let stopped = closer.close(
                &blocked,
                Instant::now(),
                process::ask_to_close,
                process::terminate,
            );
            for name in stopped {
                if let Some((_, _, list)) = by_lists.iter().find(|(_, n, _)| *n == name) {
                    info!(name = %name, list = %list, "Blocked an app");
                    let _ = eng.record_blocked(&name);
                }
            }
            if let Some(soon) = warner.check(
                &eng,
                &procs,
                chrono::Local::now(),
                process::exe_path,
                process::window_titles,
            ) {
                crate::closing::prompt(soon);
            }

            // Uninstall protection. Only while a lock actually asks for it.
            watch_uninstalls = eng.has_uninstall_protection();

            // Browser extension enforcement. Settings are read on every heavy
            // tick so a change in the UI applies without restarting the app.
            let settings = enforcement_settings(eng.db());
            if settings.enabled {
                let has_active_blocks = eng.block_lists().iter().any(|l| l.is_effectively_active());
                enforce_browsers = Some((has_active_blocks, settings));
            }
        }

        // After the engine lock is released, never inside it. Every command
        // from the window and every request from the extension waits for that
        // lock, and reading command lines is the one step here whose cost
        // depends on what else the machine is running.
        if let Some((has_active_blocks, settings)) = enforce_browsers {
            enforce_browser_extension(has_active_blocks, &settings, &mut grace_periods);
        }
        if watch_uninstalls {
            block_uninstall_attempts(&mut cleared_processes);
        }
    }
}

/// How browsers without the extension are treated.
struct Enforcement {
    /// Closed at all.
    enabled: bool,
    grace: Duration,
    /// Browsers a test tool started are left open (#45).
    spare_automation: bool,
}

fn enforcement_settings(db: &focuser_core::db::Database) -> Enforcement {
    use focuser_app::execute::{
        DEFAULT_GRACE_PERIOD_SECS, SETTING_CLOSE_BROWSERS, SETTING_GRACE_PERIOD,
        SETTING_SPARE_AUTOMATION,
    };
    let flag = |key, default: bool| {
        db.get_setting_or_default(key, if default { "true" } else { "false" })
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(default)
    };

    let grace = db
        .get_setting(SETTING_GRACE_PERIOD)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_GRACE_PERIOD_SECS);

    Enforcement {
        enabled: flag(SETTING_CLOSE_BROWSERS, true),
        grace: Duration::from_secs(grace),
        spare_automation: flag(SETTING_SPARE_AUTOMATION, false),
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

/// The running processes an app rule on an active block list blocks.
fn blocked_by_lists(
    eng: &focuser_core::BlockEngine,
    tracker: &focuser_core::allowance::AllowanceTracker,
    procs: &[process::Process],
) -> Vec<(u32, String, String)> {
    // An app still inside its daily quota is exempt. Blocking it here would
    // defeat the allowance, which exists precisely to permit some use.
    let exempt: HashSet<String> = tracker
        .active_allowance_apps(eng)
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();

    // ponytail: inside the engine lock, but only when a list has such a rule:
    // one window sweep and one path query per process, a few milliseconds.
    let titles = if active_app_rules(eng).any(|r| matches!(r, AppMatchType::WindowTitle(_))) {
        process::window_titles()
    } else {
        HashMap::new()
    };
    blocked_processes(eng, procs, &exempt, process::exe_path, &titles)
}

/// The app rules of every list that is blocking right now.
fn active_app_rules(eng: &focuser_core::BlockEngine) -> impl Iterator<Item = &AppMatchType> {
    eng.block_lists()
        .iter()
        .filter(|l| l.is_effectively_active())
        .flat_map(|l| &l.applications)
        .filter(|r| r.enabled)
        .map(|r| &r.match_type)
}

/// Which running processes a list blocks, and which list.
///
/// A program's path is read only while some rule is written against one.
/// Window titles never count for a browser: its title follows the open tab,
/// and sites are the website rules' job.
fn blocked_processes(
    eng: &focuser_core::BlockEngine,
    procs: &[process::Process],
    exempt: &HashSet<String>,
    mut exe_path: impl FnMut(u32) -> Option<String>,
    titles: &HashMap<u32, Vec<String>>,
) -> Vec<(u32, String, String)> {
    let wants_paths = active_app_rules(eng).any(|r| matches!(r, AppMatchType::ExecutablePath(_)));
    let mut blocked = Vec::new();
    for proc in procs {
        if !proc.is_killable() || exempt.contains(&proc.name.to_ascii_lowercase()) {
            continue;
        }
        let path = if wants_paths {
            exe_path(proc.pid)
        } else {
            None
        };
        let own_titles = titles
            .get(&proc.pid)
            .filter(|_| identify_browser(&proc.name).is_none())
            .filter(|t| !t.is_empty());
        let list = match own_titles {
            Some(titles) => titles
                .iter()
                .find_map(|t| eng.check_app(&proc.name, path.as_deref(), Some(t))),
            None => eng.check_app(&proc.name, path.as_deref(), None),
        };
        if let Some(list) = list {
            blocked.push((proc.pid, proc.name.clone(), list.to_string()));
        }
    }
    blocked
}

/// The running processes whose allowance is used up for today.
fn over_allowance(
    tracker: &focuser_core::allowance::AllowanceTracker,
    eng: &focuser_core::BlockEngine,
    procs: &[process::Process],
) -> Vec<(u32, String)> {
    let exhausted: HashSet<String> = tracker
        .blocked_apps()
        .into_iter()
        .filter(|exe| !eng.shared_covers_app(exe))
        .map(|s| s.to_ascii_lowercase())
        .collect();
    procs
        .iter()
        .filter(|p| p.is_killable() && exhausted.contains(&p.name.to_ascii_lowercase()))
        .map(|p| (p.pid, p.name.clone()))
        .collect()
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
    settings: &Enforcement,
    grace_periods: &mut HashMap<BrowserType, Grace>,
) {
    let grace_duration = settings.grace;
    if !has_active_blocks {
        grace_periods.clear();
        return;
    }

    let now = Instant::now();

    let mut running = browser_main_processes(&process::list(), process::cmdline);
    if settings.spare_automation {
        spare_automation_browsers(&mut running, process::cmdline);
    }

    // Generous 2-minute window: extensions use chrome.alarms, which fires
    // every 30s once the service worker sleeps. Anything tighter would call a
    // healthy extension dead during a slow startup or a suspended worker.
    //
    // "Safely connected" requires *both* a recent check-in *and* "Allow in
    // Incognito" being granted. A browser that is merely connected but has
    // not been granted incognito access is not actually covered: Chrome
    // hides private windows from an extension without that permission, so a
    // private window there has nothing blocking it at all. Treating that the
    // same as "extension not installed" is deliberate — it is the same gap.
    let safely_connected = crate::api::get_safely_connected_browsers(120);

    for (browser, pids) in &running {
        if safely_connected.contains(browser) {
            grace_periods.remove(browser);
            continue;
        }

        let grace = grace_periods.entry(browser.clone()).or_insert_with(|| {
            warn!(
                browser = ?browser,
                grace_secs = grace_duration.as_secs(),
                "Browser running without full Focuser coverage — grace period started"
            );
            Grace {
                started: now,
                warned: false,
            }
        });
        let started = grace.started;
        let waited = now.duration_since(started);

        // The extension may still be checking in — just without incognito
        // access — which changes what the user needs to be told to fix.
        let reason = || {
            if crate::api::get_connected_browsers(180).contains(browser) {
                crate::api::ClosedReason::IncognitoNotAllowed
            } else {
                crate::api::ClosedReason::NotInstalled
            }
        };
        let name = focuser_common::browser::KNOWN_BROWSERS
            .iter()
            .find(|b| b.browser_type == *browser)
            .map(|b| b.display_name)
            .unwrap_or("your browser");

        match grace_step(waited, grace_duration, grace.warned) {
            GraceStep::Wait => continue,
            GraceStep::Warn => {
                grace.warned = true;
                crate::api::prompt_about_browser(crate::api::BrowserPrompt {
                    browser: name.to_string(),
                    reason: reason(),
                    closing_in_secs: Some(grace_duration.saturating_sub(waited).as_secs()),
                });
                continue;
            }
            GraceStep::Close => {}
        }

        // Last look before closing anything: the extension may have been
        // installed (or granted incognito) mid-grace. Only a check-in since the
        // grace began counts. The one before it is the extension going quiet.
        if crate::api::safely_checked_in_since(browser, started) {
            info!(
                browser = ?browser,
                "Extension fully connected during grace period — cancelling termination"
            );
            grace_periods.remove(browser);
            continue;
        }

        let reason = reason();
        info!(
            browser = ?browser,
            pid_count = pids.len(),
            reason = ?reason,
            "Grace period expired — terminating browser without full extension coverage"
        );
        for &pid in pids {
            process::terminate(pid);
        }

        crate::api::prompt_about_browser(crate::api::BrowserPrompt {
            browser: name.to_string(),
            reason,
            closing_in_secs: None,
        });
        crate::api::SHOW_WINDOW_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);

        // Reset so the grace period restarts if the browser is relaunched.
        grace_periods.remove(browser);
    }

    grace_periods.retain(|browser, _| running.contains_key(browser));
}

/// Leave out the browsers a test tool started (#45). A browser type with
/// none left is not running as far as closing it goes.
fn spare_automation_browsers(
    running: &mut HashMap<BrowserType, Vec<u32>>,
    mut read_cmdline: impl FnMut(u32) -> Option<String>,
) {
    for pids in running.values_mut() {
        pids.retain(|&pid| {
            !read_cmdline(pid).is_some_and(|line| focuser_common::browser::is_automation(&line))
        });
    }
    running.retain(|_, pids| !pids.is_empty());
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
        result.push_str(HOSTS_NOTE);
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

    fn engine_with_app_rules(
        rules: Vec<focuser_common::types::AppRule>,
    ) -> focuser_core::BlockEngine {
        let db = focuser_core::db::Database::open_in_memory().unwrap();
        let mut list = focuser_common::types::BlockList::new("Games");
        list.applications = rules;
        db.create_block_list(&list).unwrap();
        focuser_core::BlockEngine::new(db).unwrap()
    }

    fn proc(pid: u32, name: &str) -> process::Process {
        process::Process {
            pid,
            name: name.into(),
        }
    }

    /// Both kinds were offered on the Applications page and never blocked
    /// anything, because the loop only ever passed the name (#56).
    #[test]
    fn a_program_is_closed_by_its_path_or_its_window_title_too() {
        use focuser_common::types::AppRule;
        let eng = engine_with_app_rules(vec![
            AppRule::path(r"C:\Games\game.exe"),
            AppRule::window_title("Minecraft"),
        ]);
        let browser = focuser_common::browser::KNOWN_BROWSERS[0].exe_names[0];
        let procs = [
            proc(10, "game.exe"),
            proc(11, "javaw.exe"),
            proc(12, "notepad.exe"),
            proc(13, browser),
        ];
        let titles = HashMap::from([
            (11, vec!["Minecraft 1.21".to_string()]),
            // A browser's title follows its tab, and sites are the website
            // rules' job. Closing the whole browser for it would be wrong.
            (13, vec!["Minecraft - YouTube".to_string()]),
        ]);
        let path = |pid| (pid == 10).then(|| r"C:\Games\game.exe".to_string());

        let closed: Vec<u32> = blocked_processes(&eng, &procs, &HashSet::new(), path, &titles)
            .into_iter()
            .map(|(pid, _, _)| pid)
            .collect();
        assert_eq!(closed, [10, 11]);
    }

    #[test]
    fn nobody_is_asked_for_a_path_while_no_rule_needs_one() {
        let eng =
            engine_with_app_rules(vec![focuser_common::types::AppRule::executable("game.exe")]);
        let mut asked = 0;
        let closed = blocked_processes(
            &eng,
            &[proc(10, "game.exe"), proc(11, "other.exe")],
            &HashSet::new(),
            |_| {
                asked += 1;
                None
            },
            &HashMap::new(),
        );
        assert_eq!(closed.len(), 1);
        assert_eq!(asked, 0);
    }

    #[test]
    fn the_hosts_block_says_how_to_remove_it_by_hand() {
        let original = "127.0.0.1 localhost\n";
        let blocked = replace_section(original, &["reddit.com".into()]);
        let lines: Vec<&str> = blocked.lines().collect();
        let begin = lines.iter().position(|l| *l == HOSTS_BEGIN).unwrap();
        // Read by whoever opens the file after Focuser is gone without
        // cleaning up, for instance after an antivirus removed it (#54).
        assert!(
            lines[begin + 1].starts_with("# ") && lines[begin + 1].contains("delete"),
            "{blocked}"
        );

        assert_eq!(
            replace_section(&blocked, &["reddit.com".into()]),
            blocked,
            "writing it again changes nothing"
        );
        assert_eq!(replace_section(&blocked, &[]), original);
    }

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
    fn a_browser_a_test_tool_started_is_left_out_and_the_others_stay() {
        use focuser_common::extension::BrowserType::{Chrome, Firefox};
        let mut running = HashMap::from([(Chrome, vec![20, 21]), (Firefox, vec![30])]);
        spare_automation_browsers(&mut running, |pid| {
            Some(
                match pid {
                    20 => "chrome --remote-debugging-pipe",
                    30 => "firefox -juggler-pipe",
                    _ => "chrome",
                }
                .to_string(),
            )
        });
        // The person's own Chrome is still there to be closed. Firefox ran
        // only for a test, so it is not counted as running at all.
        assert_eq!(running, HashMap::from([(Chrome, vec![21])]));
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
