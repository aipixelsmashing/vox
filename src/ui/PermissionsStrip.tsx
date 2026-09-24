/**
 * The persistent strip at the top of Settings while any permission is missing
 * (docs/SETTINGS.md, "Settings UI"). Says what happened and what to do, in the deck's words,
 * with a button that opens the exact OS pane, or "Quit and reopen" when the grant is in but
 * the app has to restart to use it.
 */

import { useEffect, useState } from 'react'
import { commands } from '../lib/commands'
import type { PermissionReport } from '../lib/contract'
import { copy } from '../lib/copy'
import { useVoxEvent } from '../lib/events'

type Key = keyof PermissionReport

const ORDER: Key[] = ['inputMonitoring', 'accessibility', 'microphone', 'inputGroup']

export function PermissionsStrip() {
  const [report, setReport] = useState<PermissionReport | null>(null)

  useEffect(() => {
    let live = true
    commands
      .permissions_status()
      .then((r) => live && setReport(r))
      .catch(() => {})
    return () => {
      live = false
    }
  }, [])
  useVoxEvent('vox://permissions-changed', setReport, [])

  if (!report) return null
  const missing = ORDER.filter((k) => report[k] === 'denied')
  const restart = ORDER.filter((k) => report[k] === 'needsRestart')
  if (missing.length === 0 && restart.length === 0) return null

  return (
    <div
      role="alert"
      style={{
        padding: 'var(--s-3) var(--s-4)',
        borderBottom: '1px solid var(--rule)',
        display: 'flex',
        flexDirection: 'column',
        gap: 'var(--s-2)',
      }}
    >
      {missing.map((k) => (
        <div key={k} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 'var(--s-4)' }}>
          <span style={{ color: 'var(--fail)' }}>{copy.permissions.missing[k]}</span>
          <button
            style={{ font: 'inherit', color: 'inherit', background: 'var(--field)', border: '1px solid var(--rule)', borderRadius: 'var(--radius-input)', padding: 'var(--s-1) var(--s-3)', cursor: 'pointer', flexShrink: 0 }}
            onClick={() => void commands.permissions_open_pane({ which: k })}
          >
            {copy.permissions.openSettings}
          </button>
        </div>
      ))}
      {restart.length > 0 && (
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 'var(--s-4)' }}>
          <span>{copy.permissions.needsRestart}</span>
          <button
            style={{ font: 'inherit', color: 'inherit', background: 'var(--field)', border: '1px solid var(--rule)', borderRadius: 'var(--radius-input)', padding: 'var(--s-1) var(--s-3)', cursor: 'pointer', flexShrink: 0 }}
            onClick={() => void commands.app_relaunch()}
          >
            {copy.permissions.quitAndReopen}
          </button>
        </div>
      )}
    </div>
  )
}
