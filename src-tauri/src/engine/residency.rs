//! Adaptive residency. See docs/FOOTPRINT.md and docs/adr/0010.
//!
//! A warm model costs ~750 MB for the 99% of the day nobody is speaking. Idle resident memory
//! is what decides whether a tray app survives its first look in Activity Monitor.
//!
//! There is deliberately **no setting** for this. Asking users to trade memory against speed
//! is asking them to make an engineering judgement they have no basis for.

use std::time::Duration;

pub const IDLE_UNLOAD_AFTER: Duration = Duration::from_secs(10 * 60);
/// Below this, the prediction heuristic is wrong and gets fixed — not exposed as a toggle.
pub const MIN_PREDICTION_HIT_RATE: f32 = 0.80;

/// Signals that trigger a predictive preload. All are already available; none require new
/// permissions or new observation surfaces.
#[derive(Debug, Clone, Copy)]
pub enum PreloadSignal {
    /// An editable field gained focus in an app the user has dictated into before.
    EditableFocusInKnownApp,
    /// Time of day matches this user's dictation pattern.
    HabitualWindow,
    /// First key of a hotkey chord went down — the strongest and latest signal.
    HotkeyPrefix,
}

pub struct Residency {
    // engine: Box<dyn super::SpeechEngine>, last_used: Instant, stats: PredictionStats
}

impl Residency {
    /// Weights are memory-mapped: RSS is file-backed rather than anonymous, the OS can evict
    /// under pressure instead of us being blamed for a hard 750 MB, and the *second* load
    /// costs a few hundred ms from page cache rather than 0.5–2 s cold. mmap and unloading
    /// only work as a pair.
    pub fn load_mmap(&mut self) -> anyhow::Result<()> {
        todo!()
    }

    pub fn on_signal(&mut self, _signal: PreloadSignal) {
        todo!("preload if unloaded and the signal clears the confidence bar")
    }

    pub fn tick(&mut self) {
        todo!("unload after IDLE_UNLOAD_AFTER")
    }

    /// Local only, shown in Diagnostics. Hit rate and p95 wait when prediction misses.
    pub fn stats(&self) -> PredictionStats {
        todo!()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PredictionStats {
    pub hit_rate: f32,
    pub p95_miss_wait_ms: u32,
}
