//! Setup-only discovery and conservative repair. These commands never grant
//! removal, replace an enrolled binary, or initialize over existing policy.
use crate::storage::{ServiceConfiguration, production_root, verify_code, verify_private};
use focuser_ipc::{IpcError, windows};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use windows_service::{service::ServiceAccess, service_manager::*};
use winreg::{RegKey, enums::*};

fn failure(code: &str, error: impl std::fmt::Display) -> IpcError {
    IpcError::new(code, error.to_string())
}

/// Read the interactive desktop shell's real token, rather than the elevated
/// installer's account (which may be alternate administrator credentials).
pub fn desktop_owner() -> Result<String, IpcError> {
    use ::windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};
    let (_, session) = windows::current_identity()?;
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(GetShellWindow(), Some(&mut pid)) };
    if pid == 0 {
        return Err(failure(
            "owner_detection_failed",
            "No interactive desktop was found. Choose the intended user's SID under advanced setup.",
        ));
    }
    let peer = windows::process_identity(pid)?;
    if peer.session != session || !interactive_sid(&peer.sid) {
        return Err(failure(
            "owner_detection_failed",
            "The desktop owner is outside this interactive session. Select the intended user explicitly.",
        ));
    }
    Ok(peer.sid)
}

fn interactive_sid(sid: &str) -> bool {
    sid.starts_with("S-1-5-21-") || sid.starts_with("S-1-12-1-")
}

fn program_files() -> Result<PathBuf, IpcError> {
    use ::windows::{Win32::UI::Shell::*, core::PCWSTR};
    let raw = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None) }
        .map_err(|e| failure("setup_failed", e))?;
    let path = unsafe { PCWSTR(raw.0).to_string() };
    unsafe { ::windows::Win32::System::Com::CoTaskMemFree(Some(raw.0.cast())) };
    path.map(PathBuf::from)
        .map_err(|e| failure("setup_failed", e))
}

pub enum RepairOutcome {
    Repaired,
    Archived(PathBuf),
}

fn require_repair_payload(helper: &Path) -> Result<(), IpcError> {
    // Missing installed code is a damaged installation, not a path-syntax error
    // or permission to extract a different build over retained commitments.
    match std::fs::symlink_metadata(helper) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(failure(
            "repair_payload_missing",
            format!(
                "The installed service executable is missing: {}. Existing authoritative data was preserved. Ordinary Repair cannot replace an unknown enrolled build. An administrator must back up the complete FocuserAuthority data directory and restore the original matching installation files using the administrator recovery procedure in docs/service-phase2.md. Do not delete the data directory or initialize an empty policy.",
                helper.display()
            ),
        )),
        _ => verify_code(helper),
    }
}

/// `true` is only for same-code repair of an enrolled installation. Otherwise
/// a verified unprovisioned payload may be archived, preserving every byte.
pub fn repair() -> Result<RepairOutcome, IpcError> {
    let bundled = crate::installation::elevated()?;
    let parent = program_files()?;
    let installed = parent.join("Focuser");
    let root = production_root()?;
    if root.try_exists().map_err(|e| failure("setup_failed", e))? {
        verify_private(&root)?;
        verify_private(&root.join("service.json"))?;
        let config: ServiceConfiguration = serde_json::from_slice(
            &std::fs::read(root.join("service.json")).map_err(|e| failure("setup_failed", e))?,
        )
        .map_err(|e| failure("setup_failed", e))?;
        let helper = installed.join("focuser-service.exe");
        require_repair_payload(&helper)?;
        let hash = |p: &Path| -> Result<_, IpcError> {
            Ok(Sha256::digest(
                std::fs::read(p).map_err(|e| failure("setup_failed", e))?,
            ))
        };
        if hash(&bundled)? != hash(&helper)? {
            return Err(failure(
                "repair_version_mismatch",
                "Repair requires the exact installer used for this installation. No binaries or policy were replaced. Use the documented administrator recovery route for a damaged older installation.",
            ));
        }
        crate::installation::install_at(&helper, config.owner_sid, None, false, true)?;
        return Ok(RepairOutcome::Repaired);
    }
    // Missing storage is not evidence of permission to remove an installation.
    // Every independent registration must also be absent before archiving.
    let scm = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| failure("setup_failed", e))?;
    match scm.open_service(focuser_ipc::SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(1060) => {}
        _ => {
            return Err(failure(
                "recovery_required",
                "A service registration exists or cannot be verified. Use administrator recovery; setup did not change it.",
            ));
        }
    }
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        match RegKey::predef(hive).open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Focuser",
            KEY_READ | KEY_WOW64_64KEY,
        ) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(failure(
                    "recovery_required",
                    "An uninstall registration exists or cannot be verified. Ordinary removal must use the authority.",
                ));
            }
        }
    }
    // Alternate UAC credentials must not hide the desktop user's legacy entry.
    let users = RegKey::predef(HKEY_USERS);
    for name in users.enum_keys() {
        let name = name.map_err(|e| failure("recovery_required", e))?;
        match users.open_subkey_with_flags(
            format!(r"{name}\Software\Microsoft\Windows\CurrentVersion\Uninstall\Focuser"),
            KEY_READ | KEY_WOW64_64KEY,
        ) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(failure(
                    "recovery_required",
                    "A user's Focuser registration exists or cannot be verified. No payload was archived.",
                ));
            }
        }
    }
    for name in [
        r"Software\Google\Chrome\NativeMessagingHosts\com.focuser.native",
        r"Software\Microsoft\Edge\NativeMessagingHosts\com.focuser.native",
        r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.focuser.native",
        r"Software\Mozilla\NativeMessagingHosts\com.focuser.native",
    ] {
        match RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(name, KEY_READ | KEY_WOW64_64KEY)
        {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(failure(
                    "recovery_required",
                    "Browser integration exists or cannot be verified. Use administrator recovery.",
                ));
            }
        }
    }
    let mut system = vec![0u16; 32768];
    let len = unsafe {
        ::windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut system))
    } as usize;
    if len == 0 || len >= system.len() {
        return Err(failure(
            "setup_failed",
            "Cannot verify startup task storage",
        ));
    }
    let tasks = PathBuf::from(String::from_utf16_lossy(&system[..len])).join(r"Tasks\Focuser");
    if tasks
        .try_exists()
        .map_err(|e| failure("recovery_required", e))?
    {
        return Err(failure(
            "recovery_required",
            "Startup task storage exists. Use administrator recovery.",
        ));
    }
    archive_payload(&parent, &installed).map(RepairOutcome::Archived)
}

