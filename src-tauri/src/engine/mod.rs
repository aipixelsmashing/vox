//! Speech engines.
//!
//! One trait, one implementation in v1: Apple SpeechAnalyzer (docs/adr/0013, 0016). The engine
//! lives on its own thread. Parakeet and Whisper are designed for M8 and cfg'd out.

use std::sync::Arc;

use parking_lot::RwLock;

pub mod residency;

#[cfg(feature = "engine-parakeet")]
pub mod parakeet;
#[cfg(all(target_os = "macos", feature = "engine-speechanalyzer"))]
pub mod speechanalyzer;
#[cfg(feature = "engine-whisper")]
pub mod whisper;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("model not loaded")]
    NotLoaded,
    #[error("engine unavailable: {0}")]
    Unavailable(String),
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
    /// silently discard a result.
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
    /// "auto" or a locale identifier.
    pub language: String,
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

    /// `pcm` is 16 kHz mono f32 in -1.0..=1.0, already trimmed.
    fn transcribe(&mut self, pcm: &[f32], hint: &LanguageHint) -> Result<Transcript, Error>;

    /// Learned vocabulary passed as a recognition hint, where the backend supports it.
    fn set_vocabulary(&mut self, _terms: &[String]) -> bool {
        false
    }
}

enum Job {
    Transcribe {
        pcm: Vec<f32>,
        hint: LanguageHint,
        reply: crossbeam_channel::Sender<Result<Transcript, Error>>,
    },
    Status {
        reply: crossbeam_channel::Sender<Result<String, Error>>,
    },
}

/// Owns the engine thread. Requests are serialised; a transcription in flight blocks the
/// next, which is what the pipeline wants (overlapping dictations are never queued).
#[derive(Clone)]
pub struct Handle {
    tx: crossbeam_channel::Sender<Job>,
}

impl Handle {
    pub fn transcribe(&self, pcm: Vec<f32>, hint: LanguageHint) -> Result<Transcript, Error> {
        let (reply, rx) = crossbeam_channel::bounded(1);
        self.tx
            .send(Job::Transcribe { pcm, hint, reply })
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?;
        rx.recv()
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?
    }

    /// Ok(engine id) when loaded; the load error otherwise.
    pub fn status(&self) -> Result<String, Error> {
        let (reply, rx) = crossbeam_channel::bounded(1);
        self.tx
            .send(Job::Status { reply })
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?;
        rx.recv()
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?
    }
}

/// Resolves "auto": Apple SpeechAnalyzer on macOS 26+. Nothing else exists in v1.
pub fn resolve_default() -> Result<&'static str, Error> {
    #[cfg(all(target_os = "macos", feature = "engine-speechanalyzer"))]
    {
        if speechanalyzer::SpeechAnalyzerEngine::available() {
            return Ok("speechanalyzer");
        }
        return Err(Error::Unavailable(
            "Apple SpeechAnalyzer needs macOS 26 or later".into(),
        ));
    }
    #[allow(unreachable_code)]
    Err(Error::Unavailable(
        "no speech engine built for this platform".into(),
    ))
}

fn build(id: &str) -> Result<Box<dyn SpeechEngine>, Error> {
    match id {
        #[cfg(all(target_os = "macos", feature = "engine-speechanalyzer"))]
        "speechanalyzer" => Ok(Box::new(speechanalyzer::SpeechAnalyzerEngine::new())),
        other => Err(Error::Unavailable(format!("unknown engine {other}"))),
    }
}

pub fn spawn(settings: Arc<RwLock<crate::settings::Settings>>) -> anyhow::Result<Handle> {
    let (tx, rx) = crossbeam_channel::bounded::<Job>(4);
    let cfg = settings.read().engine.clone();
    std::thread::Builder::new()
        .name("vox-inference".into())
        .spawn(move || {
            let id = if cfg.model_id == "auto" {
                resolve_default().map(str::to_string)
            } else {
                Ok(cfg.model_id.clone())
            };
            let opts = EngineOptions {
                model_dir: crate::settings::data_dir()
                    .map(|d| d.join("models"))
                    .unwrap_or_default(),
                device: Device::Auto,
                language: cfg.language.clone(),
            };
            let mut state: Result<Box<dyn SpeechEngine>, Error> = id.and_then(|id| {
                let mut e = build(&id)?;
                let t = std::time::Instant::now();
                e.load(&opts)?;
                tracing::info!("engine {} loaded in {} ms", e.id(), t.elapsed().as_millis());
                Ok(e)
            });
            if let Err(e) = &state {
                tracing::warn!("engine unavailable: {e}");
            }
            for job in rx {
                match job {
                    Job::Status { reply } => {
                        let _ = reply.send(match &state {
                            Ok(e) => Ok(e.id().to_string()),
                            Err(e) => Err(e.clone()),
                        });
                    }
                    Job::Transcribe { pcm, hint, reply } => {
                        let result = match &mut state {
                            Ok(e) => e.transcribe(&pcm, &hint),
                            Err(e) => Err(e.clone()),
                        };
                        let _ = reply.send(result);
                    }
                }
            }
        })?;
    Ok(Handle { tx })
}
