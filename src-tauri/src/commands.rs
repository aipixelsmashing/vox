//! The Tauri command surface. `src/lib/contract.ts` is the source of truth
//! (docs/UI-CONTRACT.md): change a command here, the contract, and the mock in the same
//! commit. Commands return data or a structured `VoxError`, never an envelope, and nothing
//! on the dictation path crosses this boundary — the webview is woken to display things.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use crate::{audio, clipboard, history, hotkey, inject, panel, permissions, settings, AppState};

// ─── Error model ─────────────────────────────────────────────────────────────

/// Mirrors `VoxError` in the contract. `user_message` comes from the copy deck
/// (docs/UI-STATES.md); `detail` is for the log and is never rendered.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoxError {
    pub kind: &'static str,
    pub detail: String,
    pub user_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<&'static str>,
}

impl VoxError {
    fn io(e: impl std::fmt::Display) -> Self {
        // Every command failure is logged here, because the UI may not show it (a toggle that
        // "did nothing" is this line in the log). Never transcript text: these are I/O,
        // settings and window errors.
        tracing::warn!("command failed: {e}");
        Self {
            kind: "io",
            detail: e.to_string(),
            user_message: "Couldn't read Vox's data. Try again.".into(),
            action_label: Some("Try again".into()),
            action: Some("retry"),
        }
    }

    fn injection(e: impl std::fmt::Display) -> Self {
        Self {
            kind: "injection",
            detail: e.to_string(),
            user_message: "Copied. No text field was focused.".into(),
            action_label: None,
            action: None,
        }
    }

    fn unsupported(detail: &str) -> Self {
        Self {
            kind: "unsupported",
            detail: detail.into(),
            user_message: "That isn't available in this version of Vox.".into(),
            action_label: None,
            action: None,
        }
    }
}

type CmdResult<T> = Result<T, VoxError>;

// ─── Data transfer shapes (camelCase, as the contract spells them) ────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntryDto {
    pub id: i64,
    pub created_at: i64,
    pub text: String,
    pub word_count: u32,
    pub duration_ms: u32,
    pub latency_ms: u32,
    pub engine_id: String,
    pub language: Option<String>,
    pub target_app: Option<String>,
    pub outcome: &'static str,
    pub outcome_note: Option<String>,
    pub method: Option<&'static str>,
    pub long_form: bool,
    /// How many recognition hints this dictation was given. Never the hints.
    pub context_terms: u32,
}

