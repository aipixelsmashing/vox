/**
 * The recording overlay: a small pill near the caret while a dictation is in progress
 * (docs/UI-SPEC.md, "Recording overlay"). It answers one question, "is it hearing me?",
 * with the level ring, and shows the elapsed time so the cap is never a surprise. It never
 * takes focus, never accepts a click, and disappears the moment the text is placed.
 *
 * The core shows and hides the window and positions it; this page only draws. Driven by
 * `vox://state` and `vox://level`, both of which the mock's `recording` scenario scripts.
 */

import { useEffect, useRef, useState } from 'react'
import type { PipelineState } from '../lib/contract'
import { copy } from '../lib/copy'
import { useVoxEvent } from '../lib/events'

type Phase = 'idle' | 'recording' | 'transcribing' | 'injecting'

/** The pill's fixed size; the core sizes the window to it plus room for the shadow. */
const PILL_WIDTH = 180
const PILL_HEIGHT = 36
const SHADOW_ROOM = 4

function format(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000))
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
}

export function RecordingOverlay() {
  const [phase, setPhase] = useState<Phase>('idle')
  const [startedAt, setStartedAt] = useState<number | null>(null)
  const [level, setLevel] = useState(0)
  const [, tick] = useState(0)
  const reduced = useRef(
    typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches,
  )

  useVoxEvent('vox://state', (s: PipelineState) => {
    switch (s.state) {
      case 'idle':
        setPhase('idle')
        setStartedAt(null)
        setLevel(0)
        break
      case 'arming':
        setPhase('recording')
        setStartedAt((a) => a ?? Date.now())
        break
      case 'recording':
        setPhase('recording')
        // The core's elapsed is the truth; the local clock only fills the gaps between ticks.
        setStartedAt(Date.now() - s.elapsedMs)
        break
      default:
        setPhase(s.state)
    }
  }, [])

  // A level event can only mean recording. The window may have been created after the
  // first state event; this keeps the ring live until the next state tick corrects it.
  useVoxEvent('vox://level', ({ rms }) => {
    setLevel((prev) => Math.max(Math.min(1, rms), prev * 0.72))
    setPhase((p) => (p === 'idle' ? 'recording' : p))
  }, [])

  useEffect(() => {
    if (phase !== 'recording') return
    const t = setInterval(() => tick((n) => n + 1), 250)
    return () => clearInterval(t)
  }, [phase])

  if (phase === 'idle') return null

  const working = phase !== 'recording'
  const elapsed = startedAt === null ? 0 : Date.now() - startedAt
  const label = working ? copy.history.live[phase] : format(elapsed)

  return (
    <div
      role="status"
      aria-live="off"
      aria-label={working ? label : `${copy.history.live.recording} ${label}`}
      style={{
        boxSizing: 'border-box',
        width: PILL_WIDTH,
        height: PILL_HEIGHT,
        margin: SHADOW_ROOM,
        padding: '0 var(--s-3)',
        display: 'flex',
        alignItems: 'center',
        gap: 'var(--s-3)',
        background: 'var(--panel)',
        borderRadius: PILL_HEIGHT / 2,
        boxShadow: '0 4px 16px #00000026',
        pointerEvents: 'none',
        userSelect: 'none',
        whiteSpace: 'nowrap',
        overflow: 'hidden',
      }}
    >
      {reduced.current ? <StaticMeter level={level} working={working} /> : <LevelRing level={level} working={working} />}
      <span className={working ? undefined : 'numeric'} style={{ color: working ? 'var(--ink-dim)' : 'var(--ink)' }}>
        {label}
      </span>
    </div>
  )
}

/**
 * The one piece of motion the kit allows (docs/UI-KIT.md, "Motion"): a ring whose fill
 * follows the input level. Amber, because amber means listening and nothing else. Once the
 * key is released the ring fills solid: working, not listening.
 */
function LevelRing({ level, working }: { level: number; working: boolean }) {
  const scale = working ? 1 : 0.25 + level * 0.75
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden style={{ flexShrink: 0 }}>
      <circle cx="10" cy="10" r="8.5" fill="none" stroke="var(--signal)" strokeWidth="1.5" />
      <circle
        cx="10"
        cy="10"
        r="8.5"
        fill="var(--signal)"
        style={{
          transformOrigin: '10px 10px',
          transform: `scale(${scale.toFixed(3)})`,
          transition: 'transform 50ms linear',
        }}
      />
    </svg>
  )
}

/** Under prefers-reduced-motion the ring becomes a static three-step meter (UI-KIT.md). */
function StaticMeter({ level, working }: { level: number; working: boolean }) {
  const steps = [0.12, 0.35, 0.65]
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden style={{ flexShrink: 0 }}>
      {steps.map((threshold, i) => {
        const lit = working || level >= threshold
        const h = 6 + i * 4
        return (
          <rect
            key={threshold}
            x={3 + i * 6}
            y={17 - h}
            width="3"
            height={h}
            rx="1"
            fill={lit ? 'var(--signal)' : 'var(--rule)'}
          />
        )
      })}
    </svg>
  )
}
