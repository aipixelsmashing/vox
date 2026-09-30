/**
 * Fixture data for every screen. Realistic, not lorem: transcripts read like real dictation
 * (first person, mid-thought, occasionally wrong), because prose that reads like filler hides
 * real layout problems — line lengths, awkward wraps, and the two-line truncation point.
 */

import type {
  AudioDevice, Diagnostics, HistoryEntry, ModelInfo, PermissionReport, Settings, VocabTerm,
} from '../lib/contract'

const now = Date.now()
const min = 60_000

export const historyEntries: HistoryEntry[] = [
  {
    id: 1, createdAt: now - 2 * min,
    text: 'Can you take a look at the deploy script before I merge it — I think the retry logic is wrong when the first attempt times out.',
    wordCount: 25, durationMs: 7400, latencyMs: 412, engineId: 'parakeet-tdt-0.6b-v3-int8',
    language: 'en', targetApp: 'Slack', outcome: 'inserted', outcomeNote: null,
    method: 'accessibility', longForm: false, contextTerms: 6,
  },
  {
    id: 2, createdAt: now - 18 * min,
    text: 'refactor the auth middleware to use the new session store',
    wordCount: 10, durationMs: 3100, latencyMs: 288, engineId: 'parakeet-tdt-0.6b-v3-int8',
    language: 'en', targetApp: 'Code', outcome: 'inserted', outcomeNote: null,
    method: 'paste', longForm: false, contextTerms: 0,
  },
  {
    id: 3, createdAt: now - 64 * min,
    text: 'The quarterly numbers came in higher than we forecast, mostly on the enterprise side, which means the capacity plan needs redoing before the board meeting.',
    wordCount: 26, durationMs: 8800, latencyMs: 505, engineId: 'parakeet-tdt-0.6b-v3-int8',
    language: 'en', targetApp: 'Terminal', outcome: 'clipboardOnly',
    outcomeNote: 'Terminal is running as administrator', method: null, longForm: false, contextTerms: 0,
  },
  {
    id: 4, createdAt: now - 3 * 60 * min,
    text: "So the problem with the current approach is that we're paying the model load cost on every single dictation, which means the first one after lunch always feels broken even though nothing is wrong. What if we predicted it instead — we know when someone focuses a text field, we know which apps they dictate into…",
    wordCount: 412, durationMs: 254_000, latencyMs: 1900, engineId: 'parakeet-tdt-0.6b-v3-int8',
    language: 'en', targetApp: null, outcome: 'inserted', outcomeNote: null,
    method: 'accessibility', longForm: true, contextTerms: 0,
  },
  {
    id: 5, createdAt: now - 26 * 60 * min,
    text: 'remind me to send Priya the migration notes',
    wordCount: 8, durationMs: 2400, latencyMs: 245, engineId: 'parakeet-tdt-0.6b-v3-int8',
    language: 'en', targetApp: 'Notes', outcome: 'inserted', outcomeNote: null,
    method: 'accessibility', longForm: false, contextTerms: 0,
  },
]

export const vocabTerms: VocabTerm[] = [
  {
    id: 1, wrongForm: 'cuber netties', rightForm: 'Kubernetes', count: 3,
    firstSeen: now - 30 * 24 * 60 * min, lastSeen: now - 2 * 24 * 60 * min,
    sourceApps: ['Slack', 'Code'], reversals: 0, state: 'applied',
    termCount: 3, termSessions: 2, hinted: true,
  },
  {
    id: 2, wrongForm: 'prea', rightForm: 'Priya', count: 4,
    firstSeen: now - 40 * 24 * 60 * min, lastSeen: now - 60 * min,
    sourceApps: ['Slack'], reversals: 0, state: 'applied',
    termCount: 6, termSessions: 3, hinted: true,
  },
  {
    id: 3, wrongForm: 'tail wind', rightForm: 'Tailwind', count: 5,
    firstSeen: now - 20 * 24 * 60 * min, lastSeen: now - 3 * 24 * 60 * min,
    sourceApps: ['Code'], reversals: 2, state: 'suspended',
    termCount: 0, termSessions: 0, hinted: false,
  },
  {
    id: 4, wrongForm: 'post hog', rightForm: 'PostHog', count: 2,
    firstSeen: now - 5 * 24 * 60 * min, lastSeen: now - 24 * 60 * min,
    sourceApps: ['Chrome'], reversals: 0, state: 'candidate',
    termCount: 2, termSessions: 1, hinted: false,
  },
  // A second way the same word was misheard: the pane shows one row for Priya, both
  // wrong forms beneath it, six corrections, Slack and Mail.
  {
    id: 5, wrongForm: 'pre a', rightForm: 'Priya', count: 2,
    firstSeen: now - 10 * 24 * 60 * min, lastSeen: now - 3 * 60 * min,
    sourceApps: ['Mail'], reversals: 0, state: 'candidate',
    termCount: 6, termSessions: 3, hinted: true,
  },
]

/** Fourteen learned words, for the filter field that appears past twelve. */
export const manyVocabTerms: VocabTerm[] = ([
  ['Kubernetes', 'cuber netties'], ['Priya', 'prea'], ['Tailwind', 'tail wind'],
  ['PostHog', 'post hog'], ['Adi', 'Eddie'], ['Tauri', 'tory'], ['Grafana', 'graph on a'],
  ['Anthropic', 'and tropic'], ['Vercel', 'versal'], ['Supabase', 'super base'],
  ['Figma', 'fig ma'], ['Ollama', 'oh llama'], ['Zed', 'said'], ['Neovim', 'neo vim'],
] as Array<[string, string]>).map(([rightForm, wrongForm], i) => ({
  id: 100 + i,
  wrongForm,
  rightForm,
  count: 3 + (i % 4),
  firstSeen: now - (30 - i) * 24 * 60 * min,
  lastSeen: now - i * 60 * min,
  sourceApps: i % 2 ? ['Slack'] : ['Code', 'Notes'],
  reversals: 0,
  state: 'applied',
  termCount: 3 + (i % 4),
  termSessions: 2,
  hinted: true,
}))

