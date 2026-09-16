//! Settings: plain JSON at <app-config>/settings.json, mode 0600.
//!
//! Plain text on purpose — a privacy tool should let you read, diff and version-control your
//! own configuration. Schema and defaults are documented in docs/SETTINGS.md; that document
//! and this file must agree.

use serde::{Deserialize, Serialize};

pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    pub hotkey: Hotkey,
    pub audio: Audio,
    pub engine: Engine,
    pub learning: Learning,
    pub long_form: LongForm,
    pub output: Output,
    pub history: History,
    pub network: Network,
    pub ui: Ui,
    /// Unknown keys are preserved so a downgrade doesn't destroy newer settings.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotkey {
    pub keys: Vec<String>,
    pub mode: crate::hotkey::Mode,
    pub min_hold_ms: u32,
    /// Swallow the key system-wide. macOS/Windows only; off by default because a tray app
    /// silently eating a modifier is worse behaviour than the AltGr overlap.
    pub consume: bool,
    pub cancel_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Audio {
    pub input_device: String,
    /// "off" | "300ms". Pre-roll keeps the mic stream open continuously, which recovers the
    /// user's first syllable but means an open microphone whenever the app runs. Off by
    /// default; when on, the tray shows a distinct always-listening state.
    pub preroll: String,
    pub max_recording_sec: u32,
    pub vad: Vad,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vad {
    pub enabled: bool,
    pub trim_silence: bool,
    pub min_speech_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Engine {
    /// "auto" resolves to Apple SpeechAnalyzer on macOS 26+, Parakeet elsewhere.
    pub model_id: String,
    pub device: String,
    pub language: String,
    // Deliberately no residency fields. The system unloads when idle and reloads
    // predictively — see engine::residency and docs/adr/0010. Users should not be asked to
    // trade memory against speed.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Learning {
    /// On by default: local, kilobytes, and the corpus takes months to accumulate, so every
    /// week without capture is signal that cannot be recovered later.
    pub capture_corrections: bool,
    /// Off by default for the first year. A system that learns silently can be confidently
    /// wrong; earn the default with real data. See docs/LEARNING.md#rollout.
    pub apply_learned_terms: bool,
    pub min_occurrences: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongForm {
    /// Pressed while the hotkey is held to lock a session.
    pub lock_key: String,
    pub max_session_min: u32,
    pub default_destination: crate::longform::Destination,
    pub file_directory: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    pub method: String,
    pub restore_clipboard: bool,
    pub trailing_space: bool,
    pub capitalize_first: bool,
    pub collapse_newlines_in_terminals: bool,
    pub on_focus_change: OnFocusChange,
    pub dictionary: Vec<Replacement>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OnFocusChange {
    /// Default. Typing into whatever the user switched to is worse than not typing.
    Clipboard,
    InsertAnyway,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    pub enabled: bool,
    pub max_items: u32,
    pub max_days: u32,
    pub panel_hotkey: Option<String>,
    pub panic_wipe_hotkey: Option<String>,
    pub store_audio_for_debug: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    /// "startup" | "manual" | "off"
    pub update_check: String,
    /// When true the HTTP client is never constructed. Enforced at the lowest practical level
    /// and asserted by tests/offline_lock.rs.
    pub offline_lock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ui {
    pub theme: String,
    pub sound_cues: bool,
    pub level_overlay: bool,
    pub launch_at_login: bool,
    pub language: String,
}

impl Settings {
    pub fn load_or_default() -> anyhow::Result<Self> {
        todo!("read, migrate from `version`, validate, write back with mode 0600")
    }

    pub fn save(&self) -> anyhow::Result<()> {
        todo!("atomic write: temp file in the same directory, fsync, rename, chmod 0600")
    }
}

/// Each migration is a pure function with a test covering every version pair.
fn migrate(_value: serde_json::Value, _from: u32) -> anyhow::Result<serde_json::Value> {
    todo!()
}

/// Keyed to the bundle identifier, never to the product name. A rebrand must not strand
/// anyone's history, vocabulary or settings in an orphaned directory — see docs/adr/0009.
pub const APP_QUALIFIER: &str = "com";
pub const APP_ORG: &str = "pixelsmashing";
pub const APP_NAME_STABLE: &str = "dictation";

pub fn data_dir() -> anyhow::Result<std::path::PathBuf> {
    todo!("directories::ProjectDirs::from(APP_QUALIFIER, APP_ORG, APP_NAME_STABLE), mode 0700")
}
