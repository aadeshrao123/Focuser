//! Provision/migrate only through explicit operator commands. Starting a client
//! or a development authority never imports the user's live profile database.
use focuser_ipc::IpcError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfiguration {
    pub version: u32,
    pub owner_sid: String,
    pub agent_image: PathBuf,
    pub native_image: PathBuf,
}

pub fn ensure_same_configuration(
    existing: &ServiceConfiguration,
    requested: &ServiceConfiguration,
) -> Result<(), IpcError> {
    if existing != requested {
        return Err(IpcError::new(
            "installation_mismatch",
            "Repair cannot change the owner, endpoint version or installed binary paths",
        ));
    }
    Ok(())
}

/// Recovery/repair is never permission to manufacture a new empty policy.
pub fn validate_initialization(
    existing_root: bool,
    db_exists: bool,
    removal_started: bool,
    empty: bool,
    repair: bool,
) -> Result<(), IpcError> {
    if removal_started {
        return Err(IpcError::new(
            "maintenance_interrupted",
            "Inspect the removal journal and use documented administrator recovery",
        ));
    }
    if existing_root && !db_exists && (empty || repair) {
        return Err(IpcError::new(
            "storage_missing",
            "Existing authority cannot be replaced with an empty database; restore a verified backup through administrator recovery",
        ));
    }
    Ok(())
}

pub use focuser_ipc::paths::{ensure_development_database, production_root};

/// Same-code repair must not open an unchanged protected manifest for writing.
/// Changed content still needs ordinary filesystem authorization; no lock bypass.
pub fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    match std::fs::read(path) {
        Ok(current) if current == bytes => Ok(()),
        Ok(_) => std::fs::write(path, bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => std::fs::write(path, bytes),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
pub fn secure_directory(path: &Path) -> Result<(), IpcError> {
    if path.exists() {
        return verify_private(path);
    }
    use windows::{Win32::Storage::FileSystem::CreateDirectoryW, core::PCWSTR};
    let descriptor =
        focuser_ipc::windows::Descriptor::from_sddl("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)")?;
    let attributes = descriptor.attributes();
    unsafe {
        CreateDirectoryW(
            PCWSTR(focuser_ipc::windows::wide(&path.to_string_lossy()).as_ptr()),
            Some(&attributes),
        )
    }
    .map_err(storage)?;
    verify_private(path)
}
#[cfg(not(windows))]
pub fn secure_directory(_: &Path) -> Result<(), IpcError> {
    Err(IpcError::new(
        "unsupported",
        "Private Windows storage required",
    ))
}

fn storage(error: impl std::fmt::Display) -> IpcError {
    IpcError::new("storage", error.to_string())
}

/// Reject a missing/corrupt import before provisioning creates persistent state.
pub fn validate_source(source: &Path) -> Result<(), IpcError> {
    let db =
        rusqlite::Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| IpcError::new("import_invalid", e.to_string()))?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(storage)?;
    let result: String = db
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| IpcError::new("import_invalid", e.to_string()))?;
    if result != "ok" {
        return Err(IpcError::new("import_invalid", result));
    }
    let tables: i64 = db.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('block_lists', 'settings')",
        [], |row| row.get(0),
    ).map_err(storage)?;
    if tables != 2 {
        return Err(IpcError::new(
            "import_invalid",
            "Choose an existing Focuser profile database",
        ));
    }
    Ok(())
}

