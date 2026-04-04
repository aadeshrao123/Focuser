use focuser_common::error::{FocuserError, Result};
use focuser_common::platform::{PlatformBlocker, RunningProcess};
use focuser_common::types::{AppRule, WebsiteRule};
use tracing::{debug, info, warn};

use crate::hosts;

pub struct LinuxBlocker;

impl LinuxBlocker {
    pub fn new() -> Self {
        Self
    }

    /// Read running processes from /proc.
    fn read_proc_processes() -> Result<Vec<RunningProcess>> {
        let mut processes = Vec::new();

        let entries = std::fs::read_dir("/proc")
            .map_err(|e| FocuserError::Platform(format!("Cannot read /proc: {e}")))?;

        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Only look at numeric directories (PIDs)
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
}