export const models: ModelInfo[] = [
  {
    id: 'speechanalyzer', displayName: 'Apple Speech (built in)', engine: 'speechanalyzer',
    sizeBytes: 0, languages: ['en', 'es', 'fr', 'de', 'it', 'pt', 'ja', 'zh'],
    installed: true, isDefault: true, provider: 'Neural Engine',
    license: { spdx: 'OS-provided', attribution: 'Apple SpeechAnalyzer, macOS 26+' },
  },
  {
    id: 'parakeet-tdt-0.6b-v3-int8', displayName: 'Parakeet v3 — 25 languages', engine: 'parakeet',
    sizeBytes: 712_849_408, languages: ['en', 'es', 'fr', 'de', 'it', 'pt', 'nl', 'pl'],
    installed: false, isDefault: false,
    license: { spdx: 'CC-BY-4.0', attribution: 'NVIDIA Parakeet TDT 0.6B v3' },
  },
  {
    id: 'whisper-base-en-q5', displayName: 'Whisper Base (English) — small download', engine: 'whisper',
    sizeBytes: 62_914_560, languages: ['en'], installed: false, isDefault: false,
    license: { spdx: 'MIT', attribution: 'OpenAI Whisper' },
  },
]

export const devices: AudioDevice[] = [
  { id: 'default', name: 'MacBook Pro Microphone', isDefault: true },
  { id: 'usb-1', name: 'Shure MV7', isDefault: false },
  { id: 'bt-1', name: 'AirPods Pro', isDefault: false },
]

export const permissionsAllGranted: PermissionReport = {
  microphone: 'granted', inputMonitoring: 'granted',
  accessibility: 'granted', inputGroup: 'notApplicable',
}

export const permissionsMissing: PermissionReport = {
  microphone: 'granted', inputMonitoring: 'denied',
  accessibility: 'needsRestart', inputGroup: 'notApplicable',
}

export const diagnosticsRich: Diagnostics = {
  context: { dictations: 412, withHints: 57 },
  correctionsPer100Words: [
    { weekStart: now - 42 * 24 * 60 * min, value: 6.8 },
    { weekStart: now - 35 * 24 * 60 * min, value: 6.1 },
    { weekStart: now - 28 * 24 * 60 * min, value: 5.2 },
    { weekStart: now - 21 * 24 * 60 * min, value: 4.4 },
    { weekStart: now - 14 * 24 * 60 * min, value: 3.9 },
    { weekStart: now - 7 * 24 * 60 * min, value: 3.1 },
  ],
  insertionsByApp: [
    { app: 'Slack', inserted: 214, clipboardOnly: 0 },
    { app: 'Code', inserted: 186, clipboardOnly: 2 },
    { app: 'Chrome', inserted: 97, clipboardOnly: 1 },
    { app: 'Terminal', inserted: 12, clipboardOnly: 9 },
  ],
  stageTimingsMs: { capture: 18, inference: 264, injection: 14 },
  memory: { idleRssMb: 96, peakRssMb: 812 },
  preloadHitRate: 0.87,
}

export const diagnosticsThin: Diagnostics = {
  context: { dictations: 3, withHints: 0 },
  correctionsPer100Words: [],
  insertionsByApp: [{ app: 'Slack', inserted: 3, clipboardOnly: 0 }],
  stageTimingsMs: { capture: 21, inference: 301, injection: 16 },
  memory: { idleRssMb: 94, peakRssMb: 780 },
  preloadHitRate: 0,
}

export const settings: Settings = {
  version: 1,
  hotkey: { keys: ['AltRight'], mode: 'hold', minHoldMs: 120, consume: false },
  audio: { inputDevice: 'default', preroll: 'off', maxRecordingSec: 120 },
  engine: { modelId: 'auto', device: 'auto', language: 'auto' },
  learning: { captureCorrections: true, applyLearnedTerms: false, minOccurrences: 3 },
  longForm: { maxSessionMin: 30, defaultDestination: 'clipboard' },
  output: {
    method: 'auto', restoreClipboard: true, trailingSpace: true, capitalizeFirst: false,
    collapseNewlinesInTerminals: true, onFocusChange: 'clipboard',
    dictionary: [{ from: 'our company name', to: 'OurCompany' }],
  },
  privacy: { readFocusedField: false },
  history: { enabled: true, maxItems: 200, maxDays: 30 },
  network: { updateCheck: 'startup', offlineLock: false },
  ui: { theme: 'system', soundCues: true, levelOverlay: true },
  onboarding: { completedStep: 6, done: true },
}

/** Amplitude envelope of a real 7-second utterance, for the level meter. 20 Hz. */
export const levelEnvelope: number[] = [
  0.02, 0.04, 0.11, 0.28, 0.41, 0.38, 0.44, 0.52, 0.47, 0.33, 0.19, 0.08, 0.05, 0.03,
  0.09, 0.26, 0.45, 0.58, 0.61, 0.54, 0.49, 0.55, 0.62, 0.51, 0.36, 0.21, 0.10, 0.04,
  0.03, 0.07, 0.22, 0.39, 0.48, 0.43, 0.35, 0.24, 0.13, 0.06, 0.03, 0.02,
]
