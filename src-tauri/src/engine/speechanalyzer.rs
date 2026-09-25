//! Apple SpeechAnalyzer — the only engine in v1. See docs/adr/0013 and docs/spikes/s3-engine.md.
//!
//! Zero model download of ours, no weights of ours resident, Neural Engine execution. The
//! framework is Swift-only, so the actual calls live in ../../swift/SpeechAnalyzerBridge.swift,
//! compiled by build.rs and reached through the C ABI below.

#![cfg(target_os = "macos")]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use super::{EngineOptions, Error, LanguageHint, SpeechEngine, Transcript};

extern "C" {
    fn vox_sa_available() -> bool;
    fn vox_sa_prepare(locale: *const c_char) -> *mut c_char;
    fn vox_sa_transcribe(
        pcm: *const f32,
        len: usize,
        sample_rate: f64,
        locale: *const c_char,
    ) -> *mut c_char;
    fn vox_sa_free(p: *mut c_char);
    fn vox_sa_stream_start(
        locale: *const c_char,
        sample_rate: f64,
        context: *const c_char,
    ) -> *mut c_char;
    fn vox_sa_prepare_dictation();
    /// Only the ignored engine test polls this; the app reads readiness from the bridge's
    /// stream-start report instead.
    #[allow(dead_code)]
    fn vox_sa_dictation_ready() -> bool;
    fn vox_sa_stream_push(handle: i32, pcm: *const f32, len: usize) -> bool;
    fn vox_sa_stream_finish(handle: i32) -> *mut c_char;
    fn vox_sa_stream_cancel(handle: i32);
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamStartReport {
    ok: bool,
    handle: i32,
    /// "speech" or "dictation": which module the session runs on.
    module: Option<String>,
    /// True when hints were supplied but the dictation module was not ready, so the
    /// session runs without them.
    #[serde(default)]
    hints_dropped: bool,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamFinishReport {
    text: String,
    ms: f64,
    finals: usize,
    volatiles: usize,
    /// True when the text ends with a result the module never finalised.
    #[serde(default)]
    used_volatile_tail: bool,
    pushed_seconds: f64,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrepareReport {
    ok: bool,
    locale: Option<String>,
    asset_status: Option<String>,
    download_ms: Option<f64>,
    warmup_ms: Option<f64>,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
struct TranscribeReport {
    text: String,
    ms: f64,
    error: Option<String>,
}

/// Calls a bridge function that returns a strdup'd JSON string and frees it.
fn call_json<T: serde::de::DeserializeOwned>(f: impl FnOnce() -> *mut c_char) -> Result<T, Error> {
    let p = f();
    if p.is_null() {
        return Err(Error::Inference("bridge returned null".into()));
    }
    // SAFETY: the bridge returns a NUL-terminated string from strdup; we free it exactly once.
    let raw = unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() };
    unsafe { vox_sa_free(p) };
    serde_json::from_str(&raw).map_err(|e| Error::Inference(format!("bridge JSON: {e}")))
}

pub struct SpeechAnalyzerEngine {
    loaded: bool,
    locale: String,
    /// Live streaming session handle in the bridge, if a dictation is in progress.
    stream: Option<i32>,
}

impl SpeechAnalyzerEngine {
    pub fn new() -> Self {
        Self {
            loaded: false,
            locale: "auto".into(),
            stream: None,
        }
    }

    /// Runtime check — never a compile-time assumption. False below macOS 26.
    pub fn available() -> bool {
        // SAFETY: no preconditions.
        unsafe { vox_sa_available() }
    }
}

impl Default for SpeechAnalyzerEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeechEngine for SpeechAnalyzerEngine {
    fn id(&self) -> &str {
        "speechanalyzer"
    }

    fn load(&mut self, opts: &EngineOptions) -> Result<(), Error> {
        if !Self::available() {
            return Err(Error::Unavailable(
                "Apple SpeechAnalyzer needs macOS 26 or later".into(),
            ));
        }
        let locale = CString::new(opts.language.clone()).unwrap_or_default();
        // SAFETY: valid C string for the duration of the call; the bridge copies it.
        let rep: PrepareReport = call_json(|| unsafe { vox_sa_prepare(locale.as_ptr()) })?;
        if !rep.ok {
            return Err(Error::Unavailable(
                rep.error.unwrap_or_else(|| "prepare failed".into()),
            ));
        }
        tracing::info!(
            "speechanalyzer ready: locale {:?}, assets {:?}, download {:?} ms, warm-up {:?} ms",
            rep.locale,
            rep.asset_status,
            rep.download_ms,
            rep.warmup_ms
        );
        self.locale = rep.locale.unwrap_or_else(|| opts.language.clone());
        self.loaded = true;
        Ok(())
    }

    fn unload(&mut self) {
        // Nothing of ours to release; Apple's service follows modelRetention.
        self.loaded = false;
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn transcribe(&mut self, pcm: &[f32], hint: &LanguageHint) -> Result<Transcript, Error> {
        if !self.loaded {
            return Err(Error::NotLoaded);
        }
        let locale = match hint {
            LanguageHint::Fixed(l) => l.clone(),
            LanguageHint::Auto => self.locale.clone(),
        };
        let c_locale = CString::new(locale.clone()).unwrap_or_default();
        // SAFETY: pcm is valid for len samples for the duration of the call; the bridge
        // copies it before returning.
        let rep: TranscribeReport = call_json(|| unsafe {
            vox_sa_transcribe(
                pcm.as_ptr(),
                pcm.len(),
                f64::from(crate::audio::TARGET_SAMPLE_RATE),
                c_locale.as_ptr(),
            )
        })?;
        if let Some(e) = rep.error {
            return Err(Error::Inference(e));
        }
        Ok(Transcript {
            text: rep.text,
            language: Some(locale),
            confidence: None,
            inference_ms: rep.ms as u32,
        })
    }

    fn stream_start(&mut self, hint: &LanguageHint, context: &[String]) -> Result<(), Error> {
        if !self.loaded {
            return Err(Error::NotLoaded);
        }
        if let Some(h) = self.stream.take() {
            // A session left over from a cancelled or failed dictation.
            unsafe { vox_sa_stream_cancel(h) };
        }
        let locale = match hint {
            LanguageHint::Fixed(l) => l.clone(),
            LanguageHint::Auto => self.locale.clone(),
        };
        let c_locale = CString::new(locale).unwrap_or_default();
        // One term per line. Never logged here or in the bridge (tests/guards.rs).
        let joined: String = context
            .iter()
            .map(|t| t.replace(['\n', '\0'], " "))
            .collect::<Vec<_>>()
            .join("\n");
        let c_context = CString::new(joined).unwrap_or_default();
        // SAFETY: valid C strings for the call; the bridge copies them.
        let rep: StreamStartReport = call_json(|| unsafe {
            vox_sa_stream_start(
                c_locale.as_ptr(),
                f64::from(crate::audio::TARGET_SAMPLE_RATE),
                c_context.as_ptr(),
            )
        })?;
        if !rep.ok {
            return Err(Error::Inference(
                rep.error.unwrap_or_else(|| "stream start failed".into()),
            ));
        }
        if rep.hints_dropped {
            tracing::warn!(
                "stream start: {} hints dropped, the dictation module is not ready yet",
                context.len()
            );
        } else if !context.is_empty() {
            tracing::info!(
                "stream start: {} module with {} hints",
                rep.module.as_deref().unwrap_or("?"),
                context.len()
            );
        }
        self.stream = Some(rep.handle);
        Ok(())
    }

    fn prepare_context(&mut self) {
        if !self.loaded {
            return;
        }
        // SAFETY: no preconditions; the bridge does the work on its own task.
        unsafe { vox_sa_prepare_dictation() };
    }

    fn stream_push(&mut self, pcm: &[f32]) -> Result<(), Error> {
        let Some(h) = self.stream else {
            return Err(Error::Inference("no stream".into()));
        };
        if pcm.is_empty() {
            return Ok(());
        }
        // SAFETY: pcm is valid for len samples for the call; the bridge copies it.
        if unsafe { vox_sa_stream_push(h, pcm.as_ptr(), pcm.len()) } {
            Ok(())
        } else {
            Err(Error::Inference("stream push rejected".into()))
        }
    }

    fn stream_finish(&mut self) -> Result<Transcript, Error> {
        let Some(h) = self.stream.take() else {
            return Err(Error::Inference("no stream".into()));
        };
        // SAFETY: handle came from the bridge and is consumed exactly once.
        let rep: StreamFinishReport = call_json(|| unsafe { vox_sa_stream_finish(h) })?;
        if let Some(e) = rep.error {
            return Err(Error::Inference(e));
        }
        tracing::info!(
            "stream finish: {:.0} ms for {:.2} s pushed, {} final / {} volatile results{}",
            rep.ms,
            rep.pushed_seconds,
            rep.finals,
            rep.volatiles,
            if rep.used_volatile_tail {
                ", volatile tail used"
            } else {
                ""
            }
        );
        Ok(Transcript {
            text: rep.text,
            language: Some(self.locale.clone()),
            confidence: None,
            inference_ms: rep.ms as u32,
        })
    }

    fn stream_cancel(&mut self) {
        if let Some(h) = self.stream.take() {
            // SAFETY: handle came from the bridge and is consumed exactly once.
            unsafe { vox_sa_stream_cancel(h) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Streams the S5 fixture with its five terms as hints through the dictation module,
    /// which is the path a real dictation with `privacy.readFocusedField` on takes. Needs
    /// macOS 26, Apple's assets for both modules and the fixture, so it does not run in
    /// CI: `cargo test -- --ignored streams_with_hints`.
    #[test]
    #[ignore = "needs macOS 26, Apple's speech assets and fixtures/audio/context-6s.wav"]
    fn streams_with_hints_through_the_dictation_module() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fixtures/audio/context-6s.wav"
        );
        // So the bridge's "stream finish: … final / volatile results" line is visible
        // under --nocapture; it answers whether the dictation module finalises when streamed.
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::INFO)
            .try_init();
        let mut reader = hound::WavReader::open(path).expect("fixture");
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, crate::audio::TARGET_SAMPLE_RATE);
        let pcm: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect();

        let mut e = SpeechAnalyzerEngine::new();
        e.load(&EngineOptions {
            model_dir: std::env::temp_dir(),
            device: super::super::Device::Auto,
            language: "en-US".into(),
        })
        .expect("engine loads");
        e.prepare_context();
        let t0 = std::time::Instant::now();
        while !unsafe { vox_sa_dictation_ready() } {
            assert!(
                t0.elapsed() < Duration::from_secs(120),
                "dictation module never became ready"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        let hints: Vec<String> = [
            "Orsolya Csernák",
            "Kubestrix",
            "keytap",
            "axuielement",
            "Tauri",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let run = |e: &mut SpeechAnalyzerEngine, hints: &[String]| -> Transcript {
            e.stream_start(&LanguageHint::Auto, hints)
                .expect("stream starts");
            for chunk in pcm.chunks(320) {
                e.stream_push(chunk).expect("push");
                std::thread::sleep(Duration::from_millis(2));
            }
            e.stream_finish().expect("finish")
        };
        let with = run(&mut e, &hints);
        let without = run(&mut e, &[]);
        eprintln!("without hints: {:?}", without.text);
        eprintln!("with hints:    {:?}", with.text);
        assert!(
            !with.text.trim().is_empty(),
            "the dictation module produced no text"
        );
        assert!(!without.text.trim().is_empty());
        let lower = with.text.to_lowercase();
        assert!(
            hints.iter().any(|h| lower.contains(&h.to_lowercase())),
            "no hinted term recovered: {}",
            with.text
        );
    }

    use std::time::Duration;
}
