/**
 * Mock core. Lets every screen — including all the failure states — be built and reviewed in a
 * browser with no Rust toolchain, no model, no microphone and no permissions.
 * See docs/UI-DEVELOPMENT.md.
 *
 * Deliberately simulates latency. Instant data hides loading states, and loading states are
 * where UI bugs live.
 */

import type { Events, HistoryEntry, Settings, VoxError } from '../lib/contract'
import * as fx from './fixtures'
import { currentScenario, type ScenarioId } from './scenarios'

const LATENCY_MS = 150
const wait = (ms = LATENCY_MS) => new Promise((r) => setTimeout(r, ms))

const scenario = (): ScenarioId => currentScenario()

function fail(e: VoxError): never {
  throw e
}

// ─── Event bus ───────────────────────────────────────────────────────────────

type Handler = (payload: unknown) => void
const handlers = new Map<string, Set<Handler>>()

export function mockOn<K extends keyof Events>(
  event: K,
  handler: (payload: Events[K]) => void,
): () => void {
  const set = handlers.get(event) ?? new Set()
  set.add(handler as Handler)
  handlers.set(event, set)
  return () => set.delete(handler as Handler)
}

function emit<K extends keyof Events>(event: K, payload: Events[K]) {
  handlers.get(event)?.forEach((h) => h(payload))
}

// ─── Scripted dictation ──────────────────────────────────────────────────────

/**
 * Drives the whole lifecycle so the recording indicator, level meter, transcribing state and
 * insertion result can be built without ever speaking into a microphone.
 */
export async function runScriptedDictation(succeed = true) {
  emit('vox://state', { state: 'arming' })
  await wait(60)
  const started = Date.now()
  for (const rms of fx.levelEnvelope) {
    emit('vox://state', { state: 'recording', elapsedMs: Date.now() - started, longForm: false })
    emit('vox://level', { rms })
    await wait(50)
  }
  emit('vox://state', { state: 'transcribing' })
  await wait(380)
  emit('vox://state', { state: 'injecting' })
  await wait(40)
  const text = 'Can you send me the link to the dashboard when you get a second.'
  history = [{
    id: Date.now(), createdAt: Date.now(), text, wordCount: text.split(' ').length,
    durationMs: 3200, latencyMs: succeed ? 180 : 240, engineId: 'speechanalyzer', language: 'en',
    targetApp: 'Slack', outcome: succeed ? 'inserted' : 'clipboardOnly',
    outcomeNote: succeed ? null : 'window was elevated', method: succeed ? 'accessibility' : null,
    longForm: false,
  }, ...history]
  emit('vox://insertion-result', succeed
    ? { outcome: 'inserted', method: 'accessibility', elapsedMs: 12, entryId: 1 }
    : {
        outcome: 'clipboardOnly', reason: 'elevatedTarget', entryId: 3,
        userMessage: 'Copied instead — Terminal is running as administrator. Press Ctrl+Shift+V to paste.',
      })
  emit('vox://state', { state: 'idle' })
}

if (typeof window !== 'undefined') {
  const s = scenario()
  if (s === 'recording') setTimeout(() => runScriptedDictation(true), 500)
  if (s === 'recording-fails') setTimeout(() => runScriptedDictation(false), 500)
  if (s === 'model-downloading') {
    let received = 0
    const total = 712_849_408
    setInterval(() => {
      received = Math.min(total, received + total * 0.02)
      emit('vox://model-progress', {
        id: 'parakeet-tdt-0.6b-v3-int8', received, total,
        phase: received >= total ? 'verify' : 'download',
      })
    }, 400)
  }
  if (s === 'longform-session') {
    const chunks = [
      'So the problem with the current approach is that we are paying the model load cost on every single dictation, ',
      'which means the first one after lunch always feels broken even though nothing is wrong. ',
      'What if we predicted it instead — we know when someone focuses a text field, ',
      'we know which apps they dictate into, and we know roughly when they work.',
    ]
    let i = 0
    setInterval(() => {
      const text = chunks[i]
      if (text !== undefined) emit('vox://longform-chunk', { text, elapsedMs: ++i * 30_000 })
    }, 2500)
  }
}

// ─── Commands ────────────────────────────────────────────────────────────────

/** Settings the mock hands back; settings_set merges into it so panes reflect what they did. */
let mockSettings: Settings = structuredClone(fx.settings)

/** The mock's own copy of the history, so deletes and new dictations show up like real ones. */
let history: HistoryEntry[] = fx.historyEntries.map((e) => ({ ...e }))

/**
 * Same ordering rule as the core (docs/HISTORY.md, "Ranking, not filing"): recency, with a
 * boost for rows dictated into the currently focused app and a larger one for failed rows,
 * expressed as minutes of virtual recency so nothing older than an hour jumps the queue.
 */
const CURRENT_APP = 'Slack'
function rank(list: HistoryEntry[], now = Date.now()): HistoryEntry[] {
  const score = (e: HistoryEntry) => {
    const ageMin = (now - e.createdAt) / 60_000
    const app = e.targetApp === CURRENT_APP ? 10 : 0
    const failed = e.outcome === 'clipboardOnly' ? 30 : 0
    return ageMin - app - failed
  }
  return [...list].sort((a, b) => score(a) - score(b))
}

