import { useFonts } from '@goliapkg/gds'
import { useSetThemeDensity, useSetThemeMotion, useThemeEffect } from '@goliapkg/gds/systems'
import { useEffect } from 'react'
import { Link, Outlet } from 'react-router'

import { ThemeToggle } from './components/theme-toggle'

export function AppLayout() {
  useThemeEffect()
  useFonts()

  const setDensity = useSetThemeDensity()
  const setMotion = useSetThemeMotion()
  useEffect(() => {
    setDensity('default')
    setMotion('full')
  }, [setDensity, setMotion])

  return (
    <div className="flex h-full flex-col">
      <header className="border-border bg-bg/80 flex h-12 shrink-0 items-center justify-between border-b px-6 backdrop-blur-xl">
        <Link className="text-fg flex items-center gap-2 text-sm font-semibold" to="/">
          <img
            alt="simx"
            className="h-5 w-5 rounded-sm"
            src="https://cdn.golia.jp/logo-icon.png"
            style={{ filter: 'drop-shadow(0 0 6px var(--gds-accent, #3b82f6))' }}
          />
          simx
        </Link>
        <div className="flex items-center gap-4">
          <a
            className="text-fg-muted hover:text-fg text-sm transition-colors"
            href="https://github.com/goliajp/simx"
            rel="noopener noreferrer"
            target="_blank"
          >
            GitHub
          </a>
          <ThemeToggle />
        </div>
      </header>

      <main className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-4xl px-6 py-12">
          <Outlet />
        </div>
      </main>

      <footer className="border-border text-fg-muted flex h-8 shrink-0 items-center justify-center border-t bg-white/5 text-xs backdrop-blur-sm">
        simx v1.0 &middot; MIT &middot; built on GOLIA design system
      </footer>
    </div>
  )
}
