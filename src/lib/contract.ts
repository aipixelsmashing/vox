/**
 * The UI ↔ core contract. Single source of truth — see docs/UI-CONTRACT.md.
 *
 * Changing anything here means changing three things in the same PR:
 *   1. the Rust command in src-tauri/src/lib.rs
 *   2. this file
 *   3. the mock in src/mock/backend.ts
 * A test fails when a command declared here has no mock implementation.
 */

export const CONTRACT_VERSION = 1

// ─── Core types ──────────────────────────────────────────────────────────────

export type PipelineState =
  | { state: 'idle' }
  | { state: 'arming' }
  | { state: 'recording'; elapsedMs: number; longForm: boolean }
  | { state: 'transcribing' }
  | { state: 'injecting' }

export type InjectionMethod = 'accessibility' | 'paste' | 'type'

/**
 * Two variants only. There is deliberately no "probably worked" — a method that cannot confirm
 * delivery reports clipboardOnly with a reason. See docs/adr/0005.
 */
export type InjectionOutcome =
  | { outcome: 'inserted'; method: InjectionMethod; elapsedMs: number }
  | { outcome: 'clipboardOnly'; reason: FallbackReason; userMessage: string }

export type FallbackReason =
  | 'noTextTarget'
  | 'focusChanged'
  | 'secureInput'
  | 'passwordField'
  | 'elevatedTarget'
  | 'waylandUnverifiable'
  | 'methodFailed'

export interface HistoryEntry {
  id: number
  createdAt: number
  text: string
  wordCount: number
  durationMs: number
  /** Release-to-text. */
  latencyMs: number
  engineId: string
  language: string | null
  targetApp: string | null
  outcome: 'inserted' | 'clipboardOnly'
  outcomeNote: string | null
  method: InjectionMethod | null
  longForm: boolean
}

export type VocabState = 'candidate' | 'applied' | 'suspended' | 'rejected'

export interface VocabTerm {
  id: number
  wrongForm: string
  rightForm: string
  count: number
  firstSeen: number
  lastSeen: number
  /** App names only — provenance for the UI, never surrounding text. */
  sourceApps: string[]
  reversals: number
  state: VocabState
}

export type PermissionStatus = 'granted' | 'denied' | 'needsRestart' | 'notApplicable'

export interface PermissionReport {
  microphone: PermissionStatus
  inputMonitoring: PermissionStatus
  accessibility: PermissionStatus
  inputGroup: PermissionStatus
}

export interface ModelInfo {
  id: string
  displayName: string
  engine: 'parakeet' | 'whisper' | 'speechanalyzer'
  sizeBytes: number
  languages: string[]
  installed: boolean
  isDefault: boolean
  /** Which execution provider is actually in use, so a slow machine is diagnosable. */
  provider?: string
  license: { spdx: string; attribution: string }
}

export interface AudioDevice {
  id: string
  name: string
  isDefault: boolean
}

export interface Diagnostics {
  /** The number that says whether Vox is getting better at *your* words. */
  correctionsPer100Words: Array<{ weekStart: number; value: number }>
  insertionsByApp: Array<{ app: string; inserted: number; clipboardOnly: number }>
  stageTimingsMs: { capture: number; inference: number; injection: number }
  memory: { idleRssMb: number; peakRssMb: number }
  /** Adaptive residency health — below 0.8 the heuristic needs fixing, not a setting. */
  preloadHitRate: number
}

export type Destination = 'clipboard' | 'newFile' | 'insertAtCursor' | 'appendToFile'

export interface HotkeyBinding {
  keys: string[]
  mode: 'hold' | 'toggle' | 'double-tap-hold'
  minHoldMs: number
  consume: boolean
}

/** Mirrors src-tauri/src/settings.rs. Kept loose here; the core returns the merged truth. */
export interface Settings {
  version: number
  hotkey: HotkeyBinding
  audio: { inputDevice: string; preroll: 'off' | '300ms'; maxRecordingSec: number }
  engine: { modelId: string; device: 'auto' | 'cpu' | 'gpu'; language: string }
  learning: { captureCorrections: boolean; applyLearnedTerms: boolean; minOccurrences: number }
  longForm: { lockKey: string; maxSessionMin: number; defaultDestination: Destination }
  output: {
    method: 'auto' | 'accessibility' | 'paste' | 'type'
    trailingSpace: boolean
    onFocusChange: 'clipboard' | 'insert-anyway'
    dictionary: Array<{ from: string; to: string }>
  }
  history: { enabled: boolean; maxItems: number; maxDays: number }
  network: { updateCheck: 'startup' | 'manual' | 'off'; offlineLock: boolean }
  ui: { theme: 'system' | 'light' | 'dark'; soundCues: boolean; levelOverlay: boolean }
}

