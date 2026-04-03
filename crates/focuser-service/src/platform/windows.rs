#![cfg(windows)]

use focuser_common::error::{FocuserError, Result};
use focuser_common::platform::{PlatformBlocker, RunningProcess};
use focuser_common::types::{AppRule, WebsiteRule};
use tracing::{debug, info, warn};

use crate::hosts;

pub struct WindowsBlocker;

impl WindowsBlocker {
    pub fn new() -> Self {
        Self
    }

    /// Get running processes using Windows ToolHelp API.
    fn snapshot_processes() -> Result<Vec<RunningProcess>> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
            TH32CS_SNAPPROCESS,
        };

        let mut processes = Vec::new();

        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
                .map_err(|e| FocuserError::Platform(format!("Failed to create snapshot: {e}")))?;

            let mut entry = PROCESSENTRY32 {
                dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
                ..Default::default()
            };

            if Process32First(snapshot, &mut entry).is_ok() {
                loop {
                    let name = entry
                        .szExeFile
                        .iter()
                        .take_while(|&&c| c != 0)
                        .map(|&c| c as u8 as char)
                        .collect::<String>();

                    processes.push(RunningProcess {
                        pid: entry.th32ProcessID,
                        name,
                        exe_path: None,   // Would need OpenProcess + QueryFullProcessImageName
                        window_title: None, // Would need EnumWindows
                    });

                    if Process32Next(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
        }

        Ok(processes)
    }

    /// Terminate a process by PID.
    fn kill_process(pid: u32) -> Result<()> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
                .map_err(|e| FocuserError::Platform(format!("Cannot open process {pid}: {e}")))?;

            TerminateProcess(handle, 1)
                .map_err(|e| FocuserError::Platform(format!("Cannot terminate process {pid}: {e}")))?;

            let _ = CloseHandle(handle);
        }

        info!(pid, "Terminated blocked process");
        Ok(())
    }
}

impl PlatformBlocker for WindowsBlocker {
    fn block_websites(&self, rules: &[WebsiteRule]) -> Result<()> {
        // Collect domains from rules that can be represented in hosts file
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
        let processes = Self::snapshot_processes()?;
        let mut killed_any = false;

        for proc in &processes {
            if rule.matches_process(&proc.name, proc.exe_path.as_deref(), proc.window_title.as_deref()) {
                match Self::kill_process(proc.pid) {
                    Ok(()) => killed_any = true,
                    Err(e) => warn!(pid = proc.pid, name = %proc.name, error = %e, "Failed to kill process"),
                }
            }
        }

        Ok(killed_any)
    }

    fn list_running_processes(&self) -> Result<Vec<RunningProcess>> {
        Self::snapshot_processes()
    }

    fn protect_service(&self) -> Result<()> {
        // TODO: Set process to critical, prevent task manager kill
        debug!("Service protection not yet implemented on Windows");
        Ok(())
    }

    fn hosts_file_path(&self) -> &str {
        r"C:\Windows\System32\drivers\etc\hosts"
    }
}
