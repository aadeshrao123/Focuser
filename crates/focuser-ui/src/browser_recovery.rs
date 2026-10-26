//! Temporary, journalled browser policies for installation-only recovery.
//! Browsers must restart on entry/exit: policy refresh is not instantaneous.
use focuser_common::extension::BrowserType;
use focuser_common::extension_watch::BrowserProfile;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

static POLICY_LOCK: Mutex<()> = Mutex::new(());
static STOPPING: AtomicBool = AtomicBool::new(false);

pub fn is_stopping() -> bool {
    STOPPING.load(Ordering::Acquire)
}

pub fn install(browser: &BrowserType, profile: Option<&BrowserProfile>) -> Result<(), String> {
    let _guard = POLICY_LOCK
        .lock()
        .map_err(|_| "Recovery policy lock poisoned")?;
    if is_stopping() {
        return Err("Focuser is shutting down".into());
    }
    imp::install(browser, profile)
}

pub fn restore(browser: &BrowserType) -> Result<(), String> {
    let _guard = POLICY_LOCK
        .lock()
        .map_err(|_| "Recovery policy lock poisoned")?;
    imp::restore(browser)
}

pub fn pending() -> Vec<BrowserType> {
    imp::pending()
}

pub fn saved_profile(browser: &BrowserType) -> Option<BrowserProfile> {
    imp::saved_profile(browser)
}

/// Called before an authorised quit as well as when enforcement is switched off.
pub fn restore_all() -> Result<(), String> {
    let _guard = POLICY_LOCK
        .lock()
        .map_err(|_| "Recovery policy lock poisoned")?;
    for browser in pending() {
        imp::restore(&browser)?;
    }
    Ok(())
}

pub fn shutdown() -> Result<(), String> {
    STOPPING.store(true, Ordering::Release);
    let result = restore_all();
    if result.is_err() {
        STOPPING.store(false, Ordering::Release);
    }
    result
}

#[cfg(windows)]
mod imp {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::io::Write;
    use std::path::PathBuf;
    use winreg::{RegKey, RegValue, enums::*};

    #[derive(Serialize, Deserialize)]
    struct SavedValue {
        name: String,
        bytes: Vec<u8>,
        kind: u32,
    }

    #[derive(Serialize, Deserialize)]
    struct Table {
        path: String,
        existed: bool,
        values: Vec<SavedValue>,
        recovery: Vec<String>,
    }

    #[derive(Serialize, Deserialize)]
    struct Journal {
        profile: Option<BrowserProfile>,
        tables: Vec<Table>,
    }

    fn tables(browser: &BrowserType) -> Result<Vec<(String, Vec<String>)>, String> {
        let (root, block, allow, denied, allowed): (&str, &str, &str, &str, Vec<&str>) =
            match browser {
                BrowserType::Chrome => (
                    r"Software\Policies\Google\Chrome",
                    "URLBlocklist",
                    "URLAllowlist",
                    "*",
                    chromium_allowlist(),
                ),
                BrowserType::Edge => (
                    r"Software\Policies\Microsoft\Edge",
                    "URLBlocklist",
                    "URLAllowlist",
                    "*",
                    chromium_allowlist(),
                ),
                BrowserType::Brave => (
                    r"Software\Policies\BraveSoftware\Brave",
                    "URLBlocklist",
                    "URLAllowlist",
                    "*",
                    chromium_allowlist(),
                ),
                BrowserType::Firefox => (
                    r"Software\Policies\Mozilla\Firefox\WebsiteFilter",
                    "Block",
                    "Exceptions",
                    "<all_urls>",
                    vec![
                        "https://addons.mozilla.org/*/firefox/addon/focuser-website-blocker/*",
                        "https://addons.mozilla.org/firefox/downloads/*",
                        "https://addons.cdn.mozilla.net/*",
                        "https://*.cdn.mozilla.net/*",
                        "http://127.0.0.1:17549/*",
                    ],
                ),
                // Opera does not document an equivalent supported policy. Keep it
                // closed rather than presenting an unrestricted recovery window.
                _ => {
                    return Err(
                        "Installation-only recovery is not supported by this browser".into(),
                    );
                }
            };
        Ok(vec![
            (format!(r"{root}\{block}"), vec![denied.into()]),
            (
                format!(r"{root}\{allow}"),
                allowed.into_iter().map(str::to_owned).collect(),
            ),
        ])
    }

