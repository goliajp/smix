// Pre-paint persisted theme hook. Kept in a separate file so the
// ThemeToggle component module is component-only (fast refresh friendly).

const STORAGE_KEY = 'smix-theme'

type Mode = 'dark' | 'light' | 'system'

function safeLocalStorage(): Storage | null {
  try {
    return typeof window !== 'undefined' && window.localStorage ? window.localStorage : null
  } catch {
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

export function preMountApplyTheme(): void {
  if (typeof document === 'undefined') return
  const ls = safeLocalStorage()
  const raw = ls?.getItem(STORAGE_KEY) ?? null
  const mode: Mode = raw === 'dark' || raw === 'light' || raw === 'system' ? (raw as Mode) : 'light'
  const resolved: 'dark' | 'light' = mode === 'system' ? getSystemMode() : mode
  document.documentElement.dataset.theme = resolved
}
