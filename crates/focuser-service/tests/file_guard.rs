#![cfg(windows)]
use focuser_app::{Command, CommandResult, command::LockSetup};
use focuser_common::types::{BlockList, Protection};
use focuser_service::{
    authority::{Authority, Configuration},
    file_guard::Guard,
};
use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

struct Fixture {
    _temp: tempfile::TempDir,
    installed: PathBuf,
    storage: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::Builder::new()
            .prefix("Focuser-FileGuard-")
            .tempdir()
            .unwrap();
        let installed = temp.path().join("installed");
        let storage = temp.path().join("authority");
        fs::create_dir_all(installed.join("resources")).unwrap();
        fs::create_dir(&storage).unwrap();
        for name in [
            "focuser-service.exe",
            "focuser-ui.exe",
            "focuser-cli.exe",
            "focuser-native.exe",
            "focuser-session-agent.exe",
            "WebView2Loader.dll",
            "resources/manifest.json",
        ] {
            fs::write(installed.join(name), b"original payload").unwrap();
        }
        fs::write(storage.join("service.json"), b"{}").unwrap();
        Self {
            _temp: temp,
            installed,
            storage,
        }
    }
    fn authority(&self) -> Authority {
        Authority::open(
            &self.storage.join("authority.db"),
            Configuration {
                owner_sid: "S-1-5-21-123".into(),
                agent_image: std::env::current_exe().unwrap(),
                native_image: std::env::current_exe().unwrap(),
                enforce: false,
            },
        )
        .unwrap()
    }
    fn guard(&self) -> Guard {
        Guard::new(self.installed.clone(), self.storage.clone()).unwrap()
    }
    fn attach(&self, authority: &Authority) {
        authority.attach_file_guard(self.guard()).unwrap();
    }
    fn image(&self) -> PathBuf {
        self.installed.join("focuser-ui.exe")
    }
}

fn list(authority: &Authority) -> BlockList {
    let item = BlockList::new("Focus commitment");
    authority
        .context
        .engine
        .lock()
        .unwrap()
        .db()
        .create_block_list(&item)
        .unwrap();
    item
}
fn protect(authority: &Authority, id: uuid::Uuid) {
    authority
        .command(Command::EnableProtection {
            list_id: id,
            duration_minutes: Some(5),
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            lock: None,
        })
        .unwrap();
}
fn status(authority: &Authority) -> serde_json::Value {
    let CommandResult::Setting(Some(value)) = authority
        .command(Command::GetSetting {
            key: "_authority.file_guard".into(),
            default: None,
        })
        .unwrap()
    else {
        panic!("Status missing")
    };
    serde_json::from_str(&value).unwrap()
}
fn assert_denied(path: &Path) {
    assert!(
        fs::remove_file(path).is_err(),
        "Delete must fail: {}",
        path.display()
    );
    assert!(
        fs::rename(path, path.with_extension("moved")).is_err(),
        "Rename must fail"
    );
}

#[test]
fn active_guard_protects_installed_names_and_alias_writes_without_locking_unrelated_files() {
    let f = Fixture::new();
    let _authority = f.authority();
    let alias = f._temp.path().join("existing-hardlink.exe");
    fs::hard_link(f.image(), &alias).unwrap();
    let unrelated = f._temp.path().join("unrelated-browser-updater.exe");
    fs::write(&unrelated, b"unrelated").unwrap();
    let mut guard = f.guard();
    guard.synchronize(true).unwrap();
    assert_denied(&f.image());
    // A distinct outside hardlink name may be unlinked on modern NTFS. That
    // does not remove the protected installed name or its data. Writes through
    // an alias must still fail the file's sharing check.
    assert!(fs::write(&alias, b"alias overwrite").is_err());
    fs::remove_file(&alias).unwrap();
    assert_eq!(fs::read(f.image()).unwrap(), b"original payload");
    assert!(fs::write(f.image(), b"replace").is_err());
    assert!(fs::rename(&f.installed, f.installed.with_extension("moved")).is_err());
    assert_eq!(fs::read(f.image()).unwrap(), b"original payload");
    fs::remove_file(unrelated).unwrap();
    assert_eq!(guard.status().state, "protected");
    guard.synchronize(false).unwrap();
    assert_eq!(guard.status().held_objects, 0);
    fs::remove_file(f.image()).unwrap();
}