// ─── Errors ──────────────────────────────────────────────────────────────────

export interface VoxError {
  kind: 'permission' | 'model' | 'engine' | 'injection' | 'io' | 'offline-lock' | 'unsupported'
  /** For the log. Never rendered. */
  detail: string
  /** What happened and what to do. Supplied by the core so every surface says the same thing. */
  userMessage: string
  actionLabel?: string
  action?: 'open-permissions' | 'download-model' | 'retry' | 'open-settings'
}

export function isVoxError(e: unknown): e is VoxError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'userMessage' in e
}

// ─── Commands ────────────────────────────────────────────────────────────────

export interface Commands {
  history_list(a: { query?: string; limit: number; before?: number }): Promise<HistoryEntry[]>
  history_delete(a: { id: number }): Promise<void>
  history_delete_all(): Promise<{ deleted: number }>
  history_copy(a: { id: number }): Promise<void>
  history_reinsert(a: { id: number }): Promise<InjectionOutcome>
  history_export(a: { format: 'md' | 'json' }): Promise<{ path: string }>

  vocab_list(a: { state?: VocabState }): Promise<VocabTerm[]>
  /** Deletes the term and the candidate evidence behind it. */
  vocab_forget(a: { id: number }): Promise<void>
  vocab_export(): Promise<{ path: string }>

  settings_get(): Promise<Settings>
  /** Returns the merged result, so the UI never has to guess what the core accepted. */
  settings_set(a: Partial<Settings>): Promise<Settings>
  hotkey_capture_start(): Promise<HotkeyBinding>

  models_list(): Promise<ModelInfo[]>
  models_download(a: { id: string }): Promise<void>
  models_import(a: { path: string }): Promise<ModelInfo>
  models_remove(a: { id: string }): Promise<void>

  permissions_status(): Promise<PermissionReport>
  permissions_open_pane(a: { which: keyof PermissionReport }): Promise<void>
  audio_devices(): Promise<AudioDevice[]>
  diagnostics_recent(a: { limit: number }): Promise<Diagnostics>
  export_everything(a: { dest: string }): Promise<{ path: string; counts: Record<string, number> }>

  longform_stop(a: { destination: Destination }): Promise<{ path?: string }>
  longform_set_destination(a: { destination: Destination }): Promise<void>
}

/**
 * Runtime list of the commands above, so a test can check every one has a mock. The two
 * `_exhaustive` lines fail to compile if this list and `Commands` drift apart in either
 * direction.
 */
export const COMMAND_NAMES = [
  'history_list',
  'history_delete',
  'history_delete_all',
  'history_copy',
  'history_reinsert',
  'history_export',
  'vocab_list',
  'vocab_forget',
  'vocab_export',
  'settings_get',
  'settings_set',
  'hotkey_capture_start',
  'models_list',
  'models_download',
  'models_import',
  'models_remove',
  'permissions_status',
  'permissions_open_pane',
  'audio_devices',
  'diagnostics_recent',
  'export_everything',
  'longform_stop',
  'longform_set_destination',
] as const satisfies readonly (keyof Commands)[]

type _MissingFromList = Exclude<keyof Commands, (typeof COMMAND_NAMES)[number]>
const _exhaustive: _MissingFromList extends never ? true : never = true
void _exhaustive

// ─── Events ──────────────────────────────────────────────────────────────────

export interface Events {
  'vox://state': PipelineState
  /** ~20 Hz, only while recording. Stops the instant recording stops. */
  'vox://level': { rms: number }
  'vox://longform-chunk': { text: string; elapsedMs: number }
  'vox://model-progress': { id: string; received: number; total: number; phase: 'download' | 'verify' }
  'vox://permissions-changed': PermissionReport
  'vox://vocab-updated': { added: number; suspended: number }
  'vox://insertion-result': InjectionOutcome & { entryId: number }
}
