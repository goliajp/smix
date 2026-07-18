// Pre-paint the persisted theme before React mounts to avoid a flash.
// Dark is canonical for the marketing instrument look; an explicit stored
// preference (or system) can override it.

const STORAGE_KEY = 'smix-web-theme'

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
    return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
  } catch {
    return 'dark'
  }
}

export function resolveStoredMode(): Mode {
  const raw = safeLocalStorage()?.getItem(STORAGE_KEY) ?? null
  return raw === 'dark' || raw === 'light' || raw === 'system' ? raw : 'dark'
}

export function applyMode(mode: Mode): void {
  if (typeof document === 'undefined') return
  const resolved: 'dark' | 'light' = mode === 'system' ? getSystemMode() : mode
  document.documentElement.dataset.theme = resolved
}

export function persistMode(mode: Mode): void {
  try {
    safeLocalStorage()?.setItem(STORAGE_KEY, mode)
  } catch {
    /* quota / private mode — ignore */
  }
}

export function preMountApplyTheme(): void {
  applyMode(resolveStoredMode())
}

export type { Mode }
