//! Microphone capture.
//!
//! The cpal input callback runs on a real-time thread owned by the audio subsystem. Nothing
//! but a lock-free push into a ring buffer happens there — no allocation, no locking, no
//! logging. Everything else (drain, resample, trim) happens on the pipeline thread.
//!
//! v1 trims and rejects with an energy gate rather than Silero VAD, because Silero needs the
//! ONNX runtime that v1 does not ship (ROADMAP.md, "The one decision S3 gates"). The gate only
//! has to answer "was anything said, and where does it start and end"; Apple's engine does its
//! own endpointing on top.

use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapRb};

/// What the engine wants: 16 kHz mono f32.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Energy-gate frame: 20 ms at 16 kHz.
const FRAME: usize = 320;
/// Kept on either side of detected speech so onsets and tails are not clipped.
const PAD_MS: u32 = 300;

pub struct Capture {
    // Dropped in `finish`, which stops the device.
    _stream: cpal::Stream,
    cons: HeapCons<f32>,
    samples: Vec<f32>,
    device_rate: u32,
    started: Instant,
}

impl Capture {
    /// Opens the input stream. Device start costs 20–80 ms depending on platform, which is
    /// why pre-roll exists as an option (docs/LATENCY.md).
    pub fn start(device_id: &str) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = if device_id == "default" {
            host.default_input_device()
        } else {
            host.input_devices()?
                .find(|d| d.name().map(|n| n == device_id).unwrap_or(false))
                .or_else(|| host.default_input_device())
        }
        .ok_or_else(|| anyhow::anyhow!("no input device"))?;
        let config = device.default_input_config()?;
        let device_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        // Two seconds of mono at the device rate. The pipeline drains every ~20 ms while
        // recording, so this never fills; if it ever does, samples are dropped, not blocked on.
        let rb = HeapRb::<f32>::new((device_rate as usize) * 2);
        let (prod, cons) = rb.split();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config.into(), channels, prod)?,
            cpal::SampleFormat::I16 => build::<i16>(&device, &config.into(), channels, prod)?,
            cpal::SampleFormat::U16 => build::<u16>(&device, &config.into(), channels, prod)?,
            cpal::SampleFormat::I32 => build::<i32>(&device, &config.into(), channels, prod)?,
            other => anyhow::bail!("unsupported input sample format {other:?}"),
        };
        stream.play()?;
        Ok(Self {
            _stream: stream,
            cons,
            samples: Vec::with_capacity(device_rate as usize * 30),
            device_rate,
            started: Instant::now(),
        })
    }

    /// Move whatever the callback has produced into the pipeline-side buffer. Cheap; call it
    /// every few tens of milliseconds while recording.
    pub fn drain(&mut self) {
        let mut buf = [0f32; 4096];
        loop {
            let n = self.cons.pop_slice(&mut buf);
            if n == 0 {
                break;
            }
            self.samples.extend_from_slice(&buf[..n]);
        }
    }

    pub fn elapsed_ms(&self) -> u32 {
        self.started.elapsed().as_millis() as u32
    }

    /// Stops capture and returns 16 kHz mono f32, trimmed. Returns None when no speech was
    /// detected, so an accidental hold produces nothing rather than a hallucinated line.
    pub fn finish(mut self, vad: &crate::settings::Vad) -> anyhow::Result<Option<Vec<f32>>> {
        self.drain();
        drop(self._stream);
        let raw = std::mem::take(&mut self.samples);
        let pcm = resample(&raw, self.device_rate, TARGET_SAMPLE_RATE)?;
        Ok(gate(&pcm, vad))
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    mut prod: ringbuf::HeapProd<f32>,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _| {
            // Downmix to mono on the fly; no allocation.
            let inv = 1.0 / channels.max(1) as f32;
            for frame in data.chunks(channels.max(1)) {
                let mut acc = 0f32;
                for s in frame {
                    acc += (*s).to_sample::<f32>();
                }
                let _ = prod.try_push(acc * inv);
            }
        },
        |err| tracing::warn!("audio input error: {err}"),
        None,
    )?;
    Ok(stream)
}

