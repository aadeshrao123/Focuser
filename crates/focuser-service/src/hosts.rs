//! Hosts file manipulation for domain blocking.
//!
//! This is the primary website blocking mechanism. It works on all platforms
//! by redirecting blocked domains to 127.0.0.1 in the system hosts file.

use focuser_common::error::{FocuserError, Result};
use std::fs;
use tracing::{debug, info};

const FOCUSER_BEGIN: &str = "# ──── BEGIN FOCUSER BLOCK ────";
const FOCUSER_END: &str = "# ──── END FOCUSER BLOCK ────";

/// Get the hosts file path for the current platform.
pub fn hosts_file_path() -> &'static str {
    #[cfg(windows)]
    {
        r"C:\Windows\System32\drivers\etc\hosts"
    }
    #[cfg(target_os = "macos")]
    {
        "/etc/hosts"
    }
    #[cfg(target_os = "linux")]
    {
        "/etc/hosts"
    }
}

/// Apply domain blocks to the hosts file.
///
/// Domains are redirected to 127.0.0.1. Existing Focuser entries are replaced.
pub fn apply_blocks(domains: &[String]) -> Result<()> {
    let path = hosts_file_path();
    info!(count = domains.len(), path, "Applying hosts file blocks");

    let content = fs::read_to_string(path).map_err(|e| {
        FocuserError::Platform(format!("Cannot read hosts file at {path}: {e}"))
    })?;

    let new_content = replace_focuser_section(&content, domains);

    fs::write(path, &new_content).map_err(|e| {
        FocuserError::Platform(format!("Cannot write hosts file at {path}: {e}. Run as admin/root."))
    })?;

    // Flush DNS cache
    flush_dns_cache();

    debug!("Hosts file updated successfully");
    Ok(())
}

/// Remove all Focuser entries from the hosts file.
pub fn remove_all_blocks() -> Result<()> {
    let path = hosts_file_path();
    info!(path, "Removing all hosts file blocks");

    let content = fs::read_to_string(path).map_err(|e| {
        FocuserError::Platform(format!("Cannot read hosts file: {e}"))
    })?;

    let new_content = replace_focuser_section(&content, &[]);

    fs::write(path, &new_content).map_err(|e| {
        FocuserError::Platform(format!("Cannot write hosts file: {e}. Run as admin/root."))
    })?;

    flush_dns_cache();
    Ok(())
}

/// Check if a domain is currently blocked in the hosts file.
pub fn is_domain_blocked(domain: &str) -> Result<bool> {
    let path = hosts_file_path();
    let content = fs::read_to_string(path).map_err(|e| {
        FocuserError::Platform(format!("Cannot read hosts file: {e}"))
    })?;

    let domain_lower = domain.to_lowercase();
    Ok(content.lines().any(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            return false;
        }
        // Lines look like: 127.0.0.1 reddit.com
        trimmed
            .split_whitespace()
            .nth(1)
            .map(|d| d.to_lowercase() == domain_lower)
            .unwrap_or(false)
    }))
}

/// Replace the Focuser-managed section of the hosts file.
fn replace_focuser_section(content: &str, domains: &[String]) -> String {
    let mut result = String::with_capacity(content.len() + domains.len() * 30);
    let mut in_focuser_section = false;
    for line in content.lines() {
        if line.trim() == FOCUSER_BEGIN {
            in_focuser_section = true;
            continue;
        }
        if line.trim() == FOCUSER_END {
            in_focuser_section = false;
            continue;
        }
        if !in_focuser_section {
            result.push_str(line);
            result.push('\n');
        }
    }

    // Add the new Focuser section if there are domains to block
    if !domains.is_empty() {
        // Ensure there's a blank line before our section
        if !result.ends_with("\n\n") && !result.ends_with("\n") {
            result.push('\n');
        }
        result.push_str(FOCUSER_BEGIN);
        result.push('\n');
        for domain in domains {
            result.push_str(&format!("127.0.0.1 {domain}\n"));
            result.push_str(&format!("::1 {domain}\n"));
        }
        result.push_str(FOCUSER_END);
        result.push('\n');
    }

    result
}

/// Flush the system DNS cache so hosts file changes take effect immediately.
fn flush_dns_cache() {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("ipconfig")
            .args(["/flushdns"])
            .output();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("dscacheutil")
            .args(["-flushcache"])
            .output();
        let _ = std::process::Command::new("sudo")
            .args(["killall", "-HUP", "mDNSResponder"])
            .output();
    }
    #[cfg(target_os = "linux")]
    {
        // systemd-resolved
        let _ = std::process::Command::new("systemd-resolve")
            .args(["--flush-caches"])
            .output();
        // nscd
        let _ = std::process::Command::new("nscd")
            .args(["-i", "hosts"])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_empty_section() {
        let content = "127.0.0.1 localhost\n::1 localhost\n";
        let domains = vec!["reddit.com".into(), "twitter.com".into()];
        let result = replace_focuser_section(content, &domains);

        assert!(result.contains(FOCUSER_BEGIN));
        assert!(result.contains("127.0.0.1 reddit.com"));
        assert!(result.contains("::1 twitter.com"));
        assert!(result.contains(FOCUSER_END));
        assert!(result.contains("127.0.0.1 localhost"));
    }

    #[test]
    fn test_replace_existing_section() {
        let content = format!(
            "127.0.0.1 localhost\n{FOCUSER_BEGIN}\n127.0.0.1 old.com\n{FOCUSER_END}\n"
        );
        let domains = vec!["new.com".into()];
        let result = replace_focuser_section(&content, &domains);

        assert!(!result.contains("old.com"));
        assert!(result.contains("127.0.0.1 new.com"));
    }

    #[test]
    fn test_remove_all() {
        let content = format!(
            "127.0.0.1 localhost\n{FOCUSER_BEGIN}\n127.0.0.1 blocked.com\n{FOCUSER_END}\n"
        );
        let result = replace_focuser_section(&content, &[]);

        assert!(!result.contains("blocked.com"));
        assert!(!result.contains(FOCUSER_BEGIN));
        assert!(result.contains("localhost"));
    }
}
