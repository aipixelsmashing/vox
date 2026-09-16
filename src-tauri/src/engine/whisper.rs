//! Whisper via whisper.cpp (`whisper-rs`).
//!
//! Not a legacy fallback: Parakeet v3 covers 25 languages, Whisper around 99. For a user
//! dictating in Japanese or Arabic this is the only path. Also the low-RAM option.

use super::{EngineOptions, Error, LanguageHint, SpeechEngine, Transcript};

pub struct WhisperEngine {
    // ctx: Option<whisper_rs::WhisperContext>,
}

impl SpeechEngine for WhisperEngine {
    fn id(&self) -> &str {
        "whisper"
    }
    fn load(&mut self, _opts: &EngineOptions) -> Result<(), Error> {
        todo!()
    }
    fn unload(&mut self) {
        todo!()
    }
    fn is_loaded(&self) -> bool {
        todo!()
    }
    fn transcribe(&mut self, _pcm: &[f32], _hint: &LanguageHint) -> Result<Transcript, Error> {
        // Suppress Whisper's known hallucinations on silence ("Thank you.", subtitle credits)
        // by rejecting results from segments the VAD marked as non-speech.
        todo!()
    }
}
