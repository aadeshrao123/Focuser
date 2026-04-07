use chrono::{DateTime, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for all entities.
pub type EntityId = Uuid;

/// Generate a new unique ID.
pub fn new_id() -> EntityId {
    Uuid::new_v4()
}

/// A named collection of blocking rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockList {
    pub id: EntityId,
    pub name: String,
    pub enabled: bool,
    pub websites: Vec<WebsiteRule>,
    pub applications: Vec<AppRule>,
    pub exceptions: Vec<ExceptionRule>,
    pub lock: Option<Lock>,
    pub protection: Option<Protection>,
    pub schedule: Option<Schedule>,
    pub breaks: Option<BreakConfig>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BlockList {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: new_id(),
            name: name.into(),
            enabled: true,
            websites: Vec::new(),
            applications: Vec::new(),
            exceptions: Vec::new(),
            lock: None,
            protection: None,
            schedule: None,
            breaks: None,
            created_at: now,
            updated_at: now,
        }
    }
}

impl BlockList {
    /// Whether this block list should actually enforce blocking right now.
    /// Combines the user's `enabled` toggle AND the schedule (if set).
    ///
    /// Rules:
    /// - If `enabled` is false → never active (user override wins)
    /// - If no schedule → always active when enabled
    /// - If schedule has no time slots → always active when enabled
    /// - If schedule has time slots → active only when current time matches a slot
    pub fn is_effectively_active(&self) -> bool {
        if !self.enabled {
            return false;
        }
        match &self.schedule {
            None => true,
            Some(schedule) => {
                if schedule.time_slots.is_empty() {
                    true
                } else {
                    schedule.is_active_now()
                }
            }
        }
    }

    pub fn has_active_protection(&self) -> bool {
        self.enabled && self.protection.as_ref().is_some_and(|p| p.is_active())
    }

    pub fn is_modification_protected(&self) -> bool {
        self.protection
            .as_ref()
            .is_some_and(|p| p.is_active() && p.prevent_modification)
    }

    pub fn has_uninstall_protection(&self) -> bool {
        self.enabled
            && self
                .protection
                .as_ref()
                .is_some_and(|p| p.is_active() && p.prevent_uninstall)
    }

    pub fn has_service_protection(&self) -> bool {
        self.enabled
            && self
                .protection
                .as_ref()
                .is_some_and(|p| p.is_active() && p.prevent_service_stop)
    }
}

// ─── Website Blocking ───────────────────────────────────────────────

/// How a website rule matches URLs/domains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebsiteMatchType {
    /// Exact domain match (e.g., "reddit.com" blocks reddit.com and *.reddit.com)
    Domain(String),
    /// Wildcard pattern (e.g., "*.social.*")
    Wildcard(String),
    /// Keyword anywhere in URL (e.g., "game" blocks any URL containing "game")
    Keyword(String),
    /// Exact URL path match (e.g., "reddit.com/r/gaming")
    UrlPath(String),
    /// Block the entire internet (with exceptions only)
    EntireInternet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteRule {
    pub id: EntityId,
    pub match_type: WebsiteMatchType,
    pub enabled: bool,
}

impl WebsiteRule {
    pub fn domain(domain: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            match_type: WebsiteMatchType::Domain(domain.into()),
            enabled: true,
        }
    }

    pub fn keyword(kw: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            match_type: WebsiteMatchType::Keyword(kw.into()),
            enabled: true,
        }
    }

    pub fn wildcard(pattern: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            match_type: WebsiteMatchType::Wildcard(pattern.into()),
            enabled: true,
        }
    }

    pub fn url_path(path: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            match_type: WebsiteMatchType::UrlPath(path.into()),
            enabled: true,
        }
    }

    pub fn entire_internet() -> Self {
        Self {
            id: new_id(),
            match_type: WebsiteMatchType::EntireInternet,
            enabled: true,
        }
    }
}

