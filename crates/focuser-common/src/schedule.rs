use chrono::{Datelike, Local, NaiveTime};

use crate::types::{Schedule, TimeSlot};

impl Schedule {
    /// Check if the schedule is active right now (local time).
    pub fn is_active_now(&self) -> bool {
        self.active_period_at(Local::now()).is_some()
    }

    /// Maximal continuous weekly occurrence, merging adjacent/overlapping slots.
    /// Slots that wrap midnight belong to their starting day.
    pub fn active_period_at<T: chrono::TimeZone>(
        &self,
        now: chrono::DateTime<T>,
    ) -> Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
        use chrono::{Duration, Utc};
        if !self.enabled || self.time_slots.is_empty() {
            return None;
        }
        let tz = now.timezone();
        let boundary = |mut local: chrono::NaiveDateTime, end: bool| {
            // A nonexistent DST boundary advances to the first real local minute.
            loop {
                let result = tz.from_local_datetime(&local);
                if let Some(value) = if end {
                    result.latest()
                } else {
                    result.earliest()
                } {
                    break value.with_timezone(&Utc);
                }
                local += Duration::minutes(1);
            }
        };
        let mut periods = Vec::new();
        for offset in -8..=8 {
            let day = now.date_naive() + Duration::days(offset);
            for slot in &self.time_slots {
                if slot.day != day.weekday()
                    || (slot.start == slot.end && slot.start != NaiveTime::MIN)
                {
                    continue;
                }
                let end_day = if slot.end <= slot.start {
                    day + Duration::days(1)
                } else {
                    day
                };
                periods.push((
                    boundary(day.and_time(slot.start), false),
                    boundary(end_day.and_time(slot.end), true),
                ));
            }
        }
        periods.sort_unstable();
        let mut merged: Vec<(chrono::DateTime<Utc>, chrono::DateTime<Utc>)> = Vec::new();
        for (start, end) in periods {
            if let Some(last) = merged.last_mut() {
                if start <= last.1 {
                    last.1 = last.1.max(end);
                    continue;
                }
            }
            merged.push((start, end));
        }
        let now = now.with_timezone(&Utc);
        merged
            .into_iter()
            .find(|(start, end)| *start <= now && now < *end)
            .map(|(start, end)| {
                // A continuous full week never transitions inactive.
                if end - start > Duration::days(8) {
                    (
                        chrono::DateTime::<Utc>::MIN_UTC,
                        chrono::DateTime::<Utc>::MAX_UTC,
                    )
                } else {
                    (start, end)
                }
            })
    }
}

impl TimeSlot {
    /// Create a new time slot.
    pub fn new(day: chrono::Weekday, start: NaiveTime, end: NaiveTime) -> Self {
        Self { day, start, end }
    }

