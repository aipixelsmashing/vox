/**
 * The toast: Vox's own surface for the one message per failure (docs/UI-SPEC.md,
 * "Notifications"). A small pill at the bottom of the screen, never focused, gone after a
 * few seconds or on click. The words are the deck's, identical to the notification and the
 * history row; this window only shows them.
 */

import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { commands } from '../lib/commands'
import { useVoxEvent } from '../lib/events'

const LIFETIME_MS = 4500

export function Toast() {
  const [message, setMessage] = useState<string | null>(null)
  const pill = useRef<HTMLDivElement>(null)

  // Size to the text: deck strings run from one line to three. Measured after layout, then
  // the core resizes the window to fit and shows it.
  useLayoutEffect(() => {
    if (!message || !pill.current) return
    const height = Math.ceil(pill.current.getBoundingClientRect().height)
    void commands.toast_fit({ height })
  }, [message])

  useEffect(() => {
    commands
      .toast_current()
      .then((t) => t && setMessage(t.message))
      .catch(() => {})
  }, [])
  useVoxEvent('vox://toast', ({ message }) => setMessage(message), [])

  // The core hides the window on its own timer; in a browser tab nothing does, so the
  // page fades itself as well, on the same clock.
  useEffect(() => {
    if (!message) return
    const t = setTimeout(() => setMessage(null), LIFETIME_MS)
    return () => clearTimeout(t)
  }, [message])

  if (!message) return null
  return (
    <div
      ref={pill}
      role="status"
      aria-live="polite"
      onClick={() => {
        setMessage(null)
        void commands.panel_hide()
      }}
      className="panel"
      style={{
        boxSizing: 'border-box',
        padding: 'var(--s-3) var(--s-4)',
        display: 'flex',
        alignItems: 'center',
        gap: 'var(--s-3)',
        cursor: 'pointer',
        userSelect: 'none',
      }}
    >
      <span aria-hidden style={{ width: 8, height: 8, borderRadius: 4, background: 'var(--fail)', flexShrink: 0 }} />
      <span>{message}</span>
    </div>
  )
}
