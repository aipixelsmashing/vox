//! Settings: plain JSON at <app-config>/settings.json, mode 0600.
//!
//! Plain text on purpose — a privacy tool should let you read, diff and version-control your
//! own configuration. Schema and defaults are documented in docs/SETTINGS.md; that document
//! and this file must agree, and `tests` below pin the defaults to it.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const CURRENT_VERSION: u32 = 2;

fn current_version() -> u32 {
    CURRENT_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "current_version")]
    pub version: u32,
    #[serde(default)]
    pub hotkey: Hotkey,
    #[serde(default)]
    pub audio: Audio,
    #[serde(default)]
    pub engine: Engine,
    #[serde(default)]
    pub learning: Learning,
    #[serde(default)]
    pub long_form: LongForm,
    #[serde(default)]
    pub output: Output,
    #[serde(default)]
    pub privacy: Privacy,
    #[serde(default)]
    pub onboarding: Onboarding,
    #[serde(default)]
    pub history: History,
    #[serde(default)]
    pub network: Network,
    #[serde(default)]
    pub ui: Ui,
    #[serde(default)]
    pub advanced: Advanced,
    /// Unknown keys are preserved so a downgrade doesn't destroy newer settings.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Hotkey {
    pub keys: Vec<String>,
    pub mode: crate::hotkey::Mode,
    pub min_hold_ms: u32,
    /// Swallow the key system-wide. macOS/Windows only; off by default because a tray app
    /// silently eating a modifier is worse behaviour than the AltGr overlap.
    pub consume: bool,
    pub cancel_key: String,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            keys: vec!["AltRight".into()],
            mode: crate::hotkey::Mode::Hold,
            min_hold_ms: 120,
            consume: false,
            cancel_key: "Escape".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Audio {
    pub input_device: String,
    /// "off" | "300ms". Pre-roll keeps the mic stream open continuously, which recovers the
    /// user's first syllable but means an open microphone whenever the app runs. Off by
    /// default; when on, the tray shows a distinct always-listening state.
    pub preroll: String,
    pub max_recording_sec: u32,
    pub vad: Vad,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            input_device: "default".into(),
            preroll: "off".into(),
            max_recording_sec: 120,
            vad: Vad::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Vad {
    pub enabled: bool,
    pub trim_silence: bool,
    pub min_speech_ms: u32,
}

impl Default for Vad {
    fn default() -> Self {
        Self {
            enabled: true,
            trim_silence: true,
            min_speech_ms: 250,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Engine {
    /// "auto" resolves to Apple SpeechAnalyzer on macOS 26+, Parakeet elsewhere (M8).
    pub model_id: String,
    pub device: String,
    pub language: String,
    // Deliberately no residency fields — docs/adr/0010.
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            model_id: "auto".into(),
            device: "auto".into(),
            language: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Learning {
    /// On by default: local, kilobytes, and the corpus takes months to accumulate.
    pub capture_corrections: bool,
    /// Off by default for the first year. See docs/LEARNING.md#rollout.
    pub apply_learned_terms: bool,
    pub min_occurrences: u32,
}

impl Default for Learning {
    fn default() -> Self {
        Self {
            capture_corrections: true,
            apply_learned_terms: false,
            min_occurrences: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LongForm {
    /// Pressed while the hotkey is held to lock a session.
    pub lock_key: String,
    pub max_session_min: u32,
    pub default_destination: crate::longform::Destination,
    pub file_directory: Option<PathBuf>,
}

impl Default for LongForm {
    fn default() -> Self {
        Self {
            lock_key: "KeyL".into(),
            max_session_min: 30,
            default_destination: crate::longform::Destination::Clipboard,
            file_directory: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Output {
    /// auto | accessibility | paste | type
    pub method: String,
    pub restore_clipboard: bool,
    pub trailing_space: bool,
    pub capitalize_first: bool,
    pub collapse_newlines_in_terminals: bool,
    pub on_focus_change: OnFocusChange,
    pub dictionary: Vec<Replacement>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            method: "auto".into(),
            restore_clipboard: true,
            trailing_space: true,
            capitalize_first: false,
            collapse_newlines_in_terminals: true,
            on_focus_change: OnFocusChange::Clipboard,
            dictionary: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OnFocusChange {
    /// Default. Typing into whatever the user switched to is worse than not typing.
    Clipboard,
    InsertAnyway,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct History {
    pub enabled: bool,
    pub max_items: u32,
    pub max_days: u32,
    pub panel_hotkey: Option<String>,
    pub panic_wipe_hotkey: Option<String>,
    pub store_audio_for_debug: bool,
}

impl Default for History {
    fn default() -> Self {
        Self {
            enabled: true,
            max_items: 200,
            max_days: 30,
            panel_hotkey: Some("CmdOrCtrl+Shift+Space".into()),
            panic_wipe_hotkey: None,
            store_audio_for_debug: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Network {
    /// "startup" | "manual" | "off"
    pub update_check: String,
    /// When true the HTTP client is never constructed.
    pub offline_lock: bool,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            update_check: "startup".into(),
            offline_lock: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ui {
    pub theme: String,
    pub sound_cues: bool,
    pub level_overlay: bool,
    pub launch_at_login: bool,
    pub language: String,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            sound_cues: true,
            level_overlay: true,
            launch_at_login: true,
            language: "system".into(),
        }
    }
}

/// docs/CONTEXT.md, adr/0017. Off by default: Vox reading the user's documents is opt-in.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Privacy {
    pub read_focused_field: bool,
}

/// Where onboarding got to, so quitting at step three does not start over (docs/UI-SPEC.md).
/// State, not a preference: nothing here changes how Vox behaves.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Onboarding {
    pub completed_step: u32,
    pub done: bool,
}

/// Bumped on every accepted `settings_set`; threads that cache a binding (the hotkey thread)
/// compare it and rebuild, so changes apply without a restart.
pub static SETTINGS_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Advanced {
    pub log_level: String,
    pub diagnostics_panel: bool,
}

impl Default for Advanced {
    fn default() -> Self {
        Self {
            log_level: "warn".into(),
            diagnostics_panel: false,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            hotkey: Hotkey::default(),
            audio: Audio::default(),
            engine: Engine::default(),
            learning: Learning::default(),
            long_form: LongForm::default(),
            output: Output::default(),
            privacy: Privacy::default(),
            onboarding: Onboarding::default(),
            history: History::default(),
            network: Network::default(),
            ui: Ui::default(),
            advanced: Advanced::default(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Settings {
    pub fn path() -> anyhow::Result<PathBuf> {
        Ok(config_dir()?.join("settings.json"))
    }

    /// Reads, migrates from `version`, and writes the result back (mode 0600) so the file on
    /// disk always reflects the current schema with every default spelled out.
    pub fn load_or_default() -> anyhow::Result<Self> {
        let path = Self::path()?;
        let settings = match std::fs::read_to_string(&path) {
            Ok(raw) => Self::parse(&raw)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };
        settings.save()?;
        Ok(settings)
    }

    pub fn parse(raw: &str) -> anyhow::Result<Self> {
        let value: serde_json::Value = serde_json::from_str(raw)?;
        let from = value
            .get("version")
            .and_then(|v| v.as_u64())
            .unwrap_or(CURRENT_VERSION as u64) as u32;
        let value = migrate(value, from)?;
        let mut settings: Settings = serde_json::from_value(value)?;
        settings.version = CURRENT_VERSION;
        Ok(settings)
    }

    /// Atomic: temp file in the same directory, fsync, rename, mode 0600.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path()?;
        let dir = path.parent().expect("settings path has a parent");
        let tmp = dir.join(".settings.json.tmp");
        let json = serde_json::to_string_pretty(self)?;
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(json.as_bytes())?;
            f.write_all(b"\n")?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
}

/// Each migration is a pure function with a test covering every version pair. There is one
/// version so far.
fn migrate(value: serde_json::Value, from: u32) -> anyhow::Result<serde_json::Value> {
    match from {
        CURRENT_VERSION => Ok(value),
        1 => migrate(migrate_1_to_2(value), 2),
        v if v > CURRENT_VERSION => {
            // Newer file than this build. Read what we understand; unknown keys survive in
            // `extra` and are written back untouched.
            Ok(value)
        }
        v => anyhow::bail!("settings version {v} has no migration path"),
    }
}

/// v1 shipped the history panel on ⌘⇧V, which is "Paste and Match Style" in most macOS apps;
/// Vox observes the chord rather than swallowing it, so both fired. Files still on the old
/// default move to ⌘⇧Space; a hotkey the user chose themselves is left alone.
fn migrate_1_to_2(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(hk) = value.pointer_mut("/history/panelHotkey") {
        if hk.as_str() == Some("CmdOrCtrl+Shift+V") {
            *hk = serde_json::Value::String("CmdOrCtrl+Shift+Space".into());
        }
    }
    value["version"] = serde_json::Value::from(2);
    value
}

/// Keyed to the bundle identifier, never to the product name. A rebrand must not strand
/// anyone's history, vocabulary or settings in an orphaned directory — see docs/adr/0009.
pub const APP_QUALIFIER: &str = "com";
pub const APP_ORG: &str = "pixelsmashing";
pub const APP_NAME_STABLE: &str = "dictation";

fn project_dirs() -> anyhow::Result<directories::ProjectDirs> {
    directories::ProjectDirs::from(APP_QUALIFIER, APP_ORG, APP_NAME_STABLE)
        .ok_or_else(|| anyhow::anyhow!("no home directory"))
}

fn ensure_dir(dir: PathBuf) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(dir)
}

pub fn config_dir() -> anyhow::Result<PathBuf> {
    ensure_dir(project_dirs()?.config_dir().to_path_buf())
}

pub fn data_dir() -> anyhow::Result<PathBuf> {
    ensure_dir(project_dirs()?.data_dir().to_path_buf())
}

/// ~/Library/Logs/<bundle id>/ on macOS. Transcripts are never written here.
pub fn log_dir() -> anyhow::Result<PathBuf> {
    let base = directories::BaseDirs::new().ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    let dir = if cfg!(target_os = "macos") {
        base.home_dir()
            .join("Library")
            .join("Logs")
            .join(format!("{APP_QUALIFIER}.{APP_ORG}.{APP_NAME_STABLE}"))
    } else {
        project_dirs()?.data_dir().join("logs")
    };
    ensure_dir(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_settings_md() {
        let s = Settings::default();
        assert_eq!(s.version, 2);
        assert_eq!(
            s.history.panel_hotkey.as_deref(),
            Some("CmdOrCtrl+Shift+Space")
        );
        assert_eq!(s.hotkey.keys, vec!["AltRight".to_string()]);
        assert_eq!(s.hotkey.min_hold_ms, 120);
        assert!(!s.hotkey.consume);
        assert_eq!(s.audio.max_recording_sec, 120);
        assert_eq!(s.audio.preroll, "off");
        assert_eq!(s.engine.model_id, "auto");
        assert!(s.learning.capture_corrections);
        assert!(!s.learning.apply_learned_terms);
        assert_eq!(s.output.on_focus_change, OnFocusChange::Clipboard);
        assert!(s.output.trailing_space);
        assert_eq!(s.history.max_items, 200);
        assert_eq!(s.history.max_days, 30);
        assert_eq!(s.network.update_check, "startup");
        assert!(!s.network.offline_lock);
    }

    #[test]
    fn json_uses_camel_case_keys_from_the_doc() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert!(json["hotkey"]["minHoldMs"].is_number());
        assert!(json["audio"]["maxRecordingSec"].is_number());
        assert_eq!(json["output"]["onFocusChange"], "clipboard");
        assert_eq!(json["longForm"]["defaultDestination"], "clipboard");
        assert_eq!(json["hotkey"]["mode"], "hold");
    }

    #[test]
    fn v1_old_panel_hotkey_default_moves_off_paste_and_match_style() {
        let old = r#"{"version":1,"history":{"panelHotkey":"CmdOrCtrl+Shift+V"}}"#;
        let s = Settings::parse(old).unwrap();
        assert_eq!(
            s.history.panel_hotkey.as_deref(),
            Some("CmdOrCtrl+Shift+Space")
        );
        assert_eq!(s.version, CURRENT_VERSION);
        let chosen = r#"{"version":1,"history":{"panelHotkey":"CmdOrCtrl+Shift+H"}}"#;
        let s = Settings::parse(chosen).unwrap();
        assert_eq!(
            s.history.panel_hotkey.as_deref(),
            Some("CmdOrCtrl+Shift+H"),
            "a choice is kept"
        );
        let off = r#"{"version":1,"history":{"panelHotkey":null}}"#;
        assert_eq!(Settings::parse(off).unwrap().history.panel_hotkey, None);
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let raw = r#"{"version":1,"hotkey":{"keys":["ControlRight"]},"futureThing":{"x":1}}"#;
        let s = Settings::parse(raw).unwrap();
        assert_eq!(s.hotkey.keys, vec!["ControlRight".to_string()]);
        assert_eq!(s.hotkey.min_hold_ms, 120, "missing fields take defaults");
        let out = serde_json::to_value(&s).unwrap();
        assert_eq!(out["futureThing"]["x"], 1);
    }

    #[test]
    fn partial_file_never_panics() {
        for raw in ["{}", r#"{"version":1}"#, r#"{"version":1,"audio":{}}"#] {
            Settings::parse(raw).unwrap();
        }
        assert!(Settings::parse("not json").is_err());
    }
}
