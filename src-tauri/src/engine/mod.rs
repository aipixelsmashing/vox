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

    /// `pcm` is 16 kHz mono f32 in -1.0..=1.0, already trimmed. `context` as for
    /// `stream_start`: the hints for this one pass, never kept.
    fn transcribe(
        &mut self,
        pcm: &[f32],
        hint: &LanguageHint,
        context: &[String],
    ) -> Result<Transcript, Error>;

    /// Learned vocabulary passed as a recognition hint, where the backend supports it.
    fn set_vocabulary(&mut self, _terms: &[String]) -> bool {
        false
    }

    // Streaming: audio is pushed while the key is held so only the tail is left to
    // finalise at release. Engines that cannot stream keep the defaults and the pipeline
    // falls back to `transcribe` with the whole clip. `context` is the recognition hints
    // for this one session (docs/CONTEXT.md): at most twenty terms, never stored by the
    // engine, never logged.
    fn stream_start(&mut self, _hint: &LanguageHint, _context: &[String]) -> Result<(), Error> {
        Err(Error::Unavailable("streaming not supported".into()))
    }
    /// Get whatever the hinted path needs ready, off the critical path. Called when
    /// either source of hints is on (the focused field, learned terms); a no-op for
    /// engines whose hints need nothing.
    fn prepare_context(&mut self) {}
    fn stream_push(&mut self, _pcm: &[f32]) -> Result<(), Error> {
        Err(Error::Unavailable("streaming not supported".into()))
    }
    fn stream_finish(&mut self) -> Result<Transcript, Error> {
        Err(Error::Unavailable("streaming not supported".into()))
    }
    fn stream_cancel(&mut self) {}
}

enum Job {
    Transcribe {
        pcm: Vec<f32>,
        hint: LanguageHint,
        context: Vec<String>,
        reply: crossbeam_channel::Sender<Result<Transcript, Error>>,
    },
    Status {
        reply: crossbeam_channel::Sender<Result<String, Error>>,
    },
    StreamStart {
        hint: LanguageHint,
        context: Vec<String>,
        reply: crossbeam_channel::Sender<Result<(), Error>>,
    },
    PrepareContext,
    StreamPush {
        pcm: Vec<f32>,
    },
    StreamFinish {
        reply: crossbeam_channel::Sender<Result<Transcript, Error>>,
    },
    StreamCancel,
}

/// Owns the engine thread. Requests are serialised; a transcription in flight blocks the
/// next, which is what the pipeline wants (overlapping dictations are never queued).
#[derive(Clone)]
pub struct Handle {
    tx: crossbeam_channel::Sender<Job>,
}

impl Handle {
    pub fn transcribe(
        &self,
        pcm: Vec<f32>,
        hint: LanguageHint,
        context: Vec<String>,
    ) -> Result<Transcript, Error> {
        let (reply, rx) = crossbeam_channel::bounded(1);
        self.tx
            .send(Job::Transcribe {
                pcm,
                hint,
                context,
                reply,
            })
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?;
        rx.recv()
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?
    }

    pub fn stream_start(&self, hint: LanguageHint, context: Vec<String>) -> Result<(), Error> {
        let (reply, rx) = crossbeam_channel::bounded(1);
        self.tx
            .send(Job::StreamStart {
                hint,
                context,
                reply,
            })
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?;
        rx.recv()
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?
    }

    /// Blocks only if the engine thread is far behind, which it never is: pushes are cheap.
    pub fn stream_push(&self, pcm: Vec<f32>) {
        let _ = self.tx.send(Job::StreamPush { pcm });
    }

    pub fn stream_finish(&self) -> Result<Transcript, Error> {
        let (reply, rx) = crossbeam_channel::bounded(1);
        self.tx
            .send(Job::StreamFinish { reply })
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?;
        rx.recv()
            .map_err(|_| Error::Unavailable("engine thread gone".into()))?
    }

    pub fn stream_cancel(&self) {
        let _ = self.tx.send(Job::StreamCancel);
    }

    /// Fire-and-forget: the engine readies its hinted path in the background.
    pub fn prepare_context(&self) {
        let _ = self.tx.send(Job::PrepareContext);
    }

    /// A handle nobody answers, for tests of code that only needs to hold one.
    #[doc(hidden)]
    pub fn disconnected() -> Self {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        Self { tx }
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
    let (tx, rx) = crossbeam_channel::bounded::<Job>(256);
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
                    Job::Transcribe {
                        pcm,
                        hint,
                        context,
                        reply,
                    } => {
                        let result = match &mut state {
                            Ok(e) => e.transcribe(&pcm, &hint, &context),
                            Err(e) => Err(e.clone()),
                        };
                        let _ = reply.send(result);
                    }
                    Job::StreamStart {
                        hint,
                        context,
                        reply,
                    } => {
                        let result = match &mut state {
                            Ok(e) => e.stream_start(&hint, &context),
                            Err(e) => Err(e.clone()),
                        };
                        let _ = reply.send(result);
                    }
                    Job::PrepareContext => {
                        if let Ok(e) = &mut state {
                            e.prepare_context();
                        }
                    }
                    Job::StreamPush { pcm } => {
                        if let Ok(e) = &mut state {
                            if let Err(err) = e.stream_push(&pcm) {
                                tracing::warn!("stream push failed: {err}");
                            }
                        }
                    }
                    Job::StreamFinish { reply } => {
                        let result = match &mut state {
                            Ok(e) => e.stream_finish(),
                            Err(e) => Err(e.clone()),
                        };
                        let _ = reply.send(result);
                    }
                    Job::StreamCancel => {
                        if let Ok(e) = &mut state {
                            e.stream_cancel();
                        }
                    }
                }
            }
        })?;
    Ok(Handle { tx })
}
