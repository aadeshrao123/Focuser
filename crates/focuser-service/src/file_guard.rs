//! Service-owned Windows sharing locks. These reject ordinary delete/rename and
//! replacement requests, including elevated ones, while the service lives. They
//! are not a kernel security boundary against an administrator stopping it.
use focuser_ipc::IpcError;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub state: &'static str,
    pub held_objects: usize,
    pub error: Option<String>,
    pub administrator_resistant: bool,
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        collections::BTreeMap,
        fs::{File, OpenOptions},
        os::windows::fs::{MetadataExt, OpenOptionsExt},
    };
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    const REPARSE_POINT: u32 = 0x400;
    const MAX_OBJECTS: usize = 4096;

    pub struct Guard {
        installed: PathBuf,
        authority: PathBuf,
        held: BTreeMap<PathBuf, File>,
        active: bool,
        error: Option<String>,
    }

    impl Guard {
        /// Production callers supply only the verified installed executable's
        /// directory and service-owned storage root. No IPC-supplied paths.
        pub fn new(installed: PathBuf, authority: PathBuf) -> Result<Self, IpcError> {
            if !installed.is_absolute() || !authority.is_absolute() || installed == authority {
                return Err(failure(
                    "file_guard_configuration",
                    "Distinct absolute enrolled roots required",
                ));
            }
            Ok(Self {
                installed,
                authority,
                held: BTreeMap::new(),
                active: false,
                error: None,
            })
        }

        pub fn status(&self) -> Status {
            Status {
                state: if self.error.is_some() {
                    "degraded"
                } else if self.active {
                    "protected"
                } else {
                    "idle"
                },
                held_objects: self.held.len(),
                error: self.error.clone(),
                administrator_resistant: false,
            }
        }

        /// Called before a durable policy mutation, not after acknowledging a
        /// newly started commitment. Keep any acquired handles on partial failure.
        pub fn fence(&mut self) -> Result<(), IpcError> {
            self.active = true;
            let result = self.acquire();
            self.error = result.as_ref().err().map(|e| e.message.clone());
            result
        }

        pub fn synchronize(&mut self, protected: bool) -> Result<(), IpcError> {
            if protected {
                self.fence()
            } else {
                self.held.clear();
                self.active = false;
                self.error = None;
                Ok(())
            }
        }

        fn acquire(&mut self) -> Result<(), IpcError> {
            // Directories pin their names; every existing installed file pins
            // its file object. Outside hardlink names may still be unlinked;
            // sharing locks protect the installed name and deny alias writes.
            self.pin(&self.installed.clone(), false)?;
            for name in [
                "focuser-service.exe",
                "focuser-ui.exe",
                "focuser-cli.exe",
                "focuser-native.exe",
                "focuser-session-agent.exe",
            ] {
                let path = self.installed.join(name);
                self.pin(&path, false)?;
                if !self.held[&path]
                    .metadata()
                    .map_err(|e| io_failure(&path, e))?
                    .is_file()
                {
                    return Err(failure(
                        "file_guard_path",
                        "An installed executable is not a file",
                    ));
                }
            }
            #[cfg(target_env = "gnu")]
            self.pin(&self.installed.join("WebView2Loader.dll"), false)?;
            let mut pending = vec![self.installed.clone()];
            while let Some(path) = pending.pop() {
                self.pin(&path, false)?;
                let metadata = self.held[&path]
                    .metadata()
                    .map_err(|e| io_failure(&path, e))?;
                if metadata.is_dir() {
                    for entry in std::fs::read_dir(&path).map_err(|e| io_failure(&path, e))? {
                        pending.push(entry.map_err(|e| io_failure(&path, e))?.path());
                        if pending.len() + self.held.len() > MAX_OBJECTS {
                            return Err(failure(
                                "file_guard_limit",
                                "Installed payload exceeds the file protection limit",
                            ));
                        }
                    }
                }
            }
            self.pin(&self.authority.clone(), true)?;
            // SQLite must retain ordinary read/write/WAL/checkpoint behavior.
            // Pin the main DB against delete/rename only. Never hold WAL/SHM,
            // temporary journals, operation logs or staging files: SQLite and
            // the existing broker legitimately replace/delete those files.
            self.pin(&self.authority.join("authority.db"), true)?;
            self.pin(&self.authority.join("service.json"), false)?;
            // First enrollment starts SCM before provisioning its recovery
            // broker/tasks. Do not pin an empty file while setup is filling it.
            let broker_metadata = self.authority.join("broker.json");
            if std::fs::read(&broker_metadata)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .is_some()
            {
                self.pin(&self.authority.join("update-broker.exe"), false)?;
                self.pin(&broker_metadata, false)?;
            }
            for name in ["session-agent.xml", "update-recovery.xml"] {
                let path = self.authority.join(name);
                if path.try_exists().map_err(|e| io_failure(&path, e))? {
                    // Same-code repair legitimately regenerates these inputs.
                    self.pin(&path, true)?;
                }
            }
            Ok(())
        }

        fn pin(&mut self, path: &Path, mutable: bool) -> Result<(), IpcError> {
            if self.held.contains_key(path) {
                return Ok(());
            }
            if self.held.len() >= MAX_OBJECTS {
                return Err(failure(
                    "file_guard_limit",
                    "Too many installed objects to protect",
                ));
            }
            // OPEN_REPARSE_POINT opens the link itself, not its target; inspect
            // the actual opened object before trusting it. Traversal is top-down,
            // with ancestor directory handles held before enumerating children.
            let file = OpenOptions::new()
                // Attribute-only opens do not participate in NT sharing checks.
                // Read data/list-directory access is needed for a real lease.
                .read(true)
                .share_mode(FILE_SHARE_READ.0 | if mutable { FILE_SHARE_WRITE.0 } else { 0 })
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(path)
                .map_err(|e| io_failure(path, e))?;
            let metadata = file.metadata().map_err(|e| io_failure(path, e))?;
            if metadata.file_attributes() & REPARSE_POINT != 0
                || !(metadata.is_dir() || metadata.is_file())
            {
                return Err(failure(
                    "file_guard_path",
                    format!(
                        "Reparse or unsupported protected object: {}",
                        path.display()
                    ),
                ));
            }
            self.held.insert(path.to_owned(), file);
            Ok(())
        }
    }

    fn io_failure(path: &Path, error: std::io::Error) -> IpcError {
        failure(
            "file_guard_unavailable",
            format!("Cannot protect {}: {error}", path.display()),
        )
    }
}

fn failure(code: &str, message: impl Into<String>) -> IpcError {
    IpcError::new(code, message)
}

#[cfg(windows)]
pub use platform::Guard;

#[cfg(not(windows))]
pub struct Guard;
#[cfg(not(windows))]
impl Guard {
    pub fn new(_: PathBuf, _: PathBuf) -> Result<Self, IpcError> {
        Err(failure("unsupported", "Windows file protection required"))
    }
    pub fn status(&self) -> Status {
        Status {
            state: "unsupported",
            held_objects: 0,
            error: None,
            administrator_resistant: false,
        }
    }
    pub fn fence(&mut self) -> Result<(), IpcError> {
        Err(failure("unsupported", "Windows file protection required"))
    }
    pub fn synchronize(&mut self, _: bool) -> Result<(), IpcError> {
        self.fence()
    }
}