impl From<history::Entry> for HistoryEntryDto {
    fn from(e: history::Entry) -> Self {
        Self {
            id: e.id,
            created_at: e.created_at,
            text: e.text,
            word_count: e.word_count,
            duration_ms: e.duration_ms,
            latency_ms: e.latency_ms,
            engine_id: e.engine_id,
            language: e.language,
            target_app: e.target_app.map(|b| display_app_name(&b)),
            outcome: if e.outcome == "inserted" {
                "inserted"
            } else {
                "clipboardOnly"
            },
            outcome_note: e
                .outcome_note
                .as_deref()
                .map(inject::FallbackReason::note_from_code),
            method: e.method.as_deref().map(inject::Method::contract_name),
            long_form: false,
            context_terms: e.context_terms,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "outcome",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InjectionOutcomeDto {
    Inserted {
        method: &'static str,
        elapsed_ms: u32,
    },
    ClipboardOnly {
        reason: &'static str,
        user_message: String,
    },
}

pub fn outcome_dto(o: &inject::InjectionOutcome, elapsed_ms: u32) -> InjectionOutcomeDto {
    match o {
        inject::InjectionOutcome::Inserted { method, .. } => InjectionOutcomeDto::Inserted {
            method: inject::Method::contract_name(method.as_str()),
            elapsed_ms,
        },
        inject::InjectionOutcome::ClipboardOnly { reason } => InjectionOutcomeDto::ClipboardOnly {
            reason: inject::FallbackReason::contract_kind_from_code(&reason.code()),
            user_message: reason.user_message(),
        },
    }
}

/// `vox://insertion-result`: the outcome plus the history row it produced (0 when history
/// is off).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertionResultEvent {
    #[serde(flatten)]
    pub outcome: InjectionOutcomeDto,
    pub entry_id: i64,
}

/// `vox://state`.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PipelineStateDto {
    Idle,
    Arming,
    Recording { elapsed_ms: u64, long_form: bool },
    Transcribing,
    Injecting,
}

/// A bundle id becomes the name people know the app by. Falls back to the last component of
/// the id, which is what most of them are anyway.
fn display_app_name(bundle_id: &str) -> String {
    #[cfg(target_os = "macos")]
    if let Some(name) = inject::macos::app_name_for_bundle(bundle_id) {
        return name;
    }
    bundle_id
        .rsplit('.')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(bundle_id)
        .to_string()
}

// ─── History ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn history_list(
    state: State<'_, AppState>,
    query: Option<String>,
    limit: u32,
    before: Option<i64>,
) -> CmdResult<Vec<HistoryEntryDto>> {
    if !state.settings.read().history.enabled {
        return Ok(vec![]);
    }
    // Rank over a larger window than the page, so a boosted row can rise into view.
    let window = limit.saturating_mul(4).max(200);
    let query = query.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let mut rows = match query {
        Some(q) => state.history.search(q, window),
        None => state.history.recent(window),
    }
    .map_err(VoxError::io)?;
    if let Some(b) = before {
        rows.retain(|r| r.created_at < b);
    }
    let ranked = history::rank(rows, panel::opened_over().as_deref(), history::now_millis());
    Ok(ranked
        .into_iter()
        .take(limit as usize)
        .map(HistoryEntryDto::from)
        .collect())
}

