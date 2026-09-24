//! Sound cues: a short rising two-tone when recording starts, a falling one when it stops.
//!
//! An alternative channel for recording state, not decoration (docs/UI-SPEC.md): the tray
//! icon may be on another monitor, and the moment the microphone opens is the one thing the
//! user must never be unsure about. On by default, `ui.soundCues` (docs/SETTINGS.md).
//!
//! The tones are generated here rather than shipped as files: nothing to license, nothing to
//! go missing from a bundle, and the waveform is a pure function the tests can check. They
//! play through cpal's default output device on a throwaway thread, so the pipeline never
//! waits on audio output and a machine with no output device simply hears nothing.
//!
//! Timing: the start cue is played right after the microphone opens, so the first syllable
//! is never lost to it; the microphone does hear the cue, which is harmless to recognition
//! and is why the cue is short and quiet.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// Recording started: two notes, low then high.
    Start,
    /// Recording stopped: the same two notes, high then low.
    Stop,
}

const NOTE_MS: u32 = 45;
const GAP_MS: u32 = 10;
const RAMP_MS: u32 = 5;
/// About -14 dBFS: audible over typing, under speech.
const AMPLITUDE: f32 = 0.2;
const LOW_HZ: f32 = 660.0;
const HIGH_HZ: f32 = 880.0;

/// Total length of a cue.
pub fn duration() -> Duration {
    Duration::from_millis(u64::from(NOTE_MS * 2 + GAP_MS))
}

/// Mono samples for `cue` at `sample_rate`. Deterministic; linear 5 ms attack and release on
/// each note so nothing clicks.
pub fn samples(cue: Cue, sample_rate: u32) -> Vec<f32> {
    let (first, second) = match cue {
        Cue::Start => (LOW_HZ, HIGH_HZ),
        Cue::Stop => (HIGH_HZ, LOW_HZ),
    };
    let per_ms = sample_rate as f32 / 1000.0;
    let note_len = (NOTE_MS as f32 * per_ms) as usize;
    let gap_len = (GAP_MS as f32 * per_ms) as usize;
    let ramp_len = ((RAMP_MS as f32 * per_ms) as usize).max(1);
    let mut out = Vec::with_capacity(note_len * 2 + gap_len);
    for (i, hz) in [first, second].into_iter().enumerate() {
        if i == 1 {
            out.extend(std::iter::repeat_n(0.0, gap_len));
        }
        for n in 0..note_len {
            let env = if n < ramp_len {
                n as f32 / ramp_len as f32
            } else if n + ramp_len >= note_len {
                (note_len - n) as f32 / ramp_len as f32
            } else {
                1.0
            };
            let t = n as f32 / sample_rate as f32;
            out.push(AMPLITUDE * env * (2.0 * std::f32::consts::PI * hz * t).sin());
        }
    }
    out
}

/// Plays `cue` on the default output device and returns immediately. Failures are logged at
/// debug level only: a cue that cannot play is not an error the user needs to hear about.
pub fn play(cue: Cue) {
    let spawned = std::thread::Builder::new()
        .name("vox-cue".into())
        .spawn(move || {
            if let Err(e) = play_blocking(cue) {
                tracing::debug!("sound cue not played: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::debug!("sound cue thread: {e}");
    }
}

fn play_blocking(cue: Cue) -> anyhow::Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let config = device.default_output_config()?;
    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let pcm = Arc::new(samples(cue, rate));
    let done = Arc::new(AtomicBool::new(false));
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build::<f32>(&device, &config.into(), channels, pcm, done.clone())?
        }
        cpal::SampleFormat::I16 => {
            build::<i16>(&device, &config.into(), channels, pcm, done.clone())?
        }
        cpal::SampleFormat::U16 => {
            build::<u16>(&device, &config.into(), channels, pcm, done.clone())?
        }
        cpal::SampleFormat::I32 => {
            build::<i32>(&device, &config.into(), channels, pcm, done.clone())?
        }
        other => anyhow::bail!("unsupported output sample format {other:?}"),
    };
    stream.play()?;
    // Wait for the callback to run out of samples, then a little longer so the device's own
    // buffer drains before the stream is torn down.
    let deadline = Instant::now() + duration() + Duration::from_millis(400);
    while !done.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(60));
    drop(stream);
    Ok(())
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    pcm: Arc<Vec<f32>>,
    done: Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let mut pos = 0usize;
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            for frame in data.chunks_mut(channels.max(1)) {
                let s = pcm.get(pos).copied().unwrap_or(0.0);
                pos += 1;
                for out in frame {
                    *out = T::from_sample(s);
                }
            }
            if pos >= pcm.len() {
                done.store(true, Ordering::Release);
            }
        },
        |err| tracing::debug!("sound cue output error: {err}"),
        None,
    )?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_matches_the_declared_duration() {
        let rate = 48_000;
        let s = samples(Cue::Start, rate);
        let expected = duration().as_millis() as usize * rate as usize / 1000;
        assert_eq!(s.len(), expected);
    }

    #[test]
    fn stays_under_the_amplitude_and_starts_and_ends_silent() {
        for cue in [Cue::Start, Cue::Stop] {
            let s = samples(cue, 44_100);
            let peak = s.iter().fold(0f32, |m, v| m.max(v.abs()));
            assert!(peak <= AMPLITUDE + 1e-6, "{cue:?} peaks at {peak}");
            assert!(peak > AMPLITUDE * 0.9, "{cue:?} is not silent");
            assert_eq!(s[0], 0.0);
            assert!(s.last().unwrap().abs() < 0.02);
        }
    }

    #[test]
    fn start_rises_and_stop_falls() {
        // Zero crossings in each half: the higher note crosses more often.
        let crossings = |s: &[f32]| {
            s.windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count()
        };
        let rate = 48_000;
        let start = samples(Cue::Start, rate);
        let stop = samples(Cue::Stop, rate);
        let half = start.len() / 2;
        assert!(crossings(&start[..half]) < crossings(&start[half..]));
        assert!(crossings(&stop[..half]) > crossings(&stop[half..]));
    }
}
