/**
 * Reference implementation — the pattern every other screen should follow.
 *
 * Loading, empty, disabled and error states handled up front; tokens instead of hard-coded
 * values; copy pulled from src/lib/copy.ts rather than invented here. Ranking is the core's
 * job (docs/HISTORY.md): the list arrives in the order it should be shown. Fully operable
 * without a mouse — that is a requirement, not polish (docs/UI-KIT.md).
 *
 * Keys: arrows move · Enter copies and closes · Cmd/Ctrl+Enter inserts · Delete removes ·
 * Space expands · Escape closes · typing filters.
 */

import { useCallback, useEffect, useRef, useState } from 'react'
import { commands, toDisplayError } from '../lib/commands'
import type { HistoryEntry, InjectionOutcome, PipelineState, VoxError } from '../lib/contract'
import { copy } from '../lib/copy'
import { useVoxEvent } from '../lib/events'

const PAGE = 50
/** Transcripts longer than this get a More/Less control; shorter ones fit in two lines. */
const LONG_TEXT = 160

function relative(ts: number): string {
  const m = Math.round((Date.now() - ts) / 60_000)
  if (m < 1) return copy.time.justNow
  if (m < 60) return copy.time.minutes(m)
  const h = Math.round(m / 60)
  if (h < 24) return copy.time.hours(h)
  return copy.time.days(Math.round(h / 24))
}

const buttonStyle: React.CSSProperties = {
  background: 'none',
  border: 'none',
  color: 'inherit',
  cursor: 'pointer',
  font: 'inherit',
  padding: 'var(--s-1) var(--s-2)',
  borderRadius: 'var(--radius-input)',
}