#[tauri::command]
pub fn history_delete(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    state.history.delete(id).map_err(VoxError::io)?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Deleted {
    pub deleted: usize,
}

#[tauri::command]
pub fn history_delete_all(state: State<'_, AppState>) -> CmdResult<Deleted> {
    let deleted = state.history.delete_all().map_err(VoxError::io)?;
    Ok(Deleted { deleted })
}

#[tauri::command]
pub fn history_copy(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let entry = state
        .history
        .get(id)
        .map_err(VoxError::io)?
        .ok_or_else(|| VoxError::io(format!("no history row {id}")))?;
    clipboard::set_private(&entry.text).map_err(VoxError::io)
}

/// Re-runs injection against whatever is focused once the panel is out of the way. Blocking
/// work, so it runs off the main thread; the panel hides first so focus returns to the app
/// the user was in.
#[tauri::command]
pub async fn history_reinsert(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> CmdResult<InjectionOutcomeDto> {
    let entry = state
        .history
        .get(id)
        .map_err(VoxError::io)?
        .ok_or_else(|| VoxError::io(format!("no history row {id}")))?;
    let injector = state.injector.clone();
    let text = entry.text;
    panel::hide_history(&app);

    let (outcome, elapsed_ms, text) = tauri::async_runtime::spawn_blocking(move || {
        // Focus needs a beat to return to the app underneath.
        std::thread::sleep(Duration::from_millis(250));
        let t0 = Instant::now();
        let target = injector.capture_target().map_err(VoxError::injection)?;
        let outcome = injector.inject(&text, &target).unwrap_or_else(|e| {
            tracing::warn!("re-insert failed: {e}");
            inject::InjectionOutcome::ClipboardOnly {
                reason: inject::FallbackReason::MethodFailed(inject::Method::Accessibility),
            }
        });
        Ok::<_, VoxError>((outcome, t0.elapsed().as_millis() as u32, text))
    })
    .await
    .map_err(VoxError::io)??;

    if let inject::InjectionOutcome::ClipboardOnly { reason } = &outcome {
        if !reason.is_secret_context() {
            let _ = clipboard::set_private(&text);
        }
        crate::notify(&app, &reason.user_message());
    }
    tracing::info!(
        "re-insert: {}",
        match &outcome {
            inject::InjectionOutcome::Inserted { method, .. } => method.as_str(),
            inject::InjectionOutcome::ClipboardOnly { .. } => "clipboard_only",
        }
    );
    Ok(outcome_dto(&outcome, elapsed_ms))
}

#[derive(Debug, Serialize)]
pub struct ExportedTo {
    pub path: String,
}

#[tauri::command]
pub fn history_export(state: State<'_, AppState>, format: String) -> CmdResult<ExportedTo> {
    let fmt = match format.as_str() {
        "md" => history::ExportFormat::Markdown,
        "json" => history::ExportFormat::Json,
        other => return Err(VoxError::unsupported(&format!("export format {other}"))),
    };
    let dir = export_dir().map_err(VoxError::io)?;
    let path = state.history.export_to(&dir, fmt).map_err(VoxError::io)?;
    Ok(ExportedTo {
        path: path.display().to_string(),
    })
}

/// `~/Documents/vox-export`, or the data directory when there is no Documents folder.
fn export_dir() -> anyhow::Result<PathBuf> {
    let base = directories::UserDirs::new()
        .and_then(|u| u.document_dir().map(PathBuf::from))
        .unwrap_or(settings::data_dir()?);
    let dir = base.join("vox-export");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ─── Settings ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> CmdResult<settings::Settings> {
    Ok(state.settings.read().clone())
}

/// The contract passes the patch as the whole argument object — `settings_set(partial)`,
/// not `settings_set({ patch })` — so this argument reads the raw invoke payload instead of
/// one key of it. Any other signature makes Tauri reject the call before the command runs,
/// which is how a toggle can "do nothing" without a line in the log.
pub struct SettingsPatch(pub serde_json::Value);

impl<'de, R: tauri::Runtime> tauri::ipc::CommandArg<'de, R> for SettingsPatch {
    fn from_command(
        command: tauri::ipc::CommandItem<'de, R>,
    ) -> Result<Self, tauri::ipc::InvokeError> {
        match command.message.payload() {
            tauri::ipc::InvokeBody::Json(v) => Ok(Self(v.clone())),
            tauri::ipc::InvokeBody::Raw(bytes) => serde_json::from_slice(bytes)
                .map(Self)
                .map_err(tauri::ipc::InvokeError::from_error),
        }
    }
}

/// Deep-merges the patch into the current settings, validates by round-tripping through the
/// parser, saves, and returns the merged result so the UI never guesses what was accepted.
#[tauri::command]
pub fn settings_set(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> CmdResult<settings::Settings> {
    let patch = patch.0;
    // A patch is `Partial<Settings>`: every top-level key is one the schema knows. Anything
    // else is the UI and the core disagreeing about the call's shape, which is exactly the
    // bug that once made every call fail silently; it errors loudly now instead of being
    // absorbed into the file as an unknown key.
    if let Some(unknown) = unknown_top_level_key(&patch) {
        tracing::warn!("settings_set rejected: unknown key {unknown:?}");
        return Err(VoxError::unsupported(&format!(
            "settings_set: {unknown:?} is not a settings section"
        )));
    }
    let mut current = serde_json::to_value(&*state.settings.read()).map_err(VoxError::io)?;
    let touched = patch_keys(&patch);
    deep_merge(&mut current, patch);
    let merged = settings::Settings::parse(&current.to_string()).map_err(VoxError::io)?;
    merged.save().map_err(VoxError::io)?;
    *state.settings.write() = merged.clone();
    settings::SETTINGS_GEN.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    tracing::info!("settings updated: {touched}");
    if merged.privacy.read_focused_field {
        // Get the word list and the dictation module ready now, in the background, so the
        // next dictation does not pay for either inside its 150 ms (docs/CONTEXT.md).
        crate::context::warm();
        state.engine.prepare_context();
    }
    Ok(merged)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyBindingDto {
    pub keys: Vec<String>,
    pub mode: hotkey::Mode,
    pub min_hold_ms: u32,
    pub consume: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub alt_gr: bool,
}

/// Resolves with the next chord the user presses and fully releases, or errors after 15 s.
/// The keys are not saved here; Settings decides what to do with them.
#[tauri::command]
pub async fn hotkey_capture_start(state: State<'_, AppState>) -> CmdResult<HotkeyBindingDto> {
    let rx = hotkey::capture_next();
    let keys =
        tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(15)))
            .await
            .map_err(VoxError::io)?
            .map_err(|_| {
                hotkey::capture_cancel();
                VoxError::unsupported("no key pressed within 15 s")
            })?;
    let h = state.settings.read().hotkey.clone();
    let alt_gr = hotkey::layout_uses_altgr() && keys.iter().any(|k| k == "AltRight");
    Ok(HotkeyBindingDto {
        keys,
        mode: h.mode,
        min_hold_ms: h.min_hold_ms,
        consume: h.consume,
        alt_gr,
    })
}

// ─── Permissions, devices, models, vocabulary, diagnostics, export ────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionReportDto {
    pub microphone: &'static str,
    pub input_monitoring: &'static str,
    pub accessibility: &'static str,
    pub input_group: &'static str,
}

fn status_name(s: permissions::Status) -> &'static str {
    match s {
        permissions::Status::Granted => "granted",
        permissions::Status::Denied => "denied",
        permissions::Status::NeedsRestart => "needsRestart",
        permissions::Status::NotApplicable => "notApplicable",
    }
}

#[tauri::command]
pub fn permissions_status() -> CmdResult<PermissionReportDto> {
    let r = permissions::check();
    Ok(PermissionReportDto {
        microphone: status_name(r.microphone),
        input_monitoring: status_name(r.input_monitoring),
        accessibility: status_name(r.accessibility),
        input_group: status_name(r.input_group),
    })
}

#[tauri::command]
pub fn permissions_open_pane(which: String) -> CmdResult<()> {
    let pane = match which.as_str() {
        "inputMonitoring" => "input-monitoring",
        "accessibility" => "accessibility",
        "microphone" => "microphone",
        other => return Err(VoxError::unsupported(&format!("no pane for {other}"))),
    };
    permissions::open_pane(pane).map_err(VoxError::io)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceDto {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[tauri::command]
pub fn audio_devices() -> CmdResult<Vec<AudioDeviceDto>> {
    Ok(audio::input_devices()
        .into_iter()
        .map(|(id, name, is_default)| AudioDeviceDto {
            id,
            name,
            is_default,
        })
        .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelLicenseDto {
    pub spdx: &'static str,
    pub attribution: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfoDto {
    pub id: &'static str,
    pub display_name: &'static str,
    pub engine: &'static str,
    pub size_bytes: u64,
    pub languages: Vec<&'static str>,
    pub installed: bool,
    pub is_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<&'static str>,
    pub license: ModelLicenseDto,
}

/// v1 has one engine, Apple's, and no downloads (docs/adr/0016). The row says whether it is
/// usable on this machine; the M8 engines return as further rows.
#[tauri::command]
pub fn models_list(state: State<'_, AppState>) -> CmdResult<Vec<ModelInfoDto>> {
    let usable = state.pipeline_engine_ok();
    Ok(vec![ModelInfoDto {
        id: "speechanalyzer",
        display_name: "Apple Speech (built in)",
        engine: "speechanalyzer",
        size_bytes: 0,
        languages: vec!["en"],
        installed: usable,
        is_default: true,
        provider: Some("Apple, on device"),
        license: ModelLicenseDto {
            spdx: "OS-provided",
            attribution: "Apple SpeechAnalyzer, macOS 26+",
        },
    }])
}

#[tauri::command]
pub fn models_download(id: String) -> CmdResult<()> {
    Err(VoxError::unsupported(&format!(
        "download {id}: v1 ships no downloadable engines"
    )))
}

#[tauri::command]
pub fn models_import(path: String) -> CmdResult<ModelInfoDto> {
    Err(VoxError::unsupported(&format!(
        "import {path}: v1 ships no importable engines"
    )))
}

#[tauri::command]
pub fn models_remove(id: String) -> CmdResult<()> {
    Err(VoxError::unsupported(&format!(
        "remove {id}: the built-in engine cannot be removed"
    )))
}

#[tauri::command]
pub fn vocab_list(state: State<'_, AppState>) -> CmdResult<Vec<history::VocabTerm>> {
    state.history.vocab_list().map_err(VoxError::io)
}

#[tauri::command]
pub fn vocab_forget(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    state.history.vocab_forget(id).map_err(VoxError::io)?;
    Ok(())
}

/// Every stored correction, candidates included, then VACUUM (docs/LEARNING.md). The
/// Privacy pane's delete button; it confirms once, like the history wipe.
#[tauri::command]
pub fn vocab_forget_all(state: State<'_, AppState>) -> CmdResult<Deleted> {
    let deleted = state.history.vocab_forget_all().map_err(VoxError::io)?;
    Ok(Deleted { deleted })
}

#[tauri::command]
pub fn vocab_export(state: State<'_, AppState>) -> CmdResult<ExportedTo> {
    let dir = export_dir().map_err(VoxError::io)?;
    let (path, _) = state.history.vocab_export_to(&dir).map_err(VoxError::io)?;
    Ok(ExportedTo {
        path: path.display().to_string(),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsDto {
    pub corrections_per_100_words: Vec<serde_json::Value>,
    pub insertions_by_app: Vec<InsertionsByAppDto>,
    pub stage_timings_ms: StageTimingsDto,
    pub memory: MemoryDto,
    pub preload_hit_rate: f64,
    pub context: ContextUseDto,
}

/// Whether recognition hints are reaching the engine: dictations with any, out of all.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUseDto {
    pub dictations: u32,
    pub with_hints: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertionsByAppDto {
    pub app: String,
    pub inserted: u32,
    pub clipboard_only: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageTimingsDto {
    pub capture: u32,
    pub inference: u32,
    pub injection: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDto {
    pub idle_rss_mb: u32,
    pub peak_rss_mb: u32,
}

/// The user's own numbers, from the history database and this process. v1 stores one
/// timing per dictation (release-to-text), reported as `inference`; capture and injection
/// stay 0 until they are recorded separately. Corrections need M4's capture to exist.
#[tauri::command]
pub fn diagnostics_recent(state: State<'_, AppState>, limit: u32) -> CmdResult<DiagnosticsDto> {
    let _ = limit;
    let stats = state.history.stats().map_err(VoxError::io)?;
    let (idle, peak) = process_memory_mb();
    Ok(DiagnosticsDto {
        corrections_per_100_words: vec![],
        insertions_by_app: stats
            .by_app
            .into_iter()
            .map(|(app, inserted, clipboard_only)| InsertionsByAppDto {
                app: if app.is_empty() {
                    "—".into()
                } else {
                    display_app_name(&app)
                },
                inserted,
                clipboard_only,
            })
            .collect(),
        stage_timings_ms: StageTimingsDto {
            capture: 0,
            inference: stats.median_latency_ms,
            injection: 0,
        },
        memory: MemoryDto {
            idle_rss_mb: idle,
            peak_rss_mb: peak,
        },
        preload_hit_rate: 0.0,
        context: ContextUseDto {
            dictations: stats.total,
            with_hints: stats.with_hints,
        },
    })
}

/// (resident now, lifetime peak physical footprint) in MB for this process.
#[cfg(target_os = "macos")]
fn process_memory_mb() -> (u32, u32) {
    // SAFETY: RUSAGE_INFO_V4 matches the struct passed; the kernel fills it or fails.
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::proc_pid_rusage(
            std::process::id() as i32,
            libc::RUSAGE_INFO_V4,
            &mut info as *mut _ as *mut libc::rusage_info_t,
        )
    };
    if rc != 0 {
        return (0, 0);
    }
    const MB: u64 = 1_048_576;
    (
        (info.ri_resident_size / MB) as u32,
        (info.ri_lifetime_max_phys_footprint / MB) as u32,
    )
}

#[cfg(not(target_os = "macos"))]
fn process_memory_mb() -> (u32, u32) {
    (0, 0)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedEverything {
    pub path: String,
    pub counts: std::collections::BTreeMap<&'static str, usize>,
}

/// History as .md and .json and the vocabulary as a plain list, into `dest` or the default
/// export folder (adr/0015).
#[tauri::command]
pub fn export_everything(
    state: State<'_, AppState>,
    dest: String,
) -> CmdResult<ExportedEverything> {
    let dir = if dest.trim().is_empty() {
        export_dir().map_err(VoxError::io)?
    } else {
        let d = PathBuf::from(dest.trim());
        std::fs::create_dir_all(&d).map_err(VoxError::io)?;
        d
    };
    state
        .history
        .export_to(&dir, history::ExportFormat::Markdown)
        .map_err(VoxError::io)?;
    state
        .history
        .export_to(&dir, history::ExportFormat::Json)
        .map_err(VoxError::io)?;
    let (_, words) = state.history.vocab_export_to(&dir).map_err(VoxError::io)?;
    let transcripts = state.history.count().map_err(VoxError::io)? as usize;
    let mut counts = std::collections::BTreeMap::new();
    counts.insert("transcripts", transcripts);
    counts.insert("words", words);
    Ok(ExportedEverything {
        path: dir.display().to_string(),
        counts,
    })
}

#[tauri::command]
pub fn app_relaunch(app: AppHandle) {
    app.restart();
}

// ─── Microphone test and onboarding ──────────────────────────────────────────

static MIC_TEST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Opens the input device and streams `vox://level` at ~20 Hz until `mic_test_stop`. Only
/// onboarding's "say something" uses it; dictation has its own capture. Audio is measured
/// and dropped, never kept.
#[tauri::command]
pub fn mic_test_start(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    if MIC_TEST.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let device = state.settings.read().audio.input_device.clone();
    std::thread::Builder::new()
        .name("vox-mic-test".into())
        .spawn(move || {
            let mut capture = match audio::Capture::start(&device) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("mic test could not open the device: {e}");
                    MIC_TEST.store(false, Ordering::Release);
                    return;
                }
            };
            tracing::info!("mic test: device open");
            let deadline = Instant::now() + Duration::from_secs(120);
            let (mut frames, mut peak, mut emitted) = (0usize, 0f32, 0u32);
            while MIC_TEST.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
                let pcm = capture.drain();
                if pcm.is_empty() {
                    continue;
                }
                frames += pcm.len();
                let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt();
                peak = peak.max(rms);
                // Scaled so ordinary speech fills most of the meter.
                let level = (rms * 6.0).min(1.0);
                if let Err(e) = app.emit("vox://level", serde_json::json!({ "rms": level })) {
                    tracing::warn!("mic test: level event not delivered: {e}");
                    break;
                }
                emitted += 1;
            }
            tracing::info!(
                "mic test: stopped after {frames} frames, {emitted} level events, peak rms {peak:.3}"
            );
            MIC_TEST.store(false, Ordering::Release);
        })
        .map_err(VoxError::io)?;
    Ok(())
}

#[tauri::command]
pub fn mic_test_stop() -> CmdResult<()> {
    MIC_TEST.store(false, std::sync::atomic::Ordering::Release);
    Ok(())
}

#[tauri::command]
pub fn onboarding_open(app: AppHandle) -> CmdResult<()> {
    panel::show_onboarding(&app);
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ToastMessage {
    pub message: String,
}

#[tauri::command]
pub fn toast_current() -> CmdResult<Option<ToastMessage>> {
    Ok(panel::toast_current().map(|message| ToastMessage { message }))
}

#[tauri::command]
pub fn toast_fit(app: AppHandle, height: f64) -> CmdResult<()> {
    panel::fit_toast(&app, height);
    Ok(())
}

/// The first top-level key of `patch` that `Settings` has no field for, if any. The known
/// keys are read off the schema itself rather than listed here, so a new section needs no
/// change in this file. A non-object patch counts as unknown.
fn unknown_top_level_key(patch: &serde_json::Value) -> Option<String> {
    let Some(map) = patch.as_object() else {
        return Some("<not an object>".into());
    };
    let known = serde_json::to_value(settings::Settings::default()).ok()?;
    let known = known.as_object()?;
    map.keys().find(|k| !known.contains_key(*k)).cloned()
}

/// "history.enabled, privacy.readFocusedField" — the paths a patch touched, for the log.
/// Keys only, never values.
fn patch_keys(patch: &serde_json::Value) -> String {
    fn walk(v: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
        match v.as_object() {
            Some(m) if !m.is_empty() => {
                for (k, v) in m {
                    let p = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    walk(v, &p, out);
                }
            }
            _ => out.push(prefix.to_string()),
        }
    }
    let mut out = Vec::new();
    walk(patch, "", &mut out);
    out.join(", ")
}

fn deep_merge(base: &mut serde_json::Value, patch: serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(&k) {
                    Some(slot) if slot.is_object() && v.is_object() => deep_merge(slot, v),
                    _ => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (b, p) => *b = p,
    }
}

// ─── Windows ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn panel_hide(window: WebviewWindow) -> CmdResult<()> {
    window.hide().map_err(VoxError::io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deep_merge_replaces_leaves_and_keeps_siblings() {
        let mut base =
            json!({ "history": { "enabled": true, "maxItems": 200 }, "ui": { "theme": "system" } });
        deep_merge(&mut base, json!({ "history": { "enabled": false } }));
        assert_eq!(base["history"]["enabled"], json!(false));
        assert_eq!(base["history"]["maxItems"], json!(200));
        assert_eq!(base["ui"]["theme"], json!("system"));
    }

    #[test]
    fn outcome_serialises_as_the_contract_spells_it() {
        let inserted = outcome_dto(
            &inject::InjectionOutcome::Inserted {
                method: inject::Method::Accessibility,
                elapsed_ms: 12,
            },
            12,
        );
        let v = serde_json::to_value(inserted).unwrap();
        assert_eq!(v["outcome"], "inserted");
        assert_eq!(v["method"], "accessibility");
        assert_eq!(v["elapsedMs"], 12);

        let fallback = outcome_dto(
            &inject::InjectionOutcome::ClipboardOnly {
                reason: inject::FallbackReason::FocusChanged {
                    from: "Notes".into(),
                    to: "Finder".into(),
                },
            },
            0,
        );
        let v = serde_json::to_value(fallback).unwrap();
        assert_eq!(v["outcome"], "clipboardOnly");
        assert_eq!(v["reason"], "focusChanged");
        assert!(v["userMessage"].as_str().unwrap().contains("Finder"));
    }

    #[test]
    fn state_event_shape() {
        let v = serde_json::to_value(PipelineStateDto::Recording {
            elapsed_ms: 1500,
            long_form: false,
        })
        .unwrap();
        assert_eq!(
            v,
            json!({ "state": "recording", "elapsedMs": 1500, "longForm": false })
        );
        assert_eq!(
            serde_json::to_value(PipelineStateDto::Idle).unwrap(),
            json!({ "state": "idle" })
        );
    }

    #[test]
    fn unknown_sections_are_named_known_ones_pass() {
        assert_eq!(
            unknown_top_level_key(&json!({ "history": { "enabled": false } })),
            None
        );
        assert_eq!(
            unknown_top_level_key(&json!({ "patch": { "history": { "enabled": false } } })),
            Some("patch".into())
        );
        assert_eq!(
            unknown_top_level_key(&json!(7)),
            Some("<not an object>".into())
        );
    }

    #[test]
    fn patch_keys_lists_paths_not_values() {
        let keys = patch_keys(
            &json!({ "history": { "enabled": false }, "privacy": { "readFocusedField": true } }),
        );
        assert_eq!(keys, "history.enabled, privacy.readFocusedField");
        assert!(!keys.contains("true"));
    }

    #[test]
    fn bundle_ids_become_names() {
        assert_eq!(display_app_name("com.example.thing"), "thing");
    }
}
