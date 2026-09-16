/** Mock-mode only. Jump between windows and states without editing URLs. */

import { scenarios, currentScenario, currentWindow } from '../mock/scenarios'

const windows = ['history', 'settings', 'onboarding', 'longform', 'overlay'] as const

function go(params: Record<string, string>) {
  const q = new URLSearchParams(location.search)
  Object.entries(params).forEach(([k, v]) => q.set(k, v))
  location.search = q.toString()
}

export function DevOverlay() {
  return (
    <div style={{
      position: 'fixed', bottom: 12, right: 12, zIndex: 9999,
      background: 'var(--panel)', border: '1px solid var(--rule)',
      borderRadius: 'var(--radius-panel)', padding: 'var(--s-2)',
      display: 'flex', gap: 'var(--s-2)', boxShadow: '0 4px 16px #00000022',
    }}>
      <select value={currentWindow()} onChange={(e) => go({ window: e.target.value })}>
        {windows.map((w) => <option key={w} value={w}>{w}</option>)}
      </select>
      <select value={currentScenario()} onChange={(e) => go({ scenario: e.target.value })}>
        {Object.entries(scenarios).map(([id, s]) => (
          <option key={id} value={id}>{s.label}</option>
        ))}
      </select>
    </div>
  )
}
