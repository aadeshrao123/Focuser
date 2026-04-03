use chrono::{Datelike, Local, NaiveTime, Timelike};

use crate::types::{Schedule, TimeSlot};

impl Schedule {
    /// Check if the schedule is active right now (local time).
    pub fn is_active_now(&self) -> bool {
        if !self.enabled {
            return false;
        }
        let now = Local::now();
        let current_day = now.weekday();
        let current_time = NaiveTime::from_hms_opt(
            now.hour(),
            now.minute(),
            now.second(),
        )
        .unwrap_or_default();

        self.time_slots.iter().any(|slot| {
            slot.day == current_day && slot.contains_time(current_time)
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
