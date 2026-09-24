/**
 * The UI ↔ core boundary is three things changed together: the Rust command, contract.ts and
 * the mock. This is the test docs/TESTING.md promises: a command declared in the contract with
 * no mock implementation fails the build, and so does a mock for a command nobody declared.
 */
import { describe, expect, it } from 'vitest'
import { COMMAND_NAMES } from './contract'
import { mockedCommands } from '../mock/backend'

describe('UI contract coverage', () => {
  it('every declared command has a mock implementation', () => {
    const missing = COMMAND_NAMES.filter((c) => !mockedCommands.includes(c))
    expect(missing).toEqual([])
  })

  it('every mock implements a declared command', () => {
    const undeclared = mockedCommands.filter((c) => !(COMMAND_NAMES as readonly string[]).includes(c))
    expect(undeclared).toEqual([])
  })
})
