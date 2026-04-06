use focuser_common::error::{FocuserError, Result};
use focuser_common::platform::{PlatformBlocker, RunningProcess};
use focuser_common::types::{AppRule, WebsiteRule};
use tracing::{debug, info, warn};

use crate::hosts;

const PACKAGE_MANAGERS: &[&str] = &[
    "apt", "apt-get", "dpkg", "dnf", "yum", "rpm", "pacman", "zypper", "snap", "flatpak",
];

const PROTECTED_PATHS: &[&str] = &[
    "/usr/bin/focuser-service",
    "/usr/bin/focuser-cli",
    "/usr/bin/focuser-ui",
    "/etc/systemd/system/focuser.service",
];

pub struct LinuxBlocker;

impl LinuxBlocker {
    pub fn new() -> Self {
        Self
    }

    fn read_proc_processes() -> Result<Vec<RunningProcess>> {
        let mut processes = Vec::new();

        let entries = std::fs::read_dir("/proc")
            .map_err(|e| FocuserError::Platform(format!("Cannot read /proc: {e}")))?;

        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if let Ok(pid) = name_str.parse::<u32>() {
                let comm_path = entry.path().join("comm");
                if let Ok(comm) = std::fs::read_to_string(&comm_path) {
                    let proc_name = comm.trim().to_string();

                    let exe_path = std::fs::read_link(entry.path().join("exe"))
                        .ok()
                        .map(|p| p.to_string_lossy().into_owned());

                    processes.push(RunningProcess {
                        pid,
                        name: proc_name,
                        exe_path,
                        window_title: None,
                    });
                }
            }
        }

        Ok(processes)
    }

    fn kill_process(pid: u32) -> Result<()> {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
            .map_err(|e| FocuserError::Platform(format!("Cannot kill process {pid}: {e}")))?;

        info!(pid, "Terminated blocked process");
        Ok(())
    }

    fn read_proc_cmdline(pid: u32) -> Option<String> {
        let path = format!("/proc/{pid}/cmdline");
        let data = std::fs::read_to_string(&path).ok()?;
        Some(data.replace('\0', " ").trim().to_string())
    }

    fn cmdline_targets_focuser(cmdline: &str) -> bool {
        let lower = cmdline.to_lowercase();
        let has_remove = lower.contains("remove")
            || lower.contains("purge")
            || lower.contains("erase")
            || lower.contains("-r ")
            || lower.contains("--remove");
        let has_focuser = lower.contains("focuser");
        has_remove && has_focuser
    }

    fn set_immutable(path: &str, immutable: bool) -> Result<()> {
        if !std::path::Path::new(path).exists() {
            debug!(path, "Skipping chattr — file does not exist");
            return Ok(());
        }

        let flag = if immutable { "+i" } else { "-i" };
        let status = std::process::Command::new("chattr")
            .args([flag, path])
            .status()
            .map_err(|e| FocuserError::Platform(format!("Failed to run chattr: {e}")))?;

        if status.success() {
            info!(path, immutable, "Set immutable attribute");
        } else {
            warn!(path, immutable, "chattr failed (may need root)");
        }
        Ok(())
    }
}

impl PlatformBlocker for LinuxBlocker {
    fn block_websites(&self, rules: &[WebsiteRule]) -> Result<()> {
        let domains: Vec<String> = rules
            .iter()
            .filter(|r| r.enabled)
            .filter_map(|r| match &r.match_type {
                focuser_common::types::WebsiteMatchType::Domain(d) => {
                    let mut ds = vec![d.clone()];
                    if !d.starts_with("www.") {
                        ds.push(format!("www.{d}"));
                    }
                    Some(ds)
                }
                _ => None,
            })
            .flatten()
            .collect();

        hosts::apply_blocks(&domains)
    }

    fn unblock_all_websites(&self) -> Result<()> {
        hosts::remove_all_blocks()
    }

    fn is_website_blocked(&self, domain: &str) -> Result<bool> {
        hosts::is_domain_blocked(domain)
    }

    fn kill_blocked_app(&self, rule: &AppRule) -> Result<bool> {
        let processes = Self::read_proc_processes()?;
        let mut killed_any = false;

        for proc in &processes {
            if rule.matches_process(
                &proc.name,
                proc.exe_path.as_deref(),
                proc.window_title.as_deref(),
            ) {
                match Self::kill_process(proc.pid) {
                    Ok(()) => killed_any = true,
                    Err(e) => {
                        warn!(pid = proc.pid, name = %proc.name, error = %e, "Failed to kill")
                    }
                }
            }
        }

        Ok(killed_any)
    }

    fn list_running_processes(&self) -> Result<Vec<RunningProcess>> {
        Self::read_proc_processes()
    }

    fn protect_service(&self) -> Result<()> {
        debug!("Service protection not yet implemented on Linux");
        Ok(())
    }

    fn hosts_file_path(&self) -> &str {
        "/etc/hosts"
    }

    fn detect_uninstall_attempts(&self, processes: &[RunningProcess]) -> Vec<u32> {
        let mut pids_to_kill = Vec::new();

        for proc in processes {
            let name_lower = proc.name.to_lowercase();

            let is_pkg_manager = PACKAGE_MANAGERS.iter().any(|pm| name_lower == *pm);

            if is_pkg_manager {
                if let Some(cmdline) = self.get_process_cmdline(proc.pid) {
                    if Self::cmdline_targets_focuser(&cmdline) {
                        info!(
                            pid = proc.pid,
                            name = %proc.name,
                            "Detected package manager uninstall attempt"
                        );
                        pids_to_kill.push(proc.pid);
                    }
                }
            }

            if name_lower == "rm" {
                if let Some(cmdline) = self.get_process_cmdline(proc.pid) {
                    let lower = cmdline.to_lowercase();
                    if lower.contains("focuser") || lower.contains("/usr/bin/focuser") {
                        info!(
                            pid = proc.pid,
                            "Detected manual deletion attempt of Focuser binaries"
                        );
                        pids_to_kill.push(proc.pid);
                    }
                }
            }
        }

        pids_to_kill
    }

    fn protect_installation(&self) -> Result<()> {
        info!("Enabling installation protection on Linux");
        for path in PROTECTED_PATHS {
            let _ = Self::set_immutable(path, true);
        }
        Ok(())
    }

    fn unprotect_installation(&self) -> Result<()> {
        info!("Disabling installation protection on Linux");
        for path in PROTECTED_PATHS {
            let _ = Self::set_immutable(path, false);
        }
        Ok(())
    }

    fn get_process_cmdline(&self, pid: u32) -> Option<String> {
        Self::read_proc_cmdline(pid)
    }
}
