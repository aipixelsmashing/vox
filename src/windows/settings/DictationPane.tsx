/** Dictation: hotkey, mode, cancel key, input device, level meter, recording cap. */

import { useEffect, useState } from 'react'
import { commands, toDisplayError } from '../../lib/commands'
import type { AudioDevice, HotkeyBinding, Settings } from '../../lib/contract'
import { copy } from '../../lib/copy'
import { useVoxEvent } from '../../lib/events'
import type { DeepPartial } from '../../lib/useSettings'
import { Button, Note, NumberField, Row, Section, Select } from '../../ui/form'

const KEY_NAMES: Record<string, string> = {
  AltRight: 'Right Option',
  AltLeft: 'Left Option',
  ControlRight: 'Right Control',
  ControlLeft: 'Left Control',
  MetaRight: 'Right Command',
  MetaLeft: 'Left Command',
  ShiftRight: 'Right Shift',
  ShiftLeft: 'Left Shift',
}

export function describeKeys(keys: string[]): string {
  return keys.map((k) => KEY_NAMES[k] ?? k).join(' + ')
}

export function DictationPane({ settings, patch }: { settings: Settings; patch: (p: DeepPartial<Settings>) => Promise<void> }) {
  const [capturing, setCapturing] = useState(false)
  const [captured, setCaptured] = useState<HotkeyBinding | null>(null)
  const [captureError, setCaptureError] = useState<string | null>(null)
  const [devices, setDevices] = useState<AudioDevice[] | null>(null)
  const [level, setLevel] = useState(0)
  const [levelSeen, setLevelSeen] = useState(false)

  useEffect(() => {
    commands.audio_devices().then(setDevices).catch(() => setDevices([]))
  }, [])

  useVoxEvent('vox://level', ({ rms }) => {
    setLevel(rms)
    setLevelSeen(true)
  }, [])
  useVoxEvent('vox://state', (s) => {
    if (s.state === 'idle') setLevel(0)
  }, [])

  const capture = async () => {
    setCapturing(true)
    setCaptured(null)
    setCaptureError(null)
    try {
      const b = await commands.hotkey_capture_start()
      setCaptured(b)
      await patch({ hotkey: { keys: b.keys } })
    } catch (e) {
      setCaptureError(toDisplayError(e).userMessage)
    } finally {
      setCapturing(false)
    }
  }

  const altGr = captured?.altGr === true

  return (
    <>
      <Section title={copy.settings.dictation.title}>
        <Row
          label={copy.settings.dictation.hotkey}
          hint={
            capturing ? copy.settings.dictation.pressKeys
            : altGr ? <span style={{ color: 'var(--fail)' }}>{copy.settings.dictation.altGrWarning}</span>
            : captureError ? <span style={{ color: 'var(--fail)' }}>{captureError}</span>
            : copy.settings.dictation.hotkeyHint
          }
        >
          <span>{describeKeys(settings.hotkey.keys)}</span>
          <Button onClick={() => void capture()} disabled={capturing}>
            {copy.settings.dictation.change}
          </Button>
        </Row>
        <Row label={copy.settings.dictation.mode} hint={copy.settings.dictation.modeHint}>
          <Select
            label={copy.settings.dictation.mode}
            value={settings.hotkey.mode}
            onChange={(mode) => void patch({ hotkey: { mode } })}
            options={[
              { value: 'hold', label: copy.settings.dictation.modes.hold },
              { value: 'toggle', label: copy.settings.dictation.modes.toggle },
              { value: 'double-tap-hold', label: copy.settings.dictation.modes['double-tap-hold'] },
            ]}
          />
        </Row>
        <Row label={copy.settings.dictation.cancelKey} hint={copy.settings.dictation.cancelHint}>
          <span>Escape</span>
        </Row>
        <Row label={copy.settings.dictation.minHold}>
          <NumberField
            label={copy.settings.dictation.minHold}
            value={settings.hotkey.minHoldMs}
            min={0}
            max={1000}
            step={10}
            suffix="ms"
            onChange={(minHoldMs) => void patch({ hotkey: { minHoldMs } })}
          />
        </Row>
      </Section>

      <Section title={copy.settings.dictation.audioTitle}>
        <Row label={copy.settings.dictation.device} hint={devices?.length === 0 ? copy.settings.dictation.noDevices : undefined}>
          {devices === null ? (
            <Note>{copy.history.loading}</Note>
          ) : devices.length === 0 ? null : (
            <Select
              label={copy.settings.dictation.device}
              value={settings.audio.inputDevice}
              onChange={(inputDevice) => void patch({ audio: { inputDevice } })}
              options={[
                { value: 'default', label: copy.settings.dictation.systemDefault },
                ...devices.filter((d) => d.id !== 'default').map((d) => ({ value: d.id, label: d.name })),
              ]}
            />
          )}
        </Row>
        <Row label={copy.settings.dictation.level} hint={levelSeen ? undefined : copy.settings.dictation.levelHint}>
          <div
            role="meter"
            aria-label={copy.settings.dictation.level}
            aria-valuemin={0}
            aria-valuemax={1}
            aria-valuenow={Math.min(1, level)}
            style={{ width: 160, height: 6, background: 'var(--field)', borderRadius: 3, overflow: 'hidden' }}
          >
            <div style={{ width: `${Math.min(100, level * 100)}%`, height: '100%', background: 'var(--signal)' }} />
          </div>
        </Row>
        <Row label={copy.settings.dictation.cap} hint={copy.settings.dictation.capHint}>
          <NumberField
            label={copy.settings.dictation.cap}
            value={settings.audio.maxRecordingSec}
            min={10}
            max={600}
            step={10}
            suffix="s"
            onChange={(maxRecordingSec) => void patch({ audio: { maxRecordingSec } })}
          />
        </Row>
      </Section>
    </>
  )
}
