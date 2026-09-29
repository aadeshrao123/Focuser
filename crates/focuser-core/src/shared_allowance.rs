//! List-scoped scheduled budgets. Runtime activity is stored separately from
//! exported configuration, as a union of intervals to avoid double charging.
use chrono::{DateTime, Local, TimeZone, Utc};
use focuser_common::allowance::{
    AllowanceMatch, AllowanceStatus, AllowanceTick, SharedAllowanceStatus,
};
use focuser_common::error::{FocuserError, Result};
use focuser_common::types::{BlockList, EntityId, ExceptionType};
use rusqlite::{Connection, OptionalExtension, params};

use crate::Database;

pub fn occurrence<T: TimeZone>(
    list: &BlockList,
    now: DateTime<T>,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    if !list.enabled || list.shared_allowance.as_ref()?.validate().is_err() {
        return None;
    }
    list.schedule.as_ref()?.active_period_at(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveTime, Weekday};
    use focuser_common::allowance::{Allowance, SharedAllowanceConfig};
    use focuser_common::types::{AppRule, Schedule, TimeSlot, WebsiteRule};

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, hour, minute, 0).unwrap()
    }
    fn list() -> BlockList {
        let mut l = BlockList::new("Work");
        l.websites = vec![
            WebsiteRule::domain("youtube.com"),
            WebsiteRule::domain("reddit.com"),
        ];
        l.applications = vec![AppRule::executable("game")];
        l.schedule = Some(Schedule {
            id: l.id,
            name: "hours".into(),
            enabled: true,
            time_slots: vec![TimeSlot::new(
                Weekday::Mon,
                NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            )],
        });
        l.shared_allowance = Some(SharedAllowanceConfig { minutes: 1 });
        l
    }
    fn tick(host: &str, secs: u32) -> AllowanceTick {
        AllowanceTick {
            hostname: Some(host.into()),
            url: Some(format!("https://{host}/")),
            app_exe: None,
            active: true,
            shared_active: true,
            shared_only: false,
            increment_secs: Some(secs),
            source: "test".into(),
        }
    }
    fn remaining(db: &Database, l: &BlockList, now: DateTime<Utc>) -> u32 {
        db.shared_status_at(l, now).unwrap().unwrap().remaining_secs
    }

    fn edit_hours(db: &Database, l: &mut BlockList, start: u32, end: u32, now: DateTime<Utc>) {
        l.schedule.as_mut().unwrap().time_slots = vec![TimeSlot::new(
            Weekday::Mon,
            NaiveTime::from_hms_opt(start, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(end, 0, 0).unwrap(),
        )];
        db.update_block_list_at(l, now).unwrap();
    }

    fn spent_list(db: &Database) -> BlockList {
        let mut l = list();
        l.shared_allowance.as_mut().unwrap().minutes = 30;
        db.create_block_list(&l).unwrap();
        // Twenty minutes consumed before any schedule edits.
        for minute in 1..=20 {
            db.ingest_shared_at(&tick("youtube.com", 60), at(11, minute))
                .unwrap();
        }
        assert_eq!(remaining(db, &l, at(12, 0)), 600);
        l
    }

    #[test]
    fn schedule_edit_moving_start_later_preserves_consumption() {
        let db = Database::open_in_memory().unwrap();
        let mut l = spent_list(&db);
        edit_hours(&db, &mut l, 10, 17, at(12, 0));
        assert_eq!(remaining(&db, &l, at(12, 0)), 600);
        db.ingest_shared_at(&tick("reddit.com", 60), at(12, 1))
            .unwrap();
        assert_eq!(remaining(&db, &l, at(12, 1)), 540);
    }

    #[test]
    fn schedule_edit_moving_start_earlier_preserves_consumption() {
        let db = Database::open_in_memory().unwrap();
        let mut l = spent_list(&db);
        edit_hours(&db, &mut l, 8, 17, at(12, 0));
        assert_eq!(remaining(&db, &l, at(12, 0)), 600);
    }

    #[test]
    fn schedule_edit_extending_end_preserves_consumption_past_old_end() {
        let db = Database::open_in_memory().unwrap();
        let mut l = spent_list(&db);
        edit_hours(&db, &mut l, 9, 19, at(12, 0));
        assert_eq!(remaining(&db, &l, at(18, 0)), 600);
    }

    #[test]
    fn schedule_edit_shortening_end_preserves_consumption_until_new_end() {
        let db = Database::open_in_memory().unwrap();
        let mut l = spent_list(&db);
        edit_hours(&db, &mut l, 9, 13, at(12, 0));
        assert_eq!(remaining(&db, &l, at(12, 30)), 600);
        assert!(!db.shared_status_at(&l, at(13, 0)).unwrap().unwrap().active);
    }

    #[test]
    fn schedule_edit_inactive_gap_ends_occurrence_and_later_period_is_fresh() {
        let db = Database::open_in_memory().unwrap();
        let mut l = spent_list(&db);
        edit_hours(&db, &mut l, 13, 17, at(12, 0));
        assert!(!db.shared_status_at(&l, at(12, 0)).unwrap().unwrap().active);
        assert_eq!(remaining(&db, &l, at(13, 0)), 1800);
        db.ingest_shared_at(&tick("youtube.com", 60), at(13, 1))
            .unwrap();
        // Restoring an old start while this new occurrence is active must
        // retain the new anchor, not recover or erase the morning's budget.
        edit_hours(&db, &mut l, 9, 17, at(13, 2));
        assert_eq!(remaining(&db, &l, at(13, 2)), 1740);
    }

    #[test]
    fn schedule_edit_restoring_same_boundaries_after_gap_uses_fresh_identity() {
        let db = Database::open_in_memory().unwrap();
        let mut l = spent_list(&db);
        edit_hours(&db, &mut l, 13, 17, at(12, 0));
        // No status/activity poll is needed to notice the inactive gap.
        edit_hours(&db, &mut l, 9, 17, at(12, 30));
        assert_eq!(remaining(&db, &l, at(12, 30)), 1800);
        db.ingest_shared_at(&tick("youtube.com", 60), at(12, 31))
            .unwrap();
        edit_hours(&db, &mut l, 8, 18, at(12, 32));
        assert_eq!(remaining(&db, &l, at(12, 32)), 1740);
    }

    #[test]
    fn repeated_active_schedule_edits_and_allowance_toggles_cannot_refill() {
        let db = Database::open_in_memory().unwrap();
        let mut l = spent_list(&db);
        for (start, end) in [(10, 17), (8, 19), (11, 13), (9, 17), (10, 18)] {
            edit_hours(&db, &mut l, start, end, at(12, 0));
            assert_eq!(remaining(&db, &l, at(12, 0)), 600);
        }
        l.shared_allowance = None;
        db.update_block_list_at(&l, at(12, 1)).unwrap();
        edit_hours(&db, &mut l, 7, 20, at(12, 2));
        l.shared_allowance = Some(SharedAllowanceConfig { minutes: 30 });
        db.update_block_list_at(&l, at(12, 3)).unwrap();
        assert_eq!(remaining(&db, &l, at(12, 3)), 600);
        assert_eq!(remaining(&db, &l, at(12, 0) + Duration::weeks(1)), 1800);
    }

    #[test]
    fn restart_after_active_schedule_edit_preserves_anchor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("edited.db");
        let id = {
            let db = Database::open(&path).unwrap();
            let mut l = spent_list(&db);
            edit_hours(&db, &mut l, 10, 19, at(12, 0));
            l.id
        };
        let db = Database::open(&path).unwrap();
        let mut l = db.get_block_list(id).unwrap();
        assert_eq!(remaining(&db, &l, at(18, 0)), 600);
        edit_hours(&db, &mut l, 8, 20, at(18, 1));
        assert_eq!(remaining(&db, &l, at(18, 1)), 600);
    }

    #[test]
    fn occurrence_anchor_migration_preserves_existing_usage_before_first_edit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v6.db");
        let id = {
            let db = Database::open(&path).unwrap();
            let l = spent_list(&db);
            // Recreate the previously implemented v6 shape with its usage
            // intact, but without the new occurrence-identity table.
            db.conn_lock().unwrap().execute_batch(
                "DROP TABLE shared_allowance_occurrences; DELETE FROM schema_version WHERE version=7;"
            ).unwrap();
            l.id
        };
        let db = Database::open(&path).unwrap();
        let mut l = db.get_block_list(id).unwrap();
        // The edit itself must recover the old usage, without a prior UI read.
        edit_hours(&db, &mut l, 10, 19, at(12, 0));
        assert_eq!(remaining(&db, &l, at(18, 0)), 600);
    }

    #[test]
    fn schedule_edit_across_midnight_and_merged_slots_retains_anchor() {
        let db = Database::open_in_memory().unwrap();
        let mut l = list();
        edit_hours_for_new_overnight(&mut l);
        db.create_block_list(&l).unwrap();
        db.ingest_shared_at(&tick("youtube.com", 30), at(23, 30))
            .unwrap();
        let now = at(1, 0) + Duration::days(1);
        l.schedule.as_mut().unwrap().time_slots = vec![
            TimeSlot::new(
                Weekday::Mon,
                NaiveTime::from_hms_opt(23, 0, 0).unwrap(),
                NaiveTime::MIN,
            ),
            TimeSlot::new(
                Weekday::Tue,
                NaiveTime::MIN,
                NaiveTime::from_hms_opt(3, 0, 0).unwrap(),
            ),
            TimeSlot::new(
                Weekday::Tue,
                NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(4, 0, 0).unwrap(),
            ),
        ];
        db.update_block_list_at(&l, now).unwrap();
        assert_eq!(remaining(&db, &l, at(3, 30) + Duration::days(1)), 30);
        assert_eq!(remaining(&db, &l, at(23, 30) + Duration::weeks(1)), 60);
    }

    fn edit_hours_for_new_overnight(l: &mut BlockList) {
        l.schedule.as_mut().unwrap().time_slots = vec![TimeSlot::new(
            Weekday::Mon,
            NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        )];
    }

    #[test]
    fn disabled_and_inactive_never_exempt_and_old_json_defaults_off() {
        let db = Database::open_in_memory().unwrap();
        let mut l = list();
        assert!(!db.shared_permits_at(&l, at(8, 0)));
        l.shared_allowance = None;
        assert!(!db.shared_permits_at(&l, at(10, 0)));
        let mut json = serde_json::to_value(&l).unwrap();
        json.as_object_mut().unwrap().remove("shared_allowance");
        assert!(
            serde_json::from_value::<BlockList>(json)
                .unwrap()
                .shared_allowance
                .is_none()
        );
    }

    #[test]
    fn targets_share_one_budget_and_exhaustion_has_no_fallback() {
        let db = Database::open_in_memory().unwrap();
        let l = list();
        db.create_block_list(&l).unwrap();
        assert_eq!(remaining(&db, &l, at(9, 0)), 60);
        assert!(
            db.ingest_shared_at(&tick("youtube.com", 20), at(10, 0))
                .unwrap()
        );
        assert_eq!(remaining(&db, &l, at(10, 0)), 40);
        db.ingest_shared_at(&tick("reddit.com", 20), at(10, 1))
            .unwrap();
        let mut app = tick("", 20);
        app.hostname = None;
        app.url = None;
        app.app_exe = Some("game".into());
        db.ingest_shared_at(&app, at(10, 2)).unwrap();
        assert_eq!(remaining(&db, &l, at(10, 2)), 0);
        assert!(!db.shared_permits_at(&l, at(10, 3)));
        assert!(
            db.ingest_shared_at(&tick("youtube.com", 20), at(10, 3))
                .unwrap()
        );
        assert!(!db.shared_status_at(&l, at(18, 0)).unwrap().unwrap().active);
        assert_eq!(remaining(&db, &l, at(18, 0)), 60);
        assert_eq!(remaining(&db, &l, at(10, 0) + Duration::weeks(1)), 60);
    }

    #[test]
    fn turning_off_and_on_does_not_replenish_current_occurrence() {
        let db = Database::open_in_memory().unwrap();
        let mut l = list();
        db.create_block_list(&l).unwrap();
        db.ingest_shared_at(&tick("youtube.com", 30), at(10, 0))
            .unwrap();
        l.shared_allowance = None;
        db.update_block_list(&l).unwrap();
        assert!(!db.shared_permits_at(&l, at(10, 1)));
        l.shared_allowance = Some(SharedAllowanceConfig { minutes: 1 });
        db.update_block_list(&l).unwrap();
        assert_eq!(remaining(&db, &l, at(10, 2)), 30);
        l.enabled = false;
        db.update_block_list(&l).unwrap();
        l.enabled = true;
        db.update_block_list(&l).unwrap();
        assert_eq!(remaining(&db, &l, at(10, 3)), 30);
    }

    #[test]
    fn simultaneous_web_and_app_reports_are_union_not_sum() {
        let db = Database::open_in_memory().unwrap();
        let l = list();
        db.create_block_list(&l).unwrap();
        db.ingest_shared_at(&tick("youtube.com", 20), at(10, 0))
            .unwrap();
        let mut app = tick("", 20);
        app.hostname = None;
        app.app_exe = Some("game".into());
        db.ingest_shared_at(&app, at(10, 0)).unwrap();
        db.ingest_shared_at(&tick("reddit.com", 20), at(10, 0) + Duration::seconds(10))
            .unwrap();
        assert_eq!(remaining(&db, &l, at(10, 1)), 30);
    }

    #[test]
    fn persisted_overnight_occurrence_survives_reopening_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.db");
        let mut l = list();
        l.schedule.as_mut().unwrap().time_slots = vec![TimeSlot::new(
            Weekday::Mon,
            NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        )];
        {
            let db = Database::open(&path).unwrap();
            db.create_block_list(&l).unwrap();
            db.ingest_shared_at(&tick("youtube.com", 30), at(23, 0))
                .unwrap();
        }
        let db = Database::open(&path).unwrap();
        let l = db.get_block_list(l.id).unwrap();
        assert_eq!(remaining(&db, &l, at(0, 30) + Duration::days(1)), 30);
        assert_eq!(remaining(&db, &l, at(23, 0) + Duration::weeks(1)), 60);
    }

    #[test]
    fn adjacent_and_overlapping_slots_share_identity_disjoint_slots_reset() {
        let db = Database::open_in_memory().unwrap();
        let mut l = list();
        let slot = |a, b| {
            TimeSlot::new(
                Weekday::Mon,
                NaiveTime::from_hms_opt(a, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(b, 0, 0).unwrap(),
            )
        };
        l.schedule.as_mut().unwrap().time_slots =
            vec![slot(9, 10), slot(10, 11), slot(10, 12), slot(14, 15)];
        db.create_block_list(&l).unwrap();
        db.ingest_shared_at(&tick("youtube.com", 30), at(9, 30))
            .unwrap();
        assert_eq!(remaining(&db, &l, at(11, 30)), 30);
        assert_eq!(remaining(&db, &l, at(14, 30)), 60);
    }

    #[test]
    fn engine_keeps_exemptions_list_scoped_for_domains_and_apps() {
        let db = Database::open_in_memory().unwrap();
        let mut l = list();
        l.schedule.as_mut().unwrap().time_slots = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ]
        .into_iter()
        .map(|d| TimeSlot::new(d, NaiveTime::MIN, NaiveTime::MIN))
        .collect();
        db.create_block_list(&l).unwrap();
        let mut engine = crate::BlockEngine::new(db).unwrap();
        assert!(engine.check_domain("youtube.com").is_none());
        if focuser_common::session::app_usage_measurable() {
            assert!(engine.check_app("game", None, None).is_none());
        }
        let mut other = l.clone();
        other.id = focuser_common::types::new_id();
        other.name = "Other".into();
        other.shared_allowance = None;
        engine.db().create_block_list(&other).unwrap();
        engine.refresh().unwrap();
        assert_eq!(engine.check_domain("youtube.com"), Some("Other"));
        if focuser_common::session::app_usage_measurable() {
            assert_eq!(engine.check_app("game", None, None), Some("Other"));
        }
        other.enabled = false;
        engine.db().update_block_list(&other).unwrap();
        engine.refresh().unwrap();
        assert!(engine.check_domain("youtube.com").is_none());
        l.shared_allowance = None;
        engine.db().update_block_list(&l).unwrap();
        engine.refresh().unwrap();
        assert_eq!(engine.check_domain("youtube.com"), Some("Work"));
        assert_eq!(engine.check_app("game", None, None), Some("Work"));
    }

    #[test]
    fn another_list_blocks_access_and_prevents_spending() {
        let db = Database::open_in_memory().unwrap();
        let l = list();
        db.create_block_list(&l).unwrap();
        let mut other = list();
        other.shared_allowance = None;
        db.create_block_list(&other).unwrap();
        assert!(
            db.ingest_shared_at(&tick("youtube.com", 30), at(10, 0))
                .unwrap()
        );
        assert_eq!(remaining(&db, &l, at(10, 0)), 60);
    }

    #[test]
    fn multiple_shared_lists_consume_independently_and_do_not_stack() {
        let db = Database::open_in_memory().unwrap();
        let l = list();
        let mut other = list();
        other.shared_allowance.as_mut().unwrap().minutes = 2;
        db.create_block_list(&l).unwrap();
        db.create_block_list(&other).unwrap();
        db.ingest_shared_at(&tick("youtube.com", 60), at(10, 0))
            .unwrap();
        assert_eq!(remaining(&db, &l, at(10, 0)), 0);
        assert_eq!(remaining(&db, &other, at(10, 0)), 60);
        db.ingest_shared_at(&tick("youtube.com", 60), at(10, 1))
            .unwrap();
        assert_eq!(remaining(&db, &other, at(10, 1)), 60);
    }

    #[test]
    fn paused_individual_usage_stays_intact_and_resumes_unrelated_usage_continues() {
        let db = Database::open_in_memory().unwrap();
        let mut l = list();
        // All week, for the tracker's real-clock wrapper.
        l.schedule.as_mut().unwrap().time_slots = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ]
        .into_iter()
        .map(|d| TimeSlot::new(d, NaiveTime::MIN, NaiveTime::MIN))
        .collect();
        db.create_block_list(&l).unwrap();
        let a = Allowance::new(AllowanceMatch::Domain("youtube.com".into()), 600, true);
        db.create_allowance(&a).unwrap();
        let b = Allowance::new(AllowanceMatch::Domain("other.com".into()), 600, true);
        db.create_allowance(&b).unwrap();
        let tracker = crate::allowance::AllowanceTracker::new();
        tracker.ingest_tick(&db, &tick("youtube.com", 60)).unwrap();
        tracker.ingest_tick(&db, &tick("youtube.com", 60)).unwrap();
        assert_eq!(db.get_allowance_used_today(a.id).unwrap(), 0);
        assert!(
            db.allowance_statuses_with_shared()
                .unwrap()
                .iter()
                .find(|s| s.allowance.id == a.id)
                .unwrap()
                .paused_by_shared
        );
        tracker.ingest_tick(&db, &tick("other.com", 20)).unwrap();
        assert_eq!(db.get_allowance_used_today(b.id).unwrap(), 20);
        l.enabled = false;
        db.update_block_list(&l).unwrap();
        tracker.ingest_tick(&db, &tick("youtube.com", 20)).unwrap();
        assert_eq!(db.get_allowance_used_today(a.id).unwrap(), 20);
    }

    #[test]
    fn inactive_reports_and_unmatched_urls_do_not_consume() {
        let db = Database::open_in_memory().unwrap();
        let mut l = list();
        l.websites[0].match_type =
            focuser_common::types::WebsiteMatchType::UrlPath("youtube.com/shorts".into());
        db.create_block_list(&l).unwrap();
        assert!(
            !db.ingest_shared_at(&tick("youtube.com", 20), at(10, 0))
                .unwrap()
        );
        let mut t = tick("reddit.com", 20);
        t.active = false;
        db.ingest_shared_at(&t, at(10, 0)).unwrap();
        assert_eq!(remaining(&db, &l, at(10, 0)), 60);
    }
}

