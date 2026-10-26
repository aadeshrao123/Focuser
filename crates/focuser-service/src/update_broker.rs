//! The broker is a protected copy of the installed authority, run as SYSTEM.
//! Its fixed boot task recovers file replacement even when Tauri/agent is gone.
//! It never launches a downloaded installer or restores a policy database.
use crate::{storage::*, update::*};
use focuser_ipc::*;
use std::{
    fs::File,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows_service::{
    service::ServiceAccess,
    service::ServiceState,
    service_manager::{ServiceManager, ServiceManagerAccess},
};

fn copy_bytes(source: &Path, dest: &Path) -> Result<(), IpcError> {
    let mut input = File::open(source).map_err(|e| error("update_storage", e))?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(dest)
        .map_err(|e| error("update_storage", e))?;
    std::io::copy(&mut input, &mut output)
        .and_then(|_| output.sync_all())
        .map_err(|e| error("update_storage", e))?;
    Ok(())
}
fn schema_version(path: &Path) -> Result<u32, IpcError> {
    rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|c| c.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0)))
        .map_err(|e| error("schema_incompatible", e))
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BrokerIdentity {
    installation: String,
    sha256: String,
    protocol: u32,
}
pub fn verify_broker(root: &Path, installation: &str) -> Result<(), IpcError> {
    let metadata = root.join("broker.json");
    verify_private(&metadata)?;
    let pin: BrokerIdentity =
        serde_json::from_slice(&std::fs::read(metadata).map_err(|e| error("broker_identity", e))?)
            .map_err(|e| error("broker_identity", e))?;
    let broker = root.join(BROKER_NAME);
    verify_private(&broker)?;
    if pin.installation != installation || pin.protocol != 1 || pin.sha256 != hash_file(&broker)? {
        return Err(error(
            "broker_identity",
            "Protected broker identity changed",
        ));
    }
    Ok(())
}

pub fn verify_authenticode(path: &Path, publisher: &str) -> Result<(), IpcError> {
    use ::windows::{
        Win32::{Foundation::HWND, Security::WinTrust::*},
        core::PCWSTR,
    };
    let wide = windows::wide(&path.to_string_lossy());
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
        ..Default::default()
    };
    data.Anonymous.pFile = &mut file;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    unsafe {
        let result = WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
        let checked = (|| {
            if result != 0 {
                return Err(error(
                    "authenticode_invalid",
                    format!("Windows trust verification failed: {result:#x}"),
                ));
            }
            let provider = WTHelperProvDataFromStateData(data.hWVTStateData);
            if provider.is_null() {
                return Err(error("authenticode_invalid", "Missing signature provider"));
            }
            let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
            if signer.is_null() || (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() {
                return Err(error("authenticode_invalid", "Missing signing certificate"));
            }
            let cert = (*(*signer).pasCertChain).pCert;
            if cert.is_null() {
                return Err(error("authenticode_invalid", "Missing leaf certificate"));
            }
            let bytes =
                std::slice::from_raw_parts((*cert).pbCertEncoded, (*cert).cbCertEncoded as usize);
            if digest(bytes) != publisher {
                return Err(error(
                    "publisher_mismatch",
                    "Windows signing certificate does not match the release pin",
                ));
            }
            Ok(())
        })();
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
        checked
    }
}
fn pinned(path: &Path) -> Result<File, IpcError> {
    verify_private(path)?;
    lock_payload(path)
}
fn lock_payload(path: &Path) -> Result<File, IpcError> {
    use ::windows::{Win32::Storage::FileSystem::*, core::PCWSTR};
    use std::os::windows::fs::OpenOptionsExt;
    let attrs =
        unsafe { GetFileAttributesW(PCWSTR(windows::wide(&path.to_string_lossy()).as_ptr())) };
    if attrs == INVALID_FILE_ATTRIBUTES || attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Err(error(
            "package_path",
            "Missing or reparse-point payload refused",
        ));
    }
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .map_err(|e| error("update_storage", e))
}

