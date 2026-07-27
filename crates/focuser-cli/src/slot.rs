//! Parsing for `--slot DAY:START-END`.

use std::str::FromStr;

use chrono::{NaiveTime, Weekday};
use focuser_common::types::TimeSlot;

/// One schedule slot as written on the command line, e.g. `mon:09:00-17:00`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotSpec {
    pub day: Weekday,
    pub start: NaiveTime,
    pub end: NaiveTime,
}

impl From<SlotSpec> for TimeSlot {
    fn from(s: SlotSpec) -> Self {
        TimeSlot {
            day: s.day,
            start: s.start,
            end: s.end,
        }
    }
}

impl FromStr for SlotSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // `mon:09:00-17:00` — split on the first colon only, since the times
        // contain colons of their own.
        let (day_part, range) = s
            .split_once(':')
            .ok_or_else(|| format!("expected DAY:START-END, got `{s}`"))?;

        let day = parse_day(day_part)?;

        let (start, end) = range
            .split_once('-')
            .ok_or_else(|| format!("expected START-END in `{range}`"))?;

        Ok(SlotSpec {
            day,
            start: parse_time(start)?,
            end: parse_time(end)?,
        })
    }
}

fn parse_day(s: &str) -> Result<Weekday, String> {
    match s.to_ascii_lowercase().as_str() {
        "mon" | "monday" => Ok(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Ok(Weekday::Tue),
        "wed" | "weds" | "wednesday" => Ok(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Ok(Weekday::Thu),
        "fri" | "friday" => Ok(Weekday::Fri),
        "sat" | "saturday" => Ok(Weekday::Sat),
        "sun" | "sunday" => Ok(Weekday::Sun),
        other => Err(format!(
            "unknown day `{other}` (expected mon, tue, wed, thu, fri, sat or sun)"
        )),
    }
}

fn parse_time(s: &str) -> Result<NaiveTime, String> {
    // Accept `9`, `09`, `09:00` and `09:00:00` — writing `mon:9-17` by hand is
    // far more natural than being forced to spell out seconds.
    let s = s.trim();

    if let Ok(hour) = s.parse::<u32>() {
        return NaiveTime::from_hms_opt(hour, 0, 0)
            .ok_or_else(|| format!("hour {hour} is out of range"));
    }

    NaiveTime::parse_from_str(s, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S"))
        .map_err(|_| format!("could not parse time `{s}` (expected H, HH:MM or HH:MM:SS)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_spec() {
        let slot: SlotSpec = "mon:09:00-17:00".parse().unwrap();
        assert_eq!(slot.day, Weekday::Mon);
        assert_eq!(slot.start, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert_eq!(slot.end, NaiveTime::from_hms_opt(17, 0, 0).unwrap());
    }

    #[test]
    fn accepts_bare_hours_and_long_day_names() {
        let slot: SlotSpec = "Saturday:9-17".parse().unwrap();
        assert_eq!(slot.day, Weekday::Sat);
        assert_eq!(slot.start, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
    }

    #[test]
    fn reports_the_offending_value_rather_than_failing_vaguely() {
        let err = "funday:9-17".parse::<SlotSpec>().unwrap_err();
        assert!(err.contains("funday"), "got: {err}");

        let err = "mon".parse::<SlotSpec>().unwrap_err();
        assert!(err.contains("DAY:START-END"), "got: {err}");

        let err = "mon:99-17".parse::<SlotSpec>().unwrap_err();
        assert!(err.contains("99"), "got: {err}");
    }
}
