/**
 * Settings for the window: load once, patch with a deep partial, and take the core's merged
 * answer as the truth (docs/UI-CONTRACT.md: settings_set returns what was accepted).
 */

import { useCallback, useEffect, useState } from 'react'
import { commands, toDisplayError } from './commands'
import type { Settings, VoxError } from './contract'

export type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] }

export function useSettings() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [error, setError] = useState<VoxError | null>(null)
  const [saving, setSaving] = useState(false)

  const load = useCallback(() => {
    setError(null)
    commands
      .settings_get()
      .then(setSettings)
      .catch((e) => setError(toDisplayError(e)))
  }, [])

  useEffect(load, [load])

  const patch = useCallback(async (p: DeepPartial<Settings>) => {
    setSaving(true)
    try {
      const merged = await commands.settings_set(p as Partial<Settings>)
      setSettings(merged)
      setError(null)
    } catch (e) {
      setError(toDisplayError(e))
    } finally {
      setSaving(false)
    }
  }, [])

  return { settings, error, saving, patch, reload: load }
}
