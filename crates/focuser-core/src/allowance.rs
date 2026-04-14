//! Allowance tracker: consumes ticks (from the extension for domains or
//! from a platform watcher for apps) and maintains per-day usage. When a
//! quota is exhausted the target is added to a transient block set that
//! the extension picks up on its next rules poll.

use chrono::{Local, NaiveDate};
use focuser_common::allowance::{Allowance, AllowanceMatch, AllowanceTick};
use focuser_common::error::Result;
use focuser_common::types::EntityId;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tracing::info;

use crate::Database;

/// Seconds added per tick. Must match the extension's reporting cadence.
pub const TICK_INCREMENT_SECS: u32 = 5;

/// Warning thresholds (used by the UI / notifier to trigger toasts).
pub const WARN_THRESHOLD_PCT: u8 = 80;

/// Shared tracker that lives inside the app state. Stores the set of
/// domains/apps currently blocked due to exhausted allowances, plus a
/// one-shot warning set so we only toast once per threshold per day.
pub struct AllowanceTracker {
    inner: Mutex<TrackerInner>,
}

#[derive(Default)]
struct TrackerInner {
    /// Domains blocked today due to exhausted allowance.
    blocked_domains: HashSet<String>,
    /// App executables blocked today.
    blocked_apps: HashSet<String>,
    /// (allowance_id, threshold_pct) tuples we already notified for today.
    warned_today: HashSet<(String, u8)>,
    /// The date (local) the current tracker state corresponds to.
    tracked_date: Option<NaiveDate>,
    /// Events buffered for the UI (notifications).
    pending_notifications: Vec<AllowanceNotification>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AllowanceNotification {
    pub allowance_id: String,
    pub target: String,
    pub kind: AllowanceNotificationKind,
    pub used_secs: u32,
    pub limit_secs: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum AllowanceNotificationKind {
    Warning80,
    Exhausted,
}

impl Default for AllowanceTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AllowanceTracker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(TrackerInner::default()),
        }
    }

    /// Check and reset daily state if the local date has changed.
    fn reset_if_new_day(&self, inner: &mut TrackerInner) {
        let today = Local::now().date_naive();
        if inner.tracked_date != Some(today) {
            if inner.tracked_date.is_some() {
                info!("Allowance tracker: new day — resetting blocked set");
            }
            inner.blocked_domains.clear();
            inner.blocked_apps.clear();
            inner.warned_today.clear();
            inner.tracked_date = Some(today);
        }
    }

    /// Recompute the blocked set from DB state (on startup / manual refresh).
    pub fn rebuild_from_db(&self, db: &Database) -> Result<()> {
        let mut inner = self.inner.lock().expect("tracker mutex poisoned");
        self.reset_if_new_day(&mut inner);
        inner.blocked_domains.clear();
        inner.blocked_apps.clear();
        for a in db.list_allowances()? {
            if !a.enabled {
                continue;
            }
            let used = db.get_allowance_used_today(a.id)?;
            if used >= a.daily_limit_secs {
                match &a.target {
                    AllowanceMatch::Domain(d) => {
                        inner.blocked_domains.insert(d.to_lowercase());
                    }
                    AllowanceMatch::AppExecutable(e) => {
                        inner.blocked_apps.insert(e.to_lowercase());
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle one tick. Attributes the tick to any matching allowance,
    /// increments usage, and updates the blocked set when exhausted.
    pub fn ingest_tick(&self, db: &Database, tick: &AllowanceTick) -> Result<()> {
        let mut inner = self.inner.lock().expect("tracker mutex poisoned");
        self.reset_if_new_day(&mut inner);
        drop(inner);

        let matches = find_matching_allowances(db, tick)?;
        for a in matches {
            if !a.enabled {
                continue;
            }
            // If strict_mode, only count when active=true.
            if a.strict_mode && !tick.active {
                continue;
            }
            let used = db.increment_allowance_usage(a.id, TICK_INCREMENT_SECS)?;
            self.check_thresholds(&a, used);
        }
        Ok(())
    }

    /// Emit warning / exhausted notifications at thresholds. Updates the
    /// blocked set when a limit is hit.
    fn check_thresholds(&self, a: &Allowance, used: u32) {
        let mut inner = self.inner.lock().expect("tracker mutex poisoned");
        self.reset_if_new_day(&mut inner);

        let limit = a.daily_limit_secs.max(1);
        let pct = ((used as u64) * 100 / (limit as u64)).min(255) as u8;
        let id_str = a.id.to_string();

        if pct >= 100 {
            match &a.target {
                AllowanceMatch::Domain(d) => {
                    if inner.blocked_domains.insert(d.to_lowercase())
                        && !inner.warned_today.contains(&(id_str.clone(), 100))
                    {
                        inner.warned_today.insert((id_str.clone(), 100));
                        inner.pending_notifications.push(AllowanceNotification {
                            allowance_id: id_str,
                            target: d.clone(),
                            kind: AllowanceNotificationKind::Exhausted,
                            used_secs: used,
                            limit_secs: limit,
                        });
                    }
                }
                AllowanceMatch::AppExecutable(e) => {
                    if inner.blocked_apps.insert(e.to_lowercase())
                        && !inner.warned_today.contains(&(id_str.clone(), 100))
                    {
                        inner.warned_today.insert((id_str.clone(), 100));
                        inner.pending_notifications.push(AllowanceNotification {
                            allowance_id: id_str,
                            target: e.clone(),
                            kind: AllowanceNotificationKind::Exhausted,
                            used_secs: used,
                            limit_secs: limit,
                        });
                    }
                }
            }
        } else if pct >= WARN_THRESHOLD_PCT
            && !inner
                .warned_today
                .contains(&(id_str.clone(), WARN_THRESHOLD_PCT))
        {
            inner
                .warned_today
                .insert((id_str.clone(), WARN_THRESHOLD_PCT));
            inner.pending_notifications.push(AllowanceNotification {
                allowance_id: id_str,
                target: a.target.display().to_string(),
                kind: AllowanceNotificationKind::Warning80,
                used_secs: used,
                limit_secs: limit,
            });
        }
    }

    /// Is a hostname blocked right now due to an exhausted allowance?
    pub fn is_domain_blocked(&self, hostname: &str) -> bool {
        let mut inner = self.inner.lock().expect("tracker mutex poisoned");
        self.reset_if_new_day(&mut inner);
        let h = hostname.trim().to_ascii_lowercase();
        let h = h.strip_prefix("www.").unwrap_or(&h).to_string();
        inner
            .blocked_domains
            .iter()
            .any(|d| d == &h || h.ends_with(&format!(".{d}")))
    }

    pub fn is_app_blocked(&self, exe_name: &str) -> bool {
        let mut inner = self.inner.lock().expect("tracker mutex poisoned");
        self.reset_if_new_day(&mut inner);
        inner
            .blocked_apps
            .contains(&exe_name.trim().to_ascii_lowercase())
    }

    /// Snapshot the blocked domain set (for extension rule compilation).
    pub fn blocked_domains(&self) -> Vec<String> {
        let mut inner = self.inner.lock().expect("tracker mutex poisoned");
        self.reset_if_new_day(&mut inner);
        inner.blocked_domains.iter().cloned().collect()
    }

    pub fn blocked_apps(&self) -> Vec<String> {
        let mut inner = self.inner.lock().expect("tracker mutex poisoned");
        self.reset_if_new_day(&mut inner);
        inner.blocked_apps.iter().cloned().collect()
    }

    /// Drain buffered notifications (consumed by the UI event loop).
    pub fn take_notifications(&self) -> Vec<AllowanceNotification> {
        let mut inner = self.inner.lock().expect("tracker mutex poisoned");
        std::mem::take(&mut inner.pending_notifications)
    }
}

fn find_matching_allowances(db: &Database, tick: &AllowanceTick) -> Result<Vec<Allowance>> {
    let all = db.list_allowances()?;
    let mut out = Vec::new();
    if let Some(host) = tick.hostname.as_deref() {
        for a in &all {
            if matches!(&a.target, AllowanceMatch::Domain(_)) && a.target.matches_hostname(host) {
                out.push(a.clone());
            }
        }
    }
    if let Some(exe) = tick.app_exe.as_deref() {
        let exe_lc = exe.to_ascii_lowercase();
        for a in &all {
            if let AllowanceMatch::AppExecutable(e) = &a.target
                && e.to_ascii_lowercase() == exe_lc
            {
                out.push(a.clone());
            }
        }
    }
    Ok(out)
}

/// Build a map of {allowance_id → used_today_secs} for debug/UI.
#[allow(dead_code)]
pub fn usage_map(db: &Database) -> Result<HashMap<EntityId, u32>> {
    let mut map = HashMap::new();
    for a in db.list_allowances()? {
        map.insert(a.id, db.get_allowance_used_today(a.id)?);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use focuser_common::allowance::{Allowance, AllowanceMatch};

    fn mk_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn tick_increments_usage() {
        let db = mk_db();
        let a = Allowance::new(AllowanceMatch::Domain("youtube.com".into()), 60, false);
        db.create_allowance(&a).unwrap();

        let tracker = AllowanceTracker::new();
        let tick = AllowanceTick {
            hostname: Some("youtube.com".into()),
            app_exe: None,
            active: true,
            source: "test".into(),
        };
        tracker.ingest_tick(&db, &tick).unwrap();
        assert_eq!(
            db.get_allowance_used_today(a.id).unwrap(),
            TICK_INCREMENT_SECS
        );
    }

    #[test]
    fn strict_mode_ignores_inactive() {
        let db = mk_db();
        let a = Allowance::new(AllowanceMatch::Domain("twitter.com".into()), 60, true);
        db.create_allowance(&a).unwrap();

        let tracker = AllowanceTracker::new();
        let tick = AllowanceTick {
            hostname: Some("twitter.com".into()),
            app_exe: None,
            active: false,
            source: "test".into(),
        };
        tracker.ingest_tick(&db, &tick).unwrap();
        assert_eq!(db.get_allowance_used_today(a.id).unwrap(), 0);
    }

    #[test]
    fn exhaustion_blocks_domain() {
        let db = mk_db();
        let a = Allowance::new(AllowanceMatch::Domain("reddit.com".into()), 10, false);
        db.create_allowance(&a).unwrap();

        let tracker = AllowanceTracker::new();
        let tick = AllowanceTick {
            hostname: Some("reddit.com".into()),
            app_exe: None,
            active: true,
            source: "test".into(),
        };
        // 3 ticks of 5s = 15s > 10s limit
        for _ in 0..3 {
            tracker.ingest_tick(&db, &tick).unwrap();
        }
        assert!(tracker.is_domain_blocked("reddit.com"));
        assert!(tracker.is_domain_blocked("old.reddit.com"));
    }

    #[test]
    fn subdomain_matches_root() {
        let m = AllowanceMatch::Domain("youtube.com".into());
        assert!(m.matches_hostname("youtube.com"));
        assert!(m.matches_hostname("music.youtube.com"));
        assert!(m.matches_hostname("www.youtube.com"));
        assert!(!m.matches_hostname("notyoutube.com"));
    }
}
