#![cfg(target_os = "macos")]

use focuser_common::error::{FocuserError, Result};
use focuser_common::platform::{PlatformBlocker, RunningProcess};
use focuser_common::types::{AppRule, WebsiteRule};
use tracing::{debug, info, warn};

use crate::hosts;

pub struct MacOsBlocker;

impl MacOsBlocker {
    pub fn new() -> Self {
        Self
    }

    /// List processes using `ps` command (simple and reliable).
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
                    let short_name = name
                        .rsplit('/')
                        .next()
                        .unwrap_or(&name)
                        .to_string();

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
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
            .map_err(|e| FocuserError::Platform(format!("Cannot kill process {pid}: {e}")))?;

        info!(pid, "Terminated blocked process");
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
            if rule.matches_process(&proc.name, proc.exe_path.as_deref(), proc.window_title.as_deref()) {
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
}
