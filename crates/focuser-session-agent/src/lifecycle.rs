//! The task runs at logon with InteractiveToken/LeastPrivilege. A local named
//! mutex prevents two bridge/guard processes in the same user session.
#[cfg(windows)]
pub fn singleton() -> anyhow::Result<Option<focuser_ipc::windows::OwnedHandle>> {
    use windows::{
        Win32::{Foundation::*, System::Threading::*},
        core::PCWSTR,
    };
    let (sid, session) = focuser_ipc::windows::current_identity()?;
    anyhow::ensure!(session != 0, "An interactive session is required");
    anyhow::ensure!(
        !focuser_ipc::windows::current_is_elevated()?,
        "The session agent must run unprivileged"
    );
    let descriptor =
        focuser_ipc::windows::Descriptor::from_sddl(&format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})"))?;
    let name = focuser_ipc::windows::wide(&format!(r"Local\Focuser.SessionAgent.{sid}"));
    let handle =
        unsafe { CreateMutexW(Some(&descriptor.attributes()), false, PCWSTR(name.as_ptr())) }?;
    let existing = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let handle = focuser_ipc::windows::OwnedHandle(handle);
    Ok(if existing { None } else { Some(handle) })
}

#[derive(Default)]
pub struct FeedbackCursor {
    generation: String,
    pub sequence: u64,
}
impl FeedbackCursor {
    /// Initial connection/restart skips stale notifications; reconnect to the
    /// same generation resumes without replaying previously delivered events.
    pub fn synchronize(&mut self, generation: &str) -> bool {
        let same = self.generation == generation;
        if !same {
            self.generation = generation.into();
            self.sequence = 0;
        }
        same
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reconnect_and_service_restart_do_not_replay_feedback() {
        let mut c = FeedbackCursor::default();
        assert!(!c.synchronize("a"));
        c.sequence = 12;
        assert!(c.synchronize("a"));
        assert_eq!(c.sequence, 12);
        assert!(!c.synchronize("b"));
        assert_eq!(c.sequence, 0);
    }
}
#[cfg(windows)]
pub fn startup_targets_installation(value: &str, ui: &std::path::Path) -> bool {
    use windows::{
        Win32::{
            Foundation::{HLOCAL, LocalFree},
            UI::Shell::CommandLineToArgvW,
        },
        core::PCWSTR,
    };
    let value = focuser_ipc::windows::wide(value);
    let mut count = 0;
    let args = unsafe { CommandLineToArgvW(PCWSTR(value.as_ptr()), &mut count) };
    if args.is_null() {
        return false;
    }
    let first = if count > 0 {
        unsafe { (*args).to_string() }.ok()
    } else {
        None
    };
    unsafe {
        let _ = LocalFree(HLOCAL(args.cast()));
    }
    first.is_some_and(|p| {
        std::path::Path::new(&p).is_absolute() && p.eq_ignore_ascii_case(&ui.to_string_lossy())
    })
}
#[cfg(windows)]
pub fn cleanup_user_startup() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    cleanup_user_startup_for(exe.parent().ok_or("No installation directory")?)
}
#[cfg(windows)]
pub fn cleanup_user_startup_for(dir: &std::path::Path) -> Result<(), String> {
    use winreg::{RegKey, enums::*};
    let ui = dir.join("focuser-ui.exe");
    let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        KEY_READ | KEY_WRITE,
    ) {
        Ok(key) => key,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    for name in ["Focuser", "focuser-ui"] {
        if let Ok(value) = key.get_value::<String, _>(name)
            && startup_targets_installation(&value, &ui)
        {
            key.delete_value(name).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
#[cfg(all(test, windows))]
mod startup_tests {
    use super::*;
    #[test]
    fn cleanup_only_matches_the_actual_installed_ui() {
        let ui = std::path::Path::new(r"C:\Program Files\Focuser\focuser-ui.exe");
        assert!(startup_targets_installation(
            r#""C:\Program Files\Focuser\focuser-ui.exe" --autostart"#,
            ui
        ));
        assert!(!startup_targets_installation(
            r#""C:\Other\focuser-ui.exe" --autostart"#,
            ui
        ));
        assert!(!startup_targets_installation("focuser-ui.exe", ui));
        assert!(!startup_targets_installation(
            "cmd.exe /c focuser-ui.exe",
            ui
        ));
    }
}

/// Run only in the enrolled user's unprivileged agent, after service consumption
/// and browser journal restoration. Never walk user-writable trees as SYSTEM.
#[cfg(windows)]
pub fn cleanup_user_data() -> Result<(), focuser_ipc::IpcError> {
    let exe = std::env::current_exe()
        .map_err(|e| focuser_ipc::IpcError::new("cleanup_failed", e.to_string()))?;
    cleanup_user_data_for(exe.parent().ok_or_else(|| {
        focuser_ipc::IpcError::new("cleanup_failed", "Installation directory missing")
    })?)
}
#[cfg(windows)]
pub fn cleanup_user_data_for(dir: &std::path::Path) -> Result<(), focuser_ipc::IpcError> {
    use focuser_ipc::{IpcError, cleanup::OwnedTree};
    if focuser_ipc::windows::current_is_elevated()? {
        return Err(IpcError::new(
            "unauthorized",
            "User data cleanup must run unprivileged",
        ));
    }
    let dirs = directories::BaseDirs::new()
        .ok_or_else(|| IpcError::new("cleanup_failed", "User known folders unavailable"))?;
    // Verified against directories 6 and Tauri's com.focuser.app data paths.
    // Do not remove the containing organization folder, browser profiles,
    // extension files, imported files elsewhere, or separately stored backups.
    let roots = user_data_roots(dirs.data_dir(), dirs.data_local_dir());
    let mut trees = Vec::new();
    for root in &roots {
        match std::fs::symlink_metadata(root) {
            Ok(_) => trees.push(OwnedTree::inspect(root, &|_| Ok(()))?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(IpcError::new(
                    "cleanup_failed",
                    format!("{}: {e}", root.display()),
                ));
            }
        }
    }
    cleanup_user_native_hosts(dir)?;
    cleanup_user_shortcuts(dir)?;
    for tree in trees {
        tree.remove()?;
    }
    Ok(())
}

fn user_data_roots(roaming: &std::path::Path, local: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    for base in [roaming, local] {
        for suffix in ["focuser/Focuser", "com.focuser.app"] {
            let path = base.join(suffix);
            if !roots.contains(&path) {
                roots.push(path);
            }
        }
    }
    roots
}

#[cfg(windows)]
pub fn orphaned_installation() -> Result<std::path::PathBuf, focuser_ipc::IpcError> {
    use focuser_ipc::IpcError;
    use windows::{
        Win32::{
            System::{Com::CoTaskMemFree, Services::*},
            UI::Shell::*,
        },
        core::PCWSTR,
    };
    let fail = |e: String| IpcError::new("recovery_unverified", e);
    if focuser_ipc::windows::current_is_elevated()? {
        return Err(fail("User-profile recovery must run unprivileged".into()));
    }
    let raw = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None) }
        .map_err(|e| fail(e.to_string()))?;
    let folder = unsafe { raw.to_string() };
    unsafe {
        CoTaskMemFree(Some(raw.0.cast()));
    }
    let dir = std::path::PathBuf::from(folder.map_err(|e| fail(e.to_string()))?).join("Focuser");
    if dir.try_exists().map_err(|e| fail(e.to_string()))? {
        return Err(fail(
            "Installed files remain; use the ordinary uninstaller".into(),
        ));
    }
    if focuser_ipc::paths::production_root()?
        .try_exists()
        .map_err(|e| fail(e.to_string()))?
    {
        return Err(fail("Authority storage remains; machine recovery must authorize and archive it before user-profile cleanup".into()));
    }
    let scm = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
        .map_err(|e| fail(e.to_string()))?;
    let result = unsafe {
        OpenServiceW(
            scm,
            PCWSTR(focuser_ipc::windows::wide(focuser_ipc::SERVICE_NAME).as_ptr()),
            SERVICE_QUERY_STATUS,
        )
    };
    unsafe {
        let _ = CloseServiceHandle(scm);
    }
    match result {
        Ok(service) => {
            unsafe {
                let _ = CloseServiceHandle(service);
            }
            return Err(fail(
                "Service registration remains; complete machine recovery first".into(),
            ));
        }
        Err(e) if (e.code().0 as u32 & 0xffff) == 1060 => {}
        Err(e) => return Err(fail(e.to_string())),
    }
    Ok(dir)
}

#[cfg(windows)]
fn cleanup_user_shortcuts(dir: &std::path::Path) -> Result<(), focuser_ipc::IpcError> {
    use focuser_ipc::IpcError;
    use windows::{
        Win32::{System::Com::*, UI::Shell::*},
        core::{Interface, PCWSTR},
    };
    let fail = |e: String| IpcError::new("cleanup_failed", e);
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
        .map_err(|e| fail(e.to_string()))?;
    let _apartment = Apartment;
    for id in [FOLDERID_Desktop, FOLDERID_Programs] {
        let raw = unsafe { SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None) }
            .map_err(|e| fail(e.to_string()))?;
        let folder = unsafe { raw.to_string() };
        unsafe {
            CoTaskMemFree(Some(raw.0.cast()));
        }
        let folder = std::path::PathBuf::from(folder.map_err(|e| fail(e.to_string()))?);
        for path in [
            folder.join("Focuser.lnk"),
            folder.join("Focuser/Focuser.lnk"),
        ] {
            match std::fs::symlink_metadata(&path) {
                Ok(meta) => {
                    use std::os::windows::fs::MetadataExt;
                    if !meta.is_file() || meta.file_attributes() & 0x400 != 0 {
                        return Err(fail(format!(
                            "Refusing unexpected shortcut: {}",
                            path.display()
                        )));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(fail(e.to_string())),
            }
            for parent in path.ancestors().skip(1).filter(|p| p.parent().is_some()) {
                use std::os::windows::fs::MetadataExt;
                let meta = std::fs::symlink_metadata(parent).map_err(|e| fail(e.to_string()))?;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err(fail(format!(
                        "Refusing redirected shortcut directory: {}",
                        parent.display()
                    )));
                }
            }
            let link: IShellLinkW =
                unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
                    .map_err(|e| fail(e.to_string()))?;
            let persist: IPersistFile = link.cast().map_err(|e| fail(e.to_string()))?;
            unsafe {
                persist.Load(
                    PCWSTR(focuser_ipc::windows::wide(&path.to_string_lossy()).as_ptr()),
                    STGM_READ,
                )
            }
            .map_err(|e| fail(e.to_string()))?;
            let mut target = vec![0u16; 32768];
            unsafe { link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32) }
                .map_err(|e| fail(e.to_string()))?;
            let len = target.iter().position(|c| *c == 0).unwrap_or(target.len());
            let target = String::from_utf16_lossy(&target[..len]);
            if target.eq_ignore_ascii_case(&dir.join("focuser-ui.exe").to_string_lossy()) {
                std::fs::remove_file(&path).map_err(|e| fail(e.to_string()))?;
            }
        }
        // Removing an empty, obsolete app folder is safe; preserve others' links.
        let _ = std::fs::remove_dir(folder.join("Focuser"));
    }
    Ok(())
}