pub fn active_at<T: TimeZone>(list: &BlockList, now: DateTime<T>) -> bool {
    list.enabled
        && list
            .schedule
            .as_ref()
            .is_none_or(|s| s.time_slots.is_empty() || s.active_period_at(now).is_some())
}

/// Match the actual target, keeping exceptions local to their owning list.
pub fn covers(
    list: &BlockList,
    hostname: Option<&str>,
    url: Option<&str>,
    app: Option<&str>,
) -> bool {
    if let Some(host) = hostname {
        if list.exceptions.iter().any(|e| {
            e.enabled
                && match &e.exception_type {
                    ExceptionType::Domain(d) => focuser_common::host::host_matches(d, host),
                    ExceptionType::Wildcard(w) => focuser_common::host::wildcard_matches(w, host),
                    ExceptionType::LocalFiles => false,
                }
        }) {
            return false;
        }
        return list.websites.iter().any(|r| {
            r.enabled && url.map_or_else(|| r.matches_domain(host), |u| r.matches_url(u))
        });
    }
    app.is_some_and(|exe| {
        list.applications
            .iter()
            .any(|r| r.enabled && r.matches_process(exe, None, None))
    })
}

/// Resolve a schedule boundary to its persisted usage anchor. Changing a live
/// boundary retains this anchor; expiry or an explicit inactive gap ends it.
fn usage_anchor(conn: &Connection, id: EntityId, start: i64, end: i64, now: i64) -> Result<i64> {
    let db_error = |e: rusqlite::Error| FocuserError::Database(e.to_string());
    let current: Option<(i64, i64)> = conn
        .query_row(
            "SELECT usage_start, ends_at FROM shared_allowance_occurrences WHERE block_list_id=?1",
            [id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(db_error)?;
    let key = match current {
        Some((key, until)) if now < until => {
            if until == end {
                return Ok(key);
            }
            key
        }
        Some((previous, _)) => {
            let occupied: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM shared_allowance_usage WHERE block_list_id=?1 AND occurrence_start=?2)",
                params![id.to_string(),start], |r| r.get(0),
            ).map_err(db_error)?;
            if occupied || start == previous {
                // Restoring old boundaries after a genuine gap must not reuse
                // an earlier occurrence. Reserve an unused anchor, persisted
                // even before its first activity report arrives.
                let last: Option<i64> = conn.query_row(
                    "SELECT MAX(occurrence_start) FROM shared_allowance_usage WHERE block_list_id=?1",
                    [id.to_string()], |r| r.get(0),
                ).map_err(db_error)?;
                now.max(previous).max(last.unwrap_or(start)) + 1
            } else {
                start
            }
        }
        // Migration from the original usage model: retain any already consumed
        // time stored under the current schedule's start.
        None => start,
    };
    conn.execute(
        "INSERT INTO shared_allowance_occurrences VALUES (?1,?2,?3)
         ON CONFLICT(block_list_id) DO UPDATE SET usage_start=excluded.usage_start, ends_at=excluded.ends_at",
        params![id.to_string(),key,end],
    ).map_err(db_error)?;
    Ok(key)
}