    /// Check if a time falls within this slot.
    pub fn contains_time(&self, time: NaiveTime) -> bool {
        // The schedule grid encodes a full day as midnight to midnight.
        if self.start == NaiveTime::MIN && self.end == NaiveTime::MIN {
            return true;
        }
        if self.start <= self.end {
            // Normal range: e.g., 09:00 - 17:00
            time >= self.start && time < self.end
        } else {
            // Wraps midnight: e.g., 22:00 - 06:00
            time >= self.start || time < self.end
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    #[test]
    fn test_time_slot_normal_range() {
        let slot = TimeSlot::new(
            chrono::Weekday::Mon,
            NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        );
        assert!(slot.contains_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap()));
        assert!(!slot.contains_time(NaiveTime::from_hms_opt(18, 0, 0).unwrap()));
        assert!(!slot.contains_time(NaiveTime::from_hms_opt(8, 0, 0).unwrap()));
    }

    #[test]
    fn test_time_slot_midnight_wrap() {
        let slot = TimeSlot::new(
            chrono::Weekday::Fri,
            NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
        );
        assert!(slot.contains_time(NaiveTime::from_hms_opt(23, 0, 0).unwrap()));
        assert!(slot.contains_time(NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
        assert!(!slot.contains_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap()));
    }
}

#[cfg(test)]
mod protection_tests {
    use super::*;
    use crate::types::{BlockList, ScheduledProtection, new_id};
    use chrono::{TimeZone, Utc, Weekday};

    fn at(day: u32, hour: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, day, hour, 0, 0).unwrap()
    }

    fn list() -> BlockList {
        let mut list = BlockList::new("Weekly");
        list.schedule = Some(Schedule {
            id: new_id(),
            name: "Weekly".into(),
            enabled: true,
            time_slots: vec![TimeSlot::new(
                Weekday::Mon,
                NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            )],
        });
        list
    }

    #[test]
    fn opt_in_and_transitions() {
        let mut list = list();
        assert!(list.scheduled_protection_at(at(21, 10)).is_none());
        list.scheduled_protection = Some(ScheduledProtection { lock: None });
        assert!(list.scheduled_protection_at(at(21, 8)).is_none());
        assert!(list.scheduled_protection_at(at(21, 9)).is_some());
        assert!(list.scheduled_protection_at(at(21, 16)).is_some());
        assert!(list.scheduled_protection_at(at(21, 17)).is_none());
        assert!(list.scheduled_protection_at(at(28, 9)).is_some());
        list.enabled = false;
        assert!(list.scheduled_protection_at(at(28, 9)).is_none());
        list.enabled = true;
        assert!(list.scheduled_protection_at(at(28, 9)).is_some());
    }

    #[test]
    fn persisted_bypass_is_for_one_occurrence_only() {
        let mut list = list();
        list.scheduled_protection = Some(ScheduledProtection { lock: None });
        let window = list.scheduled_protection_at(at(21, 10)).unwrap();
        list.schedule_unlocked_until = Some(window.expires_at);
        for hour in 10..17 {
            list.name = format!("Edit {hour}");
            let json = serde_json::to_string(&list).unwrap();
            list = serde_json::from_str(&json).unwrap();
            assert!(list.scheduled_protection_at(at(21, hour)).is_none());
        }
        assert!(list.scheduled_protection_at(at(21, 17)).is_none());
        assert!(list.scheduled_protection_at(at(28, 9)).is_some());
    }

    #[test]
    fn restart_and_legacy_json_defaults() {
        let mut list = list();
        list.scheduled_protection = Some(ScheduledProtection { lock: None });
        let mut json = serde_json::to_value(&list).unwrap();
        let restored: BlockList = serde_json::from_value(json.clone()).unwrap();
        assert!(restored.scheduled_protection_at(at(21, 10)).is_some());
        json.as_object_mut().unwrap().remove("scheduled_protection");
        json.as_object_mut()
            .unwrap()
            .remove("schedule_unlocked_until");
        let legacy: BlockList = serde_json::from_value(json).unwrap();
        assert!(legacy.scheduled_protection.is_none());
        assert!(legacy.schedule_unlocked_until.is_none());
        assert!(legacy.scheduled_protection_at(at(21, 10)).is_none());
    }

    #[test]
    fn overnight_adjacent_and_overlapping_slots_are_one_occurrence() {
        let mut schedule = list().schedule.unwrap();
        schedule.time_slots = vec![
            TimeSlot::new(
                Weekday::Mon,
                NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            ),
            TimeSlot::new(
                Weekday::Tue,
                NaiveTime::from_hms_opt(5, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(7, 0, 0).unwrap(),
            ),
            TimeSlot::new(
                Weekday::Tue,
                NaiveTime::from_hms_opt(7, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            ),
        ];
        assert!(schedule.active_period_at(at(21, 3)).is_none());
        assert_eq!(
            schedule.active_period_at(at(21, 23)),
            Some((at(21, 22), at(22, 9)))
        );
        assert_eq!(
            schedule.active_period_at(at(22, 3)),
            Some((at(21, 22), at(22, 9)))
        );
        assert!(schedule.active_period_at(at(22, 9)).is_none());
    }

    #[test]
    fn edits_continue_the_bypass_until_the_edited_occurrence_ends() {
        let mut list = list();
        list.scheduled_protection = Some(ScheduledProtection { lock: None });
        list.schedule_unlocked_until = Some(at(21, 17));
        list.schedule.as_mut().unwrap().time_slots[0].end =
            NaiveTime::from_hms_opt(20, 0, 0).unwrap();
        list.reconcile_schedule_bypass_at(at(21, 12));
        assert_eq!(list.schedule_unlocked_until, Some(at(21, 20)));
        assert!(list.scheduled_protection_at(at(21, 18)).is_none());
        list.schedule.as_mut().unwrap().time_slots[0].end =
            NaiveTime::from_hms_opt(19, 0, 0).unwrap();
        list.reconcile_schedule_bypass_at(at(21, 18));
        assert_eq!(list.schedule_unlocked_until, Some(at(21, 19)));
        assert!(list.scheduled_protection_at(at(21, 19)).is_none());
        assert!(list.scheduled_protection_at(at(28, 10)).is_some());
        // A later edit must not revive a bypass from last week.
        list.reconcile_schedule_bypass_at(at(28, 10));
        assert!(list.schedule_unlocked_until.is_none());
    }

    #[test]
    fn overnight_bypass_does_not_leak_into_the_next_occurrence() {
        let mut list = list();
        list.scheduled_protection = Some(ScheduledProtection { lock: None });
        let slot = &mut list.schedule.as_mut().unwrap().time_slots[0];
        slot.start = NaiveTime::from_hms_opt(22, 0, 0).unwrap();
        slot.end = NaiveTime::from_hms_opt(6, 0, 0).unwrap();
        list.schedule_unlocked_until =
            Some(list.scheduled_protection_at(at(21, 23)).unwrap().expires_at);
        assert!(list.scheduled_protection_at(at(22, 3)).is_none());
        assert!(list.scheduled_protection_at(at(22, 6)).is_none());
        assert!(list.scheduled_protection_at(at(28, 23)).is_some());
    }

    #[test]
    fn whole_day_and_continuous_week() {
        let mut schedule = list().schedule.unwrap();
        schedule.time_slots = vec![TimeSlot::new(Weekday::Mon, NaiveTime::MIN, NaiveTime::MIN)];
        assert_eq!(
            schedule.active_period_at(at(21, 12)),
            Some((at(21, 0), at(22, 0)))
        );
        schedule.time_slots = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ]
        .into_iter()
        .map(|day| TimeSlot::new(day, NaiveTime::MIN, NaiveTime::MIN))
        .collect();
        assert_eq!(
            schedule.active_period_at(at(21, 12)).unwrap().1,
            chrono::DateTime::<Utc>::MAX_UTC
        );
    }
}
