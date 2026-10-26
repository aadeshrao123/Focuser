//! A chime when a focus session changes phase (#19).
//!
//! Played from Rust because the window usually sits hidden in the tray while a
//! session runs, and nothing in a hidden webview can be relied on to fire.

use std::sync::Arc;

#[cfg(not(windows))]
use focuser_common::pomodoro::PomodoroPhase;
#[cfg(any(not(windows), test))]
use focuser_core::db::Database;
use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};
use tauri::State;
use tracing::warn;

use crate::AppState;

#[cfg(not(windows))]
pub const ENABLED: &str = "pomodoro_sound";
pub const VOLUME: &str = "pomodoro_sound_volume";
/// Empty or absent means the built-in chime.
pub const FILE: &str = "pomodoro_sound_file";
const DEFAULT_VOLUME: u8 = 70;

type Sound = Box<dyn Source + Send>;

/// Called by the blocking loop when a phase runs out, not when one is skipped.
#[cfg(not(windows))]
pub fn phase_changed(db: &Database, to: PomodoroPhase) {
    if db.get_setting(ENABLED).ok().flatten().as_deref() != Some("true") {
        return;
    }
    let (file, volume) = settings(db);
    match load(file.as_deref(), to.is_work()) {
        Ok(sound) => play(sound, volume),
        Err(e) => warn!("phase sound not played: {e}"),
    }
}

/// The Play button in Settings. Errors come back so a bad file says so.
#[tauri::command]
pub fn preview_sound(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    #[cfg(not(windows))]
    let (file, volume) = {
        let engine = state.engine.lock().map_err(|_| "engine lock poisoned")?;
        settings(engine.db())
    };
    #[cfg(windows)]
    let (file, volume) = {
        use focuser_app::{Command, CommandResult};
        let read = |key: &str| -> Result<Option<String>, String> {
            match crate::dispatch(
                &state,
                Command::GetSetting {
                    key: key.into(),
                    default: None,
                },
            )
            .map_err(|e| e.message)?
            {
                CommandResult::Setting(value) => Ok(value),
                _ => Err("Unexpected settings reply".into()),
            }
        };
        (
            read(FILE)?.filter(|s| !s.is_empty()),
            f32::from(
                read(VOLUME)?
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(DEFAULT_VOLUME)
                    .min(100),
            ) / 100.0,
        )
    };
    play(load(file.as_deref(), true)?, volume);
    Ok(())
}

/// Pick an audio file. The path is saved by the frontend like any setting.
#[tauri::command(async)]
pub fn pick_sound_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    Ok(app
        .dialog()
        .file()
        .set_title("Choose a Sound")
        .add_filter("Audio", &["wav", "mp3", "ogg", "flac"])
        .blocking_pick_file()
        .map(|picked| picked.to_string()))
}

#[cfg(any(not(windows), test))]
fn settings(db: &Database) -> (Option<String>, f32) {
    let file = db
        .get_setting(FILE)
        .ok()
        .flatten()
        .filter(|f| !f.is_empty());
    let volume = db
        .get_setting(VOLUME)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(DEFAULT_VOLUME)
        .min(100);
    (file, f32::from(volume) / 100.0)
}

fn load(file: Option<&str>, rising: bool) -> Result<Sound, String> {
    let Some(path) = file else {
        return Ok(Box::new(chime(rising)));
    };
    let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let decoder = Decoder::try_from(file).map_err(|e| format!("{path}: {e}"))?;
    Ok(Box::new(decoder))
}

/// On its own thread: the device has to stay open until the sound finishes.
fn play(sound: Sound, volume: f32) {
    std::thread::spawn(move || {
        let Ok(sink) = DeviceSinkBuilder::open_default_sink()
            .map_err(|e| warn!("no audio output for the phase sound: {e}"))
        else {
            return;
        };
        let player = Player::connect_new(sink.mixer());
        player.set_volume(volume);
        player.append(sound);
        player.sleep_until_end();
    });
}

const RATE: u32 = 44_100;

/// Two soft bell notes: rising when focus starts, falling into a break, so
/// the two can be told apart without looking.
fn chime(rising: bool) -> SamplesBuffer {
    let (first, second) = if rising {
        (659.25, 987.77)
    } else {
        (987.77, 659.25)
    };
    let note = |freq: f32, t: f32| {
        if t < 0.0 {
            0.0
        } else {
            (t * freq * std::f32::consts::TAU).sin() * (-t * 5.0).exp()
        }
    };
    let samples: Vec<f32> = (0..RATE * 3 / 2)
        .map(|i| i as f32 / RATE as f32)
        .map(|t| 0.4 * (note(first, t) + note(second, t - 0.18)))
        .collect();
    SamplesBuffer::new(rodio::nz!(1), rodio::nz!(44_100), samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(rising: bool) -> Vec<f32> {
        chime(rising).collect()
    }

    #[test]
    fn the_chime_never_clips_and_fades_to_silence() {
        for rising in [true, false] {
            let s = samples(rising);
            assert!(s.len() > RATE as usize, "shorter than a second");
            assert!(s.iter().all(|x| x.abs() <= 1.0), "clips");
            assert!(s[0].abs() < 0.01, "starts with a click");
            assert!(s[s.len() - 1].abs() < 0.01, "ends with a click");
        }
    }

    #[test]
    fn rising_and_falling_are_different_sounds() {
        assert_ne!(samples(true), samples(false));
    }

    #[test]
    fn volume_defaults_and_is_capped() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(settings(&db), (None, 0.7));

        db.set_setting(VOLUME, "250").unwrap();
        db.set_setting(FILE, "").unwrap();
        assert_eq!(settings(&db), (None, 1.0), "an empty path is the chime");
    }
}