pub fn policy_digest(path: &Path) -> Result<String, IpcError> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| error("checkpoint_failed", e))?;
    conn.busy_timeout(Duration::from_millis(500))
        .map_err(|e| error("checkpoint_failed", e))?;
    let schema: u32 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .map_err(|e| error("schema_incompatible", e))?;
    if schema != SCHEMA_VERSION {
        return Err(error(
            "schema_incompatible",
            "Rollback/update requires the same policy schema",
        ));
    }
    let mut names=conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(|e|error("checkpoint_failed",e))?;
    let names = names
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| error("checkpoint_failed", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| error("checkpoint_failed", e))?;
    use sha2::{Digest, Sha256};
    let mut snapshot = Sha256::new();
    for name in names {
        let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut columns = conn
            .prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")
            .map_err(|e| error("checkpoint_failed", e))?;
        let columns = columns
            .query_map([&name], |r| r.get::<_, String>(0))
            .map_err(|e| error("checkpoint_failed", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| error("checkpoint_failed", e))?;
        let expressions = columns
            .iter()
            .map(|c| format!("quote({})", quote(c)))
            .collect::<Vec<_>>()
            .join(",");
        snapshot.update(serde_json::to_vec(&name).map_err(|e| error("checkpoint_failed", e))?);
        let order = (1..=columns.len())
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut statement = conn
            .prepare(&format!(
                "SELECT {expressions} FROM {} ORDER BY {order}",
                quote(&name)
            ))
            .map_err(|e| error("checkpoint_failed", e))?;
        let rows = statement
            .query_map([], |r| {
                (0..columns.len())
                    .map(|i| r.get::<_, String>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| error("checkpoint_failed", e))?;
        for row in rows {
            if Instant::now() >= deadline {
                return Err(error(
                    "checkpoint_timeout",
                    "Policy checkpoint exceeds IPC deadline; activation deferred",
                ));
            }
            snapshot.update(
                serde_json::to_vec(&row.map_err(|e| error("checkpoint_failed", e))?)
                    .map_err(|e| error("checkpoint_failed", e))?,
            );
        }
    }
    Ok(format!("{:x}", snapshot.finalize()))
}
pub fn startup_guard(root: &Path) -> Result<(), IpcError> {
    let update = root.join("updates");
    if !update.exists() {
        return Ok(());
    }
    let store = Store::new(update, true)?;
    let Some(j) = store.load()? else {
        return Ok(());
    };
    if matches!(j.phase, Phase::Replacing | Phase::RollingBack)
        || (j.phase == Phase::RecoveryFault && j.checkpoint.is_some())
    {
        return Err(error(
            "update_recovery_required",
            "Boot recovery must finish before policy storage is opened",
        ));
    }
    if matches!(j.phase, Phase::Validating | Phase::RollbackValidating)
        && policy_digest(&root.join("authority.db"))? != j.checkpoint.as_deref().unwrap_or("")
    {
        return Err(error(
            "checkpoint_mismatch",
            "Policy changed across replacement; no database rollback is allowed",
        ));
    }
    Ok(())
}
fn installed_dir() -> Result<PathBuf, IpcError> {
    use winreg::{RegKey, enums::*};
    let scm = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SYSTEM\CurrentControlSet\Services\FocuserAuthority",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .map_err(|e| error("installation_mismatch", e))?;
    let image: String = scm
        .get_value("ImagePath")
        .map_err(|e| error("installation_mismatch", e))?;
    // Registry content is not an arbitrary command: only our fixed canonical
    // installation image and service argument are accepted.
    let dir = unsafe {
        use ::windows::Win32::UI::Shell::*;
        let p = SHGetKnownFolderPath(&FOLDERID_ProgramFilesX64, KF_FLAG_DEFAULT, None)
            .map_err(|e| error("configuration", e))?;
        let text = p.to_string().map_err(|e| error("configuration", e));
        ::windows::Win32::System::Com::CoTaskMemFree(Some(p.0.cast()));
        PathBuf::from(text?).join("Focuser")
    };
    let expected = format!("\"{}\" service", dir.join("focuser-service.exe").display());
    if !image.eq_ignore_ascii_case(&expected) {
        return Err(error(
            "installation_mismatch",
            "Unexpected SCM executable/arguments",
        ));
    }
    Ok(dir)
}
fn service() -> Result<windows_service::service::Service, IpcError> {
    ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .and_then(|m| {
            m.open_service(
                SERVICE_NAME,
                ServiceAccess::QUERY_STATUS | ServiceAccess::START | ServiceAccess::STOP,
            )
        })
        .map_err(|e| error("update_service", e))
}
fn stopped() -> Result<(), IpcError> {
    let service = service()?;
    if service
        .query_status()
        .map_err(|e| error("update_service", e))?
        .current_state
        != ServiceState::Stopped
    {
        service.stop().map_err(|e| error("update_service", e))?;
    }
    let until = Instant::now() + Duration::from_secs(25);
    loop {
        if service
            .query_status()
            .map_err(|e| error("update_service", e))?
            .current_state
            == ServiceState::Stopped
        {
            return Ok(());
        }
        if Instant::now() > until {
            return Err(error("update_stop_timeout", "Service has not stopped"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn start() -> Result<(), IpcError> {
    let service = service()?;
    if service
        .query_status()
        .map_err(|e| error("update_service", e))?
        .current_state
        != ServiceState::Running
    {
        service
            .start::<&str>(&[])
            .map_err(|e| error("update_service", e))?;
    }
    Ok(())
}
fn healthy(expected: &str, security: u64, trust: &Trust) -> Result<(), IpcError> {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(ReplyBody::Update(status)) =
            Client::default().request_blocking(RequestBody::Update(UpdateRequest::Status {}))
            && status.runtime_version == expected
            && status.security_version == security
            && status.schema_version == SCHEMA_VERSION
            && status.update_trust.as_deref() == Some(trust.identity().as_str())
        {
            return Ok(());
        }
        if Instant::now() > until {
            return Err(error(
                "update_start_timeout",
                "Replacement did not validate its authenticated runtime identity",
            ));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}
fn replace_from(
    source: &Path,
    destination: &Path,
    expected: &str,
    trust: &Trust,
    private: bool,
) -> Result<(), IpcError> {
    let mut source = if private {
        pinned(source)?
    } else {
        File::open(source).map_err(|e| error("update_storage", e))?
    };
    if hash_reader(&mut source)? != expected {
        return Err(error(
            "digest_mismatch",
            "Source changed before replacement",
        ));
    }
    use std::io::Seek;
    source.rewind().map_err(|e| error("update_storage", e))?;
    let temp = destination.with_extension("update-new");
    if temp.exists() {
        verify_code(&temp)?;
        std::fs::remove_file(&temp).map_err(|e| error("update_storage", e))?;
    }
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| error("update_storage", e))?;
    std::io::copy(&mut source, &mut out)
        .and_then(|_| out.sync_all())
        .map_err(|e| error("update_storage", e))?;
    drop(out);
    verify_code(&temp)?;
    verify_authenticode(&temp, &trust.publisher)?;
    if hash_file(&temp)? != expected {
        return Err(error("digest_mismatch", "Replacement digest mismatch"));
    }
    let until = Instant::now() + Duration::from_secs(6);
    loop {
        use ::windows::Win32::Storage::FileSystem::*;
        use ::windows::core::PCWSTR;
        let result = unsafe {
            MoveFileExW(
                PCWSTR(windows::wide(&temp.to_string_lossy()).as_ptr()),
                PCWSTR(windows::wide(&destination.to_string_lossy()).as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        match result {
            Ok(()) => break,
            Err(e)
                if Instant::now() < until
                    && matches!(e.code().0 as u32, 0x80070005 | 0x80070020 | 0x80070021) =>
            {
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(error("update_storage", e)),
        }
    }
    Ok(())
}
/// Shared recovery driver, also exercised with failure-injecting harness Ops.
pub trait Ops {
    fn resume_agent(&mut self) {}
    fn rollback_schema(&mut self) -> Result<u32, IpcError> {
        Ok(SCHEMA_VERSION)
    }
    fn backup(&mut self, j: &mut Journal) -> Result<(), IpcError>;
    fn stop(&mut self) -> Result<(), IpcError>;
    fn replace(&mut self, j: &Journal) -> Result<(), IpcError>;
    fn start_validate(&mut self, j: &Journal, rollback: bool) -> Result<(), IpcError>;
    fn restore(&mut self, j: &Journal) -> Result<(), IpcError>;
}
pub fn replace_transaction(
    store: &Store,
    j: &mut Journal,
    ops: &mut impl Ops,
) -> Result<(), IpcError> {
    if let Err(failure) = ops.backup(j) {
        j.phase = Phase::Deferred;
        j.checkpoint = None;
        j.code = failure.code.clone();
        j.old_hashes.clear();
        j.expires_at = j.manifest.as_ref().unwrap().expires_at;
        store.save(j)?;
        ops.resume_agent();
        return Err(failure);
    }
    store.save(j)?;
    let attempt = (|| {
        j.phase = Phase::Replacing;
        j.code = "installing".into();
        store.save(j)?;
        ops.stop()?;
        ops.replace(j)?;
        j.phase = Phase::Validating;
        j.code = "startup_validation".into();
        store.save(j)?;
        ops.start_validate(j, false)?;
        store.raise_floor(j.manifest.as_ref().unwrap().security_floor)?;
        j.phase = Phase::Complete;
        j.code = "complete".into();
        store.save(j)?;
        ops.resume_agent();
        Ok(())
    })();
    if let Err(failure) = attempt {
        if !j.rollback_allowed(store.floor()?, ops.rollback_schema().unwrap_or(u32::MAX)) {
            j.phase = Phase::RecoveryFault;
            j.code = "rollback_incompatible".into();
            store.save(j)?;
            return Err(failure);
        }
        j.phase = Phase::RollingBack;
        j.code = failure.code.clone();
        store.save(j)?;
        let rollback = ops.stop().and_then(|_| ops.restore(j)).and_then(|_| {
            j.phase = Phase::RollbackValidating;
            store.save(j)?;
            ops.start_validate(j, true)?;
            j.phase = Phase::RolledBack;
            j.code = "rolled_back".into();
            store.save(j)?;
            ops.resume_agent();
            Ok(())
        });
        if let Err(e) = rollback {
            j.phase = Phase::RecoveryFault;
            j.code = e.code;
            store.save(j)?;
        }
        return Err(failure);
    }
    Ok(())
}
struct WindowsOps {
    dir: PathBuf,
    stage: Store,
    trust: Trust,
}
pub fn recover_transaction(
    store: &Store,
    j: &mut Journal,
    ops: &mut impl Ops,
) -> Result<(), IpcError> {
    if j.phase.terminal() {
        return Ok(());
    }
    if !j.rollback_allowed(store.floor()?, ops.rollback_schema().unwrap_or(u32::MAX))
        || j.attempts > 2
    {
        j.phase = Phase::RecoveryFault;
        j.code = "rollback_incompatible".into();
        store.save(j)?;
        return Err(error(
            "rollback_incompatible",
            "Cannot safely restore an older runtime",
        ));
    }
    j.phase = Phase::RollingBack;
    j.code = "recovering_interrupted_update".into();
    store.save(j)?;
    let result = ops.stop().and_then(|_| ops.restore(j)).and_then(|_| {
        j.phase = Phase::RollbackValidating;
        store.save(j)?;
        ops.start_validate(j, true)?;
        j.phase = Phase::RolledBack;
        j.code = "rolled_back".into();
        store.save(j)?;
        ops.resume_agent();
        Ok(())
    });
    if let Err(e) = &result {
        if e.code == "maintenance_busy" {
            return result;
        }
        j.phase = Phase::RecoveryFault;
        j.code = e.code.clone();
        store.save(j)?;
    }
    result
}
impl Ops for WindowsOps {
    fn rollback_schema(&mut self) -> Result<u32, IpcError> {
        schema_version(&production_root()?.join("authority.db"))
    }
    fn backup(&mut self, j: &mut Journal) -> Result<(), IpcError> {
        if !j.old_hashes.is_empty() {
            return Ok(());
        }
        for name in j.manifest.as_ref().unwrap().files.iter().map(|f| &f.name) {
            let current = self.dir.join(name);
            if std::fs::metadata(&current)
                .map_err(|e| error("update_storage", e))?
                .len()
                > MAX_FILE
            {
                return Err(error(
                    "known_good_too_large",
                    "Installed binary exceeds backup size bound",
                ));
            }
            verify_code(&current)?;
            verify_authenticode(&current, &self.trust.publisher)?;
            let dest = self.stage.root.join(format!("old-{name}"));
            if dest.exists() {
                self.stage.check(&dest)?;
            } else {
                self.stage.create_file(&dest)?;
            }
            copy_bytes(&current, &dest)?;
            j.old_hashes.insert(name.clone(), hash_file(&dest)?);
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<(), IpcError> {
        stopped()
    }
    fn replace(&mut self, j: &Journal) -> Result<(), IpcError> {
        for file in &j.manifest.as_ref().unwrap().files {
            replace_from(
                &self.stage.root.join(&file.name),
                &self.dir.join(&file.name),
                &file.sha256,
                &self.trust,
                true,
            )?;
        }
        Ok(())
    }
    fn restore(&mut self, j: &Journal) -> Result<(), IpcError> {
        if j.old_hashes.len() != j.manifest.as_ref().unwrap().files.len() {
            return Err(error("rollback_missing", "Known-good backup incomplete"));
        }
        for (name, hash) in &j.old_hashes {
            let target = self.dir.join(name);
            if target.exists() && hash_file(&target)? == *hash {
                continue;
            }
            replace_from(
                &self.stage.root.join(format!("old-{name}")),
                &target,
                hash,
                &self.trust,
                true,
            )?;
        }
        Ok(())
    }
    fn start_validate(&mut self, j: &Journal, rollback: bool) -> Result<(), IpcError> {
        start()?;
        let (version, security) = if rollback {
            (&j.previous_version, j.previous_security)
        } else {
            let m = j.manifest.as_ref().unwrap();
            (&m.version, m.security_version)
        };
        healthy(version, security, &self.trust)?;
        use winreg::{RegKey, enums::*};
        let registration = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Focuser",
                KEY_QUERY_VALUE | KEY_SET_VALUE | KEY_WOW64_64KEY,
            )
            .map_err(|e| error("installation_mismatch", e))?;
        let location: String = registration
            .get_value("InstallLocation")
            .map_err(|e| error("installation_mismatch", e))?;
        let uninstall: String = registration
            .get_value("UninstallString")
            .map_err(|e| error("installation_mismatch", e))?;
        if !location.eq_ignore_ascii_case(&self.dir.to_string_lossy())
            || !uninstall
                .trim_matches('"')
                .eq_ignore_ascii_case(&self.dir.join("uninstall.exe").to_string_lossy())
        {
            return Err(error(
                "installation_mismatch",
                "Uninstall registration no longer belongs to this installation",
            ));
        }
        registration
            .set_value("DisplayVersion", version)
            .map_err(|e| error("update_registry", e))?;
        Ok(())
    }
    fn resume_agent(&mut self) {
        // The task is fixed and unprivileged. Absence of a logged-on owner is
        // degraded observation, never permission to discard policy or journals.
        let _ = crate::installation::tool(
            "schtasks.exe",
            &["/Run", "/TN", crate::installation::AGENT_TASK],
        );
    }
}
pub fn system_tool(name: &str) -> Result<PathBuf, IpcError> {
    use ::windows::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buf = [0u16; 1024];
    let n = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if n == 0 || n >= buf.len() {
        return Err(error("configuration", "System directory unavailable"));
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..n])).join(name))
}
pub fn run(requested: Option<String>) -> Result<(), IpcError> {
    let result = run_inner(requested.clone());
    if let Err(e) = &result
        && e.code != "maintenance_busy"
        && windows::current_identity().is_ok_and(|(sid, _)| sid == windows::SYSTEM_SID)
        && let Ok(root) = production_root()
        && let Ok(store) = Store::new(root.join("updates"), true)
        && let Ok(Some(mut j)) = store.load()
        && requested.as_ref().is_none_or(|id| id == &j.id)
        && verify_broker(&root, &j.installation).is_ok()
    {
        if j.phase == Phase::Authorized && j.checkpoint.is_none() {
            j.phase = Phase::Deferred;
            j.code = e.code.clone();
            j.expires_at = j.manifest.as_ref().map(|m| m.expires_at).unwrap_or(0);
            let _ = store.save(&j);
        }
        let _ = store.atomic(
            "broker-error.json",
            &serde_json::to_vec(
                &serde_json::json!({"transaction":j.id,"code":e.code,"message":e.message}),
            )
            .unwrap(),
        );
    }
    result
}
fn run_inner(requested: Option<String>) -> Result<(), IpcError> {
    if windows::current_identity()?.0 != windows::SYSTEM_SID {
        return Err(error(
            "unauthorized",
            "Only the SYSTEM recovery task or authority can run the broker",
        ));
    }
    let root = production_root()?;
    verify_private(&root)?;
    verify_private(&std::env::current_exe().map_err(|e| error("identity", e))?)?;
    let _lock = StorageLock::maintenance(&root)?;
    let store = Store::new(root.join("updates"), true)?;
    let Some(mut j) = store.load()? else {
        return Ok(());
    };
    verify_broker(&root, &j.installation)?;
    if !std::env::current_exe()
        .map_err(|e| error("identity", e))?
        .to_string_lossy()
        .eq_ignore_ascii_case(&root.join(BROKER_NAME).to_string_lossy())
    {
        return Err(error("unauthorized", "Pinned broker image required"));
    }
    if j.phase.terminal()
        || !j.phase.frozen()
        || (j.phase == Phase::RecoveryFault && j.checkpoint.is_none())
    {
        return Ok(());
    }
    let dir = installed_dir()?;
    let trust = Trust::production()?;
    if j.attempts >= 2 {
        j.phase = Phase::RecoveryFault;
        j.code = "recovery_retry_limit".into();
        store.save(&j)?;
        return Err(error(
            "recovery_retry_limit",
            "Explicit administrator repair required",
        ));
    }
    let envelope = j
        .envelope
        .as_ref()
        .ok_or_else(|| error("package_malformed", "Missing signed envelope"))?;
    // Expired packages cannot activate, but authenticated known-good rollback
    // remains available after reboot. Validate signatures against issuance window.
    let (manifest, hash) =
        if j.phase == Phase::Authorized && j.checkpoint.is_none() && requested.is_some() {
            trust.verify(
                envelope,
                &j.previous_version,
                j.floor,
                chrono::Utc::now().timestamp(),
            )?
        } else {
            trust.verify_recovery(envelope, &j.previous_version, j.floor)?
        };
    if hash != j.artifact_digest {
        return Err(error("digest_mismatch", "Journal/manifest substitution"));
    }
    j.manifest = Some(manifest.clone());
    if j.phase == Phase::Authorized && j.checkpoint.is_none() {
        if requested.as_deref() != Some(&j.id) || chrono::Utc::now().timestamp() >= j.expires_at {
            j.phase = Phase::Deferred;
            j.code = "authorization_expired".into();
            store.save(&j)?;
            return Ok(());
        }
        Client::default().request_blocking(RequestBody::Update(UpdateRequest::BrokerBegin {
            transaction: j.id.clone(),
        }))?;
        j = store
            .load()?
            .ok_or_else(|| error("update_storage", "Checkpoint journal missing"))?;
    }
    j.manifest = Some(manifest.clone());
    if j.phase == Phase::Validating
        && healthy(&manifest.version, manifest.security_version, &trust).is_ok()
    {
        store.raise_floor(manifest.security_floor)?;
        j.phase = Phase::Complete;
        j.code = "complete".into();
        store.save(&j)?;
        let mut ops = WindowsOps {
            dir,
            stage: store.stage_dir(&j.id)?,
            trust,
        };
        ops.resume_agent();
        return Ok(());
    }
    j.attempts += 1;
    store.save(&j)?;
    let stage = store.stage_dir(&j.id)?;
    let mut ops = WindowsOps { dir, stage, trust };
    if j.phase == Phase::Authorized && requested.is_some() {
        replace_transaction(&store, &mut j, &mut ops)
    } else {
        recover_transaction(&store, &mut j, &mut ops)
    }
}

pub fn provision_recovery(
    root: &Path,
    installed: &Path,
    installation: &str,
) -> Result<(), IpcError> {
    Store::new(root.join("updates"), true)?;
    let broker = root.join(BROKER_NAME);
    if broker.exists() {
        verify_private(&broker)?;
        if !root.join("broker.json").exists() && hash_file(&broker)? != hash_file(installed)? {
            return Err(error(
                "broker_upgrade_required",
                "Repair cannot replace the pinned broker binary",
            ));
        }
    } else {
        secure_file(&broker)?;
        copy_bytes(installed, &broker)?;
    }
    let metadata = root.join("broker.json");
    if !metadata.exists() {
        secure_file(&metadata)?;
        std::fs::write(
            &metadata,
            serde_json::to_vec(&BrokerIdentity {
                installation: installation.into(),
                sha256: hash_file(&broker)?,
                protocol: 1,
            })
            .map_err(|e| error("broker_identity", e))?,
        )
        .map_err(|e| error("broker_identity", e))?;
    }
    verify_broker(root, installation)?;
    let (code, current) =
        crate::installation::tool("schtasks.exe", &["/Query", "/TN", RECOVERY_TASK, "/XML"])?;
    if code == 0 && !recovery_task_owned(&current, &broker, installation) {
        return Err(error(
            "installation_mismatch",
            "Update recovery task belongs to another installation",
        ));
    }
    let task = recovery_task_xml(&broker, installation);
    let path = root.join("update-recovery.xml");
    if path.exists() {
        verify_private(&path)?;
    } else {
        secure_file(&path)?;
    }
    let bytes: Vec<u8> = std::iter::once(0xfeffu16)
        .chain(task.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    std::fs::write(&path, bytes).map_err(|e| error("update_storage", e))?;
    crate::installation::checked_tool(
        "schtasks.exe",
        &[
            "/Create",
            "/TN",
            RECOVERY_TASK,
            "/XML",
            &path.to_string_lossy(),
            "/F",
        ],
    )
}

pub fn recovery_task_xml(broker: &Path, installation: &str) -> String {
    // TASK_LOGON_SERVICE_ACCOUNT is a COM/API enum, not an XML logonType.
    // XML for SYSTEM identifies S-1-5-18 and omits LogonType. See Microsoft's
    // Task Scheduler schema: only S4U, Password and interactive values exist.
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?><Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task"><RegistrationInfo><Description>Focuser update recovery {installation}</Description></RegistrationInfo><Triggers><BootTrigger><Enabled>true</Enabled></BootTrigger></Triggers><Principals><Principal id="System"><UserId>S-1-5-18</UserId><RunLevel>HighestAvailable</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><ExecutionTimeLimit>PT5M</ExecutionTimeLimit><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries></Settings><Actions Context="System"><Exec><Command>{}</Command><Arguments>update-broker</Arguments></Exec></Actions></Task>"#,
        broker
            .to_string_lossy()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    )
}
pub fn recovery_task_owned(task: &str, broker: &Path, installation: &str) -> bool {
    if task.len() > 256 * 1024 {
        return false;
    }
    let Ok(doc) = roxmltree::Document::parse(task) else {
        return false;
    };
    let find = |name: &str| {
        doc.descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == name)
            .collect::<Vec<_>>()
    };
    let text = |name: &str| {
        let nodes = find(name);
        if nodes.len() == 1 {
            nodes[0].text().map(str::trim)
        } else {
            None
        }
    };
    find("Principal").len() == 1
        && find("Exec").len() == 1
        && find("Actions").len() == 1
        && find("Actions")[0]
            .children()
            .filter(|n| n.is_element())
            .count()
            == 1
        && text("WorkingDirectory").is_none_or(|v| v.is_empty())
        && find("BootTrigger").len() == 1
        && find("Triggers").len() == 1
        && find("Triggers")[0]
            .children()
            .filter(|n| n.is_element())
            .count()
            == 1
        && text("ExecutionTimeLimit") == Some("PT5M")
        && text("MultipleInstancesPolicy") == Some("IgnoreNew")
        && text("UserId") == Some("S-1-5-18")
        && find("LogonType").is_empty()
        && text("RunLevel") == Some("HighestAvailable")
        && text("Arguments") == Some("update-broker")
        && text("Description") == Some(format!("Focuser update recovery {installation}").as_str())
        && text("Command").is_some_and(|p| p.eq_ignore_ascii_case(&broker.to_string_lossy()))
}
pub fn ensure_idle(root: &Path) -> Result<(), IpcError> {
    let updates = root.join("updates");
    if updates.exists()
        && Store::new(updates, true)?
            .load()?
            .is_some_and(|j| !j.phase.terminal())
    {
        return Err(error(
            "update_busy",
            "Cancel or recover the update before repair/removal",
        ));
    }
    Ok(())
}
pub fn remove_recovery(root: &Path, installation: &str) -> Result<(), IpcError> {
    if !root.join("update-recovery.xml").exists() {
        return Ok(());
    }
    let (code, current) =
        crate::installation::tool("schtasks.exe", &["/Query", "/TN", RECOVERY_TASK, "/XML"])?;
    if code != 0 || !recovery_task_owned(&current, &root.join(BROKER_NAME), installation) {
        return Err(error(
            "installation_mismatch",
            "Recovery task identity could not be verified",
        ));
    }
    crate::installation::checked_tool("schtasks.exe", &["/Delete", "/TN", RECOVERY_TASK, "/F"])
}

#[cfg(test)]
mod locking_tests {
    use super::*;
    #[test]
    fn verified_source_handle_prevents_replacement_or_writes_until_copy_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload.exe");
        let moved = dir.path().join("replaced.exe");
        std::fs::write(&file, b"verified content").unwrap();
        let handle = lock_payload(&file).unwrap();
        assert!(std::fs::OpenOptions::new().write(true).open(&file).is_err());
        assert!(std::fs::rename(&file, &moved).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"verified content");
        drop(handle);
        std::fs::rename(&file, &moved).unwrap();
    }
}
