/**
 * Single entry for every window. Tauri opens each with a different ?window=, and in mock mode
 * you switch by editing the URL or using the dev overlay. See docs/UI-DEVELOPMENT.md.
 */

import React from 'react'
import { createRoot, type Root } from 'react-dom/client'
import '@fontsource/literata/400.css'
import './styles/tokens.css'
import { currentWindow } from './mock/scenarios'
import { HistoryPanel } from './windows/HistoryPanel'
import { Onboarding } from './windows/Onboarding'
import { RecordingOverlay } from './windows/RecordingOverlay'
import { Settings } from './windows/Settings'
import { Toast } from './windows/Toast'
import { DevOverlay } from './ui/DevOverlay'

const isMock = !('__TAURI_INTERNALS__' in window)

function Placeholder({ name }: { name: string }) {
  return (
    <div className="panel" style={{ padding: 'var(--s-4)' }}>
      <div style={{ fontWeight: 600 }}>{name}</div>
      <p className="meta">
        Not built yet. Spec in docs/UI-SPEC.md, states in docs/UI-STATES.md, data in
        src/mock/fixtures.ts.
      </p>
    </div>
  )
}

function App() {
  const win = currentWindow()
  return (
    <>
      {win === 'history' && <HistoryPanel />}
      {win === 'settings' && <Settings />}
      {win === 'onboarding' && <Onboarding />}
      {win === 'longform' && <Placeholder name="Long-form session" />}
      {win === 'overlay' && <RecordingOverlay />}
      {win === 'toast' && <Toast />}
      {isMock && <DevOverlay />}
    </>
  )
}

declare global {
  interface Window {
    /** Vite's hot reload re-runs this module; React wants one root per container. */
    __voxRoot?: Root
  }
}

;(window.__voxRoot ??= createRoot(document.getElementById('root')!)).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
