/**
 * Every state documented in docs/UI-STATES.md names its scenario in a **[S]** column, as
 * `backticked-id`. A documented state with no scenario here fails CI (docs/TESTING.md), which
 * is what stops empty and error states from being discovered at review time. Scenarios that
 * exist only for development (a slow load, a scripted failure) need not be in the doc.
 */
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { scenarios } from './scenarios'

function documentedScenarioIds(): string[] {
  const doc = readFileSync(resolve(__dirname, '../../docs/UI-STATES.md'), 'utf8')
  const ids = new Set<string>()
  let inTable = false
  for (const line of doc.split('\n')) {
    if (line.startsWith('|') && line.includes('**[S]**')) {
      inTable = true
      continue
    }
    if (!line.startsWith('|')) {
      inTable = false
      continue
    }
    if (!inTable) continue
    const cells = line.split('|')
    const last = cells[cells.length - 2] ?? ''
    for (const m of last.matchAll(/`([a-z0-9-]+)`/g)) if (m[1]) ids.add(m[1])
  }
  return [...ids]
}

describe('UI-STATES.md scenario coverage', () => {
  it('finds the documented states', () => {
    expect(documentedScenarioIds().length).toBeGreaterThan(10)
  })

  it('every documented state has a scenario', () => {
    const missing = documentedScenarioIds().filter((id) => !(id in scenarios))
    expect(missing).toEqual([])
  })
})
