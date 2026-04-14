use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{EntityId, new_id};

/// What an allowance targets: a domain or an application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value")]
pub enum AllowanceMatch {
    /// A domain (matches the domain itself and any subdomain).
    Domain(String),
    /// An app matched by executable name (e.g., "steam.exe").
    AppExecutable(String),
}

impl AllowanceMatch {
    pub fn display(&self) -> &str {
        match self {
            Self::Domain(d) => d,
            Self::AppExecutable(e) => e,
        }
    }

    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Domain(_) => "domain",
            Self::AppExecutable(_) => "app",
        }
    }

    pub fn value_str(&self) -> &str {
        match self {
            Self::Domain(v) | Self::AppExecutable(v) => v,
        }
    }

    /// Does the given hostname match this allowance? For AppExecutable
    /// this always returns false (not a domain match).
    pub fn matches_hostname(&self, hostname: &str) -> bool {
        let host = hostname.trim().to_ascii_lowercase();
        let host = host.strip_prefix("www.").unwrap_or(&host);
        match self {
            Self::Domain(d) => {
                let d = d.trim().to_ascii_lowercase();
                let d = d.strip_prefix("www.").unwrap_or(&d);
                host == d || host.ends_with(&format!(".{d}"))
            }
            Self::AppExecutable(_) => false,
        }
    }
}

/// A user-configured daily time budget for a domain or app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allowance {
    pub id: EntityId,
    pub target: AllowanceMatch,
    pub daily_limit_secs: u32,
    /// If true, only count time when the tab/app is actively focused.
    /// If false, count as long as it's open.
    pub strict_mode: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

impl Allowance {
    pub fn new(target: AllowanceMatch, daily_limit_secs: u32, strict_mode: bool) -> Self {
        Self {
            id: new_id(),
            target,
            daily_limit_secs,
            strict_mode,
            enabled: true,
            created_at: Utc::now(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.daily_limit_secs < 60 || self.daily_limit_secs > 24 * 3600 {
            return Err("daily limit must be between 1 minute and 24 hours".into());
        }
        if self.target.value_str().trim().is_empty() {
            return Err("allowance target cannot be empty".into());
        }
        Ok(())
    }
}

/// A snapshot of an allowance with today's usage, for the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowanceStatus {
    pub allowance: Allowance,
    pub used_today_secs: u32,
    /// `remaining = max(daily_limit - used, 0)`.
    pub remaining_secs: u32,
    /// True once `used_today_secs >= daily_limit_secs`.
    pub exhausted: bool,
}

impl AllowanceStatus {
    pub fn new(allowance: Allowance, used_today_secs: u32) -> Self {
        let remaining_secs = allowance.daily_limit_secs.saturating_sub(used_today_secs);
        let exhausted = used_today_secs >= allowance.daily_limit_secs;
        Self {
            allowance,
            used_today_secs,
            remaining_secs,
            exhausted,
        }
    }
}

/// Tick payload from the extension / app watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowanceTick {
    /// Hostname of the active tab (e.g., "youtube.com"). None for app ticks.
    pub hostname: Option<String>,
    /// App executable name (e.g., "steam.exe"). None for web ticks.
    pub app_exe: Option<String>,
    /// True if this is the user-focused tab/window.
    pub active: bool,
    /// Tick source identifier (e.g., "chrome-extension", "windows-app-watcher").
    pub source: String,
    /// Seconds elapsed since the last tick (defaults to 5 if missing).
    /// Clamped to [1, 120] server-side.
    #[serde(default)]
    pub increment_secs: Option<u32>,
}
