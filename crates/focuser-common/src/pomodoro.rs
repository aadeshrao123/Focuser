use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::types::{EntityId, new_id};

/// User-editable configuration for a Pomodoro session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Type)]
pub struct PomodoroConfig {
    pub work_secs: u32,
    pub short_break_secs: u32,
    pub long_break_secs: u32,
    pub cycles_until_long_break: u32,
}

/// A named starting point for a Pomodoro session.
///
/// The three built-ins are the single source of these numbers. The GUI and the
/// CLI both read them through [`presets`] instead of restating them, so a
/// change here reaches every surface at once.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Type)]
pub struct PomodoroPreset {
    /// Stable identifier — what `--preset` accepts. Never translated.
    pub key: String,
    /// What the user sees.
    pub label: String,
    pub config: PomodoroConfig,
}

/// The presets on offer, in the order they should be shown.
pub fn presets() -> Vec<PomodoroPreset> {
    [
        ("classic", "Classic", PomodoroConfig::CLASSIC),
        ("long", "Long", PomodoroConfig::LONG),
        ("sprint", "Sprint", PomodoroConfig::SPRINT),
    ]
    .into_iter()
    .map(|(key, label, config)| PomodoroPreset {
        key: key.into(),
        label: label.into(),
        config,
    })
    .collect()
}

/// Resolve a preset key, for `--preset` on the command line.
pub fn preset_by_key(key: &str) -> Option<PomodoroConfig> {
    presets()
        .into_iter()
        .find(|p| p.key.eq_ignore_ascii_case(key))
        .map(|p| p.config)
}

impl PomodoroConfig {
    pub const CLASSIC: Self = Self {
        work_secs: 25 * 60,
        short_break_secs: 5 * 60,
        long_break_secs: 15 * 60,
        cycles_until_long_break: 4,
    };

    pub const LONG: Self = Self {
        work_secs: 50 * 60,
        short_break_secs: 10 * 60,
        long_break_secs: 30 * 60,
        cycles_until_long_break: 3,
    };

    pub const SPRINT: Self = Self {
        work_secs: 15 * 60,
        short_break_secs: 3 * 60,
        long_break_secs: 10 * 60,
        cycles_until_long_break: 4,
    };

    /// Validate that durations are within sane bounds (5s–8h for work,
    /// 30s–2h for breaks, 1–20 cycles). Returns the error string if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.work_secs < 5 || self.work_secs > 8 * 3600 {
            return Err("work duration must be between 5s and 8h".into());
        }
        if self.short_break_secs < 30 || self.short_break_secs > 2 * 3600 {
            return Err("short break must be between 30s and 2h".into());
        }
        if self.long_break_secs < 30 || self.long_break_secs > 2 * 3600 {
            return Err("long break must be between 30s and 2h".into());
        }
        if self.cycles_until_long_break < 1 || self.cycles_until_long_break > 20 {
            return Err("cycles until long break must be between 1 and 20".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroPhase {
    Work,
    ShortBreak,
    LongBreak,
}

impl PomodoroPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::ShortBreak => "short_break",
            Self::LongBreak => "long_break",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "work" => Some(Self::Work),
            "short_break" => Some(Self::ShortBreak),
            "long_break" => Some(Self::LongBreak),
            _ => None,
        }
    }

    pub fn is_work(&self) -> bool {
        matches!(self, Self::Work)
    }

    pub fn is_break(&self) -> bool {
        matches!(self, Self::ShortBreak | Self::LongBreak)
    }
}

/// An active Pomodoro session. Persisted to DB across restarts.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PomodoroSession {
    pub id: EntityId,
    pub block_list_id: EntityId,
    pub config: PomodoroConfig,
    pub current_phase: PomodoroPhase,
    pub current_cycle: u32,
    pub phase_started_at: DateTime<Utc>,
    /// If Some, the session is paused and this is the remaining time
    /// when resumed (in seconds).
    pub paused_remaining_secs: Option<u32>,
    pub completed_cycles: u32,
    pub started_at: DateTime<Utc>,
    /// The `enabled` state of the linked block list before this Pomodoro
    /// session started. Restored when the session ends.
    pub prev_enabled: bool,
}

impl PomodoroSession {
    pub fn new(block_list_id: EntityId, config: PomodoroConfig, prev_enabled: bool) -> Self {
        let now = Utc::now();
        Self {
            id: new_id(),
            block_list_id,
            config,
            current_phase: PomodoroPhase::Work,
            current_cycle: 1,
            phase_started_at: now,
            paused_remaining_secs: None,
            completed_cycles: 0,
            started_at: now,
            prev_enabled,
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused_remaining_secs.is_some()
    }

    /// Duration of the current phase in seconds.
    pub fn current_phase_duration_secs(&self) -> u32 {
        match self.current_phase {
            PomodoroPhase::Work => self.config.work_secs,
            PomodoroPhase::ShortBreak => self.config.short_break_secs,
            PomodoroPhase::LongBreak => self.config.long_break_secs,
        }
    }

    /// Remaining seconds in the current phase. Returns 0 if past the end.
    pub fn remaining_secs(&self, now: DateTime<Utc>) -> u32 {
        if let Some(rem) = self.paused_remaining_secs {
            return rem;
        }
        let elapsed = (now - self.phase_started_at).num_seconds();
        if elapsed < 0 {
            return self.current_phase_duration_secs();
        }
        let duration = self.current_phase_duration_secs() as i64;
        (duration - elapsed).max(0) as u32
    }
}

/// Snapshot of the Pomodoro state for the UI.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PomodoroStatus {
    pub session_id: Uuid,
    pub block_list_id: Uuid,
    pub block_list_name: String,
    pub config: PomodoroConfig,
    pub current_phase: PomodoroPhase,
    pub current_cycle: u32,
    pub completed_cycles: u32,
    pub remaining_secs: u32,
    pub phase_duration_secs: u32,
    pub paused: bool,
    pub started_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_passes_its_own_validation() {
        for preset in presets() {
            assert!(
                preset.config.validate().is_ok(),
                "preset {} is not a valid config",
                preset.key
            );
        }
    }

    #[test]
    fn preset_keys_are_unique() {
        let mut keys: Vec<String> = presets().into_iter().map(|p| p.key).collect();
        let total = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), total, "two presets share a key");
    }

    #[test]
    fn every_preset_is_reachable_by_its_own_key() {
        for preset in presets() {
            assert_eq!(
                preset_by_key(&preset.key),
                Some(preset.config),
                "{} did not resolve back to itself",
                preset.key
            );
        }
    }

    #[test]
    fn lookup_ignores_case_and_rejects_unknown_keys() {
        assert_eq!(preset_by_key("CLASSIC"), Some(PomodoroConfig::CLASSIC));
        assert_eq!(preset_by_key("nonsense"), None);
    }

    #[test]
    fn classic_is_the_first_preset_offered() {
        // The GUI opens on whatever comes first, so this is the default a
        // user sees before touching anything.
        assert_eq!(presets()[0].config, PomodoroConfig::CLASSIC);
    }
}
