import { useEffect, useState } from 'react'

type Mode = 'dark' | 'light' | 'system'

const STORAGE_KEY = 'smix-theme'

function safeLocalStorage(): Storage | null {
  try {
    return typeof window !== 'undefined' && window.localStorage ? window.localStorage : null
  } catch {
    // some sandboxed contexts throw on the property access itself
    return null
  }
}

function getSystemMode(): 'dark' | 'light' {
  try {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  } catch {
    return 'light'
  }
}

function applyMode(mode: Mode): void {
  if (typeof document === 'undefined') return
  const resolved: 'dark' | 'light' = mode === 'system' ? getSystemMode() : mode
  document.documentElement.dataset.theme = resolved
}

function loadPersisted(): Mode {
  const ls = safeLocalStorage()
  if (!ls) return 'light'
  const raw = ls.getItem(STORAGE_KEY)
  if (raw === 'dark' || raw === 'light' || raw === 'system') return raw
  return 'light'
}

function persist(mode: Mode): void {
  const ls = safeLocalStorage()
  if (!ls) return
  try {
    ls.setItem(STORAGE_KEY, mode)
  } catch {
    /* quota / private mode — ignore */
  }
}

const MODES: { value: Mode; glyph: string; label: string }[] = [
  { value: 'light', glyph: '☀', label: 'Light' },
  { value: 'system', glyph: '◐', label: 'System' },
  { value: 'dark', glyph: '☾', label: 'Dark' },
]

export function ThemeToggle() {
  const [mode, setMode] = useState<Mode>(() => loadPersisted())

  useEffect(() => {
    applyMode(mode)
    persist(mode)
  }, [mode])

  // React to system change while in 'system' mode.
  useEffect(() => {
    if (mode !== 'system') return
    if (typeof window === 'undefined' || !window.matchMedia) return
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const handler = () => applyMode('system')
    mq.addEventListener('change', handler)
    return () => mq.removeEventListener('change', handler)
  }, [mode])

  return (
    <div className="flex items-center border border-[color:var(--border)]">
      {MODES.map((m) => (
        <button
          aria-label={m.label}
          className={
            'flex h-7 w-7 cursor-pointer items-center justify-center text-[13px] leading-none transition-colors ' +
            (mode === m.value
              ? 'bg-[color:var(--accent)] text-[color:var(--accent-fg)]'
              : 'text-[color:var(--fg-muted)] hover:bg-[color:var(--bg-elev)] hover:text-[color:var(--fg)]')
          }
          key={m.value}
          onClick={() => setMode(m.value)}
          title={m.label}
          type="button"
        >
          {m.glyph}
        </button>
      ))}
    </div>
  )
}

// preMountApplyTheme moved to ./theme-bootstrap.ts (fast-refresh: this
// module is component-only).
