//! Temporary, journalled browser policies for installation-only recovery.
//! Browser update pages and dedicated update endpoints remain accessible during
//! recovery. Ordinary browsing still requires extension coverage.
use focuser_common::extension::BrowserType;
use focuser_common::extension_watch::BrowserProfile;
use std::sync::Mutex;
static POLICY_LOCK: Mutex<()> = Mutex::new(());

pub fn install(browser: &BrowserType, profile: Option<&BrowserProfile>) -> Result<(), String> {
    let _guard = POLICY_LOCK
        .lock()
        .map_err(|_| "Recovery policy lock poisoned")?;
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

/// Unlike the display-oriented pending list, errors must never mean clean.
pub fn is_clear() -> Result<bool, String> {
    let _guard = POLICY_LOCK
        .lock()
        .map_err(|_| "Recovery policy lock poisoned")?;
    imp::is_clear()
}
/// A deleted journal is not evidence of restoration. Cold recovery can proceed
/// without it only if every policy table Focuser uses is actually empty/absent.
#[cfg(windows)]
pub fn orphaned_policies_clear() -> Result<bool, String> {
    imp::orphaned_policies_clear()
}

pub fn saved_profile(browser: &BrowserType) -> Option<BrowserProfile> {
    imp::saved_profile(browser)
}

#[cfg(windows)]
mod imp {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::io::Write;
    use std::path::PathBuf;
    use winreg::{RegKey, RegValue, enums::*};
    pub fn orphaned_policies_clear() -> Result<bool, String> {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        Ok(orphaned_policies_clear_at(&user, KEY_WOW64_64KEY)?
            && orphaned_policies_clear_at(&user, KEY_WOW64_32KEY)?)
    }
    fn orphaned_policies_clear_at(user: &RegKey, view: u32) -> Result<bool, String> {
        for browser in [
            BrowserType::Chrome,
            BrowserType::Edge,
            BrowserType::Brave,
            BrowserType::Firefox,
        ] {
            for (path, _) in tables(&browser)? {
                let key = match user.open_subkey_with_flags(&path, KEY_READ | view) {
                    Ok(key) => key,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(error(e)),
                };
                if key
                    .enum_values()
                    .next()
                    .transpose()
                    .map_err(error)?
                    .is_some()
                    || key.enum_keys().next().transpose().map_err(error)?.is_some()
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    #[cfg(test)]
    #[test]
    fn missing_journal_requires_empty_tables_and_preserves_unknown_policy_values() {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!(r"Software\FocuserRecoveryTests\{}", uuid::Uuid::new_v4());
        let (scratch, _) = user.create_subkey(&path).unwrap();
        assert!(orphaned_policies_clear_at(&scratch, 0).unwrap());
        let (table, _) = scratch
            .create_subkey(r"Software\Policies\BraveSoftware\Brave\URLBlocklist")
            .unwrap();
        assert!(orphaned_policies_clear_at(&scratch, 0).unwrap());
        table.set_value("1", &"*").unwrap();
        assert!(!orphaned_policies_clear_at(&scratch, 0).unwrap());
        assert_eq!(table.get_value::<String, _>("1").unwrap(), "*");
        drop(table);
        drop(scratch);
        user.delete_subkey_all(&path).unwrap();
    }

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
                    chromium_allowlist(browser),
                ),
                BrowserType::Edge => (
                    r"Software\Policies\Microsoft\Edge",
                    "URLBlocklist",
                    "URLAllowlist",
                    "*",
                    chromium_allowlist(browser),
                ),
                BrowserType::Brave => (
                    r"Software\Policies\BraveSoftware\Brave",
                    "URLBlocklist",
                    "URLAllowlist",
                    "*",
                    chromium_allowlist(browser),
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
                        "https://aus5.mozilla.org/update/*",
                        "https://download.mozilla.org/*",
                        "https://archive.mozilla.org/pub/firefox/releases/*",
                        "https://download-installer.cdn.mozilla.net/pub/firefox/releases/*",
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

    fn chromium_allowlist(browser: &BrowserType) -> Vec<&'static str> {
        // Append only browser-specific update UI and delivery locations. Never
        // allow the whole settings UI, *.google.com, or a general download site.
        // Keep the original entries in order for existing recovery journals.
        let mut allowed = vec![
            "https://chromewebstore.google.com/detail/jpnhbpbcmagoonmaleppldmcnaibkbmj",
            "https://chromewebstore.google.com/detail/focuser/jpnhbpbcmagoonmaleppldmcnaibkbmj",
            "https://clients2.google.com/service/update2/crx",
            "https://clients2.googleusercontent.com/crx/",
            "http://127.0.0.1:17549",
        ];
        allowed.extend(match browser {
            BrowserType::Chrome => vec![
                "chrome://settings/help",
                "https://update.googleapis.com/service/update2",
                "https://tools.google.com/service/update2",
                "https://dl.google.com/release2/",
                "https://dl.google.com/chrome/",
            ],
            BrowserType::Brave => vec![
                "brave://settings/help",
                // Brave aliases chrome:// internally on Chromium builds.
                "chrome://settings/help",
                "https://updates.bravesoftware.com/",
                "https://updates-cdn.bravesoftware.com/",
                "https://componentupdater.brave.com/service/update2",
                "https://laptop-updates.brave.com/latest/",
            ],
            BrowserType::Edge => vec![
                "edge://settings/help",
                "https://msedge.api.cdp.microsoft.com/",
                "https://msedge.sf.dl.delivery.mp.microsoft.com/",
                "https://msedge.f.tlu.dl.delivery.mp.microsoft.com/",
                "https://msedge.f.dl.delivery.mp.microsoft.com/",
            ],
            _ => vec![],
        });
        allowed
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
            Some(mut journal) => {
                if extend_recovery(&mut journal, &spec)? {
                    persist(browser, &journal)?;
                }
                journal
            }
            None => {
                let journal = Journal {
                    profile: profile.cloned(),
                    tables: spec
                        .into_iter()
                        .map(|(path, allowed)| snapshot(path, allowed))
                        .collect::<Result<_, _>>()?,
                };
                persist(browser, &journal)?;
                journal
            }
        };
        apply_tables(&journal.tables)
    }

    // An old journal contains the originals needed for eventual restoration.
    // Extend its owned numbered values; never snapshot already-applied recovery
    // policies as the user's originals. Reject an unexpected shape before writes.
    fn extend_recovery(
        journal: &mut Journal,
        spec: &[(String, Vec<String>)],
    ) -> Result<bool, String> {
        if journal.tables.len() != spec.len()
            || journal
                .tables
                .iter()
                .zip(spec)
                .any(|(table, (path, values))| {
                    table.path != *path || !values.starts_with(&table.recovery)
                })
        {
            return Err(
                "browser_policy_conflict: Recovery journal differs from supported policy".into(),
            );
        }
        let mut changed = false;
        for (table, (_, values)) in journal.tables.iter_mut().zip(spec) {
            if table.recovery != *values {
                table.recovery.clone_from(values);
                changed = true;
            }
        }
        Ok(changed)
    }

    fn persist(browser: &BrowserType, journal: &Journal) -> Result<(), String> {
        let destination = path(browser)?;
        let parent = destination
            .parent()
            .ok_or("Invalid recovery journal path")?;
        std::fs::create_dir_all(parent).map_err(error)?;
        let temporary = destination.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(error)?;
        file.write_all(&serde_json::to_vec(journal).map_err(error)?)
            .map_err(error)?;
        file.sync_all().map_err(error)?;
        drop(file);
        // Persist the restoration ledger BEFORE mutating any registry value.
        std::fs::rename(temporary, destination).map_err(error)
    }

    fn apply_tables(tables: &[Table]) -> Result<(), String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        use winreg::types::ToRegValue;
        // Re-applying recovery after a failed removal must also preserve later
        // policy writers. Refuse the recovery browser rather than clobber them.
        for table in tables {
            let key = match root.open_subkey(&table.path) {
                Ok(key) => key,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(error(e)),
            };
            for value in key.enum_values() {
                let (name, current) = value.map_err(error)?;
                let original = table.values.iter().find(|v| v.name == name);
                let ours = name
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| table.recovery.get(i))
                    .map(|v| v.to_reg_value());
                let matches_original = original.is_some_and(|v| {
                    v.bytes == current.bytes && kind(v.kind).is_ok_and(|kind| kind == current.vtype)
                });
                let matches_ours =
                    ours.is_some_and(|v| v.bytes == current.bytes && v.vtype == current.vtype);
                if !matches_original && !matches_ours {
                    return Err(format!(
                        "browser_policy_conflict: {} changed outside Focuser",
                        table.path
                    ));
                }
            }
        }
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
        restore_tables_at(&root, tables)
    }

    fn restore_tables_at(root: &RegKey, tables: &[Table]) -> Result<(), String> {
        // Preflight every touched value before any restoration. A later policy
        // writer owns its changes: retain our journal and fail cleanup rather
        // than removing an unrelated policy at the same numbered slot.
        use winreg::types::ToRegValue;
        for table in tables {
            let key = match root.open_subkey(&table.path) {
                Ok(key) => key,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(error(e)),
            };
            let mut names: Vec<String> =
                (1..=table.recovery.len()).map(|i| i.to_string()).collect();
            names.extend(table.values.iter().map(|v| v.name.clone()));
            for name in names {
                let current = match key.get_raw_value(&name) {
                    Ok(value) => value,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(error(e)),
                };
                let original = table.values.iter().find(|v| v.name == name);
                let ours = name
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| table.recovery.get(i))
                    .map(|v| v.to_reg_value());
                let matches_original = original.is_some_and(|v| {
                    v.bytes == current.bytes && kind(v.kind).is_ok_and(|kind| kind == current.vtype)
                });
                let matches_ours =
                    ours.is_some_and(|v| v.bytes == current.bytes && v.vtype == current.vtype);
                if !matches_original && !matches_ours {
                    return Err(format!(
                        "browser_policy_conflict: {} value {} changed outside Focuser; recovery journal retained",
                        table.path, name
                    ));
                }
            }
        }
        for table in tables {
            let key = match open(root, &table.path)? {
                Some(key) => key,
                // A failed policy application may have written only its
                // journal. Absence already restores an originally absent key;
                // creating it would require permissions we never had and could
                // prevent the agent from acknowledging an otherwise clean exit.
                None if !table.existed && table.values.is_empty() => continue,
                None => root.create_subkey(&table.path).map_err(error)?.0,
            };
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

    pub fn is_clear() -> Result<bool, String> {
        for browser in [
            BrowserType::Chrome,
            BrowserType::Edge,
            BrowserType::Brave,
            BrowserType::Firefox,
        ] {
            match std::fs::symlink_metadata(path(&browser)?) {
                Ok(_) => return Ok(false),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(error(e)),
            }
        }
        Ok(true)
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
        fn update_pages_and_delivery_are_available_without_a_general_browsing_exception() {
            for (browser, update_page, endpoint) in [
                (
                    BrowserType::Brave,
                    "brave://settings/help",
                    "https://updates.bravesoftware.com/",
                ),
                (
                    BrowserType::Chrome,
                    "chrome://settings/help",
                    "https://update.googleapis.com/service/update2",
                ),
                (
                    BrowserType::Edge,
                    "edge://settings/help",
                    "https://msedge.api.cdp.microsoft.com/",
                ),
            ] {
                let spec = tables(&browser).unwrap();
                assert_eq!(spec[0].1, ["*"]);
                let allowed = &spec[1].1;
                assert!(allowed.iter().any(|url| url == update_page));
                assert!(allowed.iter().any(|url| url == endpoint));
                for broad in [
                    "*",
                    "https://*.google.com",
                    "https://*.brave.com",
                    "brave://*",
                    "chrome://settings",
                    "https://github.com/",
                ] {
                    assert!(!allowed.iter().any(|url| url == broad));
                }
            }
        }

        #[test]
        fn an_old_recovery_journal_gains_updates_and_restores_the_true_originals() {
            let scratch = format!(r"Software\Focuser\RecoveryTests\{}", uuid::Uuid::new_v4());
            let root = RegKey::predef(HKEY_CURRENT_USER);
            let (key, _) = root.create_subkey(&scratch).unwrap();
            key.set_value("1", &"https://original.example/").unwrap();
            key.set_value("19", &"https://another-original.example/")
                .unwrap();
            let old = vec!["https://extension-store.example/".to_owned()];
            let mut journal = Journal {
                profile: None,
                tables: vec![snapshot(scratch.clone(), old.clone()).unwrap()],
            };
            apply_tables(&journal.tables).unwrap();
            let mut expanded = old;
            expanded.push("brave://settings/help".into());
            expanded.push("https://updates.bravesoftware.com/".into());
            let spec = vec![(scratch.clone(), expanded)];
            assert!(extend_recovery(&mut journal, &spec).unwrap());
            assert!(!extend_recovery(&mut journal, &spec).unwrap());
            // Crash after persisting the larger journal, before policy writes:
            // the existing shorter policy is still recognized and restorable.
            restore_tables(&journal.tables).unwrap();
            assert_eq!(
                key.get_value::<String, _>("1").unwrap(),
                "https://original.example/"
            );
            apply_tables(&journal.tables).unwrap();
            assert_eq!(
                key.get_value::<String, _>("2").unwrap(),
                "brave://settings/help"
            );
            assert_eq!(
                key.get_value::<String, _>("3").unwrap(),
                "https://updates.bravesoftware.com/"
            );
            restore_tables(&journal.tables).unwrap();
            assert_eq!(
                key.get_value::<String, _>("1").unwrap(),
                "https://original.example/"
            );
            assert_eq!(
                key.get_value::<String, _>("19").unwrap(),
                "https://another-original.example/"
            );
            assert!(key.get_value::<String, _>("2").is_err());
            assert!(key.get_value::<String, _>("3").is_err());
            drop(key);
            root.delete_subkey(&scratch).unwrap();
        }

        #[test]
        fn journal_extension_rejects_changed_targets_and_never_recaptures_policy() {
            let mut journal = Journal {
                profile: None,
                tables: vec![Table {
                    path: "expected".into(),
                    existed: false,
                    values: vec![],
                    recovery: vec!["old".into()],
                }],
            };
            for spec in [
                vec![],
                vec![("different".into(), vec!["old".into(), "new".into()])],
                vec![("expected".into(), vec!["changed".into()])],
            ] {
                assert!(extend_recovery(&mut journal, &spec).is_err());
                assert_eq!(journal.tables[0].recovery, ["old"]);
                assert!(journal.tables[0].values.is_empty());
            }
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
        fn uninstall_cleanup_does_not_overwrite_a_later_policy_writer() {
            let scratch = format!(r"Software\Focuser\RecoveryTests\{}", uuid::Uuid::new_v4());
            let root = RegKey::predef(HKEY_CURRENT_USER);
            let before = snapshot(scratch.clone(), vec!["*".into()]).unwrap();
            apply_tables(std::slice::from_ref(&before)).unwrap();
            let key = root
                .open_subkey_with_flags(&scratch, KEY_ALL_ACCESS)
                .unwrap();
            key.set_value("1", &"https://other-policy.example").unwrap();
            assert!(
                apply_tables(std::slice::from_ref(&before))
                    .unwrap_err()
                    .contains("browser_policy_conflict")
            );
            assert!(
                restore_tables(&[before])
                    .unwrap_err()
                    .contains("browser_policy_conflict")
            );
            assert_eq!(
                key.get_value::<String, _>("1").unwrap(),
                "https://other-policy.example"
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

        #[test]
        fn unapplied_recovery_restores_absence_without_create_permission() {
            use windows::Win32::{
                Foundation::{BOOL, HANDLE},
                Security::{
                    Authorization::{SE_REGISTRY_KEY, SetSecurityInfo},
                    DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl,
                    PROTECTED_DACL_SECURITY_INFORMATION, PSID,
                },
            };
            let scratch = format!(r"Software\Focuser\RecoveryTests\{}", uuid::Uuid::new_v4());
            let user = RegKey::predef(HKEY_CURRENT_USER);
            let (parent, _) = user.create_subkey(&scratch).unwrap();
            let (sid, _) = focuser_ipc::windows::current_identity().unwrap();
            let descriptor = focuser_ipc::windows::Descriptor::from_sddl(&format!(
                "D:P(D;;0x0004;;;{sid})(A;;KA;;;{sid})"
            ))
            .unwrap();
            let mut present = BOOL::default();
            let mut defaulted = BOOL::default();
            let mut dacl = std::ptr::null_mut();
            unsafe {
                GetSecurityDescriptorDacl(
                    descriptor.pointer(),
                    &mut present,
                    &mut dacl,
                    &mut defaulted,
                )
                .unwrap();
                assert!(present.as_bool());
                SetSecurityInfo(
                    HANDLE(parent.raw_handle()),
                    SE_REGISTRY_KEY,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    PSID::default(),
                    PSID::default(),
                    Some(dacl.cast_const()),
                    None,
                )
                .ok()
                .unwrap();
            }
            let readonly = user.open_subkey_with_flags(&scratch, KEY_READ).unwrap();
            assert!(readonly.create_subkey("cannot-create").is_err());
            let table = Table {
                path: "NeverApplied".into(),
                existed: false,
                values: vec![],
                recovery: vec!["*".into()],
            };
            restore_tables_at(&readonly, &[table]).unwrap();
            assert_eq!(readonly.enum_keys().count(), 0);
            drop(readonly);
            drop(parent);
            user.delete_subkey(&scratch).unwrap();
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
    pub fn is_clear() -> Result<bool, String> {
        Ok(true)
    }
    pub fn saved_profile(_: &BrowserType) -> Option<BrowserProfile> {
        None
    }
}
