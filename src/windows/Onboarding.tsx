/**
 * Onboarding: three steps, each revisitable from Settings, progress saved so quitting
 * part-way does not start over (docs/UI-SPEC.md, "Onboarding"). Step one says when you do
 * not need this (docs/VALUES.md). No marketing.
 */

import { useEffect, useRef, useState } from 'react'
import { commands, toDisplayError } from '../lib/commands'
import type { ModelInfo, PermissionReport, Settings } from '../lib/contract'
import { copy } from '../lib/copy'
import { useVoxEvent } from '../lib/events'
import { useSettings, type DeepPartial } from '../lib/useSettings'
import { Button, Note, Row, Toggle } from '../ui/form'

const TOTAL = 3

function stepFromUrl(fallback: number): number {
  const p = Number(new URLSearchParams(location.search).get('step'))
  return p >= 1 && p <= TOTAL ? p : fallback
}

export function Onboarding() {
  const store = useSettings()
  const [step, setStep] = useState<number | null>(null)

  useEffect(() => {
    if (store.settings && step === null) {
      setStep(stepFromUrl(Math.min(TOTAL, store.settings.onboarding.completedStep + 1)))
    }
  }, [store.settings, step])

  useEffect(() => {
    if (step === null) return
    const q = new URLSearchParams(location.search)
    q.set('step', String(step))
    history.replaceState(null, '', `?${q}`)
  }, [step])

  if (!store.settings || step === null) {
    return (
      <div className="panel" style={{ width: 'auto', minHeight: '100vh', borderRadius: 0, boxShadow: 'none', padding: 'var(--s-8)' }}>
        <p className="meta">{store.error?.userMessage ?? copy.history.loading}</p>
      </div>
    )
  }

  const settings = store.settings
  const advance = async () => {
    await store.patch({ onboarding: { completedStep: Math.max(settings.onboarding.completedStep, step) } })
    setStep(Math.min(TOTAL, step + 1))
  }
  const finish = async () => {
    await store.patch({ onboarding: { completedStep: TOTAL, done: true } })
    void commands.panel_hide()
  }

  return (
    <div
      className="panel"
      style={{ width: 'auto', minHeight: '100vh', borderRadius: 0, boxShadow: 'none', display: 'flex', flexDirection: 'column' }}
    >
      <div className="meta" style={{ padding: 'var(--s-4) var(--s-8) 0' }}>
        {copy.onboarding.stepOf(step, TOTAL)}
      </div>
      <main style={{ flex: 1, padding: 'var(--s-4) var(--s-8)', maxWidth: 560 }}>
        {store.error && (
          <p role="alert" style={{ color: 'var(--fail)', marginTop: 0 }}>
            {store.error.userMessage}
          </p>
        )}
        {step === 1 && <Intro />}
        {step === 2 && <SetUp />}
        {step === 3 && <ChoicesAndTryIt settings={settings} patch={store.patch} />}
      </main>
      <div style={{ display: 'flex', justifyContent: 'space-between', padding: 'var(--s-4) var(--s-8)', borderTop: '1px solid var(--rule)' }}>
        <span>{step > 1 && <Button onClick={() => setStep(step - 1)}>{copy.onboarding.back}</Button>}</span>
        {step < TOTAL ? (
          <Button primary onClick={() => void advance()}>{copy.onboarding.next}</Button>
        ) : (
          <Button primary onClick={() => void finish()}>{copy.onboarding.done}</Button>
        )}
      </div>
    </div>
  )
}

function Title({ children }: { children: string }) {
  return <h1 style={{ font: '600 var(--t-title-size) / var(--t-title-lh) var(--font-ui)', margin: '0 0 var(--s-4)' }}>{children}</h1>
}

function Intro() {
  const c = copy.onboarding.steps.intro
  return (
    <>
      <Title>{c.title}</Title>
      {c.lines.map((l) => (
        <p key={l} style={{ margin: '0 0 var(--s-3)' }}>{l}</p>
      ))}
      <p style={{ margin: 'var(--s-6) 0 var(--s-3)', padding: 'var(--s-3) var(--s-4)', borderLeft: '2px solid var(--rule)' }}>
        <em>{c.honest}</em>
      </p>
      <p className="meta" style={{ margin: 0 }}>{c.adds}</p>
    </>
  )
}

