//! Signature fixtures were signed with an isolated, randomly generated test key
//! using Tauri CLI 2.10.1. Only its public key and signatures are in this tree.
use focuser_ipc::IpcError;
use focuser_service::update::*;
use std::io::{Cursor, Write};
fn envelope(name: &str) -> Envelope {
    serde_json::from_str(match name {
        "valid" => include_str!("fixtures/update/valid.json"),
        "product" => include_str!("fixtures/update/wrong-product.json"),
        "old" => include_str!("fixtures/update/old-version.json"),
        "schema" => include_str!("fixtures/update/wrong-schema.json"),
        _ => include_str!("fixtures/update/expired.json"),
    })
    .unwrap()
}
fn trust() -> Trust {
    Trust {
        minisign: include_str!("fixtures/update/test-public-key.txt").into(),
        publisher: "ab".repeat(32),
    }
}
fn prepared() -> Journal {
    let e = envelope("valid");
    let (m, d) = trust().verify(&e, "0.10.0", 1, 1_800_000_000).unwrap();
    let mut j = Journal::fresh("installation-a", 7, 1);
    j.envelope = Some(e);
    j.manifest = Some(m);
    j.artifact_digest = d;
    j.phase = Phase::Staged;
    j.expires_at = 1_800_086_400;
    j
}
#[test]
fn cached_manifest_cannot_substitute_paths_and_expired_signatures_are_recovery_only() {
    let mut j = prepared();
    j.manifest.as_mut().unwrap().files[0].name = "../../authority.db".into();
    j.reverify(&trust(), 1_800_000_000, false).unwrap();
    assert_eq!(j.manifest.as_ref().unwrap().files[0].name, "focuser-ui.exe");
    assert!(j.reverify(&trust(), 1_900_000_000, false).is_err());
    assert!(j.reverify(&trust(), 1_900_000_000, true).is_ok());
    j.artifact_digest = "00".repeat(32);
    assert_eq!(
        j.reverify(&trust(), 1_800_000_000, false).unwrap_err().code,
        "digest_mismatch"
    );
}
#[test]
fn verifies_real_minisign_signatures_and_signed_product_version_schema_expiry() {
    assert!(
        trust()
            .verify(&envelope("valid"), "0.10.0", 1, 1_800_000_000)
            .is_ok()
    );
    for (name, code) in [
        ("product", "product_mismatch"),
        ("old", "downgrade_rejected"),
        ("schema", "schema_incompatible"),
        ("expired", "package_expired"),
    ] {
        assert_eq!(
            trust()
                .verify(&envelope(name), "0.10.0", 1, 1_800_000_000)
                .unwrap_err()
                .code,
            code
        );
    }
    assert_eq!(
        trust()
            .verify(&envelope("valid"), "0.10.0", 2, 1_800_000_000)
            .unwrap_err()
            .code,
        "downgrade_rejected"
    );
    let mut substituted = envelope("valid");
    substituted.manifest = envelope("product").manifest;
    assert_eq!(
        trust()
            .verify(&substituted, "0.10.0", 1, 1_800_000_000)
            .unwrap_err()
            .code,
        "signature_invalid"
    );
    let mut unsigned = envelope("valid");
    unsigned.signature.clear();
    assert_eq!(
        trust()
            .verify(&unsigned, "0.10.0", 1, 1_800_000_000)
            .unwrap_err()
            .code,
        "signature_invalid"
    );
}
#[test]
fn rejects_arbitrary_paths_hosts_duplicate_files_sizes_and_component_versions() {
    for mutate in 0..8 {
        let mut m = prepared().manifest.unwrap();
        match mutate {
            0 => m.files[0].name = "../authority.db".into(),
            1 => m.files[0].url = "http://github.com/file".into(),
            2 => m.files[0].url = "https://127.0.0.1/private".into(),
            3 => m.files[0].bytes = MAX_FILE + 1,
            4 => m.files.push(m.files[0].clone()),
            5 => m.protocol_min = 2,
            6 => m.publisher_sha256 = "cd".repeat(32),
            _ => {
                m.files.pop();
                m.files.pop();
            }
        }
        assert!(
            m.validate("0.10.0", 1, 1_800_000_000, &"ab".repeat(32))
                .is_err()
        );
    }
}
#[test]
fn binds_installation_revision_expiry_replay_and_defers_activation_for_strict_locks() {
    let mut j = prepared();
    let id = j.id.clone();
    assert_eq!(
        j.authorize(&id, "other", 7, 1_800_000_000, false)
            .unwrap_err()
            .code,
        "transaction_mismatch"
    );
    j.authorize(&id, "installation-a", 8, 1_800_000_000, false)
        .unwrap();
    assert_eq!(j.phase, Phase::Deferred);
    assert_eq!(j.code, "policy_changed");
    j.authorize(&id, "installation-a", 8, 1_800_000_000, true)
        .unwrap();
    assert_eq!(j.phase, Phase::Deferred);
    assert_eq!(j.code, "strict_commitment_active");
    j.authorize(&id, "installation-a", 8, 1_800_000_000, false)
        .unwrap();
    assert_eq!(j.phase, Phase::Authorized);
    assert_eq!(
        j.authorize(&id, "installation-a", 8, 1_800_000_000, false)
            .unwrap_err()
            .code,
        "transaction_replayed"
    );
    let mut expired = prepared();
    assert_eq!(
        expired
            .authorize(
                &expired.id.clone(),
                "installation-a",
                7,
                1_800_086_400,
                false
            )
            .unwrap_err()
            .code,
        "transaction_expired"
    );
    assert!(j.rollback_allowed(1, 8));
    assert!(!j.rollback_allowed(2, 8));
    assert!(!j.rollback_allowed(1, 9));
}
struct FullDisk;
impl Write for FullDisk {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::from(std::io::ErrorKind::StorageFull))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn downloads_are_bounded_and_reject_truncation_extra_bytes_and_disk_full() {
    assert!(bounded_copy(&mut Cursor::new(b"1234"), &mut Vec::new(), 4).is_ok());
    for bytes in [b"123".as_slice(), b"12345".as_slice()] {
        assert_eq!(
            bounded_copy(&mut Cursor::new(bytes), &mut Vec::new(), 4)
                .unwrap_err()
                .code,
            "download_truncated"
        );
    }
    assert_eq!(
        bounded_copy(&mut Cursor::new(b"1234"), &mut FullDisk, 4)
            .unwrap_err()
            .code,
        "download_io"
    );
    assert_eq!(
        bounded_copy(&mut Cursor::new(b""), &mut Vec::new(), MAX_FILE + 1)
            .unwrap_err()
            .code,
        "package_too_large"
    );
}
#[test]
fn every_durable_boundary_reloads_without_unlocking_or_marking_incomplete_work_healthy() {
    for phase in [
        Phase::Checking,
        Phase::Available,
        Phase::Downloading,
        Phase::Verifying,
        Phase::Staged,
        Phase::Deferred,
        Phase::Authorized,
        Phase::Replacing,
        Phase::Validating,
        Phase::RollingBack,
        Phase::RollbackValidating,
        Phase::RecoveryFault,
        Phase::RolledBack,
        Phase::Complete,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("updates"), false).unwrap();
        let mut j = prepared();
        j.phase = phase;
        store.save(&j).unwrap();
        let m = Manager::open(store).unwrap();
        let loaded = m.journal.unwrap();
        assert_eq!(loaded.id, j.id);
        assert_eq!(loaded.installation, j.installation);
        assert_eq!(loaded.artifact_digest, j.artifact_digest);
        if matches!(
            phase,
            Phase::Checking | Phase::Downloading | Phase::Verifying
        ) {
            assert_eq!(loaded.phase, Phase::RecoveryFault);
            assert_eq!(loaded.code, "staging_interrupted");
        } else {
            assert_eq!(loaded.phase, phase);
        }
    }
}
#[test]
fn staging_cannot_create_an_unrestricted_engine_or_accept_traversal_and_floor_never_decreases() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("updates"), false).unwrap();
    assert!(store.stage_dir("../elsewhere").is_err());
    assert!(!dir.path().join("authority.db").exists());
    store.raise_floor(5).unwrap();
    store.raise_floor(1).unwrap();
    assert_eq!(store.floor().unwrap(), 5);
    let manager = Manager::open(store).unwrap();
    assert!(!manager.busy());
    assert!(!manager.frozen());
    assert_eq!(manager.status().phase, "idle");
}
#[cfg(windows)]
mod windows_harness {
    use super::*;
    use focuser_service::update_broker::{
        Ops, policy_digest, recover_transaction, replace_transaction, verify_authenticode,
    };
    struct Harness {
        fail: &'static str,
        log: Vec<&'static str>,
        schema: u32,
        restored: bool,
    }
    #[test]
    fn reboot_recovery_from_consumed_boundaries_is_idempotent_and_never_reinstalls_new_payload() {
        for phase in [
            Phase::Authorized,
            Phase::Replacing,
            Phase::Validating,
            Phase::RollingBack,
            Phase::RollbackValidating,
            Phase::RecoveryFault,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let store = Store::new(dir.path().join("updates"), false).unwrap();
            let mut j = prepared();
            j.phase = phase;
            j.checkpoint = Some("ab".repeat(32));
            j.attempts = 1;
            let mut ops = Harness {
                fail: "",
                log: vec![],
                schema: 8,
                restored: false,
            };
            recover_transaction(&store, &mut j, &mut ops).unwrap();
            assert_eq!(j.phase, Phase::RolledBack);
            assert_eq!(
                ops.log,
                vec!["stop", "restore", "old-health", "resume-agent"]
            );
            let count = ops.log.len();
            recover_transaction(&store, &mut j, &mut ops).unwrap();
            assert_eq!(ops.log.len(), count);
        }
    }
    #[test]
    fn durable_write_failure_keeps_prior_stage_and_does_not_consume_authorization() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("updates"), false).unwrap();
        let j = prepared();
        store.save(&j).unwrap();
        let mut manager = Manager::open(store.clone()).unwrap();
        let path = store.root.join("journal.json");
        let original = std::fs::metadata(&path).unwrap().permissions();
        let mut perms = original.clone();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms).unwrap();
        let mut next = j.clone();
        next.phase = Phase::Authorized;
        let result = manager.commit(next);
        std::fs::set_permissions(&path, original).unwrap();
        assert!(result.is_err());
        assert_eq!(manager.status().phase, "staged");
        assert_eq!(store.load().unwrap().unwrap().phase, Phase::Staged);
    }
    impl Harness {
        fn step(&mut self, name: &'static str) -> Result<(), IpcError> {
            self.log.push(name);
            if self.fail == name {
                Err(error(name, "injected failure"))
            } else {
                Ok(())
            }
        }
    }
    impl Ops for Harness {
        fn resume_agent(&mut self) {
            self.log.push("resume-agent");
        }
        fn rollback_schema(&mut self) -> Result<u32, IpcError> {
            Ok(self.schema)
        }
        fn backup(&mut self, j: &mut Journal) -> Result<(), IpcError> {
            self.step("backup")?;
            for f in &j.manifest.as_ref().unwrap().files {
                j.old_hashes.insert(f.name.clone(), "ab".repeat(32));
            }
            Ok(())
        }
        fn stop(&mut self) -> Result<(), IpcError> {
            self.step("stop")
        }
        fn replace(&mut self, _: &Journal) -> Result<(), IpcError> {
            self.step("replace")
        }
        fn restore(&mut self, _: &Journal) -> Result<(), IpcError> {
            self.restored = true;
            self.step("restore")
        }
        fn start_validate(&mut self, _: &Journal, rollback: bool) -> Result<(), IpcError> {
            self.step(if rollback { "old-health" } else { "new-health" })
        }
    }
    #[test]
    fn replacement_failures_restore_binaries_without_restoring_old_policy_and_enforce_floors() {
        for failure in ["", "backup", "stop", "replace", "new-health"] {
            let dir = tempfile::tempdir().unwrap();
            let db_path = dir.path().join("authority.db");
            let db = focuser_core::Database::open(&db_path).unwrap();
            db.set_setting("commitment", "preserve me").unwrap();
            drop(db);
            let before = policy_digest(&db_path).unwrap();
            let store = Store::new(dir.path().join("updates"), false).unwrap();
            let mut j = prepared();
            j.phase = Phase::Authorized;
            j.checkpoint = Some(before.clone());
            let mut ops = Harness {
                fail: failure,
                log: vec![],
                schema: 8,
                restored: false,
            };
            let result = replace_transaction(&store, &mut j, &mut ops);
            if failure.is_empty() {
                assert!(result.is_ok());
                assert_eq!(j.phase, Phase::Complete);
                assert!(!ops.restored);
            } else {
                assert!(result.is_err());
                assert!(matches!(
                    j.phase,
                    Phase::RolledBack | Phase::RecoveryFault | Phase::Deferred
                ));
            }
            if matches!(
                j.phase,
                Phase::Complete | Phase::RolledBack | Phase::Deferred
            ) {
                assert_eq!(ops.log.last(), Some(&"resume-agent"));
            }
            assert_eq!(policy_digest(&db_path).unwrap(), before);
            let loaded = store.load().unwrap().unwrap();
            assert_eq!(loaded.phase, j.phase);
        }
        for schema in [8, 9] {
            let dir = tempfile::tempdir().unwrap();
            let store = Store::new(dir.path().join("updates"), false).unwrap();
            if schema == 8 {
                store.raise_floor(2).unwrap();
            }
            let mut j = prepared();
            let mut ops = Harness {
                fail: "new-health",
                log: vec![],
                schema,
                restored: false,
            };
            assert!(replace_transaction(&store, &mut j, &mut ops).is_err());
            assert_eq!(j.phase, Phase::RecoveryFault);
            assert_eq!(j.code, "rollback_incompatible");
            assert!(!ops.restored);
        }
    }
    #[test]
    fn unsigned_windows_executable_is_rejected_without_a_development_bypass() {
        assert_eq!(
            verify_authenticode(&std::env::current_exe().unwrap(), &"ab".repeat(32))
                .unwrap_err()
                .code,
            "authenticode_invalid"
        );
    }
}
