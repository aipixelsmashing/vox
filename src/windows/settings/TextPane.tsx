/**
 * Text: insertion method, clipboard restore, spacing and capitalisation, the defaults Vox
 * learned for each app (docs/adr/0019), manual dictionary.
 */

import { useEffect, useState } from 'react'
import { commands, toDisplayError } from '../../lib/commands'
import type { AppDefault, Settings, VoxError } from '../../lib/contract'
import { copy } from '../../lib/copy'
import type { DeepPartial } from '../../lib/useSettings'
import { Button, Row, Section, Select, TextInput, Toggle } from '../../ui/form'

function since(ts: number): string {
  return new Date(ts).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
}

/** What a default does, in the deck's words. Pure, so it can be tested without a DOM. */
export function describeDefault(d: AppDefault): string {
  const p = copy.settings.text.perApp
  if (d.aspect === 'method') return p.paste
  if (d.aspect === 'trailingSpace') return d.value === 'true' ? p.trailingSpaceOn : p.trailingSpaceOff
  return d.value === 'true' ? p.collapseOn : p.collapseOff
}

export function describeProvenance(d: AppDefault): string {
  if (d.state === 'suspended') return copy.settings.vocabulary.suspended
  if (d.source === 'outcomes') return copy.settings.text.perAppOutcomes
  return copy.settings.text.perAppFixes(d.count, since(d.firstSeen))
}