function SetUp() {
  const c = copy.onboarding.steps.setup
  const [report, setReport] = useState<PermissionReport | null>(null)
  const [level, setLevel] = useState(0)
  const [peak, setPeak] = useState(0)
  const [models, setModels] = useState<ModelInfo[] | null>(null)
  const [engineError, setEngineError] = useState<string | null>(null)

  useEffect(() => {
    commands.permissions_status().then(setReport).catch(() => {})
    commands.models_list().then(setModels).catch((e) => setEngineError(toDisplayError(e).userMessage))
    void commands.mic_test_start()
    return () => void commands.mic_test_stop()
  }, [])
  useVoxEvent('vox://permissions-changed', setReport, [])
  useVoxEvent('vox://level', ({ rms }) => {
    setLevel(rms)
    setPeak((p) => Math.max(p, rms))
  }, [])

  const mic = report?.microphone
  const engineReady = models?.some((m) => m.installed) ?? false

  const grantRow = (key: 'inputMonitoring' | 'accessibility', label: string, hint: string) => {
    const status = report?.[key]
    return (
      <Row
        key={key}
        label={label}
        hint={status === 'granted' ? c.granted : status === 'needsRestart' ? c.needsRestart : hint}
      >
        {status === 'denied' && <Button onClick={() => void commands.permissions_open_pane({ which: key })}>{c.open}</Button>}
        {status === 'needsRestart' && <Button primary onClick={() => void commands.app_relaunch()}>{c.quitAndReopen}</Button>}
      </Row>
    )
  }

  return (
    <>
      <Title>{c.title}</Title>
      <p>{c.body}</p>
      <div style={{ borderTop: '1px solid var(--rule)' }}>
        <Row
          label={c.microphone}
          hint={
            mic === 'denied' ? <span style={{ color: 'var(--fail)' }}>{c.denied}</span>
            : peak > 0.02 ? c.hearing
            : c.micHint
          }
        >
          {mic === 'denied' ? (
            <Button onClick={() => void commands.permissions_open_pane({ which: 'microphone' })}>{c.allow}</Button>
          ) : (
            <div
              role="meter"
              aria-label={c.microphone}
              aria-valuemin={0}
              aria-valuemax={1}
              aria-valuenow={Math.min(1, level)}
              style={{ width: 160, height: 8, background: 'var(--field)', borderRadius: 4, overflow: 'hidden' }}
            >
              <div style={{ width: `${Math.min(100, level * 100)}%`, height: '100%', background: 'var(--signal)' }} />
            </div>
          )}
        </Row>
        {grantRow('inputMonitoring', c.inputMonitoring, c.inputMonitoringHint)}
        {grantRow('accessibility', c.accessibility, c.accessibilityHint)}
        <Row label={c.engine} hint={c.engineHint}>
          {models === null && !engineError && <Note>{c.checking}</Note>}
          {engineError && <Note fail>{engineError}</Note>}
          {models && (engineReady ? <Note>{c.ready}</Note> : <Note fail>{c.unavailable}</Note>)}
        </Row>
      </div>
    </>
  )
}

function ChoicesAndTryIt({ settings, patch }: { settings: Settings; patch: (p: DeepPartial<Settings>) => Promise<void> }) {
  const c = copy.onboarding.steps.tryIt
  const [text, setText] = useState('')
  const [arrived, setArrived] = useState(false)
  const ref = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    ref.current?.focus()
  }, [])
  // The real core inserts into this focused textarea like any other field. The mock cannot
  // type, so in mock mode a completed scripted dictation drops its sentence in here.
  useVoxEvent('vox://insertion-result', (r) => {
    if (r.outcome === 'inserted' && !('__TAURI_INTERNALS__' in window)) {
      setText((t) => (t ? `${t} ` : '') + 'Can you send me the link to the dashboard when you get a second.')
    }
  }, [])
  useEffect(() => {
    if (text.trim()) setArrived(true)
  }, [text])

  return (
    <>
      <Title>{c.title}</Title>
      <p className="meta" style={{ marginTop: 0 }}>{c.choices}</p>
      <div style={{ borderTop: '1px solid var(--rule)', marginBottom: 'var(--s-6)' }}>
        <Row label={c.capture} hint={c.captureHint}>
          <Toggle label={c.capture} checked={settings.learning.captureCorrections} onChange={(captureCorrections) => void patch({ learning: { captureCorrections } })} />
        </Row>
        <Row label={copy.settings.privacy.readField} hint={copy.settings.privacy.readFieldHint}>
          <Toggle label={copy.settings.privacy.readField} checked={settings.privacy.readFocusedField} onChange={(readFocusedField) => void patch({ privacy: { readFocusedField } })} />
        </Row>
      </div>
      <p>{c.body}</p>
      <textarea
        ref={ref}
        aria-label={c.title}
        className="transcript"
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder={c.placeholder}
        rows={4}
        style={{
          width: '100%',
          boxSizing: 'border-box',
          color: 'inherit',
          background: 'var(--field)',
          border: '1px solid var(--rule)',
          borderRadius: 'var(--radius-input)',
          padding: 'var(--s-3)',
          resize: 'none',
        }}
      />
      <div style={{ marginTop: 'var(--s-3)' }}>{arrived && <Note>{c.success}</Note>}</div>
    </>
  )
}
