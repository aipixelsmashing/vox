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
}

impl SpeechAnalyzerEngine {
    pub fn new() -> Self {
        Self {
            loaded: false,
            locale: "auto".into(),
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
}
