/**
 * The undo shortcut is shown the way a Mac menu shows a chord (docs/HOTKEYS.md). Pure, so
 * no DOM.
 */
import { describe, expect, it } from 'vitest'
import { describeChord } from './DictationPane'

describe('describeChord', () => {
  it('draws modifiers as glyphs in the Mac order, then the key', () => {
    expect(describeChord('Control+Cmd+Z')).toBe('⌃⌘Z')
    expect(describeChord('Cmd+Control+Z')).toBe('⌃⌘Z')
    expect(describeChord('Shift+Cmd+U')).toBe('⇧⌘U')
    expect(describeChord('CmdOrCtrl+Shift+Space')).toBe('⇧⌘Space')
    expect(describeChord('Alt+Backspace')).toBe('⌥⌫')
    expect(describeChord('Cmd+Digit1')).toBe('⌘1')
  })
})
