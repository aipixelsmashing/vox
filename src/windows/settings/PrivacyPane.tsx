/**
 * Privacy: what Vox may read, what it keeps, what it sends, and the way out
 * (docs/SETTINGS.md, docs/CONTEXT.md, docs/HISTORY.md, PRIVACY.md, docs/adr/0015).
 */

import { useEffect, useState } from 'react'
import { commands, toDisplayError } from '../../lib/commands'
import type { Settings } from '../../lib/contract'
import { copy } from '../../lib/copy'
import type { DeepPartial } from '../../lib/useSettings'
import { Button, Note, NumberField, Row, Section, Select, Toggle } from '../../ui/form'

export function PrivacyPane({ settings, patch }: { settings: Settings; patch: (p: DeepPartial<Settings>) => Promise<void> }) {
  const [confirmingWipe, setConfirmingWipe] = useState(false)
  const [wipeNote, setWipeNote] = useState<string | null>(null)
  const [confirmingCorrections, setConfirmingCorrections] = useState(false)
  const [correctionsNote, setCorrectionsNote] = useState<string | null>(null)
  const [exporting, setExporting] = useState(false)
  const [exportNote, setExportNote] = useState<{ text: string; fail: boolean } | null>(null)

  useEffect(() => {
    if (!exportNote && !wipeNote && !correctionsNote) return
    const t = setTimeout(() => {
      setExportNote(null)
      setWipeNote(null)
      setCorrectionsNote(null)
    }, 6000)
    return () => clearTimeout(t)
  }, [exportNote, wipeNote, correctionsNote])

  const deleteCorrections = async () => {
    try {
      const { deleted } = await commands.vocab_forget_all()
      setCorrectionsNote(copy.settings.privacy.deletedCorrections(deleted))
    } catch (e) {
      setCorrectionsNote(toDisplayError(e).userMessage)
    } finally {
      setConfirmingCorrections(false)
    }
  }

  const wipe = async () => {
    try {
      const { deleted } = await commands.history_delete_all()
      setWipeNote(copy.settings.privacy.wiped(deleted))
    } catch (e) {
      setWipeNote(toDisplayError(e).userMessage)
    } finally {
      setConfirmingWipe(false)
    }
  }

  const exportAll = async () => {
    setExporting(true)
    try {
      const r = await commands.export_everything({ dest: '' })
      setExportNote({ text: copy.settings.privacy.exported(r.counts.transcripts ?? 0, r.counts.words ?? 0, r.path), fail: false })
    } catch (e) {
      setExportNote({ text: toDisplayError(e).userMessage, fail: true })
    } finally {
      setExporting(false)
    }
  }

  const offline = settings.network.offlineLock

  return (
    <>
      <Section title={copy.settings.privacy.readTitle}>
        <Row label={copy.settings.privacy.readField} hint={copy.settings.privacy.readFieldHint}>
          <Toggle label={copy.settings.privacy.readField} checked={settings.privacy.readFocusedField} onChange={(readFocusedField) => void patch({ privacy: { readFocusedField } })} />
        </Row>
      </Section>

      <Section title={copy.settings.privacy.historyTitle} note={copy.settings.privacy.historyNote}>
        <Row label={copy.settings.privacy.keepHistory}>
          <Toggle label={copy.settings.privacy.keepHistory} checked={settings.history.enabled} onChange={(enabled) => void patch({ history: { enabled } })} />
        </Row>
        <Row label={copy.settings.privacy.maxItems} hint={copy.settings.privacy.zeroUnlimited}>
          <NumberField label={copy.settings.privacy.maxItems} value={settings.history.maxItems} min={0} step={50} disabled={!settings.history.enabled} onChange={(maxItems) => void patch({ history: { maxItems } })} />
        </Row>
        <Row label={copy.settings.privacy.maxDays} hint={copy.settings.privacy.zeroUnlimited}>
          <NumberField label={copy.settings.privacy.maxDays} value={settings.history.maxDays} min={0} step={1} suffix={copy.settings.privacy.days} disabled={!settings.history.enabled} onChange={(maxDays) => void patch({ history: { maxDays } })} />
        </Row>
        <Row label={copy.settings.privacy.wipe} hint={confirmingWipe ? copy.settings.privacy.wipeConfirm : undefined}>
          {wipeNote && <Note>{wipeNote}</Note>}
          {confirmingWipe ? (
            <>
              <Button danger onClick={() => void wipe()}>{copy.history.wipeYes}</Button>
              <Button onClick={() => setConfirmingWipe(false)}>{copy.history.wipeNo}</Button>
            </>
          ) : (
            <Button danger onClick={() => setConfirmingWipe(true)}>{copy.settings.privacy.wipeButton}</Button>
          )}
        </Row>
      </Section>

      <Section title={copy.settings.privacy.correctionsTitle} note={copy.settings.privacy.correctionsNote}>
        <Row label={copy.settings.privacy.deleteCorrections} hint={confirmingCorrections ? copy.settings.privacy.wipeConfirm : undefined}>
          {correctionsNote && <Note>{correctionsNote}</Note>}
          {confirmingCorrections ? (
            <>
              <Button danger onClick={() => void deleteCorrections()}>{copy.history.wipeYes}</Button>
              <Button onClick={() => setConfirmingCorrections(false)}>{copy.history.wipeNo}</Button>
            </>
          ) : (
            <Button danger onClick={() => setConfirmingCorrections(true)}>{copy.settings.privacy.deleteCorrectionsButton}</Button>
          )}
        </Row>
      </Section>

      <Section title={copy.settings.privacy.networkTitle} note={copy.settings.privacy.sends}>
        <Row label={copy.settings.privacy.updateCheck} hint={offline ? copy.settings.privacy.offlineBlocks : undefined}>
          <Select
            label={copy.settings.privacy.updateCheck}
            value={settings.network.updateCheck}
            disabled={offline}
            onChange={(updateCheck) => void patch({ network: { updateCheck } })}
            options={[
              { value: 'startup', label: copy.settings.privacy.updates.startup },
              { value: 'manual', label: copy.settings.privacy.updates.manual },
              { value: 'off', label: copy.settings.privacy.updates.off },
            ]}
          />
        </Row>
        <Row label={copy.settings.privacy.offlineLock} hint={copy.settings.privacy.offlineLockHint}>
          <Toggle label={copy.settings.privacy.offlineLock} checked={offline} onChange={(offlineLock) => void patch({ network: { offlineLock } })} />
        </Row>
      </Section>

      <Section title={copy.settings.privacy.exportTitle} note={copy.settings.privacy.exportNote}>
        <Row label={copy.settings.privacy.exportEverything}>
          {exportNote && <Note fail={exportNote.fail}>{exportNote.text}</Note>}
          <Button primary onClick={() => void exportAll()} disabled={exporting}>
            {exporting ? copy.settings.privacy.exporting : copy.settings.privacy.exportButton}
          </Button>
        </Row>
      </Section>
    </>
  )
}
