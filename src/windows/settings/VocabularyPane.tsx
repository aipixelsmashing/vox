/**
 * Vocabulary: the trust surface for learning (docs/LEARNING.md, docs/UI-SPEC.md). One row
 * per learned word, with every way it was misheard beneath it, its corrections summed and
 * its apps merged; delete removes the word and all its pairs. Suspended words explain
 * themselves; words still waiting are visible on request. The header row carries the count,
 * the export, and, once the list is longer than a screen, a filter. Nothing actionable sits
 * below the list, and the list scrolls with the pane.
 */

import { useEffect, useState } from 'react'
import { commands, toDisplayError } from '../../lib/commands'
import type { Settings, VocabTerm, VoxError } from '../../lib/contract'
import { copy } from '../../lib/copy'
import type { DeepPartial } from '../../lib/useSettings'
import { Button, Note, Row, Section, Toggle } from '../../ui/form'

/** Past this many words the filter field appears. */
export const FILTER_FROM = 12

function since(ts: number): string {
  return new Date(ts).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
}

/** One learned word: the pairs that share a right form, folded together. */
export interface Word {
  rightForm: string
  wrongForms: string[]
  count: number
  sourceApps: string[]
  firstSeen: number
  lastSeen: number
  /** In use: a hinted right form, or a pair that earned replacement by itself. */
  inUse: boolean
  /** Every pair behind it was changed back; nothing is applied. */
  suspended: boolean
}

/**
 * Pairs arrive newest first; words keep that order by their newest pair. Pure, so it can
 * be tested without a DOM.
 */
export function groupByWord(terms: VocabTerm[]): Word[] {
  const words = new Map<string, Word>()
  for (const t of terms) {
    const w = words.get(t.rightForm)
    if (!w) {
      words.set(t.rightForm, {
        rightForm: t.rightForm,
        wrongForms: [t.wrongForm],
        count: t.count,
        sourceApps: [...t.sourceApps],
        firstSeen: t.firstSeen,
        lastSeen: t.lastSeen,
        inUse: t.hinted || t.state === 'applied',
        suspended: t.state === 'suspended',
      })
      continue
    }
    if (!w.wrongForms.includes(t.wrongForm)) w.wrongForms.push(t.wrongForm)
    w.count += t.count
    for (const a of t.sourceApps) if (!w.sourceApps.includes(a)) w.sourceApps.push(a)
    w.firstSeen = Math.min(w.firstSeen, t.firstSeen)
    w.lastSeen = Math.max(w.lastSeen, t.lastSeen)
    w.inUse = w.inUse || t.hinted || t.state === 'applied'
    w.suspended = w.suspended && t.state === 'suspended'
  }
  return [...words.values()]
}

/** Case-insensitive, on the word and on every way it was misheard. */
export function matchesFilter(w: Word, filter: string): boolean {
  const q = filter.trim().toLowerCase()
  if (!q) return true
  return w.rightForm.toLowerCase().includes(q) || w.wrongForms.some((f) => f.toLowerCase().includes(q))
}

export function VocabularyPane({ settings, patch }: { settings: Settings; patch: (p: DeepPartial<Settings>) => Promise<void> }) {
  const [terms, setTerms] = useState<VocabTerm[] | null>(null)
  const [error, setError] = useState<VoxError | null>(null)
  const [showWaiting, setShowWaiting] = useState(false)
  const [exportNote, setExportNote] = useState<string | null>(null)
  const [filter, setFilter] = useState('')

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

  const forget = async (rightForm: string) => {
    try {
      await commands.vocab_forget_term({ rightForm })
      setTerms((t) => t?.filter((x) => x.rightForm !== rightForm) ?? null)
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
  const words = terms ? groupByWord(terms) : null
  const showFilter = (words?.length ?? 0) > FILTER_FROM
  const shown = words?.filter((w) => !showFilter || matchesFilter(w, filter)) ?? []
  const learned = shown.filter((w) => w.inUse || w.suspended)
  const waiting = shown.filter((w) => !w.inUse && !w.suspended)
  const learnedTotal = words?.filter((w) => w.inUse || w.suspended).length ?? 0

  const wordRow = (w: Word) => (
    <Row
      key={w.rightForm}
      label={w.suspended ? `⚠ ${w.rightForm}` : w.rightForm}
      hint={
        w.suspended
          ? copy.settings.vocabulary.suspended
          : copy.settings.vocabulary.provenance(w.count, w.sourceApps, since(w.firstSeen), w.wrongForms)
      }
    >
      <Button danger label={`${copy.history.delete} ${w.rightForm}`} onClick={() => void forget(w.rightForm)}>
        ×
      </Button>
    </Row>
  )

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

      <Section
        title={copy.settings.vocabulary.learnedTitle(learnedTotal)}
        actions={
          <>
            {exportNote && <Note>{exportNote}</Note>}
            {showFilter && (
              <input
                type="search"
                aria-label={copy.settings.vocabulary.filter}
                placeholder={copy.settings.vocabulary.filter}
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                style={{
                  width: 180,
                  padding: 'var(--s-1) var(--s-2)',
                  font: 'inherit',
                  color: 'inherit',
                  background: 'var(--field)',
                  border: '1px solid var(--rule)',
                  borderRadius: 'var(--radius-input)',
                }}
              />
            )}
            <Button label={copy.settings.vocabulary.exportLabel} onClick={() => void exportList()} disabled={!terms || terms.length === 0}>
              {copy.settings.vocabulary.export}
            </Button>
          </>
        }
      >
        {terms === null && !error && <Row label={copy.history.loading} />}
        {error && <Row label={error.userMessage} />}
        {words && learnedTotal === 0 && (
          <Row label={learningOn ? copy.settings.vocabulary.empty : copy.settings.vocabulary.off} />
        )}
        {words && learnedTotal > 0 && shown.length === 0 && <Row label={copy.settings.vocabulary.noMatch(filter)} />}
        {learned.map(wordRow)}
        {words && waiting.length > 0 && (
          <Row label={copy.settings.vocabulary.waiting(waiting.length)}>
            <Button onClick={() => setShowWaiting((s) => !s)}>{showWaiting ? copy.history.less : copy.settings.vocabulary.show}</Button>
          </Row>
        )}
        {showWaiting && waiting.map(wordRow)}
      </Section>
    </>
  )
}