const impl: Record<string, (args: any) => Promise<unknown>> = {
  async history_list({ query, limit }: { query?: string; limit: number }) {
    const s = scenario()
    if (s === 'history-slow') await wait(2000)
    else await wait()
    if (s === 'history-empty' || s === 'history-disabled') return []
    if (s === 'history-no-match') return []
    if (s === 'history-long') return [history[3]].filter(Boolean)
    const q = query?.trim().toLowerCase()
    let list = q ? history.filter((e) => e.text.toLowerCase().includes(q)) : history
    if (s === 'history-with-failures') {
      // A recent failure as well as the hour-old one, so the float is visible above
      // successes of similar age.
      list = list.map((e) =>
        e.id === 2 ? { ...e, outcome: 'clipboardOnly', outcomeNote: 'No text field was focused', method: null } : e)
    }
    return rank(list).slice(0, limit)
  },

  async history_delete({ id }: { id: number }) {
    await wait()
    history = history.filter((e) => e.id !== id)
  },
  async history_delete_all() {
    await wait()
    const deleted = history.length
    history = []
    return { deleted }
  },
  async history_copy() { await wait(60); },
  async history_reinsert({ id }: { id: number }) {
    await wait(300)
    const e = history.find((x) => x.id === id)
    if (e?.outcome === 'clipboardOnly' && scenario() === 'history-with-failures') {
      return {
        outcome: 'clipboardOnly', reason: 'noTextTarget',
        userMessage: 'Copied. No text field was focused.',
      }
    }
    return { outcome: 'inserted', method: 'paste', elapsedMs: 41 }
  },
  async panel_hide() { /* nothing to hide in a browser tab */ },
  async history_export() { await wait(600); return { path: '~/Documents/vox-export' } },

  async vocab_list() {
    await wait()
    const s = scenario()
    if (s === 'vocab-empty' || s === 'vocab-off') return []
    if (s === 'vocab-suspended') return fx.vocabTerms.filter((t) => t.state === 'suspended')
    return fx.vocabTerms
  },
  async vocab_forget() { await wait(); },
  async vocab_export() { await wait(300); return { path: '~/Documents/vox-vocabulary.txt' } },

  async settings_get() {
    if (scenario() === 'history-disabled') {
      await wait()
      return { ...fx.settings, history: { ...fx.settings.history, enabled: false } }
    }
    await wait()
    const s = scenario()
    const base = structuredClone(mockSettings)
    if (s === 'offline-locked') base.network.offlineLock = true
    if (s === 'vocab-off') base.learning.captureCorrections = false
    if (s === 'vocab-populated') base.learning.applyLearnedTerms = true
    if (s === 'history-disabled') base.history.enabled = false
    if (s === 'dictionary-empty') base.output.dictionary = []
    return base
  },
  async settings_set(patch: Record<string, unknown>) {
    await wait()
    const merge = (base: any, p: any): any => {
      if (base && p && typeof base === 'object' && typeof p === 'object' && !Array.isArray(base) && !Array.isArray(p)) {
        const out = { ...base }
        for (const k of Object.keys(p)) out[k] = merge(base[k], p[k])
        return out
      }
      return p
    }
    mockSettings = merge(mockSettings, patch)
    return structuredClone(mockSettings)
  },
  async hotkey_capture_start() {
    // Resolves when the user presses something. Two seconds, so the "waiting" state is visible.
    await wait(2000)
    return scenario() === 'hotkey-altgr'
      ? { keys: ['AltRight'], mode: 'hold', minHoldMs: 120, consume: false, altGr: true }
      : { keys: ['ControlRight'], mode: 'hold', minHoldMs: 120, consume: false }
  },
  async app_relaunch() { /* a browser tab cannot relaunch */ },

  async models_list() {
    await wait()
    if (scenario() === 'model-none') return fx.models.map((m) => ({ ...m, installed: false, isDefault: false }))
    return fx.models
  },
  async models_download() {
    if (scenario() === 'offline-locked') {
      fail({ kind: 'offline-lock', detail: 'client not constructed',
        userMessage: 'Offline lock is on. Turn it off in Privacy to download.',
        action: 'open-settings', actionLabel: 'Open Privacy settings' })
    }
    if (scenario() === 'model-hash-fail') {
      await wait(1200)
      fail({ kind: 'model', detail: 'sha256 mismatch',
        userMessage: "Download didn't verify — the file doesn't match its checksum. Try again or use a mirror.",
        action: 'retry', actionLabel: 'Try again' })
    }
    await wait(1000)
  },
  async models_import() { await wait(800); return fx.models[1] },
  async models_remove() { await wait(); },

  async permissions_status() {
    await wait(80)
    const s = scenario()
    if (s === 'permissions-missing') return fx.permissionsMissing
    if (s === 'permissions-restart') return { ...fx.permissionsAllGranted, accessibility: 'needsRestart' }
    return fx.permissionsAllGranted
  },
  async permissions_open_pane() { await wait(50); },
  async audio_devices() { await wait(); return fx.devices },
  async diagnostics_recent() {
    await wait()
    return scenario() === 'diagnostics-thin' ? fx.diagnosticsThin : fx.diagnosticsRich
  },
  async export_everything() {
    await wait(900)
    return { path: '~/Documents/vox-export', counts: { transcripts: 47, words: 12 } }
  },

  async longform_stop() { await wait(200); return { path: '~/Notes/2026-09-09-so-the-problem.md' } },
  async longform_set_destination() { await wait(50); },
}

export async function mockInvoke(cmd: string, args?: unknown): Promise<unknown> {
  if (scenario() === 'engine-crashed' && cmd === 'history_reinsert') {
    fail({ kind: 'engine', detail: 'inference thread panicked',
      userMessage: 'Transcription failed. The last recording was lost.', action: 'retry' })
  }
  const fn = impl[cmd]
  if (!fn) throw new Error(`mock: no implementation for "${cmd}" — add one (docs/UI-CONTRACT.md)`)
  return fn(args as never)
}

/** Used by the contract-coverage test. */
export const mockedCommands = Object.keys(impl)
