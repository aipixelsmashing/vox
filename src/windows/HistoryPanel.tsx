/**
 * Reference implementation — the pattern every other screen should follow.
 *
 * Shows the three things reviewers ask for: loading, empty and error states handled up front,
 * tokens instead of hard-coded values, and copy pulled from one place rather than invented in
 * the component. Ranking and behaviour: docs/HISTORY.md. States: docs/UI-STATES.md.
 */

import { useEffect, useState } from 'react'
import { commands, toDisplayError } from '../lib/commands'
import type { HistoryEntry, VoxError } from '../lib/contract'

const relative = (ts: number) => {
  const m = Math.round((Date.now() - ts) / 60000)
  if (m < 1) return 'just now'
  if (m < 60) return `${m} min ago`
  const h = Math.round(m / 60)
  return h < 24 ? `${h} h ago` : `${Math.round(h / 24)} d ago`
}

export function HistoryPanel() {
  const [query, setQuery] = useState('')
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null)
  const [error, setError] = useState<VoxError | null>(null)

  useEffect(() => {
    let live = true
    setEntries(null)
    setError(null)
    commands
      .history_list({ query: query || undefined, limit: 50 })
      .then((e) => live && setEntries(e))
      .catch((e) => live && setError(toDisplayError(e)))
    return () => { live = false }
  }, [query])

  return (
    <div className="panel" style={{ display: 'flex', flexDirection: 'column', maxHeight: 520 }}>
      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search transcripts…"
        style={{
          border: 'none', borderBottom: '1px solid var(--rule)', background: 'var(--field)',
          padding: 'var(--s-3)', font: 'inherit', color: 'inherit',
          borderRadius: 'var(--radius-panel) var(--radius-panel) 0 0',
        }}
      />

      <div style={{ overflowY: 'auto', flex: 1 }}>
        {/* Loading: rows arrive progressively. No skeleton shimmer — see docs/UI-KIT.md. */}
        {entries === null && !error && (
          <p className="meta" style={{ padding: 'var(--s-4)' }}>Loading…</p>
        )}

        {error && (
          <div style={{ padding: 'var(--s-4)', color: 'var(--fail)' }}>
            {error.userMessage}
          </div>
        )}

        {entries?.length === 0 && (
          <p className="meta" style={{ padding: 'var(--s-4)' }}>
            {query
              ? `No transcripts match “${query}”.`
              : 'Nothing dictated yet. Hold right Option and speak.'}
          </p>
        )}

        {entries?.map((e) => (
          <div key={e.id} className="row" style={{ padding: 'var(--s-3)' }}>
            <div className="meta">
              {relative(e.createdAt)}
              {e.targetApp && ` · ${e.targetApp}`}
              {e.outcome === 'clipboardOnly' && (
                <span style={{ color: 'var(--fail)' }}> · not inserted — {e.outcomeNote}</span>
              )}
              {` · ${e.wordCount} words`}
            </div>
            <div
              className="transcript"
              style={{
                display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical',
                overflow: 'hidden', marginTop: 'var(--s-1)',
              }}
            >
              {e.text}
            </div>
          </div>
        ))}
      </div>

      <div className="meta" style={{
        padding: 'var(--s-2) var(--s-3)', borderTop: '1px solid var(--rule)',
        display: 'flex', justifyContent: 'space-between',
      }}>
        <span>{entries?.length ?? 0} items</span>
        <button
          onClick={() => commands.history_delete_all()}
          style={{ background: 'none', border: 'none', color: 'inherit', cursor: 'pointer', font: 'inherit' }}
        >
          Delete all…
        </button>
      </div>
    </div>
  )
}
