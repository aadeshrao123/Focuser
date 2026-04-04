//! Browser detection and status types.
//!
//! Provides a constant table of known browsers and their executable names
//! per platform, plus a function to match a running process name to a browser.

use serde::{Deserialize, Serialize};

use crate::extension::BrowserType;

/// Information about a known browser.
pub struct BrowserInfo {
    /// Which browser this is.
    pub browser_type: BrowserType,
    /// Human-readable name (e.g., "Google Chrome").
    pub display_name: &'static str,
    /// Known executable/process names on the current platform.
    pub exe_names: &'static [&'static str],
}

/// Known browsers and their process names per platform.
#[cfg(target_os = "windows")]
pub static KNOWN_BROWSERS: &[BrowserInfo] = &[
    BrowserInfo {
        browser_type: BrowserType::Chrome,
        display_name: "Google Chrome",
        exe_names: &["chrome.exe"],
    },
    BrowserInfo {
        browser_type: BrowserType::Firefox,
        display_name: "Mozilla Firefox",
        exe_names: &["firefox.exe"],
    },
    BrowserInfo {
        browser_type: BrowserType::Edge,
        display_name: "Microsoft Edge",
        exe_names: &["msedge.exe"],
    },
    BrowserInfo {
        browser_type: BrowserType::Brave,
        display_name: "Brave Browser",
        exe_names: &["brave.exe"],
    },
    BrowserInfo {
        browser_type: BrowserType::Opera,
        display_name: "Opera",
        exe_names: &["opera.exe"],
    },
];

#[cfg(target_os = "macos")]
pub static KNOWN_BROWSERS: &[BrowserInfo] = &[
    BrowserInfo {
        browser_type: BrowserType::Chrome,
        display_name: "Google Chrome",
        exe_names: &["Google Chrome"],
    },
    BrowserInfo {
        browser_type: BrowserType::Firefox,
        display_name: "Mozilla Firefox",
        exe_names: &["firefox"],
    },
    BrowserInfo {
        browser_type: BrowserType::Edge,
        display_name: "Microsoft Edge",
        exe_names: &["Microsoft Edge"],
    },
    BrowserInfo {
        browser_type: BrowserType::Brave,
        display_name: "Brave Browser",
        exe_names: &["Brave Browser"],
    },
    BrowserInfo {
        browser_type: BrowserType::Opera,
        display_name: "Opera",
        exe_names: &["Opera"],
    },
];

#[cfg(target_os = "linux")]
pub static KNOWN_BROWSERS: &[BrowserInfo] = &[
    BrowserInfo {
        browser_type: BrowserType::Chrome,
        display_name: "Google Chrome",
        exe_names: &[
            "chrome",
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
        ],
    },
    BrowserInfo {
        browser_type: BrowserType::Firefox,
        display_name: "Mozilla Firefox",
        exe_names: &["firefox", "firefox-esr"],
    },
    BrowserInfo {
        browser_type: BrowserType::Edge,
        display_name: "Microsoft Edge",
        exe_names: &["microsoft-edge", "microsoft-edge-stable"],
    },
    BrowserInfo {
        browser_type: BrowserType::Brave,
        display_name: "Brave Browser",
        exe_names: &["brave-browser", "brave"],
    },
    BrowserInfo {
        browser_type: BrowserType::Opera,
        display_name: "Opera",
        exe_names: &["opera"],
    },
];

/// Match a running process name against known browsers.
///
/// Returns the `BrowserInfo` if the process name matches any known browser
/// executable on the current platform. Matching is case-insensitive.
pub fn identify_browser(process_name: &str) -> Option<&'static BrowserInfo> {
    let lower = process_name.to_lowercase();
    KNOWN_BROWSERS
        .iter()
        .find(|b| b.exe_names.iter().any(|name| name.to_lowercase() == lower))
}

/// Status of a detected browser — used for IPC responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserStatusInfo {
    /// Which browser.
    pub browser_type: BrowserType,
    /// Human-readable name.
    pub display_name: String,
    /// Whether at least one process for this browser is running.
    pub is_running: bool,
    /// Whether the Focuser extension is connected for this browser.
    pub extension_connected: bool,
    /// Seconds remaining in grace period before enforcement, if applicable.
    pub grace_period_remaining_secs: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_browser_chrome() {
        #[cfg(target_os = "windows")]
        {
            let info = identify_browser("chrome.exe").unwrap();
            assert_eq!(info.browser_type, BrowserType::Chrome);
            assert_eq!(info.display_name, "Google Chrome");
        }
        #[cfg(target_os = "linux")]
        {
            let info = identify_browser("google-chrome").unwrap();
            assert_eq!(info.browser_type, BrowserType::Chrome);
        }
    }

    #[test]
    fn test_identify_browser_case_insensitive() {
        #[cfg(target_os = "windows")]
        {
            let info = identify_browser("Chrome.EXE").unwrap();
            assert_eq!(info.browser_type, BrowserType::Chrome);
        }
    }

    #[test]
    fn test_identify_browser_unknown() {
        assert!(identify_browser("notepad.exe").is_none());
        assert!(identify_browser("some-random-app").is_none());
    }

    #[test]
    fn test_all_known_browsers_detectable() {
        for browser in KNOWN_BROWSERS {
            for exe in browser.exe_names {
                let result = identify_browser(exe);
                assert!(result.is_some(), "Failed to identify {exe}");
                assert_eq!(result.unwrap().browser_type, browser.browser_type);
            }
        }
    }
}
