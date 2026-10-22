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
import type { HistoryEntry, InjectionOutcome, PipelineState, Settings, TextSize, VoxError } from '../lib/contract'
import { copy } from '../lib/copy'
import { useVoxEvent } from '../lib/events'

const PAGE = 50
/** Transcripts longer than this get a More/Less control; shorter ones fit in two lines. */
const LONG_TEXT = 160
/** The three text sizes as factors of the 13px medium: 12, 13 and 15 px (docs/UI-KIT.md). */
const TEXT_STEPS: readonly TextSize[] = ['small', 'medium', 'large']
const TEXT_FACTOR: Record<TextSize, number> = { small: 12 / 13, medium: 1, large: 15 / 13 }

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
  const [textSize, setTextSize] = useState<TextSize>('medium')
  /** The Mac's Larger Text factor, applied on top of the person's choice in Vox. */
  const [systemScale, setSystemScale] = useState(1)
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
    const read = () =>
      commands
        .settings_get()
        .then((s) => {
          if (cancelled) return
          setEnabled(s.history.enabled)
          setTextSize(s.ui.textSize ?? 'medium')
        })
        .catch(() => !cancelled && setEnabled(true))
    void read()
    commands
      .ui_text_scale()
      .then((r) => !cancelled && setSystemScale(r.scale))
      .catch(() => {})
    // The window is hidden, not closed, between uses: re-read on each return so a change
    // made in Settings (or in System Settings) shows up without a relaunch.
    const onFocus = () => {
      void read()
      commands.ui_text_scale().then((r) => !cancelled && setSystemScale(r.scale)).catch(() => {})
    }
    window.addEventListener('focus', onFocus)
    return () => {
      cancelled = true
      window.removeEventListener('focus', onFocus)
    }
  }, [])

  useEffect(() => {
    document.documentElement.style.setProperty('--text-scale', String(TEXT_FACTOR[textSize] * systemScale))
  }, [textSize, systemScale])

  /** ⌘+ / ⌘−: one step, persisted; the core's merged answer is the truth. */
  const stepTextSize = async (delta: 1 | -1) => {
    const next = TEXT_STEPS[Math.min(TEXT_STEPS.length - 1, Math.max(0, TEXT_STEPS.indexOf(textSize) + delta))] ?? textSize
    if (next === textSize) return
    setTextSize(next)
    try {
      const merged = await commands.settings_set({ ui: { textSize: next } } as unknown as Partial<Settings>)
      setTextSize(merged.ui.textSize ?? next)
    } catch {
      /* the size still changed for this session; the toast layer is not the panel's */
    }
  }

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
    if (ev.metaKey && !ev.ctrlKey && !ev.altKey) {
      // The Mac's own chords: ⌘, for Settings, ⌘+ / ⌘− for text size, as in every app.
      if (ev.key === ',') {
        ev.preventDefault()
        void commands.settings_open()
        return
      }
      if (ev.key === '=' || ev.key === '+') {
        ev.preventDefault()
        void stepTextSize(1)
        return
      }
      if (ev.key === '-' || ev.key === '_') {
        ev.preventDefault()
        void stepTextSize(-1)
        return
      }
    }
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
      {/* Header: search, the live dictation state while there is one, and the way to Settings.
          The tray menu no longer lists Settings (docs/UI-SPEC.md), so this cog is the route. */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 'var(--s-2)',
          borderBottom: '1px solid var(--rule)',
          background: 'var(--field)',
          paddingRight: 'var(--s-2)',
          borderRadius: 'var(--radius-panel) var(--radius-panel) 0 0',
        }}
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 16 16"
          fill="none"
          aria-hidden
          style={{ marginLeft: 'var(--s-3)', color: 'var(--ink-dim)', flexShrink: 0 }}
        >
          <circle cx="7" cy="7" r="4.75" stroke="currentColor" strokeWidth="1.5" />
          <path d="M10.5 10.5 14 14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </svg>
        <input
          ref={searchRef}
          type="search"
          aria-label={copy.history.searchPlaceholder}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={copy.history.searchPlaceholder}
          disabled={enabled === false}
          style={{
            flex: 1,
            minWidth: 0,
            border: 'none',
            background: 'transparent',
            padding: 'var(--s-3) var(--s-2)',
            font: 'inherit',
            color: 'inherit',
          }}
        />
        {liveLabel && (
          <span
            role="status"
            className="meta"
            style={{ display: 'flex', alignItems: 'center', gap: 'var(--s-2)', whiteSpace: 'nowrap' }}
          >
            {/* mic / mic.fill at SF regular weight: filled and amber only while listening */}
            <svg
              width="14"
              height="14"
              viewBox="0 0 16 16"
              aria-hidden
              style={{ color: live.state === 'recording' ? 'var(--signal)' : 'var(--ink-dim)' }}
            >
              <rect
                x="6"
                y="1.5"
                width="4"
                height="8"
                rx="2"
                fill={live.state === 'recording' ? 'currentColor' : 'none'}
                stroke="currentColor"
                strokeWidth="1.4"
              />
              <path
                d="M3.75 7.25v.5a4.25 4.25 0 0 0 8.5 0v-.5M8 12v2.25M5.5 14.25h5"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.4"
                strokeLinecap="round"
              />
            </svg>
            {liveLabel}
            {live.state === 'recording' && (
              <span className="numeric">{Math.round(live.elapsedMs / 1000)} s</span>
            )}
          </span>
        )}
        <button
          style={{ ...buttonStyle, display: 'flex', padding: 6, color: 'var(--ink-dim)' }}
          aria-label={copy.history.settings}
          title={copy.history.settings}
          onClick={() => void commands.settings_open()}
        >
          {/* gearshape at SF regular weight, not a sun: the same glyph Settings uses everywhere */}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
            <path d="M13.54 7.21 L15.08 7.44 L15.08 8.56 L13.54 8.79 L12.48 11.36 L13.40 12.61 L12.61 13.40 L11.36 12.48 L8.79 13.54 L8.56 15.08 L7.44 15.08 L7.21 13.54 L4.64 12.48 L3.39 13.40 L2.60 12.61 L3.52 11.36 L2.46 8.79 L0.92 8.56 L0.92 7.44 L2.46 7.21 L3.52 4.64 L2.60 3.39 L3.39 2.60 L4.64 3.52 L7.21 2.46 L7.44 0.92 L8.56 0.92 L8.79 2.46 L11.36 3.52 L12.61 2.60 L13.40 3.39 L12.48 4.64 Z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
            <circle cx="8" cy="8" r="2.1" stroke="currentColor" strokeWidth="1.4" />
          </svg>
        </button>
      </div>

      <div
        ref={listRef}
        role="listbox"
        aria-label="Transcripts"
        style={{ overflowY: 'auto', flex: 1, paddingBottom: 'var(--s-1)' }}
      >
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
                className="hrow"
                onMouseEnter={() => setSelected(i)}
                onClick={() => void copyRow(e, true)}
                style={{ cursor: 'pointer' }}
              >
                <div className="meta" style={{ display: 'flex', gap: 'var(--s-1)', flexWrap: 'wrap' }}>
                  <span>{relative(e.createdAt)}</span>
                  {e.targetApp && <span>· {e.targetApp}</span>}
                  <span>· {copy.history.words(e.wordCount)}</span>
                  {e.contextTerms > 0 && <span>· {copy.history.hints(e.contextTerms)}</span>}
                  {failed && <span className="fail">· {copy.history.notInserted(e.outcomeNote ?? '')}</span>}
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
                  className="meta actions"
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
                    <span role="status" className={note.failed ? 'fail' : undefined} style={{ marginRight: 'auto' }}>
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
