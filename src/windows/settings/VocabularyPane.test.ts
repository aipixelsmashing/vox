/**
 * The grouping the Vocabulary pane renders: one row per learned word (docs/UI-SPEC.md).
 * Pure functions, so no DOM.
 */
import { describe, expect, it } from 'vitest'
import { groupByWord, matchesFilter } from './VocabularyPane'
import { manyVocabTerms, vocabTerms } from '../../mock/fixtures'

describe('groupByWord', () => {
  it('folds the pairs that share a right form into one word', () => {
    const words = groupByWord(vocabTerms)
    const priya = words.find((w) => w.rightForm === 'Priya')!
    expect(priya.wrongForms).toEqual(['prea', 'pre a'])
    expect(priya.count).toBe(6)
    expect(priya.sourceApps).toEqual(['Slack', 'Mail'])
    expect(priya.inUse).toBe(true)
    expect(words.map((w) => w.rightForm)).toEqual(['Kubernetes', 'Priya', 'Tailwind', 'PostHog'])
  })

  it('keeps a word in use while any pair is applied or its right form is hinted', () => {
    const words = groupByWord(vocabTerms)
    expect(words.find((w) => w.rightForm === 'PostHog')!.inUse).toBe(false)
    expect(words.find((w) => w.rightForm === 'Tailwind')!.suspended).toBe(true)
    const tailwind = vocabTerms[2]!
    const halfSuspended = groupByWord([
      { ...tailwind, state: 'suspended' },
      { ...tailwind, id: 99, wrongForm: 'tailwinds', state: 'applied' },
    ])
    expect(halfSuspended[0]?.suspended).toBe(false)
    expect(halfSuspended[0]?.inUse).toBe(true)
  })

  it('the many fixture is past the filter threshold', () => {
    expect(groupByWord(manyVocabTerms).length).toBeGreaterThan(12)
  })
})

describe('matchesFilter', () => {
  const kube = groupByWord(vocabTerms)[0]!
  it('matches the word or any wrong form, ignoring case', () => {
    expect(matchesFilter(kube, 'KUBER')).toBe(true)
    expect(matchesFilter(kube, 'netties')).toBe(true)
    expect(matchesFilter(kube, 'priya')).toBe(false)
    expect(matchesFilter(kube, '  ')).toBe(true)
  })
})