/// Sinc resampling via rubato; identity when the rates already match.
pub fn resample(input: &[f32], from: u32, to: u32) -> anyhow::Result<Vec<f32>> {
    if from == to || input.is_empty() {
        return Ok(input.to_vec());
    }
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };
    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    let chunk = 1024;
    let mut rs = SincFixedIn::<f32>::new(f64::from(to) / f64::from(from), 2.0, params, chunk, 1)?;
    let mut out = Vec::with_capacity(input.len() * to as usize / from as usize + chunk);
    let mut pos = 0;
    while pos + chunk <= input.len() {
        let res = rs.process(&[&input[pos..pos + chunk]], None)?;
        out.extend_from_slice(&res[0]);
        pos += chunk;
    }
    if pos < input.len() {
        let res = rs.process_partial(Some(&[&input[pos..]]), None)?;
        out.extend_from_slice(&res[0]);
    }
    // Flush the resampler's internal delay so the tail is not lost, then drop the group delay
    // from the front and the zero padding from the back so the length matches the input.
    let res = rs.process_partial::<&[f32]>(None, None)?;
    out.extend_from_slice(&res[0]);
    let delay = rs.output_delay().min(out.len());
    out.drain(..delay);
    let expected = (input.len() as u64 * u64::from(to) / u64::from(from)) as usize;
    out.truncate(expected);
    Ok(out)
}

/// Energy gate: find the first and last 20 ms frames above an adaptive threshold, keep
/// `PAD_MS` around them, and reject clips with less than `min_speech_ms` of speech.
pub fn gate(pcm: &[f32], vad: &crate::settings::Vad) -> Option<Vec<f32>> {
    if pcm.is_empty() {
        return None;
    }
    if !vad.enabled {
        return Some(pcm.to_vec());
    }
    let rms: Vec<f32> = pcm
        .chunks(FRAME)
        .map(|f| (f.iter().map(|s| s * s).sum::<f32>() / f.len() as f32).sqrt())
        .collect();
    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let noise = sorted[sorted.len() / 10]; // 10th percentile: the room
    let threshold = (noise * 4.0).max(0.004);
    let speech: Vec<usize> = rms
        .iter()
        .enumerate()
        .filter(|(_, r)| **r > threshold)
        .map(|(i, _)| i)
        .collect();
    let speech_ms = speech.len() as u32 * 20;
    if speech.is_empty() || speech_ms < vad.min_speech_ms {
        return None;
    }
    if !vad.trim_silence {
        return Some(pcm.to_vec());
    }
    let pad = (PAD_MS / 20) as usize;
    let first = speech[0].saturating_sub(pad) * FRAME;
    let last = ((speech[speech.len() - 1] + pad + 1) * FRAME).min(pcm.len());
    Some(pcm[first..last].to_vec())
}

pub fn devices() -> anyhow::Result<Vec<(String, String)>> {
    let host = cpal::default_host();
    let mut out = vec![("default".to_string(), "System default".to_string())];
    for d in host.input_devices()? {
        if let Ok(name) = d.name() {
            out.push((name.clone(), name));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Vad;

    fn tone(seconds: f32, amp: f32) -> Vec<f32> {
        (0..(seconds * TARGET_SAMPLE_RATE as f32) as usize)
            .map(|i| amp * (i as f32 * 0.05).sin())
            .collect()
    }

    #[test]
    fn silence_is_rejected() {
        let pcm = vec![0.0f32; TARGET_SAMPLE_RATE as usize * 2];
        assert!(gate(&pcm, &Vad::default()).is_none());
    }

    #[test]
    fn a_short_tap_is_rejected() {
        let mut pcm = vec![0.0f32; TARGET_SAMPLE_RATE as usize];
        let burst = tone(0.1, 0.3);
        pcm[8000..8000 + burst.len()].copy_from_slice(&burst);
        assert!(gate(&pcm, &Vad::default()).is_none());
    }

    #[test]
    fn speech_is_kept_and_trimmed() {
        let mut pcm = vec![0.0f32; TARGET_SAMPLE_RATE as usize * 3];
        let burst = tone(1.0, 0.3);
        pcm[16000..16000 + burst.len()].copy_from_slice(&burst);
        let out = gate(&pcm, &Vad::default()).expect("speech detected");
        // 1 s of speech + 300 ms padding each side, give or take a frame.
        assert!(out.len() > 16000 + 2 * 4800 - FRAME * 2);
        assert!(out.len() < 16000 + 2 * 4800 + FRAME * 2);
    }

    #[test]
    fn resample_changes_length_proportionally() {
        let input = tone(1.0, 0.5); // 16000 samples, treated as 48 kHz here
        let out = resample(&input, 48_000, 16_000).unwrap();
        let expected = input.len() / 3;
        assert!(
            (out.len() as i64 - expected as i64).abs() < 64,
            "{}",
            out.len()
        );
    }
}
