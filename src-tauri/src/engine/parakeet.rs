//! Parakeet TDT via ONNX Runtime (`parakeet-rs`). Default engine — see docs/adr/0004.
//!
//! Execution provider: CoreML is documented as unstable for this model, so Apple platforms
//! use WebGPU or CPU; CUDA where available elsewhere; DirectML as a Windows fallback. The
//! provider actually in use is shown in Settings so a slow machine can be diagnosed.

use super::{EngineOptions, Error, LanguageHint, SpeechEngine, Transcript};

pub struct ParakeetEngine {
    // model: Option<parakeet_rs::ParakeetASR>,
}

impl SpeechEngine for ParakeetEngine {
    fn id(&self) -> &str {
        "parakeet"
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
        todo!()
    }
}