export function HistoryPanel() {
  const [query, setQuery] = useState('')
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null)
  const [error, setError] = useState<VoxError | null>(null)
  /** null until settings arrive; false renders the "history is off" state. */
  const [enabled, setEnabled] = useState<boolean | null>(null)
  const [selected, setSelected] = useState(0)
  const [expanded, setExpanded] = useState<number | null>(null)
  const [confirmingWipe, setConfirmingWipe] = useState(false)
  const [live, setLive] = useState<PipelineState>({ state: 'idle' })
  /** Per-row transient feedback: a copy confirmation or a failed re-insert's message. */
  const [note, setNote] = useState<{ id: number; text: string; failed: boolean } | null>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)

  const load = useCallback(async () => {
    try {
      const list = await commands.history_list({ query: query || undefined, limit: PAGE })
      setEntries(list)
      setError(null)
      setSelected((s) => Math.min(s, Math.max(0, list.length - 1)))
    } catch (e) {
      setError(toDisplayError(e))
    }
  }, [query])

  useEffect(() => {
    let cancelled = false
    commands
      .settings_get()
      .then((s) => !cancelled && setEnabled(s.history.enabled))
      .catch(() => !cancelled && setEnabled(true))
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    let cancelled = false
    setEntries(null)
    setError(null)
    commands
      .history_list({ query: query || undefined, limit: PAGE })
      .then((list) => {
        if (cancelled) return
        setEntries(list)
        setSelected(0)
      })
      .catch((e) => !cancelled && setError(toDisplayError(e)))
    return () => {
      cancelled = true
    }
  }, [query])

  useVoxEvent('vox://state', (s) => setLive(s), [])
  useVoxEvent('vox://insertion-result', () => void load(), [load])

  useEffect(() => {
    searchRef.current?.focus()
  }, [])

  useEffect(() => {
    if (!note) return
    const t = setTimeout(() => setNote(null), 1800)
    return () => clearTimeout(t)
  }, [note])

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(`[data-index="${selected}"]`)
    el?.scrollIntoView({ block: 'nearest' })
  }, [selected])

  const current = entries?.[selected]

  const close = () => void commands.panel_hide()

  const copyRow = async (e: HistoryEntry, andClose: boolean) => {
    try {
      await commands.history_copy({ id: e.id })
      if (andClose) close()
      else setNote({ id: e.id, text: copy.history.copied, failed: false })
    } catch (err) {
      setNote({ id: e.id, text: toDisplayError(err).userMessage, failed: true })
    }
  }

  const insertRow = async (e: HistoryEntry) => {
    try {
      const out: InjectionOutcome = await commands.history_reinsert({ id: e.id })
      if (out.outcome === 'inserted') close()
      else setNote({ id: e.id, text: out.userMessage, failed: true })
    } catch (err) {
      setNote({ id: e.id, text: toDisplayError(err).userMessage, failed: true })
    }
  }

  const deleteRow = async (e: HistoryEntry) => {
    try {
      await commands.history_delete({ id: e.id })
      setEntries((list) => list?.filter((x) => x.id !== e.id) ?? null)
    } catch (err) {
      setNote({ id: e.id, text: toDisplayError(err).userMessage, failed: true })
    }
  }

  const wipe = async () => {
    try {
      await commands.history_delete_all()
      setConfirmingWipe(false)
      setEntries([])
    } catch (err) {
      setError(toDisplayError(err))
    }
  }

  // Window-level, not on the panel element: WebKit does not focus a button on click, so after
  // any click the keydown target is <body> and a handler on the panel would never see it.
  // Printable keys are routed into the search box so typing always filters.
  const onKeyDown = (ev: KeyboardEvent) => {
    const n = entries?.length ?? 0
    const inSearch = document.activeElement === searchRef.current
    if (!inSearch && ev.key.length === 1 && !ev.metaKey && !ev.ctrlKey && !ev.altKey && ev.key !== ' ') {
      searchRef.current?.focus()
      return
    }
    switch (ev.key) {
      case 'ArrowDown':
        ev.preventDefault()
        if (n) setSelected((s) => Math.min(n - 1, s + 1))
        break
      case 'ArrowUp':
        ev.preventDefault()
        if (n) setSelected((s) => Math.max(0, s - 1))
        break
      case 'Enter':
        if (!current) break
        ev.preventDefault()
        if (ev.metaKey || ev.ctrlKey) void insertRow(current)
        else void copyRow(current, true)
        break
      case 'Delete':
      case 'Backspace':
        // Backspace edits the search while there is text to edit.
        if (ev.key === 'Backspace' && query) break
        if (!current) break
        ev.preventDefault()
        void deleteRow(current)
        break
      case ' ':
        if (query || !current) break
        ev.preventDefault()
        setExpanded((x) => (x === current.id ? null : current.id))
        break
      case 'Escape':
        ev.preventDefault()
        if (confirmingWipe) setConfirmingWipe(false)
        else if (query) setQuery('')
        else close()
        break
    }
  }

  const keyHandler = useRef(onKeyDown)
  keyHandler.current = onKeyDown
  useEffect(() => {
    const h = (ev: KeyboardEvent) => keyHandler.current(ev)
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [])

  const liveLabel = live.state === 'idle' ? null : copy.history.live[live.state]

  return (
    <div
      className="panel"
      role="dialog"
      aria-label="History"
      style={{ display: 'flex', flexDirection: 'column', maxHeight: 520 }}
    >
      <input
        ref={searchRef}
        type="search"
        aria-label={copy.history.searchPlaceholder}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder={copy.history.searchPlaceholder}
        disabled={enabled === false}
        style={{
          border: 'none',
          borderBottom: '1px solid var(--rule)',
          background: 'var(--field)',
          padding: 'var(--s-3)',
          font: 'inherit',
          color: 'inherit',
          borderRadius: 'var(--radius-panel) var(--radius-panel) 0 0',
        }}
      />

      <div ref={listRef} role="listbox" aria-label="Transcripts" style={{ overflowY: 'auto', flex: 1 }}>
        {liveLabel && (
          <div
            className="row"
            role="status"
            style={{ padding: 'var(--s-3)', display: 'flex', alignItems: 'center', gap: 'var(--s-2)' }}
          >
            <span
              aria-hidden
              style={{
                width: 8,
                height: 8,
                borderRadius: 4,
                background: live.state === 'recording' ? 'var(--signal)' : 'var(--ink-dim)',
              }}
            />
            <span className="meta">
              {liveLabel}
              {live.state === 'recording' && (
                <span className="numeric"> · {Math.round(live.elapsedMs / 1000)} s</span>
              )}
            </span>
          </div>
        )}

        {enabled === false && (
          <p className="meta" style={{ padding: 'var(--s-4)' }}>
            {copy.history.disabled}
          </p>
        )}

        {/* Loading: rows arrive progressively. No skeleton shimmer — docs/UI-KIT.md. */}
        {enabled !== false && entries === null && !error && (
          <p className="meta" style={{ padding: 'var(--s-4)' }}>
            {copy.history.loading}
          </p>
        )}

        {error && (
          <div style={{ padding: 'var(--s-4)', color: 'var(--fail)' }}>
            {error.userMessage}
            {error.actionLabel && (
              <div style={{ marginTop: 'var(--s-2)' }}>
                <button style={{ ...buttonStyle, border: '1px solid var(--rule)' }} onClick={() => void load()}>
                  {error.actionLabel}
                </button>
              </div>
            )}
          </div>
        )}

        {enabled !== false && entries?.length === 0 && !error && (
          <p className="meta" style={{ padding: 'var(--s-4)' }}>
            {query ? copy.history.noMatch(query) : copy.history.empty}
          </p>
        )}

        {enabled !== false &&
          entries?.map((e, i) => {
            const isSelected = i === selected
            const isExpanded = expanded === e.id
            const failed = e.outcome === 'clipboardOnly'
            return (
              <div
                key={e.id}
                data-index={i}
                role="option"
                aria-selected={isSelected}
                className="row"
                onMouseEnter={() => setSelected(i)}
                onClick={() => void copyRow(e, true)}
                style={{
                  padding: 'var(--s-3)',
                  cursor: 'pointer',
                  background: isSelected ? 'var(--field)' : 'transparent',
                }}
              >
                <div className="meta" style={{ display: 'flex', gap: 'var(--s-1)', flexWrap: 'wrap' }}>
                  <span>{relative(e.createdAt)}</span>
                  {e.targetApp && <span>· {e.targetApp}</span>}
                  <span>· {copy.history.words(e.wordCount)}</span>
                  {e.contextTerms > 0 && <span>· {copy.history.hints(e.contextTerms)}</span>}
                  {failed && (
                    <span style={{ color: 'var(--fail)' }}>
                      · {copy.history.notInserted(e.outcomeNote ?? '')}
                    </span>
                  )}
                </div>
                <div
                  className="transcript"
                  style={
                    isExpanded
                      ? { marginTop: 'var(--s-1)', whiteSpace: 'pre-wrap' }
                      : {
                          display: '-webkit-box',
                          WebkitLineClamp: 2,
                          WebkitBoxOrient: 'vertical',
                          overflow: 'hidden',
                          marginTop: 'var(--s-1)',
                        }
                  }
                >
                  {e.text}
                </div>
                <div
                  className="meta"
                  style={{
                    display: 'flex',
                    justifyContent: 'flex-end',
                    gap: 'var(--s-1)',
                    marginTop: 'var(--s-1)',
                    alignItems: 'center',
                  }}
                  onClick={(ev) => ev.stopPropagation()}
                >
                  {note?.id === e.id && (
                    <span
                      role="status"
                      style={{ marginRight: 'auto', color: note.failed ? 'var(--fail)' : 'var(--ink-dim)' }}
                    >
                      {note.text}
                    </span>
                  )}
                  {e.text.length > LONG_TEXT && (
                    <button style={buttonStyle} onClick={() => setExpanded(isExpanded ? null : e.id)}>
                      {isExpanded ? copy.history.less : copy.history.more}
                    </button>
                  )}
                  <button style={buttonStyle} onClick={() => void copyRow(e, false)}>
                    {copy.history.copy}
                  </button>
                  <button style={buttonStyle} onClick={() => void insertRow(e)}>
                    {copy.history.insert}
                  </button>
                  <button
                    style={buttonStyle}
                    aria-label={copy.history.delete}
                    title={copy.history.delete}
                    onClick={() => void deleteRow(e)}
                  >
                    ×
                  </button>
                </div>
              </div>
            )
          })}
      </div>

      <div
        className="meta"
        style={{
          padding: 'var(--s-2) var(--s-3)',
          borderTop: '1px solid var(--rule)',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          gap: 'var(--s-2)',
        }}
      >
        {confirmingWipe ? (
          <>
            <span>{copy.history.wipeConfirm(entries?.length ?? 0)}</span>
            <span style={{ display: 'flex', gap: 'var(--s-1)' }}>
              <button style={{ ...buttonStyle, color: 'var(--fail)' }} onClick={() => void wipe()}>
                {copy.history.wipeYes}
              </button>
              <button style={buttonStyle} onClick={() => setConfirmingWipe(false)}>
                {copy.history.wipeNo}
              </button>
            </span>
          </>
        ) : (
          <>
            <span>{copy.history.items(entries?.length ?? 0)}</span>
            {enabled !== false && (entries?.length ?? 0) > 0 && (
              <button style={buttonStyle} onClick={() => setConfirmingWipe(true)}>
                {copy.history.deleteAll}
              </button>
            )}
          </>
        )}
      </div>
    </div>
  )
}
