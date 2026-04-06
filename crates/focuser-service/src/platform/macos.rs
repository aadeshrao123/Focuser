use focuser_common::error::{FocuserError, Result};
use focuser_common::platform::{PlatformBlocker, RunningProcess};
use focuser_common::types::{AppRule, WebsiteRule};
use tracing::{debug, info, warn};

use crate::hosts;

const MACOS_UNINSTALLER_APPS: &[&str] = &[
    "AppCleaner",
    "CleanMyMac",
    "AppZapper",
    "AppDelete",
    "TrashMe",
    "Pearcleaner",
];

const PROTECTED_PATHS: &[&str] = &[
    "/usr/local/bin/focuser-service",
    "/usr/local/bin/focuser-cli",
    "/usr/local/bin/focuser-ui",
    "/Library/LaunchDaemons/com.focuser.service.plist",
];

pub struct MacOsBlocker;

impl MacOsBlocker {
    pub fn new() -> Self {
        Self
    }

    fn list_ps_processes() -> Result<Vec<RunningProcess>> {
        let output = std::process::Command::new("ps")
            .args(["-eo", "pid,comm"])
            .output()
            .map_err(|e| FocuserError::Platform(format!("Cannot run ps: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut processes = Vec::new();

        for line in stdout.lines().skip(1) {
            let trimmed = line.trim();
            if let Some(space_pos) = trimmed.find(' ') {
                if let Ok(pid) = trimmed[..space_pos].trim().parse::<u32>() {
                    let name = trimmed[space_pos..].trim().to_string();
                    let short_name = name.rsplit('/').next().unwrap_or(&name).to_string();

                    processes.push(RunningProcess {
                        pid,
                        name: short_name,
                        exe_path: Some(name),
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

    fn read_ps_cmdline(pid: u32) -> Option<String> {
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "args="])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    fn set_immutable(path: &str, immutable: bool) -> Result<()> {
        if !std::path::Path::new(path).exists() {
            debug!(path, "Skipping chflags — file does not exist");
            return Ok(());
        }

        let flag = if immutable { "schg" } else { "noschg" };
        let status = std::process::Command::new("chflags")
            .args([flag, path])
            .status()
            .map_err(|e| FocuserError::Platform(format!("Failed to run chflags: {e}")))?;

        if status.success() {
            info!(path, immutable, "Set system immutable flag");
        } else {
            warn!(path, immutable, "chflags failed (may need root)");
        }
        Ok(())
    }
}

impl PlatformBlocker for MacOsBlocker {
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
        let processes = Self::list_ps_processes()?;
        let mut killed_any = false;

        for proc in &processes {
            if rule.matches_process(
                &proc.name,
                proc.exe_path.as_deref(),
                proc.window_title.as_deref(),
            ) {
                match Self::kill_process(proc.pid) {
                    Ok(()) => killed_any = true,
                    Err(e) => warn!(pid = proc.pid, error = %e, "Failed to kill"),
                }
            }
        }

        Ok(killed_any)
    }

    fn list_running_processes(&self) -> Result<Vec<RunningProcess>> {
        Self::list_ps_processes()
    }

    fn protect_service(&self) -> Result<()> {
        debug!("Service protection not yet implemented on macOS");
        Ok(())
    }

    fn hosts_file_path(&self) -> &str {
        "/etc/hosts"
    }

    fn detect_uninstall_attempts(&self, processes: &[RunningProcess]) -> Vec<u32> {
        let mut pids_to_kill = Vec::new();

        for proc in processes {
            let name_lower = proc.name.to_lowercase();

            let is_uninstaller = MACOS_UNINSTALLER_APPS
                .iter()
                .any(|app| name_lower.contains(&app.to_lowercase()));

            if is_uninstaller {
                info!(
                    pid = proc.pid,
                    name = %proc.name,
                    "Detected cleanup/uninstaller app while protection is active"
                );
                pids_to_kill.push(proc.pid);
            }

            if name_lower == "launchctl" {
                if let Some(cmdline) = self.get_process_cmdline(proc.pid) {
                    let lower = cmdline.to_lowercase();
                    if lower.contains("unload") && lower.contains("focuser") {
                        info!(pid = proc.pid, "Detected launchctl unload of Focuser");
                        pids_to_kill.push(proc.pid);
                    }
                }
            }

            if name_lower == "rm" {
                if let Some(cmdline) = self.get_process_cmdline(proc.pid) {
                    let lower = cmdline.to_lowercase();
                    if lower.contains("focuser") {
                        info!(
                            pid = proc.pid,
                            "Detected manual deletion attempt of Focuser"
                        );
                        pids_to_kill.push(proc.pid);
                    }
                }
            }
        }

        pids_to_kill
    }

    fn protect_installation(&self) -> Result<()> {
        info!("Enabling installation protection on macOS");
        for path in PROTECTED_PATHS {
            let _ = Self::set_immutable(path, true);
        }
        Ok(())
    }

    fn unprotect_installation(&self) -> Result<()> {
        info!("Disabling installation protection on macOS");
        for path in PROTECTED_PATHS {
            let _ = Self::set_immutable(path, false);
        }
        Ok(())
    }

    fn get_process_cmdline(&self, pid: u32) -> Option<String> {
        Self::read_ps_cmdline(pid)
    }
}
