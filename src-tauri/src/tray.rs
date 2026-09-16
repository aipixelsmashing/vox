//! Tray icon and menu. Four states, distinguishable by silhouette and not colour alone, plus
//! a distinct always-listening variant when pre-roll is enabled — an open microphone must
//! never be invisible. See docs/UI-SPEC.md.

use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Idle,
    Recording,
    Transcribing,
    Attention,
    AlwaysListening,
}

pub fn install(_app: &tauri::App, _pipeline: Arc<crate::pipeline::Handle>) -> anyhow::Result<()> {
    // Menu: Show history · Settings… | Pause dictation · Check for updates… | Quit
    // The icon must change within one frame of key-down regardless of pipeline state:
    // feedback that lags makes a fast system feel slow.
    todo!()
}

pub fn set_state(_app: &tauri::AppHandle, _state: IconState) {
    todo!()
}

pub fn show_history(_app: &tauri::AppHandle) {
    todo!("create the panel window lazily, anchored near the tray icon")
}
