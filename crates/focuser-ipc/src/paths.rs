use crate::IpcError;
use std::path::{Path, PathBuf};
pub fn production_root() -> Result<PathBuf, IpcError> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::{System::Com::CoTaskMemFree, UI::Shell::*};
        let path = SHGetKnownFolderPath(&FOLDERID_ProgramData, KF_FLAG_DEFAULT, None)
            .map_err(|e| IpcError::new("storage", e.to_string()))?;
        let text = path
            .to_string()
            .map_err(|e| IpcError::new("storage", e.to_string()));
        CoTaskMemFree(Some(path.0.cast()));
        Ok(PathBuf::from(text?).join("FocuserAuthority"))
    }
    #[cfg(not(windows))]
    {
        Err(IpcError::new(
            "unsupported",
            "ProgramData authority storage is Windows-only",
        ))
    }
}

pub fn ensure_development_database(path: &Path) -> Result<PathBuf, IpcError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(storage)?.join(path)
    };
    let parent = absolute
        .parent()
        .ok_or_else(|| IpcError::new("validation", "Database needs a parent directory"))?;
    let canonical_parent = parent.canonicalize().map_err(storage)?;
    let resolved = canonical_parent.join(
        absolute
            .file_name()
            .ok_or_else(|| IpcError::new("validation", "Invalid database filename"))?,
    );
    if absolute.exists()
        && absolute
            .symlink_metadata()
            .map_err(storage)?
            .file_type()
            .is_symlink()
    {
        return Err(IpcError::new(
            "validation",
            "Development databases cannot be symlinks",
        ));
    }
    #[cfg(windows)]
    if absolute.exists() {
        use ::windows::{
            Win32::{Foundation::*, Storage::FileSystem::*},
            core::PCWSTR,
        };
        let name: Vec<u16> = absolute
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                HANDLE::default(),
            )
        }
        .map_err(storage)?;
        let handle = crate::windows::OwnedHandle(handle);
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(handle.0, &mut information) }.map_err(storage)?;
        if information.nNumberOfLinks != 1 {
            return Err(IpcError::new(
                "validation",
                "Development databases cannot be hard-link aliases",
            ));
        }
    }
    if let Ok(root) = production_root() {
        let root = root.canonicalize().unwrap_or(root);
        if resolved
            .to_string_lossy()
            .to_lowercase()
            .starts_with(&root.to_string_lossy().to_lowercase())
        {
            return Err(IpcError::new(
                "validation",
                "A development client cannot open authoritative storage",
            ));
        }
    }
    if let Some(profile) = directories::ProjectDirs::from("com", "focuser", "Focuser") {
        let old = profile.data_dir().join("focuser.db");
        if resolved
            .to_string_lossy()
            .eq_ignore_ascii_case(&old.to_string_lossy())
        {
            return Err(IpcError::new(
                "validation",
                "Development cannot open the live legacy profile database; use explicit migration",
            ));
        }
    }
    Ok(resolved)
}

fn storage(error: impl std::fmt::Display) -> IpcError {
    IpcError::new("storage", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scratch_paths_are_explicit_and_do_not_create_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scratch.db");
        assert_eq!(
            ensure_development_database(&path).unwrap(),
            path.canonicalize().unwrap_or_else(|_| directory
                .path()
                .canonicalize()
                .unwrap()
                .join("scratch.db"))
        );
        assert!(!path.exists());
    }
    #[cfg(windows)]
    #[test]
    fn aliases_cannot_point_development_at_existing_live_data() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.db");
        let alias = directory.path().join("alias.db");
        std::fs::write(&original, b"unchanged").unwrap();
        std::fs::hard_link(&original, &alias).unwrap();
        assert_eq!(
            ensure_development_database(&alias).unwrap_err().code,
            "validation"
        );
        assert_eq!(std::fs::read(&original).unwrap(), b"unchanged");
    }
}
