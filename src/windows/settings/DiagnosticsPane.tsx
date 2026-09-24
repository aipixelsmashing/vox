/**
 * Diagnostics: the user's own numbers, computed locally, so they can judge whether Vox is
 * earning its place (docs/UI-SPEC.md). No achievements, no score.
 */

import { useEffect, useState } from 'react'
import { commands, toDisplayError } from '../../lib/commands'
import type { Diagnostics, VoxError } from '../../lib/contract'
import { copy } from '../../lib/copy'
import { Row, Section } from '../../ui/form'

const cell: React.CSSProperties = { padding: 'var(--s-2) var(--s-3) var(--s-2) 0', textAlign: 'right', whiteSpace: 'nowrap' }
const head: React.CSSProperties = { ...cell, fontWeight: 400, color: 'var(--ink-dim)', fontSize: 'var(--t-meta-size)' }

export function DiagnosticsPane() {
  const [d, setD] = useState<Diagnostics | null>(null)
  const [error, setError] = useState<VoxError | null>(null)

  useEffect(() => {
    commands
      .diagnostics_recent({ limit: 12 })
      .then(setD)
      .catch((e) => setError(toDisplayError(e)))
  }, [])

  if (error) return <p style={{ color: 'var(--fail)' }}>{error.userMessage}</p>
  if (!d) return <p className="meta">{copy.history.loading}</p>

  const trend = d.correctionsPer100Words
  const max = Math.max(1, ...trend.map((t) => t.value))

  return (
    <>
      <Section title={copy.settings.diagnostics.trendTitle} note={copy.settings.diagnostics.trendNote}>
        {trend.length < 2 ? (
          <Row label={copy.settings.diagnostics.thin} />
        ) : (
          <table className="numeric" style={{ borderCollapse: 'collapse', marginTop: 'var(--s-2)' }}>
            <thead>
              <tr>
                <th style={{ ...head, textAlign: 'left' }}>{copy.settings.diagnostics.week}</th>
                <th style={head}>{copy.settings.diagnostics.per100}</th>
                <th style={{ ...head, textAlign: 'left' }} aria-hidden />
              </tr>
            </thead>
            <tbody>
              {trend.map((t) => (
                <tr key={t.weekStart}>
                  <td style={{ ...cell, textAlign: 'left' }}>{new Date(t.weekStart).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}</td>
                  <td style={cell}>{t.value.toFixed(1)}</td>
                  <td style={{ ...cell, width: 200 }} aria-hidden>
                    <div style={{ height: 6, width: `${(t.value / max) * 100}%`, background: 'var(--ink-dim)', borderRadius: 3 }} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>

      <Section title={copy.settings.diagnostics.appsTitle}>
        <table className="numeric" style={{ borderCollapse: 'collapse', marginTop: 'var(--s-2)' }}>
          <thead>
            <tr>
              <th style={{ ...head, textAlign: 'left' }}>{copy.settings.diagnostics.app}</th>
              <th style={head}>{copy.settings.diagnostics.inserted}</th>
              <th style={head}>{copy.settings.diagnostics.clipboardOnly}</th>
            </tr>
          </thead>
          <tbody>
            {d.insertionsByApp.map((a) => (
              <tr key={a.app}>
                <td style={{ ...cell, textAlign: 'left' }}>{a.app}</td>
                <td style={cell}>{a.inserted}</td>
                <td style={{ ...cell, color: a.clipboardOnly > 0 ? 'var(--fail)' : undefined }}>{a.clipboardOnly}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Section>

      <Section title={copy.settings.diagnostics.timingsTitle} note={copy.settings.diagnostics.timingsNote}>
        <Row label={copy.settings.diagnostics.capture}>
          <span className="numeric">{d.stageTimingsMs.capture} ms</span>
        </Row>
        <Row label={copy.settings.diagnostics.inference}>
          <span className="numeric">{d.stageTimingsMs.inference} ms</span>
        </Row>
        <Row label={copy.settings.diagnostics.injection}>
          <span className="numeric">{d.stageTimingsMs.injection} ms</span>
        </Row>
        <Row label={copy.settings.diagnostics.memory}>
          <span className="numeric">
            {d.memory.idleRssMb} MB {copy.settings.diagnostics.idle} · {d.memory.peakRssMb} MB {copy.settings.diagnostics.peak}
          </span>
        </Row>
      </Section>
    </>
  )
}
