use chrono::{DateTime, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::error::FocuserError;

/// Unique identifier for all entities.
pub type EntityId = Uuid;

/// Generate a new unique ID.
pub fn new_id() -> EntityId {
    Uuid::new_v4()
}

/// A named collection of blocking rules.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum ExceptionType {
    /// Allow a specific domain even when other rules would block it
    Domain(String),
    /// Allow a wildcard pattern
    Wildcard(String),
    /// Allow local file:// URLs
    LocalFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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

    pub fn wildcard(pattern: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            exception_type: ExceptionType::Wildcard(pattern.into()),
            enabled: true,
        }
    }
}

// ─── Protection ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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

/// How a protection window can be ended early — Cold Turkey calls this a
/// block's "lock". Meaningless on its own; it only matters while
/// [`BlockList::protection`] is active, and it can only be set or cleared
/// through the `EnableProtection` / `UnlockProtection` commands, never
/// through a wholesale [`BlockList`] update.
///
/// With no lock, an active protection window simply cannot be ended early —
/// the only way out is to wait for `expires_at`. Adding a lock is a
/// deliberate trade: an escape hatch exists, but only through friction
/// (retyping a random string) or a secret (a password).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum Lock {
    /// Must enter this password to unlock early. Stored as an Argon2 hash —
    /// never the plaintext.
    Password { hash: String },
    /// Must retype a freshly generated random string to unlock early.
    ///
    /// The string currently on offer is *not* stored here — it lives in the
    /// database's `unlock_challenges` table (see `focuser_core::Database`)
    /// keyed by block list, separate from this JSON blob. That keeps it out
    /// of `ListBlockLists`/`ExportConfiguration`, and a wrong answer simply
    /// requires a fresh one rather than allowing retries against the same
    /// string.
    RandomText { length: u32 },
}

impl Lock {
    /// A challenge shorter than this is typed too easily to add real
    /// friction; longer than this is just a typo generator.
    pub const MIN_RANDOM_TEXT_LEN: u32 = 6;
    pub const MAX_RANDOM_TEXT_LEN: u32 = 64;

    /// Characters that stay unambiguous in a UI font — no `0`/`O`, `1`/`l`/`I`.
    /// A challenge that is impossible to transcribe correctly defeats the
    /// point, which is friction, not a puzzle.
    const CHALLENGE_ALPHABET: &'static [u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

    /// Hash `plain` with Argon2 and build a password lock. The plaintext is
    /// never stored or returned.
    pub fn password(plain: &str) -> Result<Self, FocuserError> {
        use argon2::Argon2;
        use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};

        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(plain.as_bytes(), &salt)
            .map_err(|e| FocuserError::PasswordHash(e.to_string()))?
            .to_string();
        Ok(Self::Password { hash })
    }

    /// Check `attempt` against a password lock. Always `false` for a
    /// random-text lock — that one is verified against the issued challenge
    /// instead, not through this method.
    pub fn verify_password(&self, attempt: &str) -> bool {
        use argon2::Argon2;
        use argon2::password_hash::{PasswordHash, PasswordVerifier};

        let Self::Password { hash } = self else {
            return false;
        };
        let Ok(parsed) = PasswordHash::new(hash) else {
            return false;
        };
        Argon2::default()
            .verify_password(attempt.as_bytes(), &parsed)
            .is_ok()
    }

    /// Generate a fresh challenge string for a random-text lock. `None` for
    /// a password lock, which has nothing to generate.
    pub fn generate_challenge(&self) -> Option<String> {
        match self {
            Self::RandomText { length } => Some(Self::random_text_of_length(*length)),
            Self::Password { .. } => None,
        }
    }

    /// Build a challenge string of exactly `length` characters.
    ///
    /// Split out from [`Self::generate_challenge`] so a caller that already
    /// knows it is holding a `RandomText` lock (e.g. having just matched on
    /// it) can get a `String` directly, with no `Option` to unwrap for a
    /// case the match already ruled out.
    pub fn random_text_of_length(length: u32) -> String {
        use argon2::password_hash::rand_core::{OsRng, RngCore};

        let mut rng = OsRng;
        (0..length)
            .map(|_| {
                let idx = (rng.next_u32() as usize) % Self::CHALLENGE_ALPHABET.len();
                Self::CHALLENGE_ALPHABET[idx] as char
            })
            .collect()
    }
}

// ─── Schedules ──────────────────────────────────────────────────────

/// Weekly recurring schedule.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Schedule {
    pub id: EntityId,
    pub name: String,
    pub time_slots: Vec<TimeSlot>,
    pub enabled: bool,
}

/// A time range on a specific day of the week.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TimeSlot {
    pub day: Weekday,
    pub start: NaiveTime,
    pub end: NaiveTime,
}

// ─── Breaks ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum AllowancePeriod {
    PerHour,
    PerDay,
}

// ─── Statistics ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct UsageStat {
    pub domain_or_app: String,
    // Specta refuses to export u64 by default, since values above 2^53 lose
    // precision as JavaScript numbers. Both fields are counts that cannot
    // realistically approach that — 2^53 seconds is ~285 million years — so
    // exporting them as `number` is safe and avoids pushing BigInt handling
    // through the whole frontend.
    #[specta(type = specta_typescript::Number)]
    pub duration_seconds: u64,
    #[specta(type = specta_typescript::Number)]
    pub blocked_attempts: u64,
    pub date: chrono::NaiveDate,
}

/// A single block event with precise timestamp (for timeline charts).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct BlockedEvent {
    pub domain_or_app: String,
    pub timestamp: String,
}

#[cfg(test)]
mod lock_tests {
    use super::Lock;

    #[test]
    fn password_hash_verifies_only_the_right_plaintext() {
        let lock = Lock::password("correct-horse").unwrap();
        assert!(lock.verify_password("correct-horse"));
        assert!(!lock.verify_password("wrong"));
    }

    #[test]
    fn stored_hash_never_contains_the_plaintext() {
        let Lock::Password { hash } = Lock::password("super-secret").unwrap() else {
            panic!("expected a password lock");
        };
        assert!(!hash.contains("super-secret"));
    }

    #[test]
    fn random_text_challenges_have_the_requested_length_and_alphabet() {
        let lock = Lock::RandomText { length: 20 };
        let challenge = lock.generate_challenge().unwrap();
        assert_eq!(challenge.chars().count(), 20);
        assert!(
            challenge
                .chars()
                .all(|c| Lock::CHALLENGE_ALPHABET.contains(&(c as u8)))
        );
    }

    #[test]
    fn successive_challenges_are_not_the_same_string() {
        let lock = Lock::RandomText { length: 24 };
        let a = lock.generate_challenge().unwrap();
        let b = lock.generate_challenge().unwrap();
        // Astronomically unlikely to collide at this length; a collision here
        // means the RNG is not actually being drawn from per call.
        assert_ne!(a, b);
    }

    #[test]
    fn a_password_lock_has_no_challenge_and_a_random_text_lock_has_no_password() {
        let password_lock = Lock::password("x").unwrap();
        assert!(password_lock.generate_challenge().is_none());

        let text_lock = Lock::RandomText { length: 10 };
        assert!(!text_lock.verify_password("anything"));
    }
}