export function TextPane({ settings, patch }: { settings: Settings; patch: (p: DeepPartial<Settings>) => Promise<void> }) {
  const o = settings.output
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')

  const setDictionary = (dictionary: Array<{ from: string; to: string }>) => void patch({ output: { dictionary } })

  const [defaults, setDefaults] = useState<AppDefault[] | null>(null)
  const [defaultsError, setDefaultsError] = useState<VoxError | null>(null)
  const [showWaiting, setShowWaiting] = useState(false)
  useEffect(() => {
    commands
      .app_defaults_list()
      .then((d) => {
        setDefaults(d)
        setDefaultsError(null)
      })
      .catch((e) => setDefaultsError(toDisplayError(e)))
  }, [])
  const reset = async (d: AppDefault) => {
    try {
      await commands.app_defaults_reset({ bundleId: d.bundleId, aspect: d.aspect })
      setDefaults((list) => list?.filter((x) => !(x.bundleId === d.bundleId && x.aspect === d.aspect)) ?? null)
    } catch (e) {
      setDefaultsError(toDisplayError(e))
    }
  }
  const resetAll = async () => {
    try {
      await commands.app_defaults_reset_all()
      setDefaults([])
    } catch (e) {
      setDefaultsError(toDisplayError(e))
    }
  }
  const learned = defaults?.filter((d) => d.state !== 'candidate') ?? []
  const waiting = defaults?.filter((d) => d.state === 'candidate') ?? []
  const t = copy.settings.text
  const defaultRow = (d: AppDefault) => (
    <Row
      key={`${d.bundleId}:${d.aspect}`}
      label={`${d.state === 'suspended' ? '⚠ ' : ''}${d.app} — ${describeDefault(d)}`}
      hint={describeProvenance(d)}
    >
      <Button danger label={`${t.reset} ${d.app}`} onClick={() => void reset(d)}>
        ×
      </Button>
    </Row>
  )

  const add = () => {
    if (!from.trim() || !to.trim()) return
    setDictionary([...o.dictionary, { from: from.trim(), to: to.trim() }])
    setFrom('')
    setTo('')
  }

  return (
    <>
      <Section title={copy.settings.text.title}>
        <Row label={copy.settings.text.method} hint={copy.settings.text.methodHint}>
          <Select
            label={copy.settings.text.method}
            value={o.method}
            onChange={(method) => void patch({ output: { method } })}
            options={[
              { value: 'auto', label: copy.settings.text.methods.auto },
              { value: 'accessibility', label: copy.settings.text.methods.accessibility },
              { value: 'paste', label: copy.settings.text.methods.paste },
              { value: 'type', label: copy.settings.text.methods.type },
            ]}
          />
        </Row>
        <Row label={copy.settings.text.restoreClipboard} hint={copy.settings.text.restoreHint}>
          <Toggle label={copy.settings.text.restoreClipboard} checked={o.restoreClipboard} onChange={(restoreClipboard) => void patch({ output: { restoreClipboard } })} />
        </Row>
        <Row label={copy.settings.text.onFocusChange} hint={copy.settings.text.onFocusChangeHint}>
          <Select
            label={copy.settings.text.onFocusChange}
            value={o.onFocusChange}
            onChange={(onFocusChange) => void patch({ output: { onFocusChange } })}
            options={[
              { value: 'clipboard', label: copy.settings.text.focus.clipboard },
              { value: 'insert-anyway', label: copy.settings.text.focus['insert-anyway'] },
            ]}
          />
        </Row>
      </Section>

      <Section title={copy.settings.text.shapeTitle}>
        <Row label={copy.settings.text.trailingSpace}>
          <Toggle label={copy.settings.text.trailingSpace} checked={o.trailingSpace} onChange={(trailingSpace) => void patch({ output: { trailingSpace } })} />
        </Row>
        <Row label={copy.settings.text.capitalizeFirst}>
          <Toggle label={copy.settings.text.capitalizeFirst} checked={o.capitalizeFirst} onChange={(capitalizeFirst) => void patch({ output: { capitalizeFirst } })} />
        </Row>
        <Row label={copy.settings.text.collapseNewlines} hint={copy.settings.text.collapseHint}>
          <Toggle label={copy.settings.text.collapseNewlines} checked={o.collapseNewlinesInTerminals} onChange={(collapseNewlinesInTerminals) => void patch({ output: { collapseNewlinesInTerminals } })} />
        </Row>
      </Section>

      <Section
        title={t.perAppTitle}
        note={t.perAppNote}
        actions={
          learned.length + waiting.length > 0 && (
            <Button onClick={() => void resetAll()}>{t.resetAll}</Button>
          )
        }
      >
        {defaults === null && !defaultsError && <Row label={copy.history.loading} />}
        {defaultsError && <Row label={defaultsError.userMessage} />}
        {defaults && learned.length === 0 && (
          <Row label={settings.learning.captureCorrections ? t.perAppEmpty : t.perAppOff} />
        )}
        {learned.map(defaultRow)}
        {waiting.length > 0 && (
          <Row label={t.perAppWaiting(waiting.length)}>
            <Button onClick={() => setShowWaiting((s) => !s)}>{showWaiting ? copy.history.less : copy.settings.vocabulary.show}</Button>
          </Row>
        )}
        {showWaiting && waiting.map(defaultRow)}
      </Section>

      <Section title={copy.settings.text.dictionaryTitle} note={copy.settings.text.dictionaryNote}>
        {o.dictionary.length === 0 && <Row label={copy.settings.text.dictionaryEmpty} />}
        {o.dictionary.map((d, i) => (
          <Row key={`${d.from}-${i}`} label={`${d.from} → ${d.to}`}>
            <Button danger onClick={() => setDictionary(o.dictionary.filter((_, j) => j !== i))}>
              {copy.history.delete}
            </Button>
          </Row>
        ))}
        <Row label={copy.settings.text.addWord}>
          <TextInput label={copy.settings.text.heard} value={from} onChange={setFrom} placeholder={copy.settings.text.heard} width={140} />
          <span className="meta">→</span>
          <TextInput label={copy.settings.text.meant} value={to} onChange={setTo} placeholder={copy.settings.text.meant} width={140} />
          <Button onClick={add} disabled={!from.trim() || !to.trim()}>
            {copy.settings.text.add}
          </Button>
        </Row>
      </Section>
    </>
  )
}