/// SQLite online backup includes all tables, active sessions and WAL contents.
/// The original is opened read-only. A checked staging file is atomically
/// published only when complete, and never overwrites an existing authority.
pub fn migrate(source: &Path, destination: &Path, private: bool) -> Result<(), IpcError> {
    if destination.exists() {
        return Err(IpcError::new(
            "already_initialized",
            "Destination already exists; refusing to replace authoritative state",
        ));
    }
    let source = source.canonicalize().map_err(storage)?;
    if source == destination {
        return Err(IpcError::new(
            "validation",
            "Source and destination must differ",
        ));
    }
    let stage = destination.with_extension(format!("migration-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        if private {
            secure_file(&stage)?;
        }
        let original = rusqlite::Connection::open_with_flags(
            &source,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(storage)?;
        original
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(storage)?;
        let mut copied = rusqlite::Connection::open(&stage).map_err(storage)?;
        {
            let backup = rusqlite::backup::Backup::new(&original, &mut copied).map_err(storage)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                match backup.step(128).map_err(storage)? {
                    rusqlite::backup::StepResult::Done => break,
                    _ if std::time::Instant::now() >= deadline => {
                        return Err(IpcError::new(
                            "migration_busy",
                            "Migration timed out; source remains intact",
                        ));
                    }
                    _ => std::thread::sleep(std::time::Duration::from_millis(10)),
                }
            }
        }
        let integrity: String = copied
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(storage)?;
        if integrity != "ok" {
            return Err(IpcError::new("migration_integrity", integrity));
        }
        drop(copied);
        // Upgrade only the copy; failure cannot partially migrate the original.
        let migrated = focuser_core::Database::open(&stage).map_err(storage)?;
        let _ = focuser_core::BlockEngine::new(migrated).map_err(storage)?;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&stage)
            .map_err(storage)?
            .sync_all()
            .map_err(storage)?;
        // Hard-link is an atomic, create-if-absent publication on the same NTFS
        // volume. Unlike rename on Unix it cannot silently replace a raced file.
        std::fs::hard_link(&stage, destination).map_err(storage)?;
        std::fs::remove_file(&stage).map_err(storage)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&stage);
    }
    result
}

#[cfg(windows)]
pub fn secure_file(path: &Path) -> Result<(), IpcError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::{Foundation::*, Storage::FileSystem::*};
    use windows::core::PCWSTR;
    let descriptor =
        focuser_ipc::windows::Descriptor::from_sddl("O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)")?;
    let attributes = descriptor.attributes();
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            Some(&attributes),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            HANDLE::default(),
        )
    }
    .map_err(storage)?;
    drop(focuser_ipc::windows::OwnedHandle(handle));
    Ok(())
}
#[cfg(not(windows))]
pub fn secure_file(_: &Path) -> Result<(), IpcError> {
    Err(IpcError::new(
        "unsupported",
        "Private authority files require Windows ACLs",
    ))
}

#[cfg(windows)]
pub fn provision(
    configuration: &ServiceConfiguration,
    initialize_empty: bool,
) -> Result<(), IpcError> {
    use windows::Win32::Storage::FileSystem::CreateDirectoryW;
    use windows::core::PCWSTR;
    focuser_ipc::windows::validate_sid(&configuration.owner_sid)?;
    let root = production_root()?;
    if root.exists() {
        verify_private(&root)?;
        verify_private(&root.join("service.json"))?;
        verify_private(&root.join("authority.lock"))?;
        let existing: ServiceConfiguration =
            serde_json::from_slice(&std::fs::read(root.join("service.json")).map_err(storage)?)
                .map_err(storage)?;
        ensure_same_configuration(&existing, configuration)?;
        if initialize_empty && !root.join("authority.db").exists() {
            return Err(IpcError::new(
                "migration_required",
                "Existing provisioning has no database; explicitly migrate or repair it",
            ));
        }
        return Ok(());
    }
    let descriptor =
        focuser_ipc::windows::Descriptor::from_sddl("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)")?;
    let attributes = descriptor.attributes();
    unsafe {
        CreateDirectoryW(
            PCWSTR(focuser_ipc::windows::wide(&root.to_string_lossy()).as_ptr()),
            Some(&attributes),
        )
    }
    .map_err(storage)?;
    let config_path = root.join("service.json");
    secure_file(&config_path)?;
    secure_file(&root.join("authority.lock"))?;
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(configuration).map_err(storage)?,
    )
    .map_err(storage)?;
    if initialize_empty {
        let path = root.join("authority.db");
        secure_file(&path)?;
        let db = focuser_core::Database::open(path).map_err(storage)?;
        db.set_setting("_authority.browser_cleanup_required", "false")
            .map_err(storage)?;
    }
    Ok(())
}

