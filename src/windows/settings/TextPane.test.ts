/**
 * The sentence and the provenance a per-app default row shows (docs/adr/0019). Pure, no DOM.
 */
import { describe, expect, it } from 'vitest'
import { describeDefault, describeProvenance } from './TextPane'
import { appDefaults } from '../../mock/fixtures'

describe('per-app default rows', () => {
  it('says what each default does', () => {
    const by = (app: string) => appDefaults.find((d) => d.app === app)!
    expect(describeDefault(by('Safari'))).toBe('No space after a dictation')
    expect(describeDefault(by('Slack'))).toBe('Line breaks become spaces')
    expect(describeDefault(by('Brave Browser'))).toBe('Pasted, not inserted directly')
    expect(describeDefault({ ...by('Safari'), value: 'true' })).toBe('A space after a dictation')
    expect(describeDefault({ ...by('Slack'), value: 'false' })).toBe('Line breaks kept')
  })

  it('says where it came from, and when it was suspended', () => {
    const by = (app: string) => appDefaults.find((d) => d.app === app)!
    expect(describeProvenance(by('Safari'))).toMatch(/^4 fixes · since /)
    expect(describeProvenance(by('Brave Browser'))).toBe('Direct insertion never took here')
    expect(describeProvenance(by('Notes'))).toBe('You changed this back twice — Vox has stopped applying it.')
    expect(describeProvenance({ ...by('Safari'), count: 1 })).toMatch(/^1 fix · since /)
  })
})