#[test]
fn writable_authority_keeps_sqlite_wal_transactions_and_checkpoint_working() {
    let f = Fixture::new();
    let authority = f.authority();
    let mut guard = f.guard();
    guard.synchronize(true).unwrap();
    let db = f.storage.join("authority.db");
    assert_denied(&db);
    let second = rusqlite::Connection::open(&db).unwrap();
    second.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE guard_probe(value TEXT); INSERT INTO guard_probe VALUES ('kept'); PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    authority
        .context
        .engine
        .lock()
        .unwrap()
        .db()
        .set_setting("test.file_guard", "kept")
        .unwrap();
    assert_eq!(
        second
            .query_row("SELECT value FROM guard_probe", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "kept"
    );
    drop(second);
    guard.synchronize(false).unwrap();
}

#[test]
fn overlap_breaks_ui_independence_restart_and_original_expiry_use_stored_commitments() {
    let f = Fixture::new();
    let authority = f.authority();
    let first = list(&authority);
    let second = list(&authority);
    f.attach(&authority);
    protect(&authority, first.id);
    protect(&authority, second.id);
    assert_denied(&f.image());
    assert_eq!(status(&authority)["state"], "protected");
    // UI/CLI objects do not own these leases. Change stored time/state only in
    // this isolated test DB to model expiry and a Pomodoro-disabled list.
    let original_expiry;
    {
        let mut engine = authority.context.engine.lock().unwrap();
        let mut a = engine.db().get_block_list(first.id).unwrap();
        a.protection.as_mut().unwrap().expires_at =
            Some(chrono::Utc::now() - chrono::Duration::seconds(1));
        engine.db().update_block_list(&a).unwrap();
        let mut b = engine.db().get_block_list(second.id).unwrap();
        original_expiry = b.protection.as_ref().unwrap().expires_at;
        b.enabled = false;
        engine.db().update_block_list(&b).unwrap();
        engine.refresh().unwrap();
    }
    authority.tick().unwrap();
    assert_denied(&f.image());
    drop(authority);
    let authority = f.authority();
    f.attach(&authority);
    assert_denied(&f.image());
    {
        let engine = authority.context.engine.lock().unwrap();
        let mut b = engine.db().get_block_list(second.id).unwrap();
        assert_eq!(b.protection.as_ref().unwrap().expires_at, original_expiry);
        b.protection.as_mut().unwrap().expires_at =
            Some(chrono::Utc::now() - chrono::Duration::seconds(1));
        engine.db().update_block_list(&b).unwrap();
    }
    authority.tick().unwrap();
    assert_eq!(status(&authority)["state"], "idle");
    fs::remove_file(f.image()).unwrap();
    // Subsequent idle ticks must not briefly acquire handles or fail because an
    // unprotected file has legitimately been removed.
    authority.tick().unwrap();
}

#[test]
fn verified_unlock_releases_but_wrong_answer_and_client_setting_cannot_release() {
    let f = Fixture::new();
    let authority = f.authority();
    let item = list(&authority);
    f.attach(&authority);
    authority
        .command(Command::EnableProtection {
            list_id: item.id,
            duration_minutes: Some(5),
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            lock: Some(LockSetup::Password {
                password: "test-only-password".into(),
            }),
        })
        .unwrap();
    assert!(
        authority
            .command(Command::UnlockProtection {
                list_id: item.id,
                response: "wrong".into()
            })
            .is_err()
    );
    assert!(
        authority
            .command(Command::SetSetting {
                key: "_authority.file_guard".into(),
                value: "idle".into()
            })
            .is_err()
    );
    assert_denied(&f.image());
    authority
        .command(Command::UnlockProtection {
            list_id: item.id,
            response: "test-only-password".into(),
        })
        .unwrap();
    fs::remove_file(f.image()).unwrap();
}

#[test]
fn preexisting_delete_handle_prevents_start_before_committing_policy_and_reports_failure() {
    let f = Fixture::new();
    let authority = f.authority();
    let item = list(&authority);
    f.attach(&authority);
    let hostile = fs::OpenOptions::new()
        .access_mode(0x00010000)
        .share_mode(7)
        .open(f.image())
        .unwrap();
    let error = authority
        .command(Command::EnableProtection {
            list_id: item.id,
            duration_minutes: Some(5),
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            lock: None,
        })
        .unwrap_err();
    assert_eq!(error.code, "file_guard_unavailable");
    assert!(
        authority
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(item.id)
            .unwrap()
            .protection
            .is_none()
    );
    assert_eq!(status(&authority)["state"], "degraded");
    drop(hostile);
    protect(&authority, item.id);
    assert_denied(&f.image());
}

#[test]
fn refresh_error_does_not_release_an_existing_file_lease() {
    let f = Fixture::new();
    let authority = f.authority();
    let item = list(&authority);
    f.attach(&authority);
    protect(&authority, item.id);
    let db = rusqlite::Connection::open(f.storage.join("authority.db")).unwrap();
    db.execute("UPDATE block_lists SET data='broken-json'", [])
        .unwrap();
    assert!(authority.tick().is_err());
    assert_denied(&f.image());
}

#[test]
fn fresh_provisioning_and_same_code_repair_can_generate_task_xml_without_releasing_files() {
    let f = Fixture::new();
    let _authority = f.authority();
    let mut guard = f.guard();
    guard.synchronize(true).unwrap();
    for name in ["session-agent.xml", "update-recovery.xml"] {
        fs::write(f.storage.join(name), b"first").unwrap();
    }
    fs::write(f.storage.join("update-broker.exe"), b"broker").unwrap();
    fs::write(f.storage.join("broker.json"), b"{}").unwrap();
    guard.synchronize(true).unwrap();
    for name in ["session-agent.xml", "update-recovery.xml"] {
        fs::write(f.storage.join(name), b"repaired").unwrap();
        assert_denied(&f.storage.join(name));
    }
    assert_denied(&f.storage.join("update-broker.exe"));
    let manifest = f.installed.join("resources/manifest.json");
    focuser_service::storage::write_if_changed(&manifest, b"original payload").unwrap();
    assert!(focuser_service::storage::write_if_changed(&manifest, b"changed payload").is_err());
    assert_eq!(fs::read(&manifest).unwrap(), b"original payload");
    guard.synchronize(false).unwrap();
}

#[test]
fn imported_uninstall_opt_out_is_not_silently_strengthened() {
    let f = Fixture::new();
    let authority = f.authority();
    let item = list(&authority);
    {
        let engine = authority.context.engine.lock().unwrap();
        let mut item = item;
        item.protection = Some(Protection {
            prevent_uninstall: false,
            prevent_service_stop: true,
            prevent_modification: true,
            started_at: chrono::Utc::now(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::minutes(5)),
        });
        engine.db().update_block_list(&item).unwrap();
    }
    f.attach(&authority);
    assert_eq!(status(&authority)["state"], "idle");
    fs::remove_file(f.image()).unwrap();
}

#[test]
fn newly_created_installed_files_are_acquired_on_the_next_service_tick() {
    let f = Fixture::new();
    let authority = f.authority();
    let item = list(&authority);
    f.attach(&authority);
    protect(&authority, item.id);
    let added = f.installed.join("new-owned-resource.dat");
    fs::write(&added, b"new resource").unwrap();
    authority.tick().unwrap();
    assert_denied(&added);
}

#[test]
fn missing_owned_file_reports_a_failure_instead_of_silently_starting_protection() {
    let f = Fixture::new();
    let authority = f.authority();
    let item = list(&authority);
    f.attach(&authority);
    fs::remove_file(f.storage.join("service.json")).unwrap();
    let error = authority
        .command(Command::EnableProtection {
            list_id: item.id,
            duration_minutes: Some(5),
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            lock: None,
        })
        .unwrap_err();
    assert_eq!(error.code, "file_guard_unavailable");
    assert!(
        authority
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(item.id)
            .unwrap()
            .protection
            .is_none()
    );
    assert_eq!(status(&authority)["state"], "degraded");
}

/// Opt-in: only disposable scratch files are probed, but Windows shows UAC.
#[test]
#[ignore = "requires explicit elevated scratch probe; no SCM/driver installation"]
fn elevated_cleanup_is_denied_while_the_owning_service_object_lives() {
    use std::{
        os::windows::process::CommandExt,
        time::{Duration, Instant},
    };
    let f = Fixture::new();
    let _authority = f.authority();
    let mut guard = f.guard();
    fs::write(
        f._temp.path().join("unrelated-updater.tmp"),
        b"unrelated scratch",
    )
    .unwrap();
    guard.synchronize(true).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/file-guard-admin-probe.ps1");
    let powershell = PathBuf::from(std::env::var_os("WINDIR").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let quote = |value: &str| format!("'{}'", value.replace('\'', "''"));
    let args = format!(
        "-NoProfile -ExecutionPolicy Bypass -File \"{}\" -ScratchRoot \"{}\"",
        script.display(),
        f._temp.path().display()
    );
    let command = format!(
        "$ErrorActionPreference='Stop';$p=Start-Process -FilePath {} -ArgumentList {} -Verb RunAs -WindowStyle Hidden -Wait -PassThru;exit $p.ExitCode",
        quote(&powershell.to_string_lossy()),
        quote(&args)
    );
    let mut child = std::process::Command::new(&powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .creation_flags(0x08000000)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(code) = child.try_wait().unwrap() {
            assert!(code.success(), "Elevated scratch probe failed/cancelled");
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Elevated scratch probe timed out");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let bytes = fs::read(f._temp.path().join("elevated-result.json")).unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes)).unwrap();
    for key in [
        "elevated",
        "delete_denied",
        "rename_denied",
        "replace_denied",
        "unrelated_deletion_allowed",
    ] {
        assert_eq!(report[key], true, "{report}");
    }
    guard.synchronize(false).unwrap();
    fs::remove_file(f.image()).unwrap();
}