/// Verify private ownership and an allow-only SYSTEM/Administrators ACL. Reject
/// reparse points before opening configuration/database files as LocalSystem.
#[cfg(windows)]
pub fn verify_private(path: &Path) -> Result<(), IpcError> {
    verify_security(path, true)
}

/// Require protected executable files AND parents: an unprivileged owner may
/// read/execute installed binaries but cannot replace a file or rename a parent.
#[cfg(windows)]
pub fn verify_code(path: &Path) -> Result<(), IpcError> {
    if !path.is_absolute() {
        return Err(IpcError::new(
            "configuration",
            format!(
                "An absolute installed file path is required: {}",
                path.display()
            ),
        ));
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        IpcError::new(
            "configuration",
            format!(
                "Cannot inspect installed file '{}': {error}",
                path.display()
            ),
        )
    })?;
    if !metadata.is_file() {
        return Err(IpcError::new(
            "configuration",
            format!("Installed path is not a regular file: {}", path.display()),
        ));
    }
    for component in path.ancestors().take_while(|p| p.parent().is_some()) {
        verify_security(component, false)?;
    }
    Ok(())
}

#[cfg(windows)]
fn verify_security(path: &Path, private: bool) -> Result<(), IpcError> {
    use windows::{
        Win32::{Foundation::*, Security::Authorization::*, Security::*, Storage::FileSystem::*},
        core::{PCWSTR, PWSTR},
    };
    let name = focuser_ipc::windows::wide(&path.to_string_lossy());
    unsafe {
        let attributes = GetFileAttributesW(PCWSTR(name.as_ptr()));
        if attributes == INVALID_FILE_ATTRIBUTES || attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        {
            return Err(IpcError::new(
                "storage_security",
                "Authority path is missing or is a reparse point",
            ));
        }
        let mut owner = PSID::default();
        let mut acl: *mut ACL = std::ptr::null_mut();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        GetNamedSecurityInfoW(
            PCWSTR(name.as_ptr()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            Some(&mut acl),
            None,
            &mut descriptor,
        )
        .ok()
        .map_err(storage)?;
        let check = (|| {
            let sid = |sid: PSID| -> Result<String, IpcError> {
                let mut text = PWSTR::null();
                ConvertSidToStringSidW(sid, &mut text).map_err(storage)?;
                let value = text.to_string().map_err(storage);
                let _ = LocalFree(HLOCAL(text.0.cast()));
                value
            };
            let privileged = |value: &str| {
                matches!(value, "S-1-5-18" | "S-1-5-32-544")
                    || (!private
                        && value
                            == "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")
            };
            if !privileged(&sid(owner)?) || acl.is_null() {
                return Err(IpcError::new(
                    "storage_security",
                    "Untrusted authority file owner or null ACL",
                ));
            }
            for index in 0..(*acl).AceCount as u32 {
                let mut ace = std::ptr::null_mut();
                GetAce(acl, index, &mut ace).map_err(storage)?;
                let header = &*ace.cast::<ACE_HEADER>();
                if header.AceFlags & 8 != 0 {
                    continue;
                } // INHERIT_ONLY is not access to this object.
                if header.AceType != 0 {
                    return Err(IpcError::new(
                        "storage_security",
                        "Unexpected authority ACL entry",
                    ));
                }
                let allowed = &*ace.cast::<ACCESS_ALLOWED_ACE>();
                let sid = sid(PSID((&allowed.SidStart as *const u32).cast_mut().cast()))?;
                // TrustedInstaller may own system directories, but production
                // application files are deliberately owned by Administrators.
                const WRITE_MASK: u32 = 0x10000000 | 0x40000000 | 0x000D0156;
                if !privileged(&sid) && (private || allowed.Mask & WRITE_MASK != 0) {
                    return Err(IpcError::new(
                        "storage_security",
                        "ACL grants unprivileged write access",
                    ));
                }
            }
            Ok(())
        })();
        let _ = LocalFree(HLOCAL(descriptor.0));
        check
    }
}

pub struct StorageLock(std::fs::File);
impl StorageLock {
    /// Serialize elevated repair/removal helpers independently of the running
    /// authority's lifetime lock. A failed/denied removal releases this lock.
    #[cfg(windows)]
    pub fn maintenance(directory: &Path) -> Result<Self, IpcError> {
        use std::os::windows::fs::OpenOptionsExt;
        verify_private(directory)?;
        let path = directory.join("maintenance.lock");
        if !path.exists()
            && let Err(error) = secure_file(&path)
            && !path.exists()
        {
            return Err(error);
        }
        verify_private(&path)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&path)
            .map_err(|e| {
                IpcError::new(
                    "maintenance_busy",
                    format!("Another maintenance operation may be running: {e}"),
                )
            })?;
        Ok(Self(file))
    }
    pub fn acquire(directory: &Path) -> Result<Self, IpcError> {
        let path = directory.join("authority.lock");
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(0)
                .open(path)
                .map_err(storage)?;
            Ok(Self(file))
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err(IpcError::new(
                "unsupported",
                "Authority locking is Windows-only",
            ))
        }
    }
    pub fn sync(&self) -> Result<(), IpcError> {
        self.0.sync_all().map_err(storage)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn installed_file_diagnostics_distinguish_relative_missing_and_directory_paths() {
        let relative = Path::new("focuser-service.exe");
        let error = verify_code(relative).unwrap_err();
        assert_eq!(error.code, "configuration");
        assert!(error.message.contains("absolute installed file path"));
        assert!(error.message.contains("focuser-service.exe"));

        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("focuser-service.exe");
        let error = verify_code(&missing).unwrap_err();
        assert!(error.message.contains("Cannot inspect installed file"));
        assert!(error.message.contains(missing.to_str().unwrap()));
        assert!(!missing.exists());
        let error = verify_code(dir.path()).unwrap_err();
        assert!(error.message.contains("not a regular file"));
        assert!(error.message.contains(dir.path().to_str().unwrap()));
    }

    #[test]
    fn invalid_import_is_rejected_read_only_before_any_destination_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("profile.db");
        assert!(super::validate_source(&source).is_err());
        assert!(!source.exists());
        std::fs::write(&source, b"broken profile bytes").unwrap();
        assert_eq!(
            super::validate_source(&source).unwrap_err().code,
            "import_invalid"
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"broken profile bytes");
        std::fs::remove_file(&source).unwrap();
        drop(focuser_core::Database::open(&source).unwrap());
        super::validate_source(&source).unwrap();
    }

    use super::*;
    #[test]
    fn interrupted_maintenance_and_missing_storage_cannot_initialize_an_unrestricted_replacement() {
        validate_initialization(false, false, false, true, false).unwrap();
        validate_initialization(true, true, false, false, true).unwrap();
        validate_initialization(true, true, false, false, true).unwrap();
        for repair in [false, true] {
            assert_eq!(
                validate_initialization(true, false, false, true, repair)
                    .unwrap_err()
                    .code,
                "storage_missing"
            );
            assert_eq!(
                validate_initialization(true, true, true, false, repair)
                    .unwrap_err()
                    .code,
                "maintenance_interrupted"
            );
            assert_eq!(
                validate_initialization(true, false, true, true, repair)
                    .unwrap_err()
                    .code,
                "maintenance_interrupted"
            );
        }
        assert_eq!(
            validate_initialization(true, false, false, false, true)
                .unwrap_err()
                .code,
            "storage_missing"
        );
    }
    #[test]
    fn provisioning_repair_is_idempotent_and_cannot_reenroll_or_redirect() {
        let a = ServiceConfiguration {
            version: 1,
            owner_sid: "S-1-5-21-1".into(),
            agent_image: "agent.exe".into(),
            native_image: "native.exe".into(),
        };
        let mut b = ServiceConfiguration {
            version: 1,
            owner_sid: a.owner_sid.clone(),
            agent_image: a.agent_image.clone(),
            native_image: a.native_image.clone(),
        };
        ensure_same_configuration(&a, &b).unwrap();
        ensure_same_configuration(&a, &b).unwrap();
        b.owner_sid = "S-1-5-21-2".into();
        assert_eq!(
            ensure_same_configuration(&a, &b).unwrap_err().code,
            "installation_mismatch"
        );
        b.owner_sid = a.owner_sid.clone();
        b.agent_image = "other.exe".into();
        assert!(ensure_same_configuration(&a, &b).is_err());
    }
    #[test]
    fn migration_copies_every_table_without_modifying_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.db");
        let new = dir.path().join("new.db");
        let db = focuser_core::Database::open(&old).unwrap();
        db.set_setting("sentinel", "preserved").unwrap();
        drop(db);
        let original = std::fs::read(&old).unwrap();
        migrate(&old, &new, false).unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), original);
        assert_eq!(
            focuser_core::Database::open(&new)
                .unwrap()
                .get_setting("sentinel")
                .unwrap()
                .as_deref(),
            Some("preserved")
        );
        assert_eq!(
            migrate(&old, &new, false).unwrap_err().code,
            "already_initialized"
        );
    }
    #[test]
    fn migration_includes_live_wal_commitments_budgets_and_history() {
        use focuser_common::{
            allowance::{Allowance, AllowanceMatch},
            types::{BlockList, Protection},
        };
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.db");
        let new = dir.path().join("new.db");
        let db = focuser_core::Database::open(&old).unwrap();
        let wal = rusqlite::Connection::open(&old).unwrap();
        wal.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
            .unwrap();
        let mut list = BlockList::new("Preserve");
        list.protection = Some(Protection {
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            started_at: chrono::Utc::now(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
        });
        db.create_block_list(&list).unwrap();
        let budget = Allowance::new(AllowanceMatch::Domain("example.com".into()), 600, true);
        db.create_allowance(&budget).unwrap();
        db.increment_allowance_usage(budget.id, 345).unwrap();
        db.record_blocked_attempt("example.com").unwrap();
        db.record_blocked_event("example.com").unwrap();
        // Persisted shared-budget anchors and intervals must survive unchanged.
        wal.execute(
            "INSERT INTO shared_allowance_usage VALUES (?1,123,'[[123,163]]')",
            [list.id.to_string()],
        )
        .unwrap();
        wal.execute(
            "INSERT INTO shared_allowance_occurrences VALUES (?1,123,456)",
            [list.id.to_string()],
        )
        .unwrap();
        let session = focuser_common::pomodoro::PomodoroSession::new(
            list.id,
            focuser_common::pomodoro::PomodoroConfig::CLASSIC,
            true,
        );
        db.create_pomodoro_session(&session).unwrap();
        db.record_pomodoro_phase_start(session.id, session.current_phase, 1, session.started_at)
            .unwrap();
        // An opaque future-version/history table must also survive a backup.
        wal.execute_batch("CREATE TABLE future_history(value TEXT); INSERT INTO future_history VALUES ('preserve every table');").unwrap();
        let bytes = std::fs::read(&old).unwrap();
        migrate(&old, &new, false).unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), bytes);
        let copied = focuser_core::Database::open(&new).unwrap();
        assert_eq!(copied.get_allowance_used_today(budget.id).unwrap(), 345);
        assert!(
            copied
                .get_block_list(list.id)
                .unwrap()
                .is_modification_protected()
        );
        let copied = rusqlite::Connection::open(&new).unwrap();
        let names: Vec<String> = wal
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        for name in names {
            let sql = format!("SELECT * FROM \"{}\"", name.replace('"', "\"\""));
            let rows = |conn: &rusqlite::Connection| {
                let mut query = conn.prepare(&sql).unwrap();
                let count = query.column_count();
                query
                    .query_map([], |row| {
                        (0..count)
                            .map(|i| row.get::<_, rusqlite::types::Value>(i))
                            .collect::<rusqlite::Result<Vec<_>>>()
                    })
                    .unwrap()
                    .map(Result::unwrap)
                    .collect::<Vec<_>>()
            };
            assert_eq!(rows(&wal), rows(&copied), "table {name}");
        }
    }
    #[test]
    fn failed_migration_preserves_original_and_does_not_publish() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("bad.db");
        let new = dir.path().join("new.db");
        std::fs::write(&old, b"not sqlite").unwrap();
        assert!(migrate(&old, &new, false).is_err());
        assert_eq!(std::fs::read(&old).unwrap(), b"not sqlite");
        assert!(!new.exists());
    }
}
