/**
 * The only place the UI talks to the core.
 *
 * Routes to the real Tauri runtime, or to the mock backend when running under `pnpm dev:mock`.
 * Nothing else in the UI imports @tauri-apps/api — a lint rule enforces that, so every screen
 * stays buildable in a browser. See docs/UI-DEVELOPMENT.md.
 */

import type { Commands, Events, VoxError } from './contract'

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

type Invoke = (cmd: string, args?: unknown) => Promise<unknown>

let invokeImpl: Invoke | null = null

async function getInvoke(): Promise<Invoke> {
  if (invokeImpl) return invokeImpl
  if (isTauri) {
    const { invoke } = await import('@tauri-apps/api/core')
    invokeImpl = (cmd, args) => invoke(cmd, args as Record<string, unknown>)
  } else {
    const { mockInvoke } = await import('../mock/backend')
    invokeImpl = mockInvoke
  }
  return invokeImpl
}

/** Typed proxy: `commands.history_list({ limit: 50 })` with no per-command boilerplate. */
export const commands = new Proxy({} as Commands, {
  get(_t, name: string) {
    return async (args?: unknown) => {
      const invoke = await getInvoke()
      return invoke(name, args)
    }
  },
})

/**
 * Every screen renders errors the same way: the core's userMessage, plus its suggested action.
 * Do not compose your own sentence here — copy lives in one place (docs/UI-STATES.md).
 */
export function toDisplayError(e: unknown): VoxError {
  if (typeof e === 'object' && e !== null && 'userMessage' in e) return e as VoxError
  return {
    kind: 'io',
    detail: String(e),
    userMessage: 'Something failed and Vox could not explain what. Check the log in Settings → Diagnostics.',
  }
}

export type { Commands, Events }
