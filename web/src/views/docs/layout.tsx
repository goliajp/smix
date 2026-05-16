import { Link, Outlet, useParams } from 'react-router'

import NAV from '../../../content/nav.config'

export function DocsLayout() {
  const { slug } = useParams<{ slug: string }>()
  return (
    <div className="grid grid-cols-[12rem_1fr] gap-8">
      <aside className="border-border space-y-1 border-r pr-4">
        <div className="text-fg-muted mb-3 text-[10px] font-semibold tracking-wider uppercase">
          Docs
        </div>
        {NAV.map((item) => (
          <Link
            key={item.slug}
            to={`/docs/${item.slug}`}
            className={`block rounded-md px-2 py-1 text-sm transition-colors ${
              slug === item.slug
                ? 'bg-accent/10 text-accent'
                : 'text-fg-muted hover:bg-bg-tertiary hover:text-fg'
            }`}
          >
            {item.title}
          </Link>
        ))}
      </aside>
      <div className="docs-content min-w-0">
        <Outlet />
      </div>
    </div>
  )
}
