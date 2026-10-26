use chrono::{NaiveTime, TimeZone, Utc, Weekday};
use focuser_common::types::*;
use focuser_service::removal::complete_decision_at;
use focuser_service::removal::decision_at;
fn at(hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 19, hour, 30, 0).unwrap()
}
fn scheduled() -> BlockList {
    let mut l = BlockList::new("Scheduled");
    l.schedule = Some(Schedule {
        id: new_id(),
        name: "Monday".into(),
        enabled: true,
        time_slots: vec![TimeSlot::new(
            Weekday::Mon,
            NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(11, 0, 0).unwrap(),
        )],
    });
    l.scheduled_protection = Some(ScheduledProtection { lock: None });
    l
}
#[test]
fn schedule_gaps_and_occurrence_unlocks_keep_legacy_contract() {
    let mut l = scheduled();
    assert!(decision_at(&[l.clone()], at(8)).allowed);
    let d = decision_at(&[l.clone()], at(10));
    assert!(!d.allowed);
    assert_eq!(d.blockers[0].reason, "scheduled_occurrence");
    assert_eq!(d.release_at.as_deref(), Some("2026-10-19T11:00:00+00:00"));
    assert!(decision_at(&[l.clone()], at(12)).allowed);
    l.schedule_unlocked_until = Some(at(11));
    assert!(decision_at(&[l], at(10)).allowed);
}
#[test]
fn manual_commitment_spans_gaps_pauses_and_disabled_pomodoro_break_until_original_expiry() {
    let mut l = scheduled();
    l.protection = Some(Protection {
        prevent_uninstall: true,
        prevent_service_stop: false,
        prevent_modification: false,
        started_at: at(7),
        expires_at: Some(at(13)),
    });
    l.breaks = Some(BreakConfig::Allowance {
        allowed_minutes: 5,
        period: AllowancePeriod::PerHour,
        track_active_only: true,
    });
    assert!(!decision_at(&[l.clone()], at(8)).allowed);
    assert!(!decision_at(&[l.clone()], at(12)).allowed);
    // Blocking may pause; the stored uninstall commitment and expiry do not.
    l.enabled = false;
    let d = decision_at(&[l.clone()], at(10));
    assert!(!d.allowed);
    assert_eq!(d.release_at, Some(at(13).to_rfc3339()));
    assert!(decision_at(&[l], at(14)).allowed);
}
#[test]
fn overlapping_manual_and_scheduled_protections_report_every_list_and_latest_release() {
    let a = scheduled();
    let mut b = BlockList::new("Manual");
    b.protection = Some(Protection {
        prevent_uninstall: true,
        prevent_service_stop: true,
        prevent_modification: true,
        started_at: at(7),
        expires_at: Some(at(13)),
    });
    let d = decision_at(&[a, b], at(10));
    assert_eq!(d.blockers.len(), 2);
    assert_eq!(d.release_at, Some(at(13).to_rfc3339()));
}
#[test]
fn until_unlocked_reports_no_automatic_release() {
    let mut l = BlockList::new("Password");
    l.lock = Some(Lock::RandomText { length: 6 });
    l.protection = Some(Protection::until_unlocked());
    let d = decision_at(&[l], at(10));
    assert!(!d.allowed);
    assert!(d.release_at.is_none());
}

#[test]
fn expiry_and_verified_occurrence_unlock_release_all_overlapping_commitments() {
    let a = scheduled();
    let mut b = BlockList::new("Manual during break");
    b.enabled = false;
    b.protection = Some(Protection {
        prevent_uninstall: true,
        prevent_service_stop: false,
        prevent_modification: false,
        started_at: at(7),
        expires_at: Some(at(13)),
    });
    let d = decision_at(&[a.clone(), b.clone()], at(10));
    assert_eq!(d.blockers.len(), 2);
    assert!(!decision_at(&[a.clone(), b.clone()], at(12)).allowed);
    assert!(decision_at(&[a, b.clone()], at(13)).allowed);
    b.protection = None; // The application clears this only after verified unlock.
    assert!(decision_at(&[b], at(10)).allowed);
}
#[test]
fn denial_message_names_every_blocker_and_its_release_condition() {
    let a = scheduled();
    let mut b = BlockList::new("Deep work");
    b.lock = Some(Lock::RandomText { length: 6 });
    b.protection = Some(Protection::until_unlocked());
    let d = decision_at(&[a, b], at(10));
    let message = focuser_service::removal::explanation(&d);
    assert!(message.contains("Scheduled: until 2026-10-19T11:00:00+00:00"));
    assert!(message.contains("Deep work: until it is legitimately unlocked in Focuser"));
}

#[test]
fn complete_removal_requires_all_running_blocks_to_end_even_without_uninstall_flag() {
    let mut list = BlockList::new("Unprotected running block");
    list.websites.push(WebsiteRule::domain("example.com"));
    assert!(decision_at(&[list.clone()], at(10)).allowed); // legacy opt-out unchanged
    let blocked = complete_decision_at(&[list.clone()], at(10));
    assert!(!blocked.allowed);
    assert_eq!(blocked.blockers[0].reason, "running_block");
    assert!(focuser_service::removal::explanation(&blocked).contains("until the block is ended"));
    list.websites[0].enabled = false;
    assert!(complete_decision_at(&[list.clone()], at(10)).allowed);
    list.websites[0].enabled = true;
    list.enabled = false;
    assert!(complete_decision_at(&[list], at(10)).allowed);
}
#[test]
fn complete_removal_preserves_original_commitments_across_pauses_breaks_and_overlap() {
    let mut a = scheduled();
    a.websites.push(WebsiteRule::domain("example.com"));
    let mut b = BlockList::new("Paused commitment");
    b.enabled = false;
    b.protection = Some(Protection {
        prevent_uninstall: false,
        prevent_service_stop: false,
        prevent_modification: false,
        started_at: at(7),
        expires_at: Some(at(13)),
    });
    b.breaks = Some(BreakConfig::Allowance {
        allowed_minutes: 5,
        period: AllowancePeriod::PerHour,
        track_active_only: true,
    });
    assert_eq!(
        complete_decision_at(&[a.clone(), b.clone()], at(10))
            .blockers
            .len(),
        2
    );
    assert!(!complete_decision_at(&[a.clone(), b.clone()], at(12)).allowed);
    assert!(complete_decision_at(&[a.clone(), b.clone()], at(13)).allowed);
    b.protection = None; // verified unlock, remains disabled
    assert!(complete_decision_at(&[b], at(10)).allowed);
    assert!(complete_decision_at(&[a], at(12)).allowed); // true schedule gap
}