// ─── Application Blocking ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppMatchType {
    /// Match by executable name (e.g., "steam.exe")
    ExecutableName(String),
    /// Match by full path (e.g., "C:\\Program Files\\Steam\\steam.exe")
    ExecutablePath(String),
    /// Match by window title substring
    WindowTitle(String),
    /// macOS bundle ID (e.g., "com.apple.Safari")
    BundleId(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRule {
    pub id: EntityId,
    pub match_type: AppMatchType,
    pub enabled: bool,
}

impl AppRule {
    pub fn executable(name: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            match_type: AppMatchType::ExecutableName(name.into()),
            enabled: true,
        }
    }

    pub fn path(path: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            match_type: AppMatchType::ExecutablePath(path.into()),
            enabled: true,
        }
    }

    pub fn window_title(title: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            match_type: AppMatchType::WindowTitle(title.into()),
            enabled: true,
        }
    }
}

// ─── Exceptions ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExceptionType {
    /// Allow a specific domain even when other rules would block it
    Domain(String),
    /// Allow a wildcard pattern
    Wildcard(String),
    /// Allow local file:// URLs
    LocalFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionRule {
    pub id: EntityId,
    pub exception_type: ExceptionType,
    pub enabled: bool,
}

impl ExceptionRule {
    pub fn domain(domain: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            exception_type: ExceptionType::Domain(domain.into()),
            enabled: true,
        }
    }
}

// ─── Protection ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Protection {
    pub prevent_uninstall: bool,
    pub prevent_service_stop: bool,
    pub prevent_modification: bool,
    pub started_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl Protection {
    pub fn for_duration(minutes: u32) -> Self {
        let now = Utc::now();
        Self {
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            started_at: now,
            expires_at: now + chrono::Duration::minutes(minutes as i64),
        }
    }

    pub fn is_active(&self) -> bool {
        Utc::now() < self.expires_at
    }

    pub fn remaining_seconds(&self) -> u64 {
        let remaining = self.expires_at - Utc::now();
        remaining.num_seconds().max(0) as u64
    }
}

// ─── Locks ──────────────────────────────────────────────────────────

/// How a block is enforced — determines what it takes to disable it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Lock {
    /// Block runs for a fixed duration, cannot be cancelled.
    Timer {
        duration_minutes: u32,
        started_at: Option<DateTime<Utc>>,
    },
    /// Must type a long random string to unlock.
    RandomText { length: u32 },
    /// Locked until a specific time.
    Until { unlock_at: DateTime<Utc> },
    /// Requires system restart to disable (block re-enables on boot).
    Restart,
    /// Password-protected (hashed).
    Password { hash: String },
    /// Follows the attached schedule — active during scheduled times.
    Scheduled,
}

// ─── Schedules ──────────────────────────────────────────────────────

/// Weekly recurring schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: EntityId,
    pub name: String,
    pub time_slots: Vec<TimeSlot>,
    pub enabled: bool,
}

/// A time range on a specific day of the week.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSlot {
    pub day: Weekday,
    pub start: NaiveTime,
    pub end: NaiveTime,
}

// ─── Breaks ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BreakConfig {
    /// Pomodoro-style: work for X minutes, break for Y minutes.
    Pomodoro {
        work_minutes: u32,
        break_minutes: u32,
        long_break_minutes: u32,
        sessions_before_long_break: u32,
    },
    /// Allowance: X minutes of access per day/hour, tracked by activity.
    Allowance {
        allowed_minutes: u32,
        period: AllowancePeriod,
        /// If true, only counts time when the blocked site/app is in focus.
        track_active_only: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AllowancePeriod {
    PerHour,
    PerDay,
}

// ─── Statistics ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStat {
    pub domain_or_app: String,
    pub duration_seconds: u64,
    pub blocked_attempts: u64,
    pub date: chrono::NaiveDate,
}

/// A single block event with precise timestamp (for timeline charts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedEvent {
    pub domain_or_app: String,
    pub timestamp: String,
}
