//! Microphone capture.
//!
//! The cpal input callback runs on a real-time thread owned by the audio subsystem. Nothing
//! but a lock-free push into a ring buffer happens there — no allocation, no locking, no
//! logging. Everything else (resample, VAD, normalise) happens on the pipeline thread.

/// What the models want: 16 kHz mono f32.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Silero v5 only accepts 512-sample windows at 16 kHz.
pub const VAD_WINDOW: usize = 512;

pub struct Capture {
    // stream: cpal::Stream,
    // ring: ringbuf::HeapProducer<f32>,
}

impl Capture {
    /// Opens the input stream. Device start costs 20–80 ms depending on platform, which is
    /// why pre-roll exists as an option (docs/LATENCY.md).
    pub fn start(_device: &str) -> anyhow::Result<Self> {
        todo!()
    }

    /// Stops capture and returns 16 kHz mono f32, VAD-trimmed. Returns None when no speech
    /// was detected, so an accidental hold produces nothing rather than a hallucinated line.
    pub fn finish(self) -> anyhow::Result<Option<Vec<f32>>> {
        todo!()
    }
}

pub fn devices() -> anyhow::Result<Vec<(String, String)>> {
    todo!("(id, display name), default first")
}
