import { useEffect, useState } from 'react'

import { applyMode, type Mode, persistMode, resolveStoredMode } from './theme-bootstrap'

const MODES: { value: Mode; glyph: string; label: string }[] = [
  { value: 'light', glyph: '☀', label: 'Light' },
  { value: 'system', glyph: '◐', label: 'System' },
  { value: 'dark', glyph: '☾', label: 'Dark' },
]

export function ThemeToggle() {
  const [mode, setMode] = useState<Mode>(() => resolveStoredMode())

  useEffect(() => {
    applyMode(mode)
    persistMode(mode)
  }, [mode])

  useEffect(() => {
    if (mode !== 'system') return
    if (typeof window === 'undefined' || !window.matchMedia) return
    const mq = window.matchMedia('(prefers-color-scheme: light)')
    const handler = () => applyMode('system')
    mq.addEventListener('change', handler)
    return () => mq.removeEventListener('change', handler)
  }, [mode])

  return (
    <div className="flex items-center border border-border-strong" role="group" aria-label="Theme">
      {MODES.map((m) => (
        <button
          key={m.value}
          type="button"
          aria-label={m.label}
          aria-pressed={mode === m.value}
          title={m.label}
          onClick={() => setMode(m.value)}
          className={
            'flex h-7 w-7 items-center justify-center text-[13px] leading-none transition-colors ' +
            (mode === m.value
              ? 'bg-accent text-accent-fg'
              : 'text-fg-muted hover:bg-bg-elev hover:text-fg')
          }
        >
          {m.glyph}
        </button>
      ))}
    </div>
  )
}
