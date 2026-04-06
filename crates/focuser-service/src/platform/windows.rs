use focuser_common::error::{FocuserError, Result};
use focuser_common::platform::{PlatformBlocker, RunningProcess};
use focuser_common::types::{AppRule, WebsiteRule};
use tracing::{debug, info, warn};

use crate::hosts;

const UNINSTALLER_PROCESSES: &[&str] = &[
    "msiexec.exe",
    "unins000.exe",
    "uninstall.exe",
    "uninst.exe",
    "au_.exe",
];

pub struct WindowsBlocker;

impl WindowsBlocker {
    pub fn new() -> Self {
        Self
    }

    fn snapshot_processes() -> Result<Vec<RunningProcess>> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
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
                        exe_path: None,
                        window_title: None,
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

    fn kill_process(pid: u32) -> Result<()> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
                .map_err(|e| FocuserError::Platform(format!("Cannot open process {pid}: {e}")))?;

            TerminateProcess(handle, 1).map_err(|e| {
                FocuserError::Platform(format!("Cannot terminate process {pid}: {e}"))
            })?;

            let _ = CloseHandle(handle);
        }

        info!(pid, "Terminated blocked process");
        Ok(())
    }

    fn read_process_cmdline(pid: u32) -> Option<String> {
        let output = std::process::Command::new("wmic")
            .args([
                "process",
                "where",
                &format!("ProcessId={pid}"),
                "get",
                "CommandLine",
                "/format:list",
            ])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if let Some(cmdline) = trimmed.strip_prefix("CommandLine=") {
                if !cmdline.is_empty() {
                    return Some(cmdline.to_string());
                }
            }
        }
        None
    }

    fn cmdline_targets_focuser(cmdline: &str) -> bool {
        let lower = cmdline.to_lowercase();
        lower.contains("focuser")
            || lower.contains("focuser-service")
            || lower.contains("focuser-ui")
            || lower.contains("focuser-cli")
    }

    fn hide_from_apps_list() -> Result<()> {
        use winreg::RegKey;
        use winreg::enums::*;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let uninstall_path = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

        if let Ok(uninstall_key) = hklm.open_subkey_with_flags(uninstall_path, KEY_READ) {
            for name in uninstall_key.enum_keys().filter_map(|k| k.ok()) {
                if let Ok(subkey) =
                    uninstall_key.open_subkey_with_flags(&name, KEY_READ | KEY_WRITE)
                {
                    let display_name: std::result::Result<String, _> =
                        subkey.get_value("DisplayName");
                    if let Ok(dn) = display_name {
                        if dn.to_lowercase().contains("focuser") {
                            let _ = subkey.set_value("SystemComponent", &1u32);
                            info!("Hidden Focuser from Apps & Features (key: {})", name);
                            return Ok(());
                        }
                    }
                }
            }
        }

        debug!("Focuser uninstall registry key not found");
        Ok(())
    }

    fn show_in_apps_list() -> Result<()> {
        use winreg::RegKey;
        use winreg::enums::*;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let uninstall_path = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

        if let Ok(uninstall_key) = hklm.open_subkey_with_flags(uninstall_path, KEY_READ) {
            for name in uninstall_key.enum_keys().filter_map(|k| k.ok()) {
                if let Ok(subkey) =
                    uninstall_key.open_subkey_with_flags(&name, KEY_READ | KEY_WRITE)
                {
                    let display_name: std::result::Result<String, _> =
                        subkey.get_value("DisplayName");
                    if let Ok(dn) = display_name {
                        if dn.to_lowercase().contains("focuser") {
                            let _ = subkey.delete_value("SystemComponent");
                            info!("Restored Focuser in Apps & Features (key: {})", name);
                            return Ok(());
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl PlatformBlocker for WindowsBlocker {
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
        let processes = Self::snapshot_processes()?;
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
                        warn!(pid = proc.pid, name = %proc.name, error = %e, "Failed to kill process")
                    }
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

    fn detect_uninstall_attempts(&self, processes: &[RunningProcess]) -> Vec<u32> {
        let mut pids_to_kill = Vec::new();

        for proc in processes {
            let name_lower = proc.name.to_lowercase();

            let is_uninstaller = UNINSTALLER_PROCESSES.iter().any(|u| name_lower == *u);

            if is_uninstaller {
                if let Some(cmdline) = self.get_process_cmdline(proc.pid) {
                    if Self::cmdline_targets_focuser(&cmdline) {
                        info!(
                            pid = proc.pid,
                            name = %proc.name,
                            "Detected uninstall attempt targeting Focuser"
                        );
                        pids_to_kill.push(proc.pid);
                    }
                }
            }

            if name_lower == "powershell.exe" || name_lower == "pwsh.exe" || name_lower == "cmd.exe"
            {
                if let Some(cmdline) = self.get_process_cmdline(proc.pid) {
                    let cmd_lower = cmdline.to_lowercase();
                    if (cmd_lower.contains("uninstall") || cmd_lower.contains("remove"))
                        && Self::cmdline_targets_focuser(&cmdline)
                    {
                        info!(
                            pid = proc.pid,
                            name = %proc.name,
                            "Detected shell-based uninstall attempt"
                        );
                        pids_to_kill.push(proc.pid);
                    }
                }
            }
        }

        pids_to_kill
    }

    fn protect_installation(&self) -> Result<()> {
        info!("Enabling installation protection on Windows");
        Self::hide_from_apps_list()?;
        Ok(())
    }

    fn unprotect_installation(&self) -> Result<()> {
        info!("Disabling installation protection on Windows");
        Self::show_in_apps_list()?;
        Ok(())
    }

    fn get_process_cmdline(&self, pid: u32) -> Option<String> {
        Self::read_process_cmdline(pid)
    }
}
