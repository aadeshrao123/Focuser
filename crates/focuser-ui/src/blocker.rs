//! Background blocking loop — syncs hosts file and kills blocked processes.

use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tracing::info;

use crate::AppState;

const HOSTS_BEGIN: &str = "# ──── BEGIN FOCUSER BLOCK ────";
const HOSTS_END: &str = "# ──── END FOCUSER BLOCK ────";

/// Runs the blocking loop in a background thread.
/// Every 3 seconds: re-sync hosts file and check for blocked processes.
pub fn run_blocking_loop(state: Arc<AppState>) {
    info!("Background blocker started");
    loop {
        thread::sleep(Duration::from_secs(3));

        // Refresh engine cache
        if let Ok(mut eng) = state.engine.lock() {
            let _ = eng.refresh();

            // Sync hosts file
            let domains = eng.collect_blocked_domains();
            sync_hosts_file(&domains);

            // Kill blocked processes
            kill_blocked_processes(&eng);
        }
    }
}

/// Apply blocks to the system hosts file.
pub fn apply_hosts_blocks(domains: &[String]) -> Result<(), String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let new_content = replace_section(&content, domains);
    std::fs::write(&path, &new_content)
        .map_err(|e| format!("Cannot write {path}: {e}. Run as administrator."))?;
    flush_dns();
    info!(count = domains.len(), "Hosts file updated");
    Ok(())
}

/// Remove all Focuser entries from hosts file.
pub fn remove_hosts_blocks() -> Result<(), String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let new_content = replace_section(&content, &[]);
    std::fs::write(&path, &new_content)
        .map_err(|e| format!("Cannot write {path}: {e}. Run as administrator."))?;
    flush_dns();
    info!("Hosts file cleaned");
    Ok(())
}

fn sync_hosts_file(domains: &[String]) {
    let path = hosts_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let new_content = replace_section(&content, domains);
    if content != new_content {
        if let Err(e) = std::fs::write(&path, &new_content) {
            // Silently fail if not admin — warn is done once above
            let _ = e;
        } else {
            flush_dns();
        }
    }
}

fn kill_blocked_processes(_eng: &focuser_core::BlockEngine) {
    #[cfg(windows)]
    {
        kill_blocked_processes_windows(_eng);
    }
}

#[cfg(windows)]
fn kill_blocked_processes_windows(eng: &focuser_core::BlockEngine) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::System::Threading::*;

    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return,
        };

        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };

        if Process32First(snapshot, &mut entry).is_ok() {
            loop {
                let name: String = entry
                    .szExeFile
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();

                if let Some(list_name) = eng.check_app(&name, None, None) {
                    let pid = entry.th32ProcessID;
                    // Don't kill ourselves or system processes
                    #[allow(clippy::collapsible_if)]
                    if pid > 4 && pid != std::process::id() {
                        if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                            let _ = TerminateProcess(handle, 1);
                            let _ = CloseHandle(handle);
                            info!(pid, name = %name, list = %list_name, "Killed blocked process");
                            let _ = eng.record_blocked(&name);
                        }
                    }
                }

                if Process32Next(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }
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

    for line in content.lines() {
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
            result.push('\n');
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
