//! Read-only checks of browser-owned extension state. No heartbeat timeout is
//! needed when a profile explicitly says the extension is missing or disabled.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::extension::BrowserType;

pub const CHROMIUM_EXTENSION_ID: &str = "jpnhbpbcmagoonmaleppldmcnaibkbmj";
const FIREFOX_EXTENSION_ID: &str = "focuser@focuser-app";
// Earlier packaged builds used this ID; both are genuine Focuser installs.
const CHROMIUM_EXTENSION_IDS: [&str; 2] =
    [CHROMIUM_EXTENSION_ID, "kdkmjbjegdcjdlbciifigfkifookbppg"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserProfile {
    /// Chromium user-data directory; Firefox profile directory.
    pub root: PathBuf,
    /// Chromium profile directory relative to `root`; empty for Firefox.
    pub directory: String,
}

impl BrowserProfile {
    fn path(&self) -> PathBuf {
        self.root.join(&self.directory)
    }

    pub fn launch_args(&self, firefox: bool) -> Vec<String> {
        if firefox {
            vec!["-profile".into(), self.root.to_string_lossy().into_owned()]
        } else {
            vec![
                format!("--user-data-dir={}", self.root.display()),
                format!("--profile-directory={}", self.directory),
            ]
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Enabled,
    Missing,
    /// Unreadable, being atomically replaced, or an unrecognised file format.
    Unknown,
}

#[derive(Default)]
pub struct ProfileMonitor {
    files: HashMap<PathBuf, (SystemTime, u64, Arc<Value>)>,
    installed_directories: HashMap<PathBuf, PathBuf>,
    uncovered: HashSet<PathBuf>,
}

impl ProfileMonitor {
    fn json(&mut self, path: &Path) -> Option<Arc<Value>> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok()?;
        if let Some((at, size, json)) = self.files.get(path)
            && *at == modified
            && *size == metadata.len()
        {
            return Some(json.clone());
        }
        // A failed read must not reuse stale data or manufacture an uninstall.
        let json: Arc<Value> = Arc::new(serde_json::from_slice(&std::fs::read(path).ok()?).ok()?);
        self.files
            .insert(path.into(), (modified, metadata.len(), json.clone()));
        Some(json)
    }

    pub fn coverage(&mut self, profile: &BrowserProfile, firefox: bool) -> Coverage {
        let status = self.read_coverage(profile, firefox);
        let path = profile.path();
        match status {
            Coverage::Enabled => {
                self.uncovered.remove(&path);
                Coverage::Enabled
            }
            Coverage::Missing => {
                self.uncovered.insert(path);
                Coverage::Missing
            }
            Coverage::Unknown if self.uncovered.contains(&path) => Coverage::Missing,
            Coverage::Unknown => Coverage::Unknown,
        }
    }

    fn read_coverage(&mut self, profile: &BrowserProfile, firefox: bool) -> Coverage {
        let path = profile.path();
        // The browser removes installed files before its delayed preferences
        // write. Only watch a directory we have actually seen installed, and
        // distinguish NotFound from access-denied/transient IO failures.
        if let Some(installed) = self.installed_directories.get(&path)
            && std::fs::metadata(installed).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            && !CHROMIUM_EXTENSION_IDS
                .iter()
                .any(|id| path.join("Extensions").join(id).is_dir())
        {
            return Coverage::Missing;
        }
        if firefox {
            return self
                .json(&path.join("extensions.json"))
                .map_or(Coverage::Unknown, |json| firefox_coverage(&json));
        }
        let secure_path = path.join("Secure Preferences");
        let secure = self.json(&secure_path);
        if secure.is_none() && secure_path.exists() {
            return Coverage::Unknown;
        }
        let preferences = self.json(&path.join("Preferences"));
        let status = chromium_coverage(secure.as_deref(), preferences.as_deref());
        if status == Coverage::Enabled
            && let Some(installed) = CHROMIUM_EXTENSION_IDS
                .iter()
                .map(|id| path.join("Extensions").join(id))
                .find(|p| p.is_dir())
        {
            self.installed_directories.insert(path, installed);
        }
        status
    }

    /// Select the profiles the browser is actually using, rather than declaring
    /// every dormant/unused profile on disk to be a missing extension.
    pub fn profiles(&mut self, browser: &BrowserType, cmdlines: &[String]) -> Vec<BrowserProfile> {
        let firefox = *browser == BrowserType::Firefox;
        let mut profiles = Vec::new();
        for command in cmdlines {
            if firefox {
                if let Some(root) = argument(command, "-profile") {
                    profiles.push(BrowserProfile {
                        root: root.into(),
                        directory: String::new(),
                    });
                }
            } else {
                let root = argument(command, "--user-data-dir")
                    .map(PathBuf::from)
                    .or_else(|| default_root(browser));
                let Some(root) = root else { continue };
                if let Some(directory) = argument(command, "--profile-directory") {
                    if valid_directory(&directory) {
                        profiles.push(BrowserProfile { root, directory });
                    }
                } else {
                    let state = self.json(&root.join("Local State"));
                    let active: Vec<String> = state
                        .as_ref()
                        .and_then(|s| s.pointer("/profile/last_active_profiles"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .filter(|s| valid_directory(s))
                        .map(str::to_owned)
                        .collect();
                    let directories = if active.is_empty() {
                        vec![
                            state
                                .as_ref()
                                .and_then(|s| s.pointer("/profile/last_used"))
                                .and_then(Value::as_str)
                                .filter(|s| valid_directory(s))
                                .unwrap_or("Default")
                                .to_owned(),
                        ]
                    } else {
                        active
                    };
                    profiles.extend(directories.into_iter().map(|directory| BrowserProfile {
                        root: root.clone(),
                        directory,
                    }));
                }
            }
        }
        if firefox
            && profiles.is_empty()
            && let Some(root) = default_root(browser)
            && let Ok(ini) = std::fs::read_to_string(root.join("profiles.ini"))
        {
            for section in ini.split('[').skip(1) {
                let fields: HashMap<&str, &str> = section
                    .lines()
                    .filter_map(|line| line.trim().split_once('='))
                    .collect();
                // Install sections have Default=<path>; profile sections mark
                // their default as Default=1. Do not inspect dormant profiles.
                let selected = fields.get("Default").copied();
                let path = if selected == Some("1") {
                    fields.get("Path").copied()
                } else {
                    selected.filter(|p| *p != "0")
                };
                if let Some(path) = path {
                    let path = PathBuf::from(path);
                    profiles.push(BrowserProfile {
                        root: if path.is_absolute() {
                            path
                        } else {
                            root.join(path)
                        },
                        directory: String::new(),
                    });
                }
            }
        }
        profiles.sort_by_key(BrowserProfile::path);
        profiles.dedup();
        profiles
    }
}

fn valid_directory(directory: &str) -> bool {
    !directory.is_empty()
        && directory != "."
        && directory != ".."
        && !directory.contains(['/', '\\', ':'])
}

fn chromium_coverage(secure: Option<&Value>, preferences: Option<&Value>) -> Coverage {
    let maps: Vec<_> = [secure, preferences]
        .into_iter()
        .flatten()
        .filter_map(|j| j.pointer("/extensions/settings").and_then(Value::as_object))
        .collect();
    // Secure Preferences takes precedence over an older plain entry.
    for settings in &maps {
        if let Some(extension) = CHROMIUM_EXTENSION_IDS
            .iter()
            .find_map(|id| settings.get(*id))
        {
            if extension.get("disable_reasons").is_some_and(|reasons| {
                reasons.as_u64().is_some_and(|n| n != 0)
                    || reasons.as_array().is_some_and(|values| !values.is_empty())
            }) {
                return Coverage::Missing;
            }
            if let Some(state) = extension.get("state").and_then(Value::as_u64) {
                return if state == 1 {
                    Coverage::Enabled
                } else {
                    Coverage::Missing
                };
            }
            // Current Chromium versions use disable_reasons without `state`.
            if let Some(reasons) = extension.get("disable_reasons") {
                return match (reasons.as_u64(), reasons.as_array()) {
                    (Some(0), _) => Coverage::Enabled,
                    (Some(_), _) => Coverage::Missing,
                    (_, Some(values)) if values.is_empty() => Coverage::Enabled,
                    (_, Some(_)) => Coverage::Missing,
                    _ => Coverage::Unknown,
                };
            }
            // Current Chromium omits both fields for an enabled extension.
            // Require a complete installed record so an empty/transient entry
            // does not clear a previously confirmed removal. A live extension
            // check-in is still required by the browser guard.
            if extension.get("state").is_none()
                && extension
                    .pointer("/manifest/manifest_version")
                    .and_then(Value::as_u64)
                    .is_some_and(|version| matches!(version, 2 | 3))
                && extension
                    .pointer("/manifest/version")
                    .and_then(Value::as_str)
                    .is_some_and(|version| !version.is_empty())
                && extension
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| !path.is_empty())
                && extension
                    .get("location")
                    .and_then(Value::as_u64)
                    .is_some_and(|location| location > 0)
            {
                return Coverage::Enabled;
            }
            return Coverage::Unknown;
        }
    }
    if maps.is_empty() {
        Coverage::Unknown
    } else {
        Coverage::Missing
    }
}

fn firefox_coverage(json: &Value) -> Coverage {
    let Some(addons) = json.get("addons").and_then(Value::as_array) else {
        return Coverage::Unknown;
    };
    let Some(addon) = addons
        .iter()
        .find(|a| a.get("id").and_then(Value::as_str) == Some(FIREFOX_EXTENSION_ID))
    else {
        return Coverage::Missing;
    };
    match addon.get("active").and_then(Value::as_bool) {
        Some(true) => Coverage::Enabled,
        Some(false) => Coverage::Missing,
        None => Coverage::Unknown,
    }
}

/// Extract one quoted or unquoted browser flag without passing it to a shell.
fn argument(command: &str, flag: &str) -> Option<String> {
    let arguments = split_arguments(command);
    for (index, value) in arguments.iter().enumerate() {
        if value == flag {
            return arguments.get(index + 1).filter(|s| !s.is_empty()).cloned();
        }
        if let Some(value) = value.strip_prefix(flag).and_then(|s| s.strip_prefix('=')) {
            return (!value.is_empty()).then(|| value.to_owned());
        }
    }
    None
}

// Windows quotes the whole --flag=value argument when its value has spaces.
// Also accept --flag="value", and decode backslashes before closing quotes.
fn split_arguments(command: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut value = String::new();
    let mut quoted = false;
    let mut chars = command.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\\' => {
                let mut count = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    count += 1;
                }
                if chars.peek() == Some(&'"') {
                    value.extend(std::iter::repeat_n('\\', count / 2));
                    chars.next();
                    if count.is_multiple_of(2) {
                        quoted = !quoted;
                    } else {
                        value.push('"');
                    }
                } else {
                    value.extend(std::iter::repeat_n('\\', count));
                }
            }
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !value.is_empty() {
                    arguments.push(std::mem::take(&mut value));
                }
            }
            c => value.push(c),
        }
    }
    if !value.is_empty() {
        arguments.push(value);
    }
    arguments
}