    fn chromium_allowlist() -> Vec<&'static str> {
        // The store and package-download endpoints only. In particular, never
        // allow *.google.com (which would open YouTube and Google Search).
        vec![
            "https://chromewebstore.google.com/detail/jpnhbpbcmagoonmaleppldmcnaibkbmj",
            "https://chromewebstore.google.com/detail/focuser/jpnhbpbcmagoonmaleppldmcnaibkbmj",
            "https://clients2.google.com/service/update2/crx",
            "https://clients2.googleusercontent.com/crx/",
            "http://127.0.0.1:17549",
        ]
    }

    fn path(browser: &BrowserType) -> Result<PathBuf, String> {
        let dirs = directories::ProjectDirs::from("com", "focuser", "Focuser")
            .ok_or("Cannot locate the recovery journal directory")?;
        Ok(dirs
            .data_dir()
            .join("browser-recovery")
            .join(format!("{browser:?}.json")))
    }

    fn error(e: impl std::fmt::Display) -> String {
        format!("Browser recovery policy: {e}")
    }

    fn open(root: &RegKey, path: &str) -> Result<Option<RegKey>, String> {
        match root.open_subkey_with_flags(path, KEY_READ | KEY_WRITE) {
            Ok(key) => Ok(Some(key)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(error(e)),
        }
    }

    fn snapshot(path: String, recovery: Vec<String>) -> Result<Table, String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let key = open(&root, &path)?;
        let mut values = Vec::new();
        if let Some(key) = &key {
            for entry in key.enum_values() {
                let (name, value) = entry.map_err(error)?;
                values.push(SavedValue {
                    name,
                    bytes: value.bytes,
                    kind: value.vtype as u32,
                });
            }
        }
        Ok(Table {
            path,
            existed: key.is_some(),
            values,
            recovery,
        })
    }

    fn read(browser: &BrowserType) -> Result<Option<Journal>, String> {
        match std::fs::read(path(browser)?) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(error),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(error(e)),
        }
    }

    pub fn install(browser: &BrowserType, profile: Option<&BrowserProfile>) -> Result<(), String> {
        // A machine-wide allowlist can override a per-user restriction. Do not
        // open a recovery browser under a policy we cannot actually enforce.
        let spec = tables(browser)?;
        let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
        for (path, _) in &spec {
            match machine.open_subkey(path) {
                Ok(key) if key.enum_values().next().is_some() => {
                    return Err(
                        "Machine-managed URL policies prevent installation-only recovery".into(),
                    );
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(error(e)),
                _ => {}
            }
        }
        let journal = match read(browser)? {
            Some(journal) => journal,
            None => {
                let journal = Journal {
                    profile: profile.cloned(),
                    tables: spec
                        .into_iter()
                        .map(|(path, allowed)| snapshot(path, allowed))
                        .collect::<Result<_, _>>()?,
                };
                let destination = path(browser)?;
                let parent = destination
                    .parent()
                    .ok_or("Invalid recovery journal path")?;
                std::fs::create_dir_all(parent).map_err(error)?;
                let temporary = destination.with_extension("tmp");
                let mut file = std::fs::File::create(&temporary).map_err(error)?;
                file.write_all(&serde_json::to_vec(&journal).map_err(error)?)
                    .map_err(error)?;
                file.sync_all().map_err(error)?;
                drop(file);
                // Persist the originals BEFORE the first registry mutation.
                std::fs::rename(temporary, destination).map_err(error)?;
                journal
            }
        };
        apply_tables(&journal.tables)
    }

    fn apply_tables(tables: &[Table]) -> Result<(), String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        for table in tables {
            let (key, _) = root.create_subkey(&table.path).map_err(error)?;
            let names: Vec<_> = key
                .enum_values()
                .map(|v| v.map(|(n, _)| n))
                .collect::<Result<_, _>>()
                .map_err(error)?;
            for name in names {
                key.delete_value(name).map_err(error)?;
            }
            for (index, value) in table.recovery.iter().enumerate() {
                key.set_value((index + 1).to_string(), value)
                    .map_err(error)?;
            }
        }
        Ok(())
    }

    fn kind(value: u32) -> Result<RegType, String> {
        Ok(match value {
            0 => REG_NONE,
            1 => REG_SZ,
            2 => REG_EXPAND_SZ,
            3 => REG_BINARY,
            4 => REG_DWORD,
            5 => REG_DWORD_BIG_ENDIAN,
            6 => REG_LINK,
            7 => REG_MULTI_SZ,
            8 => REG_RESOURCE_LIST,
            9 => REG_FULL_RESOURCE_DESCRIPTOR,
            10 => REG_RESOURCE_REQUIREMENTS_LIST,
            11 => REG_QWORD,
            _ => return Err(format!("Unknown saved registry value type {value}")),
        })
    }

    pub fn restore(browser: &BrowserType) -> Result<(), String> {
        let Some(journal) = read(browser)? else {
            return Ok(());
        };
        restore_tables(&journal.tables)?;
        // A failed restore retains its journal for the next run/retry.
        std::fs::remove_file(path(browser)?).map_err(error)
    }

    fn restore_tables(tables: &[Table]) -> Result<(), String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        for table in tables {
            let (key, _) = root.create_subkey(&table.path).map_err(error)?;
            // Remove our numbered entries; leave unrelated later additions alone.
            for index in 1..=table.recovery.len() {
                match key.delete_value(index.to_string()) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(error(e)),
                    _ => {}
                }
            }
            for value in &table.values {
                key.set_raw_value(
                    &value.name,
                    &RegValue {
                        bytes: value.bytes.clone(),
                        vtype: kind(value.kind)?,
                    },
                )
                .map_err(error)?;
            }
            if !table.existed
                && key.enum_values().next().is_none()
                && key.enum_keys().next().is_none()
            {
                drop(key);
                root.delete_subkey(&table.path).map_err(error)?;
            }
        }
        Ok(())
    }

    pub fn pending() -> Vec<BrowserType> {
        [
            BrowserType::Chrome,
            BrowserType::Edge,
            BrowserType::Brave,
            BrowserType::Firefox,
        ]
        .into_iter()
        .filter(|browser| path(browser).is_ok_and(|p| p.exists()))
        .collect()
    }

    pub fn saved_profile(browser: &BrowserType) -> Option<BrowserProfile> {
        read(browser)
            .ok()
            .flatten()
            .and_then(|journal| journal.profile)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn recovery_allows_installation_without_a_general_google_bypass() {
            let chrome = tables(&BrowserType::Chrome).unwrap();
            assert_eq!(chrome[0].1, ["*"]);
            assert!(
                chrome[1]
                    .1
                    .contains(&focuser_common::browser::CHROME_STORE_URL.into())
            );
            assert!(
                !chrome[1]
                    .1
                    .contains(&"https://chromewebstore.google.com".into())
            );
            assert!(!chrome[1].1.iter().any(|url| url.contains("*.google.com")));
            let firefox = tables(&BrowserType::Firefox).unwrap();
            assert_eq!(firefox[0].1, ["<all_urls>"]);
            assert!(tables(&BrowserType::Opera).is_err());
        }

        #[test]
        fn saved_policy_bytes_and_types_round_trip() {
            let table = Table {
                path: "test".into(),
                existed: true,
                values: vec![SavedValue {
                    name: "1".into(),
                    bytes: vec![1, 0, 255],
                    kind: REG_BINARY as u32,
                }],
                recovery: vec!["*".into()],
            };
            let encoded = serde_json::to_vec(&table).unwrap();
            let decoded: Table = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(decoded.values[0].bytes, [1, 0, 255]);
            assert_eq!(kind(decoded.values[0].kind).unwrap(), REG_BINARY);
        }

        #[test]
        fn replacing_and_restoring_a_list_preserves_existing_policy_values() {
            // A unique scratch key, never a real browser policy key.
            let scratch = format!(r"Software\Focuser\RecoveryTests\{}", uuid::Uuid::new_v4());
            let root = RegKey::predef(HKEY_CURRENT_USER);
            let (key, _) = root.create_subkey(&scratch).unwrap();
            key.set_value("1", &"https://existing.example").unwrap();
            key.set_value("17", &"https://another.example").unwrap();
            let before = snapshot(scratch.clone(), vec!["*".into()]).unwrap();
            apply_tables(std::slice::from_ref(&before)).unwrap();
            assert_eq!(key.get_value::<String, _>("1").unwrap(), "*");
            assert!(key.get_value::<String, _>("17").is_err());
            restore_tables(&[before]).unwrap();
            assert_eq!(
                key.get_value::<String, _>("1").unwrap(),
                "https://existing.example"
            );
            assert_eq!(
                key.get_value::<String, _>("17").unwrap(),
                "https://another.example"
            );
            drop(key);
            root.delete_subkey(&scratch).unwrap();
        }

        #[test]
        fn a_new_recovery_key_is_removed_on_restore() {
            let scratch = format!(r"Software\Focuser\RecoveryTests\{}", uuid::Uuid::new_v4());
            let before = snapshot(scratch.clone(), vec!["*".into()]).unwrap();
            assert!(!before.existed);
            apply_tables(std::slice::from_ref(&before)).unwrap();
            restore_tables(&[before]).unwrap();
            assert!(
                RegKey::predef(HKEY_CURRENT_USER)
                    .open_subkey(&scratch)
                    .is_err()
            );
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub fn install(_: &BrowserType, _: Option<&BrowserProfile>) -> Result<(), String> {
        Err("Installation-only browser policies are currently supported on Windows".into())
    }
    pub fn restore(_: &BrowserType) -> Result<(), String> {
        Ok(())
    }
    pub fn pending() -> Vec<BrowserType> {
        Vec::new()
    }
    pub fn saved_profile(_: &BrowserType) -> Option<BrowserProfile> {
        None
    }
}
