/**
 * Vocabulary: the trust surface for learning (docs/LEARNING.md, docs/UI-SPEC.md). Every
 * term with its provenance and a delete button; suspended terms explain themselves;
 * candidates are visible on request; export as a plain list.
 */

import { useEffect, useState } from 'react'
import { commands, toDisplayError } from '../../lib/commands'
import type { Settings, VocabTerm, VoxError } from '../../lib/contract'
import { copy } from '../../lib/copy'
import type { DeepPartial } from '../../lib/useSettings'
import { Button, Note, Row, Section, Toggle } from '../../ui/form'

function since(ts: number): string {
  return new Date(ts).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
}

export function VocabularyPane({ settings, patch }: { settings: Settings; patch: (p: DeepPartial<Settings>) => Promise<void> }) {
  const [terms, setTerms] = useState<VocabTerm[] | null>(null)
  const [error, setError] = useState<VoxError | null>(null)
  const [showWaiting, setShowWaiting] = useState(false)
  const [exportNote, setExportNote] = useState<string | null>(null)

  const load = () => {
    commands
      .vocab_list({})
      .then((t) => {
        setTerms(t)
        setError(null)
      })
      .catch((e) => setError(toDisplayError(e)))
  }
  useEffect(load, [])
  useEffect(() => {
    if (!exportNote) return
    const t = setTimeout(() => setExportNote(null), 4000)
    return () => clearTimeout(t)
  }, [exportNote])

  const forget = async (id: number) => {
    try {
      await commands.vocab_forget({ id })
      setTerms((t) => t?.filter((x) => x.id !== id) ?? null)
    } catch (e) {
      setError(toDisplayError(e))
    }
  }

  const exportList = async () => {
    try {
      const { path } = await commands.vocab_export()
      setExportNote(copy.settings.vocabulary.exported(path))
    } catch (e) {
      setExportNote(toDisplayError(e).userMessage)
    }
  }

  const learningOn = settings.learning.captureCorrections
  // A hinted right form is in use even while each of its pairs is still a candidate, so
  // its rows belong with what Vox has learned, not with what is waiting.
  const inUse = (t: VocabTerm) => t.state === 'applied' || t.state === 'suspended' || t.hinted
  const learned = terms?.filter(inUse) ?? []
  const waiting = terms?.filter((t) => t.state === 'candidate' && !t.hinted) ?? []

  return (
    <>
      <Section title={copy.settings.vocabulary.title} note={copy.settings.vocabulary.note}>
        <Row label={copy.settings.vocabulary.capture} hint={copy.settings.vocabulary.captureHint}>
          <Toggle label={copy.settings.vocabulary.capture} checked={settings.learning.captureCorrections} onChange={(captureCorrections) => void patch({ learning: { captureCorrections } })} />
        </Row>
        <Row label={copy.settings.vocabulary.apply} hint={copy.settings.vocabulary.applyHint}>
          <Toggle
            label={copy.settings.vocabulary.apply}
            checked={settings.learning.applyLearnedTerms}
            disabled={!learningOn}
            onChange={(applyLearnedTerms) => void patch({ learning: { applyLearnedTerms } })}
          />
        </Row>
      </Section>

      <Section title={copy.settings.vocabulary.learnedTitle}>
        {terms === null && !error && <Row label={copy.history.loading} />}
        {error && <Row label={error.userMessage} />}
        {terms && learned.length === 0 && (
          <Row label={learningOn ? copy.settings.vocabulary.empty : copy.settings.vocabulary.off} />
        )}
        {learned.map((t) => (
          <Row
            key={t.id}
            label={t.state === 'suspended' ? `⚠ ${t.rightForm}` : t.rightForm}
            hint={
              t.state === 'suspended'
                ? copy.settings.vocabulary.suspended
                : copy.settings.vocabulary.provenance(t.count, t.sourceApps, since(t.firstSeen), t.wrongForm)
            }
          >
            <Button danger onClick={() => void forget(t.id)}>
              ×
            </Button>
          </Row>
        ))}
        {terms && waiting.length > 0 && (
          <Row label={copy.settings.vocabulary.waiting(waiting.length)}>
            <Button onClick={() => setShowWaiting((s) => !s)}>{showWaiting ? copy.history.less : copy.settings.vocabulary.show}</Button>
          </Row>
        )}
        {showWaiting &&
          waiting.map((t) => (
            <Row key={t.id} label={t.rightForm} hint={copy.settings.vocabulary.provenance(t.count, t.sourceApps, since(t.firstSeen), t.wrongForm)}>
              <Button danger onClick={() => void forget(t.id)}>
                ×
              </Button>
            </Row>
          ))}
        <Row label={copy.settings.vocabulary.exportLabel}>
          {exportNote && <Note>{exportNote}</Note>}
          <Button onClick={() => void exportList()} disabled={!terms || terms.length === 0}>
            {copy.settings.vocabulary.export}
          </Button>
        </Row>
      </Section>
    </>
  )
}
