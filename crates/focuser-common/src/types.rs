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
    /// JSON defaults migrate existing lists to unprotected schedules.
    #[serde(default)]
    pub scheduled_protection: Option<ScheduledProtection>,
    /// Trusted occurrence bypass; never accepted from wholesale list updates.
    #[serde(default)]
    pub schedule_unlocked_until: Option<DateTime<Utc>>,
    pub breaks: Option<BreakConfig>,
    /// Optional shared budget per merged weekly schedule occurrence.
    #[serde(default)]
    pub shared_allowance: Option<crate::allowance::SharedAllowanceConfig>,
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
            scheduled_protection: None,
            schedule_unlocked_until: None,
            breaks: None,
            shared_allowance: None,
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

    /// The scheduled occurrence running at `now`, as (start, end). A schedule
    /// with no gap has none: it never ends, so a lock or a budget tied to
    /// "this occurrence" would never end or refill either.
    pub fn occurrence_at<T: chrono::TimeZone>(
        &self,
        now: DateTime<T>,
    ) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        self.schedule
            .as_ref()?
            .active_period_at(now)
            .filter(|(_, end)| *end != DateTime::<Utc>::MAX_UTC)
    }

    pub fn scheduled_protection_at<T: chrono::TimeZone>(
        &self,
        now: DateTime<T>,
    ) -> Option<Protection> {
        self.scheduled_protection.as_ref()?;
        if !self.enabled {
            return None;
        }
        let (started_at, expires_at) = self.occurrence_at(now.clone())?;
        if self
            .schedule_unlocked_until
            .is_some_and(|until| now.with_timezone(&Utc) < until)
        {
            return None;
        }
        Some(Protection {
            started_at,
            expires_at: Some(expires_at),
            prevent_modification: true,
            prevent_service_stop: true,
            prevent_uninstall: true,
        })
    }

    pub fn scheduled_lock_state(&self) -> ScheduledLockState {
        self.scheduled_lock_state_at(chrono::Local::now())
    }

    pub fn scheduled_lock_state_at<T: chrono::TimeZone>(
        &self,
        now: DateTime<T>,
    ) -> ScheduledLockState {
        if self.scheduled_protection.is_none() {
            return ScheduledLockState::Off;
        }
        if self.occurrence_at(now.clone()).is_none() {
            return ScheduledLockState::Inactive;
        }
        // A separate manual commitment can still prohibit editing.
        if self
            .manual_protection_at(now.with_timezone(&Utc))
            .is_some_and(|p| p.prevent_modification)
        {
            return ScheduledLockState::Locked;
        }
        if self
            .schedule_unlocked_until
            .is_some_and(|end| now.with_timezone(&Utc) < end)
        {
            return ScheduledLockState::UnlockedForEditing;
        }
        if self.enabled {
            ScheduledLockState::Locked
        } else {
            ScheduledLockState::Inactive
        }
    }

    /// The manual lock, while it is on.
    ///
    /// One with no end counts only when the list also has a way to unlock it.
    /// A single command checks that when a lock is made; an imported file or
    /// an edited database does not go through it, and a lock nobody can end is
    /// worse than no lock.
    pub fn manual_protection(&self) -> Option<&Protection> {
        self.manual_protection_at(Utc::now())
    }

    pub fn manual_protection_at(&self, now: DateTime<Utc>) -> Option<&Protection> {
        self.protection
            .as_ref()
            .filter(|p| p.is_active_at(now) && (p.expires_at.is_some() || self.lock.is_some()))
    }

    pub fn effective_protection(&self) -> Option<Protection> {
        self.effective_protection_at(chrono::Local::now())
    }

    pub fn effective_protection_at<T: chrono::TimeZone>(
        &self,
        now: DateTime<T>,
    ) -> Option<Protection> {
        let manual = self.manual_protection_at(now.with_timezone(&Utc)).cloned();
        let scheduled = self.scheduled_protection_at(now);
        match (manual, scheduled) {
            (Some(mut p), Some(s)) => {
                p.prevent_modification |= s.prevent_modification;
                p.prevent_service_stop |= s.prevent_service_stop;
                p.prevent_uninstall |= s.prevent_uninstall;
                // `None` (until unlocked) outlasts any end time.
                p.expires_at = p.expires_at.zip(s.expires_at).map(|(a, b)| a.max(b));
                Some(p)
            }
            (p, s) => p.or(s),
        }
    }

    /// Manual commitments take priority when both kinds of protection overlap.
    pub fn effective_lock(&self) -> Option<&Lock> {
        if self.manual_protection().is_some() {
            self.lock.as_ref()
        } else {
            self.scheduled_protection
                .as_ref()
                .and_then(|p| p.lock.as_ref())
        }
    }

    pub fn has_active_protection(&self) -> bool {
        self.enabled && self.effective_protection().is_some()
    }

    pub fn is_modification_protected(&self) -> bool {
        self.effective_protection()
            .is_some_and(|p| p.prevent_modification)
    }

    pub fn has_uninstall_protection(&self) -> bool {
        // A temporary disabled/paused blocking state cannot release a manual
        // uninstall commitment. Scheduled protection keeps its occurrence rules.
        self.effective_protection()
            .is_some_and(|p| p.prevent_uninstall)
    }

    pub fn has_service_protection(&self) -> bool {
        self.enabled
            && self
                .effective_protection()
                .is_some_and(|p| p.prevent_service_stop)
    }

    /// Called after edits: becoming inactive ends the occurrence bypass. Disabling
    /// the list alone does not end it, so re-enabling permits further edits.
    pub fn reconcile_schedule_bypass(&mut self) {
        self.reconcile_schedule_bypass_at(chrono::Local::now());
    }

    pub fn reconcile_schedule_bypass_at<T: chrono::TimeZone>(&mut self, now: DateTime<T>) {
        self.schedule_unlocked_until = self.schedule_unlocked_until.and_then(|until| {
            if until <= now.with_timezone(&Utc) {
                return None;
            }
            // An edit that keeps the schedule active continues this occurrence,
            // including an extension. An inactive edit ends the bypass immediately.
            self.occurrence_at(now).map(|(_, end)| end)
        });
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

impl WebsiteMatchType {
    /// Collapse a pattern that only *looks* like a wildcard into what it
    /// actually is.
    ///
    /// `*word*` and `Keyword("word")` match identically — both are a plain
    /// substring test — so a `Wildcard` of exactly that shape is a keyword
    /// someone typed with asterisks around it, not a real glob. And `Domain`
    /// does exact hostname matching with no glob support at all, so a
    /// `Domain` value containing a `*` (typed into the wrong field) never
    /// matched anything — it belongs in `Wildcard`, or in `Keyword` if it's
    /// the `*word*` shape, to actually be evaluated as a pattern.
    ///
    /// Skipped when fewer than 3 literal characters remain once the stars
    /// are stripped: too little to guess intent from, and turning it into a
    /// live pattern risks matching far more than was meant (`*r` would
    /// become "any domain ending in r"). Left alone, it stays exactly as
    /// inert as it already was.
    pub fn simplify(&mut self) {
        let was_domain = matches!(self, Self::Domain(_));
        let pattern = match self {
            Self::Wildcard(p) => p.clone(),
            Self::Domain(d) if d.contains('*') => d.clone(),
            _ => return,
        };

        if pattern.chars().filter(|&c| c != '*').count() < 3 {
            return;
        }

        if let Some(inner) = pattern.strip_prefix('*').and_then(|s| s.strip_suffix('*'))
            && !inner.is_empty()
            && !inner.contains(['*', '?'])
        {
            *self = Self::Keyword(inner.to_string());
        } else if was_domain {
            *self = Self::Wildcard(pattern);
        }
    }
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
    /// Allow one page, and what is under it, on a site that stays blocked:
    /// `reddit.com/r/programming`. Only the extension can see a path.
    UrlPath(String),
    /// Allow a wildcard pattern
    Wildcard(String),
    /// Allow local file:// URLs
    LocalFiles,
}

impl ExceptionType {
    /// The host and page this allows, when it names a page rather than a
    /// whole site. A `Domain` typed with a path counts, as lists from before
    /// 0.8.1 hold those.
    pub fn page(&self) -> Option<(String, String)> {
        match self {
            Self::Domain(v) | Self::UrlPath(v) => crate::host::host_and_page(v),
            Self::Wildcard(_) | Self::LocalFiles => None,
        }
    }

    /// The form it is stored in: a bare host, or host plus page, whichever
    /// kind was picked. `None` when there is nothing to allow.
    pub fn normalized(self) -> Option<Self> {
        let (Self::Domain(typed) | Self::UrlPath(typed)) = &self else {
            return Some(self);
        };
        if let Some((host, page)) = self.page() {
            return Some(Self::UrlPath(format!("{host}{page}")));
        }
        let host = crate::host::canonical_host(typed);
        (!host.is_empty()).then_some(Self::Domain(host))
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledLockState {
    Off,
    Inactive,
    Locked,
    UnlockedForEditing,
}

/// Opt-in recurring Focus Lock; reuses the existing early-unlock methods.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScheduledProtection {
    pub lock: Option<Lock>,
}

// ─── Protection ────────────────────────────────────────────────────

#[derive(Debug, Clone, Type)]
pub struct Protection {
    pub prevent_uninstall: bool,
    pub prevent_service_stop: bool,
    pub prevent_modification: bool,
    pub started_at: DateTime<Utc>,
    /// `None` means until unlocked: no timer, the list's lock is the only way
    /// out. It counts only together with a lock, see
    /// [`BlockList::manual_protection`].
    ///
    /// Written down as a date all the same, see [`StoredProtection`].
    #[specta(type = DateTime<Utc>)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// A protection as it is stored and sent: the end is always a date.
///
/// Every release up to 0.9.1 declares `expires_at` as a plain date and skips
/// a list it cannot read. A `null` there would make the list, and its lock,
/// vanish for an older version, so "no end" is written as the last second of
/// the year 9999. The (de)serialising is done by hand rather than with
/// `#[serde(with)]` because that attribute makes the TypeScript export split
/// every type that holds a protection in two.
#[derive(Serialize, Deserialize)]
struct StoredProtection {
    prevent_uninstall: bool,
    prevent_service_stop: bool,
    prevent_modification: bool,
    started_at: DateTime<Utc>,
    /// Optional only to read the `null` that builds of the change which added
    /// this wrote for a while.
    expires_at: Option<DateTime<Utc>>,
}

const NO_END_YEAR: i32 = 9999;

impl Serialize for Protection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use chrono::TimeZone;
        let no_end = Utc
            .with_ymd_and_hms(NO_END_YEAR, 12, 31, 23, 59, 59)
            .single();
        StoredProtection {
            prevent_uninstall: self.prevent_uninstall,
            prevent_service_stop: self.prevent_service_stop,
            prevent_modification: self.prevent_modification,
            started_at: self.started_at,
            expires_at: self.expires_at.or(no_end),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Protection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use chrono::Datelike;
        let stored = StoredProtection::deserialize(deserializer)?;
        Ok(Self {
            prevent_uninstall: stored.prevent_uninstall,
            prevent_service_stop: stored.prevent_service_stop,
            prevent_modification: stored.prevent_modification,
            started_at: stored.started_at,
            expires_at: stored.expires_at.filter(|end| end.year() < NO_END_YEAR),
        })
    }
}

impl Protection {
    pub fn for_duration(minutes: u32) -> Self {
        let now = Utc::now();
        Self {
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            started_at: now,
            expires_at: Some(now + chrono::Duration::minutes(minutes as i64)),
        }
    }

    pub fn until_unlocked() -> Self {
        Self {
            expires_at: None,
            ..Self::for_duration(0)
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_active_at(Utc::now())
    }

    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_none_or(|end| now < end)
    }

    /// `None` while it runs until unlocked.
    pub fn remaining_seconds(&self) -> Option<u64> {
        self.expires_at
            .map(|end| (end - Utc::now()).num_seconds().max(0) as u64)
    }
}

// ─── Locks ──────────────────────────────────────────────────────────

/// How a protection window can be ended early — Cold Turkey calls this a
/// block's "lock". Meaningless on its own; it only matters while
/// manual or scheduled protection is active, and it can only be configured
/// through protection commands, never
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
    /// friction. The ceiling is high on purpose: for a lock with no timer,
    /// a long text is the whole point.
    pub const MIN_RANDOM_TEXT_LEN: u32 = 6;
    pub const MAX_RANDOM_TEXT_LEN: u32 = 5000;

    /// Characters that stay unambiguous in a UI font — no `0`/`O`, `1`/`l`/`I`.
    /// A challenge that is impossible to transcribe correctly defeats the
    /// point, which is friction, not a puzzle.
    const CHALLENGE_ALPHABET: &'static [u8] =
        b"abcdefghjkmnpqrstuvwxyzABCDEFGHJKMNPQRSTUVWXYZ23456789";

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

    /// How long a challenge for `length` is. Clamped because an imported
    /// file can carry any number here.
    pub fn challenge_len(length: u32) -> usize {
        length.clamp(Self::MIN_RANDOM_TEXT_LEN, Self::MAX_RANDOM_TEXT_LEN) as usize
    }

    /// A fresh challenge string.
    pub fn random_text_of_length(length: u32) -> String {
        use argon2::password_hash::rand_core::{OsRng, RngCore};

        let mut rng = OsRng;
        (0..Self::challenge_len(length))
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
        let challenge = Lock::random_text_of_length(20);
        assert_eq!(challenge.chars().count(), 20);
        assert!(
            challenge
                .chars()
                .all(|c| Lock::CHALLENGE_ALPHABET.contains(&(c as u8)))
        );
    }

    #[test]
    fn a_length_from_a_crafted_file_is_clamped() {
        assert_eq!(
            Lock::random_text_of_length(u32::MAX).len(),
            Lock::MAX_RANDOM_TEXT_LEN as usize
        );
        assert_eq!(
            Lock::random_text_of_length(0).len(),
            Lock::MIN_RANDOM_TEXT_LEN as usize
        );
    }

    #[test]
    fn successive_challenges_are_not_the_same_string() {
        // Astronomically unlikely to collide at this length; a collision here
        // means the RNG is not actually being drawn from per call.
        assert_ne!(
            Lock::random_text_of_length(24),
            Lock::random_text_of_length(24)
        );
    }

    #[test]
    fn a_random_text_lock_has_no_password() {
        let text_lock = Lock::RandomText { length: 10 };
        assert!(!text_lock.verify_password("anything"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_word_star_wildcard_simplifies_to_a_keyword() {
        let mut m = WebsiteMatchType::Wildcard("*casino*".into());
        m.simplify();
        assert_eq!(m, WebsiteMatchType::Keyword("casino".into()));
    }

    #[test]
    fn a_subdomain_wildcard_keeps_its_real_glob_meaning() {
        for pattern in ["*.reddit.com", "*free*games*", "*a*b*", "**", "*", "*?*"] {
            let mut m = WebsiteMatchType::Wildcard(pattern.into());
            m.simplify();
            assert_eq!(
                m,
                WebsiteMatchType::Wildcard(pattern.into()),
                "{pattern:?} should not have been reclassified"
            );
        }
    }

    #[test]
    fn simplify_does_nothing_to_a_non_wildcard_match_type() {
        let mut m = WebsiteMatchType::Domain("reddit.com".into());
        m.simplify();
        assert_eq!(m, WebsiteMatchType::Domain("reddit.com".into()));
    }

    #[test]
    fn a_domain_typed_with_a_star_word_star_becomes_a_keyword() {
        // Domain does exact hostname matching only, so this never matched
        // anything — it was typed into the wrong field.
        let mut m = WebsiteMatchType::Domain("*casino*".into());
        m.simplify();
        assert_eq!(m, WebsiteMatchType::Keyword("casino".into()));
    }

    #[test]
    fn a_domain_typed_with_a_real_glob_moves_to_wildcard_unchanged() {
        for pattern in ["casino*", "*free*games*", "*.example", "search.example.*"] {
            let mut m = WebsiteMatchType::Domain(pattern.into());
            m.simplify();
            assert_eq!(m, WebsiteMatchType::Wildcard(pattern.into()), "{pattern:?}");
        }
    }

    #[test]
    fn too_little_is_left_after_stripping_stars_to_safely_promote() {
        // "*r" would become "any domain ending in r" as a live Wildcard —
        // worse than the inert Domain rule it started as. Leave it be.
        for match_type in [
            WebsiteMatchType::Domain("*r".into()),
            WebsiteMatchType::Wildcard("*ai*".into()),
        ] {
            let mut m = match_type.clone();
            m.simplify();
            assert_eq!(m, match_type);
        }
    }
}

#[cfg(test)]
mod protection_tests {
    use super::{BlockList, Lock, Protection};
    use chrono::{DateTime, Utc};

    /// The field as every release up to 0.9.1 declares it: a date, always.
    #[derive(serde::Deserialize)]
    struct ProtectionBefore {
        expires_at: DateTime<Utc>,
    }

    /// An older version reads a whole list or skips it. If it cannot read the
    /// lock, the list is gone for it, and the lock with it: installing an old
    /// build would be the way out.
    #[test]
    fn a_lock_with_no_end_is_still_a_lock_to_an_older_version() {
        let stored = serde_json::to_string(&Protection::until_unlocked()).unwrap();

        let old: ProtectionBefore =
            serde_json::from_str(&stored).expect("an older version has to be able to read it");

        assert!(old.expires_at > Utc::now() + chrono::Duration::days(365 * 1000));
    }

    #[test]
    fn a_lock_with_no_end_comes_back_from_storage_as_one() {
        let stored = serde_json::to_string(&Protection::until_unlocked()).unwrap();
        let back: Protection = serde_json::from_str(&stored).unwrap();
        assert_eq!(back.expires_at, None);
        assert!(back.is_active());
    }

    #[test]
    fn a_timed_lock_is_stored_as_the_date_it_ends() {
        let timed = Protection::for_duration(60);
        let json = serde_json::to_value(&timed).unwrap();
        assert_eq!(
            json["expires_at"],
            serde_json::to_value(timed.expires_at.unwrap()).unwrap()
        );
    }

    /// Builds of the pull request wrote `null`, and people ran those.
    #[test]
    fn an_end_stored_as_null_reads_as_no_end() {
        let stored = r#"{"prevent_uninstall":true,"prevent_service_stop":true,
            "prevent_modification":true,"started_at":"2026-10-01T08:00:00Z",
            "expires_at":null}"#;
        let p: Protection = serde_json::from_str(stored).unwrap();
        assert_eq!(p.expires_at, None);
    }

    /// Only one command checks that a lock with no end comes with a way to
    /// unlock it. An imported file or an edited database does not go through
    /// that command, and the result would be a lock nobody can end.
    #[test]
    fn a_lock_with_no_end_and_no_way_to_unlock_it_does_not_hold() {
        let mut list = BlockList::new("Stuck");
        list.protection = Some(Protection::until_unlocked());

        assert!(list.effective_protection().is_none());
        assert!(!list.is_modification_protected());
        assert!(!list.has_service_protection());
        assert!(!list.has_uninstall_protection());

        list.lock = Some(Lock::RandomText { length: 12 });
        assert!(list.is_modification_protected());
        assert!(list.has_service_protection());
    }

    #[test]
    fn a_stored_protection_with_an_end_time_still_loads() {
        let stored = r#"{"prevent_uninstall":true,"prevent_service_stop":true,
            "prevent_modification":true,"started_at":"2026-10-01T08:00:00Z",
            "expires_at":"2026-10-01T09:00:00Z"}"#;
        let p: Protection = serde_json::from_str(stored).unwrap();
        assert!(p.expires_at.is_some());
        assert!(!p.is_active());
    }

    #[test]
    fn a_protection_with_no_end_stays_active() {
        let p = Protection::until_unlocked();
        assert!(p.is_active());
        assert_eq!(p.remaining_seconds(), None);
    }
}
