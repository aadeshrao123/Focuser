use focuser_common::{browser::identify_browser, extension::BrowserType, process};
use std::collections::{HashMap, HashSet};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};
pub fn run(client: focuser_ipc::Client) {
    let mut guard = BrowserGuard::new();
    let mut was_maintenance = false;
    loop {
        if let Ok(maintenance) = crate::MAINTENANCE.lock() {
            if *maintenance {
                was_maintenance = true;
            } else {
                // A failed/interrupted removal may have restored journals.
                // Rebuild coverage state before enforcing again; cached recovery
                // membership must never imply a policy is still installed.
                if was_maintenance {
                    guard = BrowserGuard::new();
                    was_maintenance = false;
                }
                if let Some(snapshot) = crate::api::snapshot() {
                    guard.enforce(
                        snapshot.active && snapshot.enforce_browsers,
                        Duration::from_secs(snapshot.grace_secs),
                        &client,
                    );
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}
struct BrowserGuard {
    monitor: focuser_common::extension_watch::ProfileMonitor,
    profiles: HashMap<BrowserType, Vec<focuser_common::extension_watch::BrowserProfile>>,
    instances: HashMap<BrowserType, Vec<u32>>,
    grace_periods: HashMap<BrowserType, Grace>,
    recovery: HashSet<BrowserType>,
    needs_restart: HashSet<BrowserType>,
    installation_windows: HashMap<BrowserType, InstallationWindow>,
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
            instances: HashMap::new(),
            grace_periods: HashMap::new(),
            needs_restart: recovery.clone(),
            recovery,
            installation_windows: HashMap::new(),
        }
    }

    fn enforce(&mut self, active: bool, grace_duration: Duration, client: &focuser_ipc::Client) {
        use focuser_common::extension_watch::Coverage;
        if active
            && client
                .request_blocking(focuser_ipc::RequestBody::Agent(
                    focuser_ipc::AgentRequest::BrowserRecoveryIntent,
                ))
                .is_err()
        {
            // Stale snapshots cannot start new enforcement after the service
            // says the blocking period has ended. Persist intent before changes.
            return;
        }
        let owned: Vec<_> = process::list()
            .into_iter()
            .filter(|p| crate::observation::ours(p.pid))
            .collect();
        let running = browser_main_processes(&owned, process::cmdline);
        self.observe_instances(&running);
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
            self.installation_windows.clear();
            self.profiles.clear();
            if matches!(crate::browser_recovery::is_clear(), Ok(true)) {
                // The service persists this authenticated acknowledgement. A
                // clean, idle installation need not keep an agent alive merely
                // to authorize later uninstall. Never acknowledge failed restore.
                let _ = client.request_blocking(focuser_ipc::RequestBody::Agent(
                    focuser_ipc::AgentRequest::BrowserRestored,
                ));
            }
            return;
        }
        let safely_connected = crate::api::get_safely_connected_browsers(120);
        let connected = crate::api::get_connected_browsers(120);
        let recovery_ready = crate::api::get_recovery_ready_browsers();
        for (browser, pids) in &running {
            self.installation_windows
                .entry(browser.clone())
                .or_insert_with(|| InstallationWindow::new(pids, Instant::now()))
                .observe(pids, Instant::now());
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
            let profile = watched.first().cloned();
            let recovering = self.recovery.contains(browser);
            if self.finish_recovery(
                browser,
                missing,
                recovery_ready.contains(browser),
                crate::browser_recovery::restore,
            ) {
                continue;
            }
            if self.needs_restart.contains(browser) {
                if let Err(error) = crate::browser_recovery::install(browser, profile.as_ref()) {
                    self.installation_chance(
                        browser,
                        pids,
                        crate::api::ClosedReason::NotInstalled,
                        &error,
                    );
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
            if recovering {
                continue;
            }
            if !missing && safely_connected.contains(browser) {
                let _ = client.request_blocking(focuser_ipc::RequestBody::Agent(
                    focuser_ipc::AgentRequest::BrowserGrace {
                        browser: browser.clone(),
                        missing: false,
                    },
                ));
                self.grace_periods.remove(browser);
                self.installation_windows.remove(browser);
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
            let waited = match client.request_blocking(focuser_ipc::RequestBody::Agent(
                focuser_ipc::AgentRequest::BrowserGrace {
                    browser: browser.clone(),
                    missing: true,
                },
            )) {
                Ok(focuser_ipc::ReplyBody::GraceDeadline(Some(deadline))) => grace_duration
                    .saturating_sub(Duration::from_secs(
                        deadline
                            .saturating_sub(chrono::Utc::now().timestamp())
                            .max(0) as u64,
                    ))
                    .max(grace.started.elapsed()),
                // A disconnected authority never grants a new grace budget.
                _ => grace_duration,
            };
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
            // Prefer installation-only browsing. If the browser cannot accept
            // those policies, allow a bounded manual installation attempt.
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
                    if crate::browser_recovery::pending().contains(browser) {
                        self.recovery.insert(browser.clone());
                        self.needs_restart.insert(browser.clone());
                    }
                    self.installation_chance(browser, pids, reason, &error);
                }
            }
        }
        self.grace_periods
            .retain(|browser, _| running.contains_key(browser));
        self.installation_windows
            .retain(|browser, _| running.contains_key(browser));
        self.profiles
            .retain(|browser, _| running.contains_key(browser) || self.recovery.contains(browser));
    }

    /// An update can exit and relaunch between two polls. In that case there is
    /// no empty `running` sample to discard the previous process's profiles.
    /// Re-discover profiles for the new instance instead of permanently treating
    /// a profile from the old instance as an uncovered current window. Keep the
    /// service deadline and recovery journal; a restart never unlocks policy.
    fn observe_instances(&mut self, running: &HashMap<BrowserType, Vec<u32>>) {
        self.profiles.retain(|browser, _| {
            let same_instance = running.get(browser).is_some_and(|pids| {
                self.instances
                    .get(browser)
                    .is_some_and(|old| old.iter().any(|pid| pids.contains(pid)))
            });
            same_instance || (self.recovery.contains(browser) && !running.contains_key(browser))
        });
        self.instances.clone_from(running);
    }

    /// Handle a covered reconnection before any pending policy retry or timer.
    /// Restoring the policies must not terminate the newly covered browser.
    fn finish_recovery(
        &mut self,
        browser: &BrowserType,
        missing: bool,
        ready: bool,
        restore: impl FnOnce(&BrowserType) -> Result<(), String>,
    ) -> bool {
        if !self.recovery.contains(browser) || missing || !ready {
            return false;
        }
        if let Err(error) = restore(browser) {
            warn!(%error, ?browser, "Could not leave installation-only recovery");
            return true;
        }
        self.recovery.remove(browser);
        self.needs_restart.remove(browser);
        self.grace_periods.remove(browser);
        self.installation_windows.remove(browser);
        info!(
            ?browser,
            "Extension reconnected; normal browsing restored without restarting browser"
        );
        true
    }

    fn installation_chance(
        &mut self,
        browser: &BrowserType,
        pids: &[u32],
        reason: crate::api::ClosedReason,
        error: &str,
    ) {
        let window = self
            .installation_windows
            .get_mut(browser)
            .expect("Running browser has an installation window");
        let now = Instant::now();
        match window.step(now) {
            GraceStep::Wait => {}
            GraceStep::Warn => {
                warn!(?browser, %error, "Installation-only recovery unavailable; allowing a timed installation attempt");
                let name = focuser_common::browser::KNOWN_BROWSERS
                    .iter()
                    .find(|b| b.browser_type == *browser)
                    .map(|b| b.display_name)
                    .unwrap_or("your browser");
                let seconds = INSTALLATION_WINDOW
                    .saturating_sub(now.duration_since(window.started))
                    .as_secs();
                let action = match reason {
                    crate::api::ClosedReason::NotInstalled => "Install the Focuser extension",
                    crate::api::ClosedReason::IncognitoNotAllowed => {
                        "Allow the Focuser extension in private/incognito windows"
                    }
                };
                crate::notify(&format!(
                    "{name}: {action} within {seconds} seconds. Installation-only recovery is unavailable, so the browser will close if coverage is still missing. Reopen it for another 60-second installation attempt."
                ));
            }
            GraceStep::Close => {
                for &pid in pids {
                    process::terminate(pid);
                }
                crate::api::forget_browser(browser);
            }
        }
    }

    fn restart(&self, browser: &BrowserType, pids: &[u32], url: &str) -> bool {
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

const INSTALLATION_WINDOW: Duration = Duration::from_secs(60);

/// A failed policy installation must not lock the user out of installing the
/// extension. Track the actual main processes, independently of the persisted
/// missing-extension deadline. Polling or adding another window cannot renew it.
struct InstallationWindow {
    pids: Vec<u32>,
    started: Instant,
    warned: bool,
}

impl InstallationWindow {
    fn new(pids: &[u32], now: Instant) -> Self {
        Self {
            pids: pids.to_vec(),
            started: now,
            warned: false,
        }
    }

    fn observe(&mut self, pids: &[u32], now: Instant) {
        if !self.pids.iter().any(|pid| pids.contains(pid)) {
            *self = Self::new(pids, now);
        } else {
            self.pids = pids.to_vec();
        }
    }

    fn step(&mut self, now: Instant) -> GraceStep {
        if now.duration_since(self.started) >= INSTALLATION_WINDOW {
            GraceStep::Close
        } else if !self.warned {
            self.warned = true;
            GraceStep::Warn
        } else {
            GraceStep::Wait
        }
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

    fn interrupted_recovery() -> BrowserGuard {
        let browser = BrowserType::Brave;
        BrowserGuard {
            monitor: Default::default(),
            profiles: HashMap::new(),
            instances: HashMap::new(),
            grace_periods: HashMap::from([(
                browser.clone(),
                Grace {
                    started: Instant::now(),
                    warned: true,
                },
            )]),
            recovery: HashSet::from([browser.clone()]),
            needs_restart: HashSet::from([browser.clone()]),
            installation_windows: HashMap::from([(
                browser,
                InstallationWindow::new(&[10], Instant::now() - secs(61)),
            )]),
        }
    }

    #[test]
    fn reconnected_extension_cancels_expired_timer_and_pending_restart() {
        let mut guard = interrupted_recovery();
        let browser = BrowserType::Brave;
        let mut restores = 0;
        assert!(guard.finish_recovery(&browser, false, true, |_| {
            restores += 1;
            Ok(())
        }));
        assert_eq!(restores, 1);
        assert!(!guard.recovery.contains(&browser));
        assert!(!guard.needs_restart.contains(&browser));
        assert!(!guard.grace_periods.contains_key(&browser));
        assert!(!guard.installation_windows.contains_key(&browser));
        assert!(!guard.finish_recovery(&browser, false, true, |_| panic!("Already restored")));
    }

    #[test]
    fn recovery_requires_covered_checkin_and_preserves_failed_restoration() {
        let mut guard = interrupted_recovery();
        let browser = BrowserType::Brave;
        for (missing, ready) in [(true, true), (false, false), (true, false)] {
            assert!(!guard.finish_recovery(&browser, missing, ready, |_| panic!("Not covered")));
        }
        // A failed restore is handled without falling through to a close or
        // restart; the journal and pending state remain for the next poll.
        assert!(guard.finish_recovery(&browser, false, true, |_| Err("Denied".into())));
        assert!(guard.recovery.contains(&browser));
        assert!(guard.needs_restart.contains(&browser));
        assert!(guard.installation_windows.contains_key(&browser));
        assert!(guard.finish_recovery(&browser, false, true, |_| Ok(())));
    }

    #[test]
    fn update_relaunch_discards_old_profiles_without_releasing_recovery_or_deadline() {
        let mut guard = interrupted_recovery();
        let browser = BrowserType::Brave;
        let profile = focuser_common::extension_watch::BrowserProfile {
            root: "previous-instance".into(),
            directory: "Default".into(),
        };
        guard
            .profiles
            .insert(browser.clone(), vec![profile.clone()]);
        guard.instances.insert(browser.clone(), vec![10]);
        guard.observe_instances(&HashMap::from([(browser.clone(), vec![10, 11])]));
        assert_eq!(guard.profiles[&browser], [profile]);
        // Update relaunch was fast enough that no intervening empty poll occurred.
        guard.observe_instances(&HashMap::from([(browser.clone(), vec![20])]));
        assert!(!guard.profiles.contains_key(&browser));
        assert!(guard.recovery.contains(&browser));
        assert!(guard.needs_restart.contains(&browser));
        assert!(guard.grace_periods.contains_key(&browser));
        assert!(!guard.finish_recovery(&browser, false, false, |_| panic!("No fresh check-in")));
        assert!(guard.finish_recovery(&browser, false, true, |_| Ok(())));
        assert!(!guard.recovery.contains(&browser));
        assert!(!guard.needs_restart.contains(&browser));
    }

    #[test]
    fn background_browser_updaters_are_never_extension_guard_targets() {
        let processes = [
            "BraveUpdate.exe",
            "BraveUpdater.exe",
            "updater.exe",
            "setup.exe",
            "GoogleUpdate.exe",
            "MicrosoftEdgeUpdate.exe",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, name)| Process {
            pid: 100 + index as u32,
            name: name.into(),
        })
        .collect::<Vec<_>>();
        assert!(
            browser_main_processes(&processes, |_| panic!("Updater is not a browser")).is_empty()
        );
    }

    #[test]
    fn unavailable_recovery_gives_a_full_minute_then_closes() {
        let now = Instant::now();
        let mut window = InstallationWindow::new(&[10], now);
        assert_eq!(window.step(now), GraceStep::Warn);
        assert_eq!(window.step(now + secs(59)), GraceStep::Wait);
        assert_eq!(window.step(now + secs(60)), GraceStep::Close);
        assert_eq!(window.step(now + secs(61)), GraceStep::Close);
    }

    #[test]
    fn reopening_after_expiry_gets_a_new_installation_chance() {
        let now = Instant::now();
        let mut window = InstallationWindow::new(&[10], now);
        assert_eq!(window.step(now + secs(60)), GraceStep::Close);
        // The persisted authority grace can remain expired. A new main process
        // gets a separate manual-installation attempt if recovery still fails.
        window.observe(&[20], now + secs(61));
        assert_eq!(window.step(now + secs(61)), GraceStep::Warn);
        assert_eq!(window.step(now + secs(120)), GraceStep::Wait);
        assert_eq!(window.step(now + secs(121)), GraceStep::Close);
    }

    #[test]
    fn retrying_policy_installation_does_not_extend_the_window() {
        let now = Instant::now();
        let mut window = InstallationWindow::new(&[10], now);
        for second in 0..60 {
            window.observe(&[10], now + secs(second));
            assert_ne!(window.step(now + secs(second)), GraceStep::Close);
        }
        window.observe(&[10], now + secs(60));
        assert_eq!(window.step(now + secs(60)), GraceStep::Close);
    }

    #[test]
    fn additional_main_processes_do_not_renew_an_existing_attempt() {
        let now = Instant::now();
        let mut window = InstallationWindow::new(&[10], now);
        window.observe(&[20, 10], now + secs(50));
        window.observe(&[20], now + secs(55));
        assert_eq!(window.step(now + secs(60)), GraceStep::Close);
    }

    #[test]
    fn browsers_have_independent_installation_attempts() {
        let now = Instant::now();
        let mut windows = HashMap::from([
            (BrowserType::Chrome, InstallationWindow::new(&[10], now)),
            (
                BrowserType::Firefox,
                InstallationWindow::new(&[20], now + secs(30)),
            ),
        ]);
        assert_eq!(
            windows
                .get_mut(&BrowserType::Chrome)
                .unwrap()
                .step(now + secs(60)),
            GraceStep::Close
        );
        assert_eq!(
            windows
                .get_mut(&BrowserType::Firefox)
                .unwrap()
                .step(now + secs(60)),
            GraceStep::Warn
        );
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
}
