//! Signed, bounded, schema-neutral updates. No client-supplied paths, commands,
//! signing keys or updater privilege flags are accepted by the production API.
use base64::{Engine, engine::general_purpose::STANDARD};
use focuser_ipc::{IpcError, UpdateStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

pub const SECURITY_VERSION: u64 = 1;
pub const SCHEMA_VERSION: u32 = 8;
pub const PRODUCT: &str = "com.focuser.authority.windows-x64";
pub const FEED: &str = "https://github.com/aadeshrao123/Focuser/releases/latest/download/windows-authority-update.json";
pub const MAX_MANIFEST: u64 = 64 * 1024;
pub const MAX_FILE: u64 = 192 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 512 * 1024 * 1024;
pub const REQUIRED: [&str; 5] = [
    "focuser-ui.exe",
    "focuser-service.exe",
    "focuser-session-agent.exe",
    "focuser-cli.exe",
    "focuser-native.exe",
];
pub const BROKER_NAME: &str = "update-broker.exe";
pub const RECOVERY_TASK: &str = r"\Focuser\UpdateRecovery";
pub fn error(code: &str, e: impl std::fmt::Display) -> IpcError {
    IpcError::new(code, e.to_string())
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn hash_file(path: &Path) -> Result<String, IpcError> {
    let mut f = File::open(path).map_err(|e| error("update_storage", e))?;
    hash_reader(&mut f)
}
pub fn hash_reader(reader: &mut impl Read) -> Result<String, IpcError> {
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| error("update_storage", e))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub name: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub product: String,
    pub version: String,
    pub security_version: u64,
    pub security_floor: u64,
    pub protocol_min: u32,
    pub protocol_max: u32,
    pub schema_read_min: u32,
    pub schema_read_max: u32,
    pub schema_write: u32,
    pub expires_at: i64,
    pub publisher_sha256: String,
    pub files: Vec<Payload>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub manifest: String,
    pub signature: String,
}
#[derive(Clone)]
pub struct Trust {
    pub minisign: String,
    pub publisher: String,
}
impl Trust {
    pub fn identity(&self) -> String {
        digest(
            format!(
                "{}:{}",
                self.minisign.lines().nth(1).unwrap_or(""),
                self.publisher
            )
            .as_bytes(),
        )
    }
    pub fn production() -> Result<Self, IpcError> {
        // The existing Tauri public key is a public trust anchor, not a credential.
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../focuser-ui/tauri.conf.json"))
                .map_err(|e| error("signing_unconfigured", e))?;
        let encoded = config["plugins"]["updater"]["pubkey"]
            .as_str()
            .ok_or_else(|| error("signing_unconfigured", "Missing update public key"))?;
        let minisign = String::from_utf8(
            STANDARD
                .decode(encoded)
                .map_err(|e| error("signing_unconfigured", e))?,
        )
        .map_err(|e| error("signing_unconfigured", e))?;
        let publisher = option_env!("FOCUSER_AUTHENTICODE_SHA256")
            .unwrap_or("")
            .to_ascii_lowercase();
        if publisher.len() != 64 || !publisher.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(error(
                "signing_unconfigured",
                "Release Authenticode certificate SHA-256 pin is not configured; unsigned development updates are forbidden",
            ));
        }
        Ok(Self {
            minisign,
            publisher,
        })
    }
    pub fn verify(
        &self,
        envelope: &Envelope,
        current: &str,
        floor: u64,
        now: i64,
    ) -> Result<(Manifest, String), IpcError> {
        let (manifest, hash) = self.verify_signature(envelope)?;
        manifest.validate(current, floor, now, &self.publisher)?;
        Ok((manifest, hash))
    }
    pub fn verify_recovery(
        &self,
        envelope: &Envelope,
        current: &str,
        floor: u64,
    ) -> Result<(Manifest, String), IpcError> {
        let (manifest, hash) = self.verify_signature(envelope)?;
        manifest.validate(
            current,
            floor,
            manifest.expires_at.saturating_sub(1),
            &self.publisher,
        )?;
        Ok((manifest, hash))
    }
    fn verify_signature(&self, envelope: &Envelope) -> Result<(Manifest, String), IpcError> {
        if envelope.manifest.len() as u64 > MAX_MANIFEST * 2 || envelope.signature.len() > 4096 {
            return Err(error("package_malformed", "Manifest limit"));
        }
        let body = STANDARD
            .decode(&envelope.manifest)
            .map_err(|e| error("package_malformed", e))?;
        if body.len() as u64 > MAX_MANIFEST {
            return Err(error("package_malformed", "Manifest limit"));
        }
        let signature = String::from_utf8(
            STANDARD
                .decode(&envelope.signature)
                .map_err(|e| error("signature_invalid", e))?,
        )
        .map_err(|e| error("signature_invalid", e))?;
        let key = minisign_verify::PublicKey::decode(&self.minisign)
            .map_err(|e| error("signature_invalid", e))?;
        let signature = minisign_verify::Signature::decode(&signature)
            .map_err(|e| error("signature_invalid", e))?;
        key.verify(&body, &signature, false)
            .map_err(|e| error("signature_invalid", e))?;
        let manifest: Manifest =
            serde_json::from_slice(&body).map_err(|e| error("package_malformed", e))?;
        Ok((manifest, digest(&body)))
    }
}
impl Manifest {
    pub fn validate(
        &self,
        current: &str,
        floor: u64,
        now: i64,
        publisher: &str,
    ) -> Result<(), IpcError> {
        if self.product != PRODUCT {
            return Err(error("product_mismatch", "Wrong product or architecture"));
        }
        let next =
            semver::Version::parse(&self.version).map_err(|e| error("package_malformed", e))?;
        let old = semver::Version::parse(current).map_err(|e| error("package_malformed", e))?;
        if next <= old
            || !next.pre.is_empty()
            || !next.build.is_empty()
            || self.security_version < floor
            || self.security_floor < floor
            || self.security_floor > self.security_version
        {
            return Err(error(
                "downgrade_rejected",
                "Version/security floor does not advance safely",
            ));
        }
        if self.expires_at <= now || self.expires_at > now + 7 * 86400 {
            return Err(error(
                "package_expired",
                "Manifest validity must be within seven days",
            ));
        }
        if self.protocol_min > 1
            || self.protocol_max < 1
            || self.schema_read_min > SCHEMA_VERSION
            || self.schema_read_max < SCHEMA_VERSION
            || self.schema_write != SCHEMA_VERSION
        {
            return Err(error(
                "schema_incompatible",
                "Only protocol-v1, schema-neutral updates are supported; no database rollback",
            ));
        }
        if self.publisher_sha256 != publisher {
            return Err(error(
                "publisher_mismatch",
                "Authenticode signing identity changed",
            ));
        }
        if !(5..=6).contains(&self.files.len()) {
            return Err(error(
                "package_malformed",
                "Expected five fixed binaries and optional WebView loader",
            ));
        }
        let mut names = std::collections::HashSet::new();
        let mut total = 0u64;
        for file in &self.files {
            if (!REQUIRED.contains(&file.name.as_str()) && file.name != "WebView2Loader.dll")
                || !names.insert(&file.name)
                || file.bytes == 0
                || file.bytes > MAX_FILE
                || file.sha256.len() != 64
                || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || !approved_url(&file.url)
            {
                return Err(error(
                    "package_malformed",
                    "Unapproved file, URL, digest, size or duplicate",
                ));
            }
            total = total
                .checked_add(file.bytes)
                .ok_or_else(|| error("package_too_large", "Size overflow"))?;
        }
        if total > MAX_TOTAL || REQUIRED.iter().any(|n| !names.contains(&n.to_string())) {
            return Err(error(
                "package_malformed",
                "Missing payload or total size exceeded",
            ));
        }
        #[cfg(target_env = "gnu")]
        if !names.contains(&"WebView2Loader.dll".to_string()) {
            return Err(error(
                "package_malformed",
                "GNU builds require the WebView loader",
            ));
        }
        Ok(())
    }
}
pub fn approved_url(text: &str) -> bool {
    reqwest::Url::parse(text).is_ok_and(|u| {
        u.scheme() == "https"
            && u.username().is_empty()
            && u.password().is_none()
            && u.port().is_none()
            && matches!(
                u.host_str(),
                Some(
                    "github.com"
                        | "release-assets.githubusercontent.com"
                        | "objects.githubusercontent.com"
                )
            )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Checking,
    Available,
    Downloading,
    Verifying,
    Staged,
    Deferred,
    Authorized,
    Replacing,
    Validating,
    Complete,
    RollingBack,
    RollbackValidating,
    RolledBack,
    Cancelled,
    Cancelling,
    RecoveryFault,
}
impl Phase {
    pub fn frozen(self) -> bool {
        matches!(
            self,
            Self::Authorized
                | Self::Replacing
                | Self::Validating
                | Self::RollingBack
                | Self::RollbackValidating
                | Self::RecoveryFault
        )
    }
    pub fn terminal(self) -> bool {
        matches!(self, Self::Complete | Self::RolledBack | Self::Cancelled)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    #[serde(default)]
    pub origin_generation: Option<String>,
    #[serde(default)]
    pub broker_pid: Option<u32>,
    pub id: String,
    pub installation: String,
    pub phase: Phase,
    pub revision: u64,
    pub expires_at: i64,
    pub artifact_digest: String,
    pub envelope: Option<Envelope>,
    pub manifest: Option<Manifest>,
    pub previous_version: String,
    pub previous_security: u64,
    pub floor: u64,
    pub old_hashes: BTreeMap<String, String>,
    pub replaced: usize,
    pub attempts: u8,
    pub code: String,
    pub checkpoint: Option<String>,
    pub downloaded: u64,
}
impl Journal {
    pub fn reverify(&mut self, trust: &Trust, now: i64, recovery: bool) -> Result<(), IpcError> {
        let envelope = self
            .envelope
            .as_ref()
            .ok_or_else(|| error("package_malformed", "Missing signed envelope"))?;
        let (manifest, hash) = if recovery {
            trust.verify_recovery(envelope, &self.previous_version, self.floor)?
        } else {
            trust.verify(
                envelope,
                &self.previous_version,
                self.floor.max(SECURITY_VERSION),
                now,
            )?
        };
        if hash != self.artifact_digest {
            return Err(error(
                "digest_mismatch",
                "Signed manifest does not match this transaction",
            ));
        }
        self.manifest = Some(manifest);
        Ok(())
    }
    pub fn fresh(installation: &str, revision: u64, floor: u64) -> Self {
        Self {
            broker_pid: None,
            origin_generation: None,
            id: uuid::Uuid::new_v4().to_string(),
            installation: installation.into(),
            phase: Phase::Checking,
            revision,
            expires_at: chrono::Utc::now().timestamp() + 1800,
            artifact_digest: String::new(),
            envelope: None,
            manifest: None,
            previous_version: env!("CARGO_PKG_VERSION").into(),
            previous_security: SECURITY_VERSION,
            floor,
            old_hashes: Default::default(),
            replaced: 0,
            attempts: 0,
            code: "checking".into(),
            checkpoint: None,
            downloaded: 0,
        }
    }
    pub fn authorize(
        &mut self,
        id: &str,
        installation: &str,
        revision: u64,
        now: i64,
        blocked: bool,
    ) -> Result<(), IpcError> {
        if self.id != id || self.installation != installation {
            return Err(error(
                "transaction_mismatch",
                "Wrong installation/update identity",
            ));
        }
        if !matches!(self.phase, Phase::Staged | Phase::Deferred) {
            return Err(error(
                "transaction_replayed",
                "Transaction is not an unused staged update",
            ));
        }
        if now >= self.expires_at {
            return Err(error("transaction_expired", "Stage/check the update again"));
        }
        if revision != self.revision {
            self.revision = revision;
            self.phase = Phase::Deferred;
            self.code = "policy_changed".into();
            return Ok(());
        }
        if blocked {
            self.phase = Phase::Deferred;
            self.code = "strict_commitment_active".into();
            return Ok(());
        }
        self.phase = Phase::Authorized;
        self.code = "authorized".into();
        self.expires_at = now + 60;
        Ok(())
    }
    pub fn rollback_allowed(&self, floor: u64, schema: u32) -> bool {
        self.previous_security >= floor
            && schema == SCHEMA_VERSION
            && self
                .manifest
                .as_ref()
                .is_some_and(|m| m.schema_read_min <= schema && m.schema_read_max >= schema)
    }
    pub fn status(&self) -> UpdateStatus {
        UpdateStatus {
            update_trust: Trust::production().ok().map(|t| t.identity()),
            can_cancel: self.phase != Phase::Cancelling
                && !self.phase.terminal()
                && !(self.phase.frozen()
                    && (self.phase != Phase::RecoveryFault || self.checkpoint.is_some())),
            available: self.manifest.is_some() && !self.phase.terminal(),
            version: self.manifest.as_ref().map(|m| m.version.clone()),
            transaction: Some(self.id.clone()),
            phase: serde_json::to_value(self.phase)
                .unwrap()
                .as_str()
                .unwrap()
                .into(),
            code: self.code.clone(),
            message: self.code.replace('_', " "),
            downloaded: self.downloaded,
            total: self
                .manifest
                .as_ref()
                .map(|m| m.files.iter().map(|f| f.bytes).sum())
                .unwrap_or(0),
            runtime_version: env!("CARGO_PKG_VERSION").into(),
            security_version: SECURITY_VERSION,
            schema_version: SCHEMA_VERSION,
        }
    }
}

#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
    pub private: bool,
}
impl Store {
    pub fn require_space(&self, bytes: u64) -> Result<(), IpcError> {
        #[cfg(windows)]
        unsafe {
            use ::windows::{Win32::Storage::FileSystem::GetDiskFreeSpaceExW, core::PCWSTR};
            let mut available = 0u64;
            GetDiskFreeSpaceExW(
                PCWSTR(focuser_ipc::windows::wide(&self.root.to_string_lossy()).as_ptr()),
                Some(&mut available),
                None,
                None,
            )
            .map_err(|e| error("update_storage", e))?;
            if available < bytes {
                return Err(error(
                    "staging_disk_full",
                    "Insufficient space for new payload and bounded known-good backup",
                ));
            }
        }
        #[cfg(not(windows))]
        let _ = bytes;
        Ok(())
    }
    pub fn new(root: PathBuf, private: bool) -> Result<Self, IpcError> {
        if private {
            crate::storage::secure_directory(&root)?
        } else {
            std::fs::create_dir_all(&root).map_err(|e| error("update_storage", e))?
        }
        Ok(Self { root, private })
    }
    pub fn load(&self) -> Result<Option<Journal>, IpcError> {
        let p = self.root.join("journal.json");
        if !p.exists() {
            return Ok(None);
        }
        self.check(&p)?;
        if std::fs::metadata(&p)
            .map_err(|e| error("update_storage", e))?
            .len()
            > 256 * 1024
        {
            return Err(error("update_storage", "Journal oversized"));
        }
        let bytes = std::fs::read(&p).map_err(|e| error("update_storage", e))?;
        if bytes.len() > 256 * 1024 {
            return Err(error("update_storage", "Journal oversized"));
        }
        let journal: Journal =
            serde_json::from_slice(&bytes).map_err(|e| error("update_storage", e))?;
        if uuid::Uuid::parse_str(&journal.id).is_err()
            || (!matches!(
                journal.phase,
                Phase::Checking | Phase::Cancelled | Phase::Cancelling | Phase::RecoveryFault
            ) && (journal.manifest.is_none() || journal.envelope.is_none()))
            || journal.old_hashes.iter().any(|(n, h)| {
                (!REQUIRED.contains(&n.as_str()) && n != "WebView2Loader.dll")
                    || h.len() != 64
                    || !h.bytes().all(|b| b.is_ascii_hexdigit())
            })
        {
            return Err(error("update_storage", "Invalid durable transaction shape"));
        }
        Ok(Some(journal))
    }
    pub fn check(&self, path: &Path) -> Result<(), IpcError> {
        if self.private {
            crate::storage::verify_private(path)?
        } else if std::fs::symlink_metadata(path)
            .map_err(|e| error("update_storage", e))?
            .file_type()
            .is_symlink()
        {
            return Err(error("update_storage", "Symlink refused"));
        }
        Ok(())
    }
    pub fn save(&self, j: &Journal) -> Result<(), IpcError> {
        self.atomic(
            "journal.json",
            &serde_json::to_vec(j).map_err(|e| error("update_storage", e))?,
        )
    }
    pub fn atomic(&self, name: &str, bytes: &[u8]) -> Result<(), IpcError> {
        let target = self.root.join(name);
        if target.exists() {
            self.check(&target)?
        }
        let temp = self
            .root
            .join(format!("write-{}.tmp", uuid::Uuid::new_v4()));
        self.create_file(&temp)?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&temp)
            .map_err(|e| error("update_storage", e))?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| error("update_storage", e))?;
        drop(f);
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Storage::FileSystem::*;
            use windows::core::PCWSTR;
            MoveFileExW(
                PCWSTR(focuser_ipc::windows::wide(&temp.to_string_lossy()).as_ptr()),
                PCWSTR(focuser_ipc::windows::wide(&target.to_string_lossy()).as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|e| error("update_storage", e))?;
        }
        #[cfg(not(windows))]
        std::fs::rename(&temp, &target).map_err(|e| error("update_storage", e))?;
        Ok(())
    }
    pub fn create_file(&self, path: &Path) -> Result<(), IpcError> {
        if self.private {
            crate::storage::secure_file(path)
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map(|_| ())
                .map_err(|e| error("update_storage", e))
        }
    }
    pub fn floor(&self) -> Result<u64, IpcError> {
        let p = self.root.join("security-floor.json");
        if !p.exists() {
            return Ok(SECURITY_VERSION);
        }
        self.check(&p)?;
        let floor: u64 =
            serde_json::from_slice(&std::fs::read(&p).map_err(|e| error("update_storage", e))?)
                .map_err(|e| error("update_storage", e))?;
        Ok(floor.max(SECURITY_VERSION))
    }
    pub fn raise_floor(&self, floor: u64) -> Result<(), IpcError> {
        self.atomic(
            "security-floor.json",
            &serde_json::to_vec(&floor.max(self.floor()?)).unwrap(),
        )
    }
    pub fn stage_dir(&self, id: &str) -> Result<Self, IpcError> {
        uuid::Uuid::parse_str(id).map_err(|e| error("transaction_mismatch", e))?;
        Self::new(self.root.join(id), self.private)
    }
    pub fn prune_stages(&self, keep: Option<&str>) -> Result<(), IpcError> {
        for entry in std::fs::read_dir(&self.root).map_err(|e| error("update_storage", e))? {
            let entry = entry.map_err(|e| error("update_storage", e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if uuid::Uuid::parse_str(&name).is_err() || keep == Some(name.as_str()) {
                continue;
            }
            let path = entry.path();
            self.check(&path)?;
            let files = std::fs::read_dir(&path)
                .map_err(|e| error("update_storage", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| error("update_storage", e))?;
            if files.len() > 12 {
                return Err(error("staging_quota", "Unexpected cache content"));
            }
            for file in &files {
                let n = file.file_name().to_string_lossy().into_owned();
                let owned = n.strip_prefix("old-").unwrap_or(&n);
                if !REQUIRED.contains(&owned) && owned != "WebView2Loader.dll" {
                    return Err(error("staging_quota", "Unknown cache content retained"));
                }
                self.check(&file.path())?;
                if !file
                    .file_type()
                    .map_err(|e| error("update_storage", e))?
                    .is_file()
                {
                    return Err(error("staging_quota", "Cache subdirectory/reparse refused"));
                }
            }
            for file in files {
                std::fs::remove_file(file.path()).map_err(|e| error("update_storage", e))?;
            }
            std::fs::remove_dir(path).map_err(|e| error("update_storage", e))?;
        }
        Ok(())
    }
}

pub fn bounded_copy(
    reader: &mut impl Read,
    writer: &mut impl Write,
    expected: u64,
) -> Result<(), IpcError> {
    if expected > MAX_FILE {
        return Err(error("package_too_large", "File exceeds size limit"));
    }
    let mut limited = reader.take(expected + 1);
    let count = std::io::copy(&mut limited, writer).map_err(|e| error("download_io", e))?;
    if count != expected {
        return Err(error(
            "download_truncated",
            "Signed content length mismatch",
        ));
    }
    Ok(())
}
fn http() -> Result<reqwest::blocking::Client, IpcError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::custom(|a| {
            if a.previous().len() < 3 && approved_url(a.url().as_str()) {
                a.follow()
            } else {
                a.error("Unapproved update redirect")
            }
        }))
        .build()
        .map_err(|e| error("download_failed", e))
}

pub struct Manager {
    pub store: Store,
    pub journal: Option<Journal>,
    job: Arc<std::sync::atomic::AtomicBool>,
}
struct WorkerJob(Arc<std::sync::atomic::AtomicBool>);
impl Drop for WorkerJob {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}
impl Manager {
    pub fn open(store: Store) -> Result<Self, IpcError> {
        let mut journal = store.load()?;
        if let Some(j) = journal.as_mut() {
            if j.phase == Phase::Cancelling {
                j.phase = Phase::Cancelled;
                j.code = "cancelled".into();
                store.save(j)?;
            }
            if matches!(
                j.phase,
                Phase::Checking | Phase::Downloading | Phase::Verifying
            ) {
                j.phase = Phase::RecoveryFault;
                j.code = "staging_interrupted".into();
                store.save(j)?;
            }
            if j.phase == Phase::Complete {
                store.raise_floor(
                    j.manifest
                        .as_ref()
                        .ok_or_else(|| error("update_storage", "Missing completed manifest"))?
                        .security_floor,
                )?;
            }
        }
        Ok(Self {
            store,
            journal,
            job: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }
    pub fn busy(&self) -> bool {
        self.job.load(std::sync::atomic::Ordering::SeqCst)
            || self.journal.as_ref().is_some_and(|j| !j.phase.terminal())
    }
    pub fn working(&self) -> bool {
        self.job.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn frozen(&self) -> bool {
        self.journal.as_ref().is_some_and(|j| {
            j.phase.frozen() && (j.phase != Phase::RecoveryFault || j.checkpoint.is_some())
        })
    }
    pub fn status(&self) -> UpdateStatus {
        self.journal
            .as_ref()
            .map(Journal::status)
            .unwrap_or_else(|| UpdateStatus {
                update_trust: Trust::production().ok().map(|t| t.identity()),
                phase: "idle".into(),
                code: "idle".into(),
                runtime_version: env!("CARGO_PKG_VERSION").into(),
                security_version: SECURITY_VERSION,
                schema_version: SCHEMA_VERSION,
                ..Default::default()
            })
    }
    pub fn commit(&mut self, j: Journal) -> Result<(), IpcError> {
        self.store.save(&j)?;
        self.journal = Some(j);
        Ok(())
    }
    pub fn check(
        shared: Arc<Mutex<Self>>,
        installation: String,
        revision: u64,
    ) -> Result<(), IpcError> {
        let trust = Trust::production()?;
        let (expected_id, flag) = {
            let mut manager = shared
                .lock()
                .map_err(|_| error("internal", "Update lock poisoned"))?;
            if manager.busy() {
                return Err(error(
                    "update_busy",
                    "Cancel or recover the existing transaction",
                ));
            }
            let j = Journal::fresh(&installation, revision, manager.store.floor()?);
            let keep = manager
                .journal
                .as_ref()
                .filter(|j| matches!(j.phase, Phase::Complete | Phase::RolledBack))
                .map(|j| j.id.as_str());
            manager.store.prune_stages(keep)?;
            let id = j.id.clone();
            manager.commit(j)?;
            manager.job.store(true, std::sync::atomic::Ordering::SeqCst);
            (id, manager.job.clone())
        };
        let job = WorkerJob(flag);
        std::thread::Builder::new()
            .name("focuser-update-check".into())
            .spawn(move || {
                let _job = job;
                let result = (|| {
                    let mut response = http()?
                        .get(FEED)
                        .send()
                        .and_then(|r| r.error_for_status())
                        .map_err(|e| error("download_failed", e))?;
                    let mut bytes = Vec::new();
                    response
                        .by_ref()
                        .take(MAX_MANIFEST * 2 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|e| error("download_failed", e))?;
                    if bytes.len() as u64 > MAX_MANIFEST * 2 {
                        return Err(error("package_too_large", "Feed oversized"));
                    }
                    let envelope: Envelope = serde_json::from_slice(&bytes)
                        .map_err(|e| error("package_malformed", e))?;
                    let mut m = shared
                        .lock()
                        .map_err(|_| error("internal", "Update lock"))?;
                    let mut j = m.journal.clone().unwrap();
                    if j.id != expected_id || j.phase != Phase::Checking {
                        return Err(error("update_cancelled", "Check cancelled"));
                    }
                    let (manifest, digest) = trust.verify(
                        &envelope,
                        &j.previous_version,
                        j.floor,
                        chrono::Utc::now().timestamp(),
                    )?;
                    j.artifact_digest = digest;
                    j.expires_at = manifest.expires_at;
                    j.manifest = Some(manifest);
                    j.envelope = Some(envelope);
                    j.phase = Phase::Available;
                    j.code = "available".into();
                    m.commit(j)
                })();
                if let Err(e) = result {
                    Self::worker_error(&shared, &expected_id, e);
                }
            })
            .map_err(|e| error("update_worker_unavailable", e))?;
        Ok(())
    }
    fn worker_error(shared: &Arc<Mutex<Self>>, id: &str, e: IpcError) {
        if let Ok(mut m) = shared.lock()
            && let Some(mut j) = m.journal.clone()
            && j.id == id
            && !j.phase.terminal()
        {
            j.phase = if j.phase == Phase::Cancelling {
                Phase::Cancelled
            } else {
                Phase::RecoveryFault
            };
            j.code = if j.phase == Phase::Cancelled {
                "cancelled".into()
            } else {
                e.code
            };
            if m.commit(j.clone()).is_err() {
                m.journal = Some(j);
            }
        }
    }
    pub fn stage(shared: Arc<Mutex<Self>>) -> Result<(), IpcError> {
        let trust = Trust::production()?;
        let (store, journal, flag) = {
            let mut m = shared
                .lock()
                .map_err(|_| error("internal", "Update lock"))?;
            let mut j = m
                .journal
                .clone()
                .ok_or_else(|| error("update_missing", "Check for an update first"))?;
            if m.working() || j.phase != Phase::Available {
                return Err(error("update_busy", "Not an available update"));
            }
            j.reverify(&trust, chrono::Utc::now().timestamp(), false)?;
            m.store.require_space(
                MAX_TOTAL
                    + j.manifest
                        .as_ref()
                        .ok_or_else(|| error("package_malformed", "Missing staged manifest"))?
                        .files
                        .iter()
                        .map(|f| f.bytes)
                        .sum::<u64>()
                    + 64 * 1024 * 1024,
            )?;
            j.phase = Phase::Downloading;
            j.code = "downloading".into();
            m.commit(j.clone())?;
            m.job.store(true, std::sync::atomic::Ordering::SeqCst);
            (m.store.clone(), j, m.job.clone())
        };
        let job = WorkerJob(flag);
        std::thread::Builder::new()
            .name("focuser-update-stage".into())
            .spawn(move || {
                let _job = job;
                let result = (|| {
                    let manifest = journal.manifest.as_ref().unwrap();
                    let stage = store.stage_dir(&journal.id)?;
                    let client = http()?;
                    let started = std::time::Instant::now();
                    for file in &manifest.files {
                        if started.elapsed() > Duration::from_secs(600) {
                            return Err(error(
                                "download_timeout",
                                "Whole staging deadline exceeded",
                            ));
                        }
                        let path = stage.root.join(&file.name);
                        stage.create_file(&path)?;
                        let mut output = std::fs::OpenOptions::new()
                            .write(true)
                            .open(&path)
                            .map_err(|e| error("update_storage", e))?;
                        let mut response = client
                            .get(&file.url)
                            .send()
                            .and_then(|r| r.error_for_status())
                            .map_err(|e| error("download_failed", e))?;
                        bounded_copy(&mut response, &mut output, file.bytes)?;
                        output.sync_all().map_err(|e| error("update_storage", e))?;
                        drop(output);
                        if hash_file(&path)? != file.sha256 {
                            return Err(error("digest_mismatch", "Downloaded payload changed"));
                        }
                        #[cfg(windows)]
                        crate::update_broker::verify_authenticode(&path, &trust.publisher)?;
                        let mut m = shared
                            .lock()
                            .map_err(|_| error("internal", "Update lock"))?;
                        let mut j = m.journal.clone().unwrap();
                        if j.id != journal.id || j.phase != Phase::Downloading {
                            return Err(error("update_cancelled", "Staging cancelled"));
                        }
                        j.downloaded += file.bytes;
                        m.commit(j)?;
                    }
                    let mut m = shared
                        .lock()
                        .map_err(|_| error("internal", "Update lock"))?;
                    let mut j = m.journal.clone().unwrap();
                    if j.id != journal.id || j.phase != Phase::Downloading {
                        return Err(error("update_cancelled", "Staging cancelled"));
                    }
                    j.phase = Phase::Verifying;
                    j.code = "verifying".into();
                    m.commit(j.clone())?;
                    let (_, signed_digest) = trust.verify(
                        j.envelope.as_ref().unwrap(),
                        &j.previous_version,
                        store.floor()?,
                        chrono::Utc::now().timestamp(),
                    )?;
                    if signed_digest != j.artifact_digest {
                        return Err(error("digest_mismatch", "Manifest substitution"));
                    }
                    j.phase = Phase::Staged;
                    j.code = "staged".into();
                    m.commit(j)
                })();
                if let Err(e) = result {
                    Self::worker_error(&shared, &journal.id, e);
                }
            })
            .map_err(|e| error("update_worker_unavailable", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    #[test]
    fn cancellation_keeps_one_worker_reserved_and_old_failures_do_not_poison_new_transactions() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("updates"), false).unwrap();
        let mut manager = Manager::open(store).unwrap();
        let mut old = Journal::fresh("test", 0, 1);
        let old_id = old.id.clone();
        old.phase = Phase::Cancelled;
        manager.commit(old).unwrap();
        manager.job.store(true, std::sync::atomic::Ordering::SeqCst);
        let guard = WorkerJob(manager.job.clone());
        assert!(manager.busy());
        drop(guard);
        assert!(!manager.busy());
        let next = Journal::fresh("test", 1, 1);
        let next_id = next.id.clone();
        manager.commit(next).unwrap();
        let shared = Arc::new(Mutex::new(manager));
        Manager::worker_error(
            &shared,
            &old_id,
            error("old_request_failed", "injected old completion"),
        );
        assert_eq!(shared.lock().unwrap().journal.as_ref().unwrap().id, next_id);
        assert_eq!(shared.lock().unwrap().status().phase, "checking");
    }
    #[test]
    fn cache_pruning_refuses_unknown_content_and_does_not_touch_policy_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("updates"), false).unwrap();
        let stage = store.stage_dir(&uuid::Uuid::new_v4().to_string()).unwrap();
        let policy = stage.root.join("authority.db");
        std::fs::write(&policy, b"not cache content").unwrap();
        assert_eq!(store.prune_stages(None).unwrap_err().code, "staging_quota");
        assert_eq!(std::fs::read(policy).unwrap(), b"not cache content");
    }
}
