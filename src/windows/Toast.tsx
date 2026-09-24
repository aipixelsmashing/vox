/**
 * The toast: Vox's own surface for the one message per failure (docs/UI-SPEC.md,
 * "Notifications"). A small pill at the bottom of the screen, never focused, gone after a
 * few seconds or on click. The words are the deck's, identical to the notification and the
 * history row; this window only shows them.
 */

import { useEffect, useState } from 'react'
import { commands } from '../lib/commands'
import { useVoxEvent } from '../lib/events'

const LIFETIME_MS = 4500

export function Toast() {
  const [message, setMessage] = useState<string | null>(null)

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
