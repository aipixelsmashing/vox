/**
 * The handful of form pieces the settings panes are made of. Native controls, token-styled;
 * no component library (src/README.md). Every control is labelled for screen readers.
 */

import type { ReactNode } from 'react'

export function Section({ title, note, children }: { title: string; note?: ReactNode; children: ReactNode }) {
  return (
    <section style={{ marginBottom: 'var(--s-8)' }}>
      <h2
        style={{
          font: '600 var(--t-title-size) / var(--t-title-lh) var(--font-ui)',
          margin: '0 0 var(--s-2)',
        }}
      >
        {title}
      </h2>
      {note && (
        <p className="meta" style={{ margin: '0 0 var(--s-3)', maxWidth: 520 }}>
          {note}
        </p>
      )}
      <div style={{ borderTop: '1px solid var(--rule)' }}>{children}</div>
    </section>
  )
}

export function Row({ label, hint, children }: { label: string; hint?: ReactNode; children?: ReactNode }) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 'var(--s-4)',
        padding: 'var(--s-3) 0',
        borderBottom: '1px solid var(--rule)',
      }}
    >
      <div style={{ minWidth: 0 }}>
        <div>{label}</div>
        {hint && (
          <div className="meta" style={{ marginTop: 'var(--s-1)', maxWidth: 440 }}>
            {hint}
          </div>
        )}
      </div>
      <div style={{ flexShrink: 0, display: 'flex', alignItems: 'center', gap: 'var(--s-2)' }}>{children}</div>
    </div>
  )
}

export function Toggle({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <input
      type="checkbox"
      role="switch"
      aria-label={label}
      aria-checked={checked}
      checked={checked}
      disabled={disabled}
      onChange={(e) => onChange(e.target.checked)}
      style={{ width: 18, height: 18, accentColor: 'var(--signal)', cursor: disabled ? 'default' : 'pointer' }}
    />
  )
}

export function Select<T extends string>({ value, onChange, options, label, disabled }: {
  value: T
  onChange: (v: T) => void
  options: Array<{ value: T; label: string }>
  label: string
  disabled?: boolean
}) {
  return (
    <select
      aria-label={label}
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value as T)}
      style={{
        font: 'inherit',
        color: 'inherit',
        background: 'var(--field)',
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-input)',
        padding: 'var(--s-1) var(--s-2)',
      }}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  )
}

export function NumberField({ value, onChange, label, min, max, step, suffix, disabled }: {
  value: number
  onChange: (v: number) => void
  label: string
  min?: number
  max?: number
  step?: number
  suffix?: string
  disabled?: boolean
}) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 'var(--s-1)' }}>
      <input
        type="number"
        aria-label={label}
        value={value}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onChange={(e) => {
          const n = Number(e.target.value)
          if (Number.isFinite(n)) onChange(n)
        }}
        className="numeric"
        style={{
          width: 72,
          font: 'inherit',
          color: 'inherit',
          background: 'var(--field)',
          border: '1px solid var(--rule)',
          borderRadius: 'var(--radius-input)',
          padding: 'var(--s-1) var(--s-2)',
          textAlign: 'right',
        }}
      />
      {suffix && <span className="meta">{suffix}</span>}
    </span>
  )
}

export function TextInput({ value, onChange, label, placeholder, width }: {
  value: string
  onChange: (v: string) => void
  label: string
  placeholder?: string
  width?: number
}) {
  return (
    <input
      type="text"
      aria-label={label}
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
      style={{
        width: width ?? 160,
        font: 'inherit',
        color: 'inherit',
        background: 'var(--field)',
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-input)',
        padding: 'var(--s-1) var(--s-2)',
      }}
    />
  )
}

export function Button({ children, onClick, danger, disabled, primary }: {
  children: ReactNode
  onClick: () => void
  danger?: boolean
  disabled?: boolean
  primary?: boolean
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      style={{
        font: 'inherit',
        color: danger ? 'var(--fail)' : 'inherit',
        background: primary ? 'var(--field)' : 'none',
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-input)',
        padding: 'var(--s-1) var(--s-3)',
        cursor: disabled ? 'default' : 'pointer',
        opacity: disabled ? 0.5 : 1,
      }}
    >
      {children}
    </button>
  )
}

/** Inline status text next to a control: a result, or a failure in the fail colour. */
export function Note({ children, fail }: { children: ReactNode; fail?: boolean }) {
  return (
    <span className="meta" role="status" style={{ color: fail ? 'var(--fail)' : undefined }}>
      {children}
    </span>
  )
}
