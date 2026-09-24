/**
 * One-way pushes from the core. The UI never polls.
 * Same routing rule as commands.ts: real Tauri, or the mock event bus in dev.
 */

import { useEffect } from 'react'

import type { Events } from './contract'

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export async function on<K extends keyof Events>(
  event: K,
  handler: (payload: Events[K]) => void,
): Promise<() => void> {
  if (isTauri) {
    const { listen } = await import('@tauri-apps/api/event')
    const un = await listen<Events[K]>(event, (e) => handler(e.payload))
    return un
  }
  const { mockOn } = await import('../mock/backend')
  return mockOn(event, handler)
}

/** React helper. Deliberately tiny — this is the whole event layer. */
export function useVoxEvent<K extends keyof Events>(
  event: K,
  handler: (payload: Events[K]) => void,
  deps: unknown[] = [],
) {
  useEffect(() => {
    let dispose: (() => void) | undefined
    let cancelled = false
    on(event, handler).then((d) => (cancelled ? d() : (dispose = d)))
    return () => {
      cancelled = true
      dispose?.()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)
}
