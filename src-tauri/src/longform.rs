//! Long-form sessions: thinking out loud for minutes. See docs/LONG-FORM.md.
//!
//! A different job from short-form, not a longer timeout. Output goes to a destination the
//! user chooses, not into the focused field, and silence never ends the session — long pauses
//! are what thinking sounds like.

use std::time::Duration;

pub const DEFAULT_MAX_SESSION: Duration = Duration::from_secs(30 * 60);
/// Transcription window. Audio is released after each window, so a 30-minute session never
/// holds 30 minutes of audio — peak memory stays close to a short dictation.
pub const CHUNK: Duration = Duration::from_secs(30);
pub const CHUNK_OVERLAP: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Destination {
    Clipboard,
    /// Plain .md in a user-chosen directory, auto-titled from the first sentence.
    NewFile,
    InsertAtCursor,
    AppendToFile,
}

pub struct Session {
    // chunks: Vec<String>, started: Instant, paused: bool
}

impl Session {
    /// Entered from Recording when the lock key is pressed while the hotkey is held. No second
    /// hotkey to learn, and users who never lock the key see no change.
    pub fn start() -> anyhow::Result<Self> {
        todo!()
    }

    /// Chunked transcription is enough here — this does not wait for the streaming engine.
    pub fn on_chunk(&mut self, _pcm: &[f32]) -> anyhow::Result<String> {
        todo!("transcribe with overlap, reconcile the seam, emit text to the panel, drop audio")
    }

    pub fn pause(&mut self) {
        todo!()
    }

    pub fn finish(self, _dest: Destination) -> anyhow::Result<()> {
        todo!()
    }
}

/// Deterministic: the first sentence, trimmed. No model call, so it is instant and wrong in an
/// obvious way rather than a confident one.
pub fn auto_title(_text: &str) -> String {
    todo!()
}