fn archive_payload(parent: &Path, installed: &Path) -> Result<PathBuf, IpcError> {
    use std::os::windows::fs::MetadataExt;
    for p in [parent, installed] {
        let meta = std::fs::symlink_metadata(p).map_err(|e| failure("setup_failed", e))?;
        if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
            return Err(failure(
                "recovery_required",
                "Unexpected or reparse-point installation path",
            ));
        }
    }
    if installed != parent.join("Focuser") {
        return Err(failure("recovery_required", "Unexpected installation path"));
    }
    let allowed = [
        "focuser-ui.exe",
        "focuser-cli.exe",
        "focuser-native.exe",
        "focuser-service.exe",
        "focuser-session-agent.exe",
        "WebView2Loader.dll",
    ];
    let entries = std::fs::read_dir(installed).map_err(|e| failure("setup_failed", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| failure("setup_failed", e))?;
        let meta =
            std::fs::symlink_metadata(entry.path()).map_err(|e| failure("setup_failed", e))?;
        if !meta.is_file()
            || meta.file_attributes() & 0x400 != 0
            || !allowed.iter().any(|name| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(name)
            })
        {
            return Err(failure(
                "recovery_required",
                "Unexpected files in the incomplete installation. Nothing was moved or deleted.",
            ));
        }
    }
    let archive = parent.join(format!("Focuser-Incomplete-{}", uuid::Uuid::new_v4()));
    std::fs::rename(installed, &archive).map_err(|e| failure("setup_failed", e))?;
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_repair_payload_reports_recovery_without_creating_or_changing_files() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("retained-policy.db");
        std::fs::write(&marker, b"existing commitments").unwrap();
        let helper = dir.path().join("Focuser").join("focuser-service.exe");
        let error = require_repair_payload(&helper).unwrap_err();
        assert_eq!(error.code, "repair_payload_missing");
        assert!(error.message.contains(helper.to_str().unwrap()));
        assert!(error.message.contains("administrator recovery"));
        assert!(!helper.parent().unwrap().exists());
        assert_eq!(std::fs::read(marker).unwrap(), b"existing commitments");
    }

    #[test]
    fn incomplete_payload_is_archived_without_deleting_data_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("Focuser");
        std::fs::create_dir(&installed).unwrap();
        std::fs::write(installed.join("focuser-service.exe"), b"retained bytes").unwrap();
        let archive = archive_payload(dir.path(), &installed).unwrap();
        assert_eq!(
            std::fs::read(archive.join("focuser-service.exe")).unwrap(),
            b"retained bytes"
        );
        assert!(!installed.exists());
        assert!(archive_payload(dir.path(), &installed).is_err());
        std::fs::create_dir(&installed).unwrap();
        std::fs::write(installed.join("authority.db"), b"never touch policy").unwrap();
        assert!(archive_payload(dir.path(), &installed).is_err());
        assert!(installed.join("authority.db").exists());
    }
    #[test]
    fn desktop_owner_filter_rejects_service_and_group_identities() {
        assert!(!interactive_sid("S-1-5-18"));
        assert!(!interactive_sid("S-1-5-32-544"));
        assert!(interactive_sid("S-1-5-21-1-2-3-1001"));
        assert!(interactive_sid("S-1-12-1-1-2-3-4"));
    }
}
