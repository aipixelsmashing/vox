/**
 * Model: what recognises the speech. v1 ships Apple's engine only (docs/adr/0016); the
 * download, import and remove controls are driven by the contract and light up when the M8
 * engines return. No residency setting, on purpose (docs/adr/0010).
 */

import { useEffect, useState } from 'react'
import { commands, toDisplayError } from '../../lib/commands'
import type { ModelInfo, Settings, VoxError } from '../../lib/contract'
import { copy } from '../../lib/copy'
import { useVoxEvent } from '../../lib/events'
import type { DeepPartial } from '../../lib/useSettings'
import { Button, Note, Row, Section, TextInput } from '../../ui/form'

function mb(bytes: number): string {
  return bytes === 0 ? copy.settings.model.builtIn : `${Math.round(bytes / 1_048_576)} MB`
}

export function ModelPane({ settings, patch }: { settings: Settings; patch: (p: DeepPartial<Settings>) => Promise<void> }) {
  const [models, setModels] = useState<ModelInfo[] | null>(null)
  const [error, setError] = useState<VoxError | null>(null)
  const [progress, setProgress] = useState<Record<string, { received: number; total: number; phase: string }>>({})
  const [busy, setBusy] = useState<string | null>(null)
  const [importPath, setImportPath] = useState('')
  const [importNote, setImportNote] = useState<string | null>(null)

  const load = () => {
    commands
      .models_list()
      .then((m) => {
        setModels(m)
        setError(null)
      })
      .catch((e) => setError(toDisplayError(e)))
  }
  useEffect(load, [])

  useVoxEvent('vox://model-progress', (p) => {
    setProgress((prev) => ({ ...prev, [p.id]: { received: p.received, total: p.total, phase: p.phase } }))
  }, [])

  const download = async (id: string) => {
    setBusy(id)
    setError(null)
    try {
      await commands.models_download({ id })
      load()
    } catch (e) {
      setError(toDisplayError(e))
    } finally {
      setBusy(null)
    }
  }

  const remove = async (id: string) => {
    setBusy(id)
    try {
      await commands.models_remove({ id })
      load()
    } catch (e) {
      setError(toDisplayError(e))
    } finally {
      setBusy(null)
    }
  }

  const doImport = async () => {
    setImportNote(null)
    try {
      const m = await commands.models_import({ path: importPath })
      setImportNote(copy.settings.model.imported(m.displayName))
      load()
    } catch (e) {
      setImportNote(toDisplayError(e).userMessage)
    }
  }

  const installed = models?.filter((m) => m.installed) ?? []
  const inUse = models?.find((m) => m.id === settings.engine.modelId) ?? models?.find((m) => m.isDefault)

  return (
    <>
      <Section title={copy.settings.model.title} note={copy.settings.model.note}>
        {models === null && !error && <Row label={copy.history.loading} />}
        {error && (
          <Row label={error.userMessage}>
            {error.actionLabel && <Button onClick={() => (error.action === 'retry' ? load() : undefined)}>{error.actionLabel}</Button>}
          </Row>
        )}
        {models && installed.length === 0 && (
          <Row label={copy.settings.model.none} hint={copy.settings.model.noneHint} />
        )}
        {models?.map((m) => {
          const p = progress[m.id]
          const downloading = busy === m.id || (p && p.received < p.total)
          const active = inUse?.id === m.id
          return (
            <Row
              key={m.id}
              label={`${m.displayName}${active ? ` · ${copy.settings.model.inUse}` : ''}`}
              hint={
                <>
                  {mb(m.sizeBytes)} · {m.languages.length} {copy.settings.model.languages}
                  {m.provider && ` · ${m.provider}`}
                  {' · '}
                  {m.license.attribution}
                  {p && downloading && (
                    <div className="numeric" style={{ marginTop: 'var(--s-1)' }}>
                      {p.phase === 'verify' ? copy.settings.model.verifying : `${Math.round((p.received / p.total) * 100)}% · ${mb(p.received)} / ${mb(p.total)}`}
                    </div>
                  )}
                </>
              }
            >
              {m.installed && !active && (
                <Button onClick={() => void patch({ engine: { modelId: m.id } })}>{copy.settings.model.use}</Button>
              )}
              {!m.installed && (
                <Button onClick={() => void download(m.id)} disabled={!!downloading}>
                  {downloading ? copy.settings.model.downloading : copy.settings.model.download}
                </Button>
              )}
              {m.installed && m.sizeBytes > 0 && !active && (
                <Button onClick={() => void remove(m.id)} danger disabled={busy === m.id}>
                  {copy.settings.model.remove}
                </Button>
              )}
            </Row>
          )
        })}
        <Row label={copy.settings.model.residency} hint={copy.settings.model.residencyHint} />
      </Section>

      <Section title={copy.settings.model.importTitle} note={copy.settings.model.importNote}>
        <Row label={copy.settings.model.importFolder}>
          <TextInput label={copy.settings.model.importFolder} value={importPath} onChange={setImportPath} placeholder="~/Downloads/model" width={220} />
          <Button onClick={() => void doImport()} disabled={!importPath.trim()}>
            {copy.settings.model.import}
          </Button>
        </Row>
        {importNote && (
          <Row label="">
            <Note>{importNote}</Note>
          </Row>
        )}
      </Section>
    </>
  )
}
