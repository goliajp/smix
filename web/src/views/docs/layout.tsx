import { Link, Outlet, useParams } from 'react-router'

import NAV from '../../../content/nav.config'

// Docs layout — editorial style. Left sidebar has a "TOPICS" cap label
// + numbered cards (01, 02, ...) with an accent dot on the active item.
// Right column hosts the MDX prose.
export function DocsLayout() {
  const { slug } = useParams<{ slug: string }>()
  return (
    <div className="mx-auto grid max-w-[1180px] grid-cols-1 gap-12 px-8 py-12 lg:grid-cols-[16rem_minmax(0,1fr)]">
      <aside className="lg:sticky lg:top-12 lg:self-start">
        <div className="flex items-baseline justify-between font-mono text-[11px] tracking-[0.16em] text-[color:var(--fg-muted)] uppercase">
          <span>Topics</span>
          <span className="text-[color:var(--fg-dim)]">{String(NAV.length).padStart(2, '0')}</span>
        </div>
        <nav className="mt-4 flex flex-col gap-1">
          {NAV.map((item, i) => {
            const active = slug === item.slug
            return (
              <Link
                className={
                  'flex items-center gap-3 border px-3 py-2.5 font-mono text-[12px] no-underline transition-colors ' +
                  (active
                    ? 'border-[color:var(--accent)] bg-[color:var(--accent-soft)] text-[color:var(--fg)]'
                    : 'border-transparent text-[color:var(--fg-muted)] hover:bg-[color:var(--bg-elev)] hover:text-[color:var(--fg)]')
                }
                key={item.slug}
                to={`/docs/${item.slug}`}
              >
                <span className="w-5 text-[10px] tracking-[0.16em] text-[color:var(--fg-dim)] uppercase">
                  {String(i + 1).padStart(2, '0')}
                </span>
                <span className="flex-1 tracking-[0.04em]">{item.title}</span>
                {active ? (
                  <span aria-hidden className="h-1.5 w-1.5 rounded-full bg-[color:var(--accent)]" />
                ) : null}
              </Link>
            )
          })}
        </nav>

        <div className="mt-10 border-t border-[color:var(--border)] pt-4">
          <div className="font-mono text-[11px] tracking-[0.16em] text-[color:var(--fg-muted)] uppercase">
            Build
          </div>
          <div className="mt-1 font-mono text-[11px] text-[color:var(--fg)]">v1.2.4</div>
          <div className="mt-1 font-mono text-[11px] text-[color:var(--accent)]">6.73× maestro</div>
        </div>
      </aside>

      <div className="prose min-w-0">
        <Outlet />
      </div>
    </div>
  )
}