fn default_root(browser: &BrowserType) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let (environment, suffix) = match browser {
            BrowserType::Chrome => ("LOCALAPPDATA", "Google/Chrome/User Data"),
            BrowserType::Edge => ("LOCALAPPDATA", "Microsoft/Edge/User Data"),
            BrowserType::Brave => ("LOCALAPPDATA", "BraveSoftware/Brave-Browser/User Data"),
            BrowserType::Opera => ("APPDATA", "Opera Software/Opera Stable"),
            BrowserType::Firefox => ("APPDATA", "Mozilla/Firefox"),
            _ => return None,
        };
        std::env::var_os(environment).map(|p| PathBuf::from(p).join(suffix))
    }
    #[cfg(target_os = "macos")]
    {
        let suffix = match browser {
            BrowserType::Chrome => "Google/Chrome",
            BrowserType::Edge => "Microsoft Edge",
            BrowserType::Brave => "BraveSoftware/Brave-Browser",
            BrowserType::Opera => "com.operasoftware.Opera",
            BrowserType::Firefox => "Firefox",
            _ => return None,
        };
        std::env::var_os("HOME").map(|p| {
            PathBuf::from(p)
                .join("Library/Application Support")
                .join(suffix)
        })
    }
    #[cfg(target_os = "linux")]
    {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        if *browser == BrowserType::Firefox {
            return Some(home.join(".mozilla/firefox"));
        }
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let suffix = match browser {
            BrowserType::Chrome => "google-chrome",
            BrowserType::Edge => "microsoft-edge",
            BrowserType::Brave => "BraveSoftware/Brave-Browser",
            BrowserType::Opera => "opera",
            _ => return None,
        };
        Some(config.join(suffix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn removal_and_disabling_do_not_wait_for_a_heartbeat() {
        let enabled = json!({"extensions":{"settings":{CHROMIUM_EXTENSION_ID:{"state":1}}}});
        let disabled = json!({"extensions":{"settings":{CHROMIUM_EXTENSION_ID:{"state":0}}}});
        let removed = json!({"extensions":{"settings":{}}});
        assert_eq!(chromium_coverage(Some(&enabled), None), Coverage::Enabled);
        assert_eq!(
            chromium_coverage(Some(&disabled), Some(&enabled)),
            Coverage::Missing
        );
        assert_eq!(chromium_coverage(Some(&removed), None), Coverage::Missing);
        assert_eq!(chromium_coverage(None, None), Coverage::Unknown);
    }

    #[test]
    fn current_disable_reasons_and_firefox_active_state_are_supported() {
        for reasons in [json!(0), json!([])] {
            let prefs = json!({"extensions":{"settings":{CHROMIUM_EXTENSION_ID:{"disable_reasons":reasons}}}});
            assert_eq!(chromium_coverage(Some(&prefs), None), Coverage::Enabled);
        }
        let disabled =
            json!({"extensions":{"settings":{CHROMIUM_EXTENSION_ID:{"disable_reasons":[1]}}}});
        assert_eq!(chromium_coverage(Some(&disabled), None), Coverage::Missing);
        assert_eq!(
            firefox_coverage(&json!({"addons":[{"id":FIREFOX_EXTENSION_ID,"active":false}]})),
            Coverage::Missing
        );
        assert_eq!(
            firefox_coverage(&json!({"addons":[{"id":FIREFOX_EXTENSION_ID,"active":true}]})),
            Coverage::Enabled
        );
    }

    fn modern_install() -> Value {
        json!({"extensions":{"settings":{CHROMIUM_EXTENSION_ID:{
            "manifest":{"manifest_version":3,"version":"0.7.4"},
            "path":"jpnhbpbcmagoonmaleppldmcnaibkbmj/0.7.4_1",
            "location":1,"incognito":true,"active_bit":false
        }}}})
    }

    #[test]
    fn modern_enabled_install_does_not_need_legacy_state_fields() {
        let installed = modern_install();
        assert_eq!(chromium_coverage(Some(&installed), None), Coverage::Enabled);
        for (field, value, expected) in [
            ("state", json!(0), Coverage::Missing),
            ("state", json!("invalid"), Coverage::Unknown),
            ("disable_reasons", json!([1]), Coverage::Missing),
            ("disable_reasons", json!("invalid"), Coverage::Unknown),
        ] {
            let mut changed = installed.clone();
            changed["extensions"]["settings"][CHROMIUM_EXTENSION_ID][field] = value;
            assert_eq!(chromium_coverage(Some(&changed), None), expected);
        }
        assert_eq!(
            chromium_coverage(
                Some(&json!({"extensions":{"settings":{
                    CHROMIUM_EXTENSION_ID:{}
                }}})),
                None
            ),
            Coverage::Unknown
        );
    }

    #[test]
    fn modern_reinstall_clears_missing_latch_and_still_detects_a_second_removal() {
        let temp = tempfile::tempdir().unwrap();
        let profile = BrowserProfile {
            root: temp.path().into(),
            directory: "Default".into(),
        };
        std::fs::create_dir_all(profile.path()).unwrap();
        let prefs = profile.path().join("Secure Preferences");
        std::fs::write(&prefs, json!({"extensions":{"settings":{}}}).to_string()).unwrap();
        let mut monitor = ProfileMonitor::default();
        assert_eq!(monitor.coverage(&profile, false), Coverage::Missing);
        let installed = profile
            .path()
            .join("Extensions")
            .join(CHROMIUM_EXTENSION_ID);
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(&prefs, modern_install().to_string()).unwrap();
        assert_eq!(monitor.coverage(&profile, false), Coverage::Enabled);
        std::fs::write(&prefs, "{").unwrap();
        assert_eq!(monitor.coverage(&profile, false), Coverage::Unknown);
        std::fs::write(&prefs, modern_install().to_string()).unwrap();
        std::fs::remove_dir(&installed).unwrap();
        assert_eq!(monitor.coverage(&profile, false), Coverage::Missing);
        std::fs::create_dir(&installed).unwrap();
        assert_eq!(monitor.coverage(&profile, false), Coverage::Enabled);
    }

    #[test]
    fn installed_files_disappearing_precede_the_preferences_write() {
        let temp = tempfile::tempdir().unwrap();
        let profile = BrowserProfile {
            root: temp.path().into(),
            directory: "Default".into(),
        };
        let installed = profile
            .path()
            .join("Extensions")
            .join(CHROMIUM_EXTENSION_ID);
        std::fs::create_dir_all(&installed).unwrap();
        let prefs = json!({"extensions":{"settings":{CHROMIUM_EXTENSION_ID:{"state":1}}}});
        std::fs::write(profile.path().join("Secure Preferences"), prefs.to_string()).unwrap();
        let mut monitor = ProfileMonitor::default();
        assert_eq!(monitor.coverage(&profile, false), Coverage::Enabled);
        std::fs::remove_dir(&installed).unwrap();
        assert_eq!(monitor.coverage(&profile, false), Coverage::Missing);
        assert_eq!(monitor.coverage(&profile, false), Coverage::Missing);
        std::fs::create_dir(&installed).unwrap();
        std::fs::write(profile.path().join("Secure Preferences"), prefs.to_string()).unwrap();
        assert_eq!(monitor.coverage(&profile, false), Coverage::Enabled);
        std::fs::write(profile.path().join("Secure Preferences"), "{").unwrap();
        assert_eq!(monitor.coverage(&profile, false), Coverage::Unknown);
    }

    #[test]
    fn command_flags_keep_spaces_and_reject_partial_names() {
        assert_eq!(
            argument(
                r#"chrome --user-data-dir="C:\Users\A B\Data" --profile-directory="Profile 2""#,
                "--user-data-dir"
            )
            .as_deref(),
            Some(r"C:\Users\A B\Data")
        );
        assert_eq!(
            argument("chrome --profile-directory=Default", "--profile-directory").as_deref(),
            Some("Default")
        );
        assert_eq!(
            argument(
                "chrome --profile-directory-extra=Default",
                "--profile-directory"
            ),
            None
        );
        assert!(!valid_directory("../Default"));
        assert_eq!(
            argument(
                r#"browser "--user-data-dir=C:\Users\A B\Data" "--profile-directory=Profile 2""#,
                "--user-data-dir"
            )
            .as_deref(),
            Some(r"C:\Users\A B\Data")
        );
        assert_eq!(
            argument(
                r#"browser "--profile-directory=Profile 2""#,
                "--profile-directory"
            )
            .as_deref(),
            Some("Profile 2")
        );
    }

    #[test]
    fn explicit_profile_overrides_dormant_profiles_in_local_state() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("Local State"),
            json!({"profile":{"last_active_profiles":["Dormant"]}}).to_string(),
        )
        .unwrap();
        let command = format!(
            "browser --user-data-dir=\"{}\" --profile-directory=\"Profile 2\"",
            temp.path().display()
        );
        let profiles = ProfileMonitor::default().profiles(&BrowserType::Chrome, &[command]);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].directory, "Profile 2");
        assert_eq!(profiles[0].root, temp.path());
    }

    #[test]
    fn all_active_profiles_are_checked_when_no_explicit_profile_is_given() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("Local State"),
            json!({"profile":{"last_active_profiles":["Default", "Profile 2", "../escape"]}})
                .to_string(),
        )
        .unwrap();
        let command = format!("browser --user-data-dir=\"{}\"", temp.path().display());
        let profiles = ProfileMonitor::default().profiles(&BrowserType::Chrome, &[command]);
        assert_eq!(
            profiles
                .iter()
                .map(|p| p.directory.as_str())
                .collect::<Vec<_>>(),
            ["Default", "Profile 2"]
        );
    }
}