/// Called in the same transaction as every persisted list update, including
/// wholesale updates. Use schedule activity independently of feature/list
/// toggles, so toggling off, editing, and toggling on cannot refill a budget.
pub(crate) fn schedule_edited<T: TimeZone>(
    conn: &Connection,
    old: &BlockList,
    new: &BlockList,
    now: DateTime<T>,
) -> Result<()> {
    if serde_json::to_value(&old.schedule)? == serde_json::to_value(&new.schedule)? {
        return Ok(());
    }
    let before = old
        .schedule
        .as_ref()
        .and_then(|s| s.active_period_at(now.clone()));
    let after = new
        .schedule
        .as_ref()
        .and_then(|s| s.active_period_at(now.clone()));
    if let Some((start, end)) = before {
        usage_anchor(
            conn,
            old.id,
            start.timestamp(),
            end.timestamp(),
            now.timestamp(),
        )?;
    }
    if before.is_none() || after.is_none() {
        conn.execute(
            "UPDATE shared_allowance_occurrences SET ends_at=MIN(ends_at,?2) WHERE block_list_id=?1",
            params![old.id.to_string(),now.timestamp()],
        ).map_err(|e| FocuserError::Database(e.to_string()))?;
    }
    if let Some((start, end)) = after {
        usage_anchor(
            conn,
            new.id,
            start.timestamp(),
            end.timestamp(),
            now.timestamp(),
        )?;
    }
    Ok(())
}

