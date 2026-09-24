//! The Tauri command surface. `src/lib/contract.ts` is the source of truth
//! (docs/UI-CONTRACT.md): change a command here, the contract, and the mock in the same
//! commit. Commands return data or a structured `VoxError`, never an envelope, and nothing
//! on the dictation path crosses this boundary — the webview is woken to display things.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use crate::{clipboard, history, inject, panel, settings, AppState};

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

/// Deep-merges the patch into the current settings, validates by round-tripping through the
/// parser, saves, and returns the merged result so the UI never guesses what was accepted.
#[tauri::command]
pub fn settings_set(
    state: State<'_, AppState>,
    patch: serde_json::Value,
) -> CmdResult<settings::Settings> {
    let mut current = serde_json::to_value(&*state.settings.read()).map_err(VoxError::io)?;
    deep_merge(&mut current, patch);
    let merged = settings::Settings::parse(&current.to_string()).map_err(VoxError::io)?;
    merged.save().map_err(VoxError::io)?;
    *state.settings.write() = merged.clone();
    Ok(merged)
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
    fn bundle_ids_become_names() {
        assert_eq!(display_app_name("com.example.thing"), "thing");
    }
}