#[cfg(windows)]
fn cleanup_user_native_hosts(dir: &std::path::Path) -> Result<(), focuser_ipc::IpcError> {
    use focuser_ipc::IpcError;
    use winreg::{RegKey, enums::*};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
        for (browser, manifest) in [
            (r"Google\Chrome", "native-chromium.json"),
            (r"Microsoft\Edge", "native-chromium.json"),
            (r"BraveSoftware\Brave-Browser", "native-chromium.json"),
            ("Mozilla", "native-firefox.json"),
        ] {
            let path = format!(r"Software\{browser}\NativeMessagingHosts\com.focuser.native");
            let key = match hkcu.open_subkey_with_flags(&path, KEY_READ | view) {
                Ok(key) => key,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(IpcError::new("cleanup_failed", e.to_string())),
            };
            let value: String = key
                .get_value("")
                .map_err(|e| IpcError::new("cleanup_failed", e.to_string()))?;
            if value.eq_ignore_ascii_case(&dir.join(manifest).to_string_lossy()) {
                drop(key);
                hkcu.delete_subkey_with_flags(&path, view)
                    .map_err(|e| IpcError::new("cleanup_failed", e.to_string()))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[test]
    fn user_roots_are_exact_app_directories_not_profiles_or_organization_roots() {
        let roots = user_data_roots(
            std::path::Path::new("R:/Roaming"),
            std::path::Path::new("R:/Local"),
        );
        assert_eq!(roots.len(), 4);
        assert!(
            roots
                .iter()
                .all(|p| p.ends_with("focuser/Focuser") || p.ends_with("com.focuser.app"))
        );
        assert_eq!(
            user_data_roots(
                std::path::Path::new("R:/same"),
                std::path::Path::new("R:/same")
            )
            .len(),
            2
        );
    }
}
