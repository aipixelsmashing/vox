/**
 * One scenario per row in docs/UI-STATES.md. A documented state with no scenario here fails
 * CI — that test is what stops empty and error states being discovered at review time.
 *
 * Switch with ?scenario=… or the dev overlay.
 */

export const scenarios = {
  // History panel
  'history-populated':     { label: 'History — populated' },
  'history-empty':         { label: 'History — nothing dictated yet' },
  'history-disabled':      { label: 'History — turned off in Privacy' },
  'history-no-match':      { label: 'History — search with no results' },
  'history-with-failures': { label: 'History — failed insertions floated' },
  'history-long':          { label: 'History — very long transcript' },
  'history-slow':          { label: 'History — slow load (2s)' },

  // Dictation lifecycle
  'recording':             { label: 'Live dictation — succeeds' },
  'recording-fails':       { label: 'Live dictation — ends in clipboard fallback' },
  'longform-session':      { label: 'Long-form — 4 minutes in, text arriving' },

  // Model
  'model-none':            { label: 'Model — none installed' },
  'model-downloading':     { label: 'Model — downloading, 38%' },
  'model-hash-fail':       { label: 'Model — checksum mismatch' },

  // Vocabulary
  'vocab-empty':           { label: 'Vocabulary — nothing learned yet' },
  'vocab-populated':       { label: 'Vocabulary — terms with provenance' },
  'vocab-suspended':       { label: 'Vocabulary — a suspended term' },
  'vocab-off':             { label: 'Vocabulary — learning turned off' },

  // Permissions and system
  'permissions-missing':   { label: 'Permissions — Input Monitoring denied' },
  'permissions-restart':   { label: 'Permissions — granted, restart needed' },
  'engine-crashed':        { label: 'Engine — crashed and restarted' },
  'offline-locked':        { label: 'Offline lock on — network controls disabled' },
  'contract-mismatch':     { label: 'Contract version mismatch' },

  // Other
  'hotkey-capturing':      { label: 'Hotkey — waiting for a key' },
  'hotkey-altgr':          { label: 'Hotkey — AltGr layout warning' },
  'dictionary-empty':      { label: 'Dictionary — empty' },
  'diagnostics-thin':      { label: 'Diagnostics — not enough data' },
  'diagnostics-rich':      { label: 'Diagnostics — six weeks of data' },
} as const

export type ScenarioId = keyof typeof scenarios

export function currentScenario(): ScenarioId {
  const p = new URLSearchParams(location.search).get('scenario')
  return (p && p in scenarios ? p : 'history-populated') as ScenarioId
}

export function currentWindow(): 'history' | 'settings' | 'onboarding' | 'longform' | 'overlay' {
  const w = new URLSearchParams(location.search).get('window')
  return (w as never) ?? 'history'
}
