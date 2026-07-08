import { type ComponentType, lazy, Suspense } from 'react'
import { useParams } from 'react-router'

import { DocsNotFound } from './not-found'

const MDX_MODULES = import.meta.glob('../../../content/*.mdx') as Record<
  string,
  () => Promise<{ default: ComponentType }>
>

const LAZY_COMPONENTS: Record<string, ComponentType> = Object.fromEntries(
  Object.entries(MDX_MODULES).map(([path, loader]) => {
    const slug = path.replace(/^.*\/([^/]+)\.mdx$/, '$1')
    return [slug, lazy(loader)]
  })
)

export function DocsPage() {
  const { slug } = useParams<{ slug: string }>()
  if (!slug || !(slug in LAZY_COMPONENTS)) {
    return <DocsNotFound />
  }
  const Mdx = LAZY_COMPONENTS[slug]
  return (
    <Suspense
      fallback={
        <div className="font-mono text-[11px] tracking-[0.16em] text-[color:var(--fg-dim)] uppercase">
          loading…
        </div>
      }
    >
      <Mdx />
    </Suspense>
  )
}