impl Database {
    fn shared_anchor(
        &self,
        list: &BlockList,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        now: i64,
    ) -> Result<i64> {
        let conn = self.conn_lock()?;
        usage_anchor(&conn, list.id, start.timestamp(), end.timestamp(), now)
    }

    fn shared_intervals(&self, id: EntityId, start: i64) -> Result<Vec<(i64, i64)>> {
        let conn = self.conn_lock()?;
        let json: Option<String> = conn.query_row(
            "SELECT intervals FROM shared_allowance_usage WHERE block_list_id=?1 AND occurrence_start=?2",
            params![id.to_string(), start], |r| r.get(0),
        ).optional().map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(match json {
            Some(s) => serde_json::from_str(&s)?,
            None => vec![],
        })
    }

    pub fn shared_used(&self, id: EntityId, start: i64) -> Result<u32> {
        Ok(self
            .shared_intervals(id, start)?
            .iter()
            .map(|(a, b)| (b - a) as u64)
            .sum::<u64>()
            .min(u32::MAX as u64) as u32)
    }

    fn record_shared_interval(
        &self,
        id: EntityId,
        occurrence: i64,
        from: i64,
        to: i64,
    ) -> Result<()> {
        if from >= to {
            return Ok(());
        }
        // One DB transaction serializes overlapping reports even across trackers.
        let mut conn = self.conn_lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let json: Option<String> = tx.query_row(
            "SELECT intervals FROM shared_allowance_usage WHERE block_list_id=?1 AND occurrence_start=?2",
            params![id.to_string(), occurrence], |r| r.get(0),
        ).optional().map_err(|e| FocuserError::Database(e.to_string()))?;
        let mut intervals: Vec<(i64, i64)> = match json {
            Some(s) => serde_json::from_str(&s)?,
            None => vec![],
        };
        intervals.push((from, to));
        intervals.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::new();
        for (a, b) in intervals {
            if let Some(last) = merged.last_mut().filter(|last| last.1 >= a) {
                last.1 = last.1.max(b);
            } else {
                merged.push((a, b));
            }
        }
        tx.execute("INSERT INTO shared_allowance_usage VALUES (?1,?2,?3) ON CONFLICT(block_list_id,occurrence_start) DO UPDATE SET intervals=excluded.intervals",
            params![id.to_string(), occurrence, serde_json::to_string(&merged)?])
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        tx.commit()
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn shared_status_at<T: TimeZone>(
        &self,
        list: &BlockList,
        now: DateTime<T>,
    ) -> Result<Option<SharedAllowanceStatus>> {
        let Some(config) = &list.shared_allowance else {
            return Ok(None);
        };
        let period = occurrence(list, now.clone());
        let used = match period {
            Some((start, end)) => self.shared_used(
                list.id,
                self.shared_anchor(list, start, end, now.timestamp())?,
            )?,
            None => 0,
        };
        let limit_secs = config.minutes.saturating_mul(60);
        Ok(Some(SharedAllowanceStatus {
            block_list_id: list.id,
            limit_secs,
            remaining_secs: limit_secs.saturating_sub(used),
            active: period.is_some(),
        }))
    }

    pub fn shared_permits_at<T: TimeZone>(&self, list: &BlockList, now: DateTime<T>) -> bool {
        self.shared_status_at(list, now)
            .ok()
            .flatten()
            .is_some_and(|s| s.active && s.remaining_secs > 0)
    }

    /// Returns whether the individual allowance is suppressed, even on inactive
    /// reports or after exhaustion. Consumption never touches editing bypasses.
    pub fn ingest_shared_at<T: TimeZone>(
        &self,
        tick: &AllowanceTick,
        now: DateTime<T>,
    ) -> Result<bool> {
        let lists = self.list_block_lists()?;
        let applicable: Vec<_> = lists
            .iter()
            .filter(|l| {
                active_at(l, now.clone())
                    && covers(
                        l,
                        tick.hostname.as_deref(),
                        tick.url.as_deref(),
                        tick.app_exe.as_deref(),
                    )
            })
            .collect();
        let suppressed = applicable
            .iter()
            .any(|l| occurrence(l, now.clone()).is_some());
        if !suppressed
            || !tick.active
            || !tick.shared_active
            || applicable
                .iter()
                .any(|l| !self.shared_permits_at(l, now.clone()))
        {
            return Ok(suppressed);
        }
        let to = now.timestamp();
        let from = to - i64::from(tick.increment_secs.unwrap_or(5).clamp(1, 120));
        for list in applicable {
            if let Some((start, end)) = occurrence(list, now.clone()) {
                let anchor = self.shared_anchor(list, start, end, to)?;
                self.record_shared_interval(
                    list.id,
                    anchor,
                    from.max(start.timestamp()),
                    to.min(end.timestamp()),
                )?;
            }
        }
        Ok(true)
    }

    pub fn allowance_statuses_with_shared(&self) -> Result<Vec<AllowanceStatus>> {
        let lists = self.list_block_lists()?;
        let mut statuses = self.list_allowance_statuses()?;
        for s in &mut statuses {
            s.paused_by_shared = lists.iter().any(|l| {
                occurrence(l, Local::now()).is_some()
                    && match &s.allowance.target {
                        AllowanceMatch::Domain(d) => covers(l, Some(d), None, None),
                        AllowanceMatch::AppExecutable(a) => covers(l, None, None, Some(a)),
                    }
            });
        }
        Ok(statuses)
    }
}
