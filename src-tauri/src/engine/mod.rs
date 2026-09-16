//! Speech engines.
//!
//! One trait, three implementations. The engine lives on its own thread. Weights are
//! memory-mapped and unloaded when idle, with a predictive preload before the user reaches for
//! the key — see `residency` and docs/FOOTPRINT.md. On macOS 26+ the default is Apple's
//! SpeechAnalyzer, which has no weights of ours at all (docs/adr/0013).

use std::sync::Arc;

use parking_lot::RwLock;

pub mod residency;

#[cfg(feature = "engine-parakeet")]
pub mod parakeet;
#[cfg(all(target_os = "macos", feature = "engine-speechanalyzer"))]
pub mod speechanalyzer;
#[cfg(feature = "engine-whisper")]
pub mod whisper;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("model not loaded")]
    NotLoaded,
    #[error("model file failed verification: {0}")]
    Verification(String),
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    /// Model-reported confidence where available. Used for diagnostics only — never to
    /// silently discard a result, because a user who spoke and got nothing back has no way to
    /// tell the difference between "quiet" and "broken".
    pub confidence: Option<f32>,
    pub inference_ms: u32,
}

#[derive(Debug, Clone)]
pub enum LanguageHint {
    Auto,
    Fixed(String),
}

#[derive(Debug, Clone)]
pub struct EngineOptions {
    pub model_dir: std::path::PathBuf,
    pub device: Device,
}

#[derive(Debug, Clone, Copy)]
pub enum Device {
    Auto,
    Cpu,
    Gpu,
}

pub trait SpeechEngine: Send {
    fn id(&self) -> &str;
    fn load(&mut self, opts: &EngineOptions) -> Result<(), Error>;
    fn unload(&mut self);
    fn is_loaded(&self) -> bool;

    /// `pcm` is 16 kHz mono f32 in -1.0..=1.0, already trimmed by VAD.
    fn transcribe(&mut self, pcm: &[f32], hint: &LanguageHint) -> Result<Transcript, Error>;

    /// Learned vocabulary passed as a recognition hint, where the backend supports it. This
    /// fixes the error rather than patching it afterwards; engines that don't support biasing
    /// return false and the terms are applied as post-recognition replacement instead.
    fn set_vocabulary(&mut self, _terms: &[String]) -> bool {
        false
    }
}

/// Owns the engine thread. A panic inside inference restarts the thread rather than taking
/// down the app, and surfaces one error rather than one per attempt.
pub struct Handle {
    // tx: crossbeam_channel::Sender<Job>,
}

/// Resolves "auto": Apple SpeechAnalyzer on macOS 26+, Parakeet elsewhere, Whisper when the
/// user's language falls outside Parakeet's 25.
pub fn resolve_default() -> &'static str {
    todo!()
}

pub fn spawn(_settings: Arc<RwLock<crate::settings::Settings>>) -> anyhow::Result<Handle> {
    todo!("resolve engine, mmap weights, spawn inference thread wrapped in residency::Residency")
}
