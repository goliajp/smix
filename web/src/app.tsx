import { Link, Outlet, useLocation } from 'react-router'

import { ThemeToggle } from './components/theme-toggle'

// Top horizontal nav, editorial style.
//   GOLIA / SMIX  ·  QUICK-START · PLUGIN · AUTHORING · TOOLS · EXAMPLES  ·  GITHUB
// All-caps mono, fine baseline rule, accent on hover + active.

type NavItem = { slug: string; label: string; soon?: boolean; to?: string }

const NAV: NavItem[] = [
  { slug: 'quick-start', label: 'Quick start' },
  { slug: 'plugin-install', label: 'Plugin' },
  { slug: 'authoring', label: 'Authoring' },
  { slug: 'tools', label: 'Tools' },
  { slug: 'examples', label: 'Examples' },
  { slug: 'live', label: 'Live', to: '/live' },
]

export function AppLayout() {
  const { pathname } = useLocation()
  const activeSlug = pathname.startsWith('/docs/')
    ? pathname.slice('/docs/'.length).split('/')[0]
    : pathname === '/live'
      ? 'live'
      : null

  return (
    <div className="flex h-full flex-col bg-[color:var(--bg)] text-[color:var(--fg)]">
      {/* TOP NAV — horizontal, mono-uppercase */}
      <header className="border-b border-[color:var(--rule)] px-8 py-3">
        <div className="mx-auto flex max-w-[1400px] items-baseline gap-8">
          {/* Logo / breadcrumb */}
          <Link
            className="font-mono text-[12px] tracking-[0.18em] uppercase no-underline hover:text-[color:var(--accent)]"
            to="/"
          >
            <span className="text-[color:var(--fg-muted)]">GOLIA</span>
            <span className="mx-2 text-[color:var(--fg-dim)]">/</span>
            <span className="text-[color:var(--fg)]">SMIX</span>
          </Link>

          {/* Section nav — pulled toward center via mr-auto trick */}
          <nav className="flex items-baseline gap-7 text-[12px] tracking-[0.18em] uppercase">
            {NAV.map((n) => {
              const active = activeSlug === n.slug
              return (
                <Link
                  className={
                    'font-mono no-underline ' +
                    (active
                      ? 'text-[color:var(--accent)]'
                      : 'text-[color:var(--fg-muted)] hover:text-[color:var(--fg)]')
                  }
                  key={n.slug}
                  to={n.to ?? `/docs/${n.slug}`}
                >
                  {n.label}
                  {n.soon ? (
                    <span className="ml-1.5 text-[10px] text-[color:var(--fg-dim)]">soon</span>
                  ) : null}
                </Link>
              )
            })}
          </nav>

          {/* Right side — github + theme */}
          <div className="ml-auto flex items-baseline gap-5">
            <a
              className="font-mono text-[12px] tracking-[0.18em] text-[color:var(--fg-muted)] uppercase no-underline hover:text-[color:var(--accent)]"
              href="https://github.com/goliajp/smix"
              rel="noopener noreferrer"
              target="_blank"
            >
              github ↗
            </a>
            <ThemeToggle />
          </div>
        </div>
      </header>

      <main className="flex-1 overflow-y-auto">
        <Outlet />
      </main>

      <footer className="border-t border-[color:var(--border)] px-8 py-3">
        <div className="mx-auto flex max-w-[1400px] items-baseline justify-between font-mono text-[11px] tracking-[0.16em] text-[color:var(--fg-dim)] uppercase">
          <span>smix · v1.2.4 · MIT</span>
          <span>
            <span className="text-[color:var(--fg-muted)]">smix</span>{' '}
            <span className="text-[color:var(--accent)]">6.73×</span>{' '}
            <span className="text-[color:var(--fg-muted)]">maestro 2.2.0</span>
          </span>
        </div>
      </footer>
    </div>
  )
}
