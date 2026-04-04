//! Browser extension enforcement.
//!
//! Detects browsers running without the Focuser extension and enforces
//! installation via a grace period followed by browser termination.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use focuser_common::browser::identify_browser;
use focuser_common::extension::BrowserType;
use focuser_common::platform::RunningProcess;
use tracing::{info, warn};

/// Enforces that browsers have the Focuser extension installed.
pub struct BrowserEnforcement {
    /// When the grace period started for each unconnected browser type.
    grace_periods: HashMap<BrowserType, Instant>,
    /// How long to wait before killing a browser without the extension.
    grace_duration: Duration,
    /// Whether enforcement is enabled.
    enabled: bool,
}

impl BrowserEnforcement {
    /// Create a new enforcement instance.
    pub fn new(grace_seconds: u64, enabled: bool) -> Self {
        Self {
            grace_periods: HashMap::new(),
            grace_duration: Duration::from_secs(grace_seconds),
            enabled,
        }
    }

    /// Evaluate running processes and return PIDs of browsers to terminate.
    ///
    /// A browser is terminated only if:
    /// - Enforcement is enabled
    /// - There are active blocks
    /// - The browser has been running without a connected extension for longer
    ///   than the grace period
    pub fn evaluate(
        &mut self,
        running_processes: &[RunningProcess],
        connected_extensions: &HashSet<BrowserType>,
        has_active_blocks: bool,
    ) -> Vec<u32> {
        // If disabled or no active blocks, clear state and skip
        if !self.enabled || !has_active_blocks {
            self.grace_periods.clear();
            return Vec::new();
        }

        let now = Instant::now();

        // Find which browser types are currently running and collect their PIDs
        let mut running_browsers: HashMap<BrowserType, Vec<u32>> = HashMap::new();
        for proc in running_processes {
            if let Some(browser_info) = identify_browser(&proc.name) {
                running_browsers
                    .entry(browser_info.browser_type.clone())
                    .or_default()
                    .push(proc.pid);
            }
        }

        let mut pids_to_kill = Vec::new();

        // Check each running browser
        for (browser_type, pids) in &running_browsers {
            if connected_extensions.contains(browser_type) {
                // Extension is connected — clear any grace period
                self.grace_periods.remove(browser_type);
                continue;
            }

            // Browser running without extension
            match self.grace_periods.get(browser_type) {
                None => {
                    // Start grace period
                    warn!(
                        browser = ?browser_type,
                        grace_secs = self.grace_duration.as_secs(),
                        "Browser running without Focuser extension — grace period started"
                    );
                    self.grace_periods.insert(browser_type.clone(), now);
                }
                Some(started_at) => {
                    if now.duration_since(*started_at) >= self.grace_duration {
                        // Grace period expired — kill this browser
                        info!(
                            browser = ?browser_type,
                            pid_count = pids.len(),
                            "Grace period expired — terminating browser without extension"
                        );
                        pids_to_kill.extend(pids);
                        // Reset grace period so it restarts if browser is relaunched
                        self.grace_periods.remove(browser_type);
                    }
                }
            }
        }

        // Clean up grace periods for browsers that are no longer running
        self.grace_periods
            .retain(|bt, _| running_browsers.contains_key(bt));

        pids_to_kill
    }

    /// Get remaining grace period seconds for a browser type, if any.
    pub fn grace_remaining(&self, browser_type: &BrowserType) -> Option<u64> {
        self.grace_periods.get(browser_type).map(|started_at| {
            let elapsed = started_at.elapsed();
            if elapsed >= self.grace_duration {
                0
            } else {
                (self.grace_duration - elapsed).as_secs()
            }
        })
    }

    /// Whether enforcement is enabled.
    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_process(name: &str, pid: u32) -> RunningProcess {
        RunningProcess {
            pid,
            name: name.to_string(),
            exe_path: None,
            window_title: None,
        }
    }

    /// Get a Chrome process name that matches the current platform.
    fn chrome_name() -> &'static str {
        focuser_common::browser::KNOWN_BROWSERS
            .iter()
            .find(|b| b.browser_type == BrowserType::Chrome)
            .unwrap()
            .exe_names[0]
    }

    #[test]
    fn test_no_active_blocks_returns_empty() {
        let mut enf = BrowserEnforcement::new(60, true);
        let processes = vec![make_process(chrome_name(), 100)];
        let connected = HashSet::new();

        let result = enf.evaluate(&processes, &connected, false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_disabled_returns_empty() {
        let mut enf = BrowserEnforcement::new(60, false);
        let processes = vec![make_process(chrome_name(), 100)];
        let connected = HashSet::new();

        let result = enf.evaluate(&processes, &connected, true);
        assert!(result.is_empty());
    }

    #[test]
    fn test_connected_browser_not_killed() {
        let mut enf = BrowserEnforcement::new(0, true); // 0 grace = immediate
        let processes = vec![make_process(chrome_name(), 100)];
        let mut connected = HashSet::new();
        connected.insert(BrowserType::Chrome);

        let result = enf.evaluate(&processes, &connected, true);
        assert!(result.is_empty());
    }

    #[test]
    fn test_grace_period_starts_then_kills() {
        let mut enf = BrowserEnforcement::new(0, true); // 0-second grace
        let processes = vec![make_process(chrome_name(), 100)];
        let connected = HashSet::new();

        // First call: starts grace period
        let _ = enf.evaluate(&processes, &connected, true);
        // With 0-second grace, the check `now.duration_since(started_at) >= grace_duration`
        // may or may not pass on the same call depending on timing.
        // Second call should definitely kill.
        let result = enf.evaluate(&processes, &connected, true);
        assert!(result.contains(&100));
    }

    #[test]
    fn test_non_browser_process_ignored() {
        let mut enf = BrowserEnforcement::new(0, true);
        let processes = vec![make_process("notepad.exe", 200)];
        let connected = HashSet::new();

        let result = enf.evaluate(&processes, &connected, true);
        assert!(result.is_empty());
    }

    #[test]
    fn test_grace_remaining() {
        let mut enf = BrowserEnforcement::new(60, true);
        let processes = vec![make_process(chrome_name(), 100)];
        let connected = HashSet::new();

        // Start grace period
        enf.evaluate(&processes, &connected, true);

        let remaining = enf.grace_remaining(&BrowserType::Chrome);
        assert!(remaining.is_some());
        assert!(remaining.unwrap() <= 60);
    }
}
