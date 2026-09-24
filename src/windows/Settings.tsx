/**
 * Settings: one window, six panes, no search, no nesting deeper than one level
 * (docs/SETTINGS.md, "Settings UI"). The pane comes from ?pane= so every state deep-links.
 */

import { useEffect, useState } from 'react'
import { copy } from '../lib/copy'
import { useSettings } from '../lib/useSettings'
import { PermissionsStrip } from '../ui/PermissionsStrip'
import { DiagnosticsPane } from './settings/DiagnosticsPane'
import { DictationPane } from './settings/DictationPane'
import { ModelPane } from './settings/ModelPane'
import { PrivacyPane } from './settings/PrivacyPane'
import { TextPane } from './settings/TextPane'
import { VocabularyPane } from './settings/VocabularyPane'

export const PANES = ['dictation', 'model', 'text', 'vocabulary', 'privacy', 'diagnostics'] as const
export type PaneId = (typeof PANES)[number]

function paneFromUrl(): PaneId {
  const p = new URLSearchParams(location.search).get('pane')
  return (PANES as readonly string[]).includes(p ?? '') ? (p as PaneId) : 'dictation'
}

export function Settings() {
  const [pane, setPane] = useState<PaneId>(paneFromUrl)
  const store = useSettings()

  useEffect(() => {
    const q = new URLSearchParams(location.search)
    q.set('pane', pane)
    history.replaceState(null, '', `?${q}`)
  }, [pane])

  return (
    <div
      className="panel"
      style={{ width: 'auto', minHeight: '100vh', borderRadius: 0, boxShadow: 'none', display: 'flex', flexDirection: 'column' }}
    >
      <PermissionsStrip />
      <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
        <nav aria-label="Settings panes" style={{ width: 168, borderRight: '1px solid var(--rule)', padding: 'var(--s-3) 0', flexShrink: 0 }}>
          {PANES.map((id) => (
            <button
              key={id}
              aria-current={pane === id ? 'page' : undefined}
              onClick={() => setPane(id)}
              style={{
                display: 'block',
                width: '100%',
                textAlign: 'left',
                font: 'inherit',
                color: 'inherit',
                background: pane === id ? 'var(--field)' : 'none',
                border: 'none',
                padding: 'var(--s-2) var(--s-4)',
                cursor: 'pointer',
              }}
            >
              {copy.settings.panes[id]}
            </button>
          ))}
        </nav>
        <main style={{ flex: 1, padding: 'var(--s-6) var(--s-8)', overflowY: 'auto', minWidth: 0 }}>
          {store.error && (
            <p role="alert" style={{ color: 'var(--fail)', marginTop: 0 }}>
              {store.error.userMessage}
            </p>
          )}
          {!store.settings && !store.error && <p className="meta">{copy.history.loading}</p>}
          {store.settings && (
            <>
              {pane === 'dictation' && <DictationPane settings={store.settings} patch={store.patch} />}
              {pane === 'model' && <ModelPane settings={store.settings} patch={store.patch} />}
              {pane === 'text' && <TextPane settings={store.settings} patch={store.patch} />}
              {pane === 'vocabulary' && <VocabularyPane settings={store.settings} patch={store.patch} />}
              {pane === 'privacy' && <PrivacyPane settings={store.settings} patch={store.patch} />}
              {pane === 'diagnostics' && <DiagnosticsPane />}
            </>
          )}
        </main>
      </div>
    </div>
  )
}
