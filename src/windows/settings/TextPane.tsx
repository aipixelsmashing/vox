/** Text: insertion method, clipboard restore, spacing and capitalisation, manual dictionary. */

import { useState } from 'react'
import type { Settings } from '../../lib/contract'
import { copy } from '../../lib/copy'
import type { DeepPartial } from '../../lib/useSettings'
import { Button, Row, Section, Select, TextInput, Toggle } from '../../ui/form'

export function TextPane({ settings, patch }: { settings: Settings; patch: (p: DeepPartial<Settings>) => Promise<void> }) {
  const o = settings.output
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')

  const setDictionary = (dictionary: Array<{ from: string; to: string }>) => void patch({ output: { dictionary } })

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
