//! Apple SpeechAnalyzer — the default engine on macOS 26+. See docs/adr/0013.
//!
//! Zero model download, no weights of ours resident, Neural Engine execution. This is the
//! footprint story on Mac solved outright: idle memory in the tens of megabytes rather than
//! hundreds, and the 700 MB first-run download removed.
//!
//! We keep everything Apple's own dictation doesn't give the user: hold-to-talk on any key, no
//! session cutoff, transcript history, learned vocabulary, long-form sessions, and identical
//! behaviour on their other machines.

#![cfg(target_os = "macos")]

use super::{EngineOptions, Error, LanguageHint, SpeechEngine, Transcript};

pub struct SpeechAnalyzerEngine {
    // analyzer: Retained<SFSpeechAnalyzer>
}

impl SpeechAnalyzerEngine {
    /// macOS 26.0. Below that the registry falls back to Parakeet.
    pub fn available() -> bool {
        todo!("runtime OS version check — never a compile-time assumption")
    }
}

impl SpeechEngine for SpeechAnalyzerEngine {
    fn id(&self) -> &str {
        "speechanalyzer"
    }
    fn load(&mut self, _opts: &EngineOptions) -> Result<(), Error> {
        todo!()
    }
    fn unload(&mut self) {
        // Cheap: there are no weights of ours to release.
        todo!()
    }
    fn is_loaded(&self) -> bool {
        todo!()
    }
    fn transcribe(&mut self, _pcm: &[f32], _hint: &LanguageHint) -> Result<Transcript, Error> {
        // Pass learned vocabulary as a contextual hint where the framework accepts one, so the
        // user's own terms are fixed at recognition rather than patched afterwards.
        todo!()
    }
}
