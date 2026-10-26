//! OS hosts fallback, owned by the authority rather than the desktop window.
const HOSTS_BEGIN: &str = "# ──── BEGIN FOCUSER BLOCK ────";
const HOSTS_END: &str = "# ──── END FOCUSER BLOCK ────";
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
}
pub fn apply(domains: &[String]) -> Result<(), String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let domains: Vec<_> = domains.iter().filter(|d| valid_host(d)).cloned().collect();
    let updated = replace_section(&content, &domains);
    if content != updated {
        std::fs::write(&path, updated).map_err(|e| e.to_string())?;
        flush_dns();
    }
    Ok(())
}
pub fn writable() -> bool {
    std::fs::OpenOptions::new()
        .append(true)
        .open(hosts_path())
        .is_ok()
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

    for line in content.split_inclusive('\n') {
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
    use super::*;
    #[test]
    fn idle_enforcement_never_rewrites_unrelated_hosts_bytes() {
        for original in [
            "",
            "127.0.0.1 localhost",
            "# system\r\n127.0.0.1 localhost\r\n\r\n",
        ] {
            assert_eq!(replace_section(original, &[]), original);
        }
    }
    #[test]
    fn release_removes_only_the_owned_section_and_preserves_other_line_endings() {
        let prefix = "# system\r\n127.0.0.1 localhost\r\n";
        let suffix = "# other app\r\n10.0.0.1 intranet";
        let active = format!("{prefix}{HOSTS_BEGIN}\n127.0.0.1 focus.test\n{HOSTS_END}\n{suffix}");
        assert_eq!(replace_section(&active, &[]), format!("{prefix}{suffix}"));
    }
    #[test]
    fn active_hosts_section_is_idempotent_and_releasable() {
        let original = "# system\r\n127.0.0.1 localhost\r\n";
        let domains = vec!["focus.test".to_string()];
        let active = replace_section(original, &domains);
        assert!(active.contains("127.0.0.1 focus.test"));
        assert_eq!(replace_section(&active, &domains), active);
        assert_eq!(replace_section(&active, &[]), original);
    }
}
