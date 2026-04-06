use crate::error::Result;
use crate::types::{AppRule, WebsiteRule};

/// Platform-specific blocking operations.
///
/// Each OS implements this trait to provide actual system-level blocking.
/// The service calls these methods; the trait abstracts away OS differences.
pub trait PlatformBlocker: Send + Sync {
    /// Apply website blocks by modifying the system's DNS resolution.
    /// On all platforms this modifies the hosts file to redirect blocked domains to 127.0.0.1.
    fn block_websites(&self, rules: &[WebsiteRule]) -> Result<()>;

    /// Remove all website blocks (restore hosts file).
    fn unblock_all_websites(&self) -> Result<()>;

    /// Check if a specific domain is currently blocked at the system level.
    fn is_website_blocked(&self, domain: &str) -> Result<bool>;

    /// Terminate a running application that matches the rule.
    fn kill_blocked_app(&self, rule: &AppRule) -> Result<bool>;

    /// Get list of running process names.
    fn list_running_processes(&self) -> Result<Vec<RunningProcess>>;

    /// Protect the service from being killed (platform-specific hardening).
    fn protect_service(&self) -> Result<()>;

    /// Get the path to the system hosts file.
    fn hosts_file_path(&self) -> &str;

    // ─── Uninstall Protection ──────────────────────────────────

    fn detect_uninstall_attempts(&self, processes: &[RunningProcess]) -> Vec<u32>;
    fn protect_installation(&self) -> Result<()>;
    fn unprotect_installation(&self) -> Result<()>;
    fn get_process_cmdline(&self, pid: u32) -> Option<String>;
}

/// Info about a running process.
#[derive(Debug, Clone)]
pub struct RunningProcess {
    pub pid: u32,
    pub name: String,
    pub exe_path: Option<String>,
    pub window_title: Option<String>,
}
