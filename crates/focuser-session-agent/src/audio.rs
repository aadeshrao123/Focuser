use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};
use tracing::warn;
type Sound = Box<dyn Source + Send>;
pub fn phase(snapshot: &focuser_ipc::AgentSnapshot, rising: bool) {
    if snapshot.sound_enabled {
        match load(snapshot.sound_file.as_deref(), rising) {
            Ok(sound) => play(sound, f32::from(snapshot.sound_volume) / 100.0),
            Err(error) => tracing::warn!(%error, "Sound unavailable"),
        }
    }
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
