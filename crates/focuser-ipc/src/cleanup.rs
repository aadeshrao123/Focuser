//! Bounded deletion of a caller-selected, fixed application-owned directory.
//! Not an IPC operation: paths must come from known folders/configuration, never
//! from a removal request. Inspect the entire tree before deleting any entry.
use crate::IpcError;
use std::path::{Path, PathBuf};

fn error(path: &Path, error: impl std::fmt::Display) -> IpcError {
    IpcError::new("cleanup_failed", format!("{}: {error}", path.display()))
}
fn inspect(path: &Path) -> Result<std::fs::Metadata, IpcError> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| error(path, e))?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = meta.file_type().is_symlink();
    if reparse || !(meta.is_file() || meta.is_dir()) {
        return Err(error(path, "Refusing reparse point or special file"));
    }
    Ok(meta)
}

struct Entry {
    path: PathBuf,
    directory: bool,
    // Keep directories from being renamed/replaced while traversing children.
    #[cfg(windows)]
    pin: Option<std::fs::File>,
}
pub struct OwnedTree {
    entries: Vec<Entry>,
}
impl OwnedTree {
    pub fn inspect(
        root: &Path,
        validate: &impl Fn(&Path) -> Result<(), IpcError>,
    ) -> Result<Self, IpcError> {
        if !root.is_absolute() || root.parent().is_none() || root.file_name().is_none() {
            return Err(error(root, "An absolute application directory is required"));
        }
        if !inspect(root)?.is_dir() {
            return Err(error(root, "An application directory is required"));
        }
        // Reject links in ancestors too. In particular, do not elevate through
        // a user-controlled junction into somebody else's data.
        for ancestor in root.ancestors().skip(1) {
            if ancestor.parent().is_some() {
                inspect(ancestor)?;
            }
        }
        let mut tree = Self { entries: vec![] };
        let mut count = 0;
        tree.walk(root, 0, &mut count, validate)?;
        Ok(tree)
    }
    fn walk(
        &mut self,
        path: &Path,
        depth: usize,
        count: &mut usize,
        validate: &impl Fn(&Path) -> Result<(), IpcError>,
    ) -> Result<(), IpcError> {
        *count += 1;
        if depth > 32 || *count > 50_000 {
            return Err(error(path, "Cleanup inventory exceeds its safety bound"));
        }
        let meta = inspect(path)?;
        #[cfg(windows)]
        let pin = if meta.is_dir() {
            use std::os::windows::fs::OpenOptionsExt;
            Some(
                std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(3) // READ | WRITE, no DELETE
                    .custom_flags(0x02200000) // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
                    .open(path)
                    .map_err(|e| error(path, e))?,
            )
        } else {
            None
        };
        inspect(path)?;
        validate(path)?;
        if meta.is_dir() {
            for entry in std::fs::read_dir(path).map_err(|e| error(path, e))? {
                self.walk(
                    &entry.map_err(|e| error(path, e))?.path(),
                    depth + 1,
                    count,
                    validate,
                )?;
            }
        }
        self.entries.push(Entry {
            path: path.into(),
            directory: meta.is_dir(),
            #[cfg(windows)]
            pin,
        });
        Ok(())
    }
    pub fn remove(self) -> Result<(), IpcError> {
        // Postorder; parent handles remain pinned until all children are gone.
        // No delayed-on-reboot deletion, no traversing reparse points, no success
        // if a lock, unexpected new entry, or permission prevents cleanup.
        for entry in self.entries {
            #[cfg(windows)]
            drop(entry.pin);
            if inspect(&entry.path)?.is_dir() != entry.directory {
                return Err(error(&entry.path, "Cleanup object changed type"));
            }
            let result = if entry.directory {
                std::fs::remove_dir(&entry.path)
            } else {
                std::fs::remove_file(&entry.path)
            };
            result.map_err(|e| error(&entry.path, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_only_the_owned_tree_including_enrollment_and_update_leftovers() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().join("FocuserAuthority");
        std::fs::create_dir_all(root.join("updates/staged")).unwrap();
        for name in [
            "service.json",
            "authority.db",
            "authority.lock",
            "removal-progress.json",
            "updates/staged/payload.exe",
        ] {
            std::fs::write(root.join(name), b"test").unwrap();
        }
        let backup = scratch.path().join("Focuser.Backup");
        std::fs::write(&backup, b"preserve").unwrap();
        OwnedTree::inspect(&root, &|_| Ok(()))
            .unwrap()
            .remove()
            .unwrap();
        assert!(!root.exists());
        assert_eq!(std::fs::read(backup).unwrap(), b"preserve");
        // No retained enrollment tombstone prevents a later explicit setup.
        std::fs::create_dir(&root).unwrap();
    }
    #[test]
    fn inventory_failure_deletes_nothing() {
        let scratch = tempfile::tempdir().unwrap();
        std::fs::write(scratch.path().join("keep"), b"policy").unwrap();
        assert!(
            OwnedTree::inspect(scratch.path(), &|_| Err(IpcError::new("test", "denied"))).is_err()
        );
        assert!(scratch.path().join("keep").exists());
        assert!(OwnedTree::inspect(Path::new("relative"), &|_| Ok(())).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn junction_inventory_cannot_delete_outside_data_or_any_owned_data() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().join("owned");
        let outside = scratch.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(root.join("policy"), b"keep").unwrap();
        std::fs::write(outside.join("unrelated"), b"keep").unwrap();
        let junction = root.join("redirected");
        // Creation only. Deletion stays in Rust and never traverses this link.
        let status = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(status.status.success(), "{:?}", status);
        assert!(OwnedTree::inspect(&root, &|_| Ok(())).is_err());
        assert!(root.join("policy").exists());
        assert!(outside.join("unrelated").exists());
        assert!(OwnedTree::inspect(&junction, &|_| Ok(())).is_err());
        assert!(OwnedTree::inspect(&junction.join("unrelated"), &|_| Ok(())).is_err());
        std::fs::remove_dir(junction).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn locked_file_is_a_failure_and_directory_pins_prevent_replacement() {
        use std::os::windows::fs::OpenOptionsExt;
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().join("owned");
        std::fs::create_dir(&root).unwrap();
        let file = root.join("locked");
        std::fs::write(&file, b"test").unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&file)
            .unwrap();
        let tree = OwnedTree::inspect(&root, &|_| Ok(())).unwrap();
        assert!(std::fs::rename(&root, scratch.path().join("other")).is_err());
        assert_eq!(tree.remove().unwrap_err().code, "cleanup_failed");
        assert!(file.exists());
        drop(lock);
        OwnedTree::inspect(&root, &|_| Ok(()))
            .unwrap()
            .remove()
            .unwrap();
    }
}
