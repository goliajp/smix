import { Link } from 'react-router'

export function DocsNotFound() {
  return (
    <div className="space-y-4 py-8">
      <h1 className="text-fg text-2xl font-bold">Not found</h1>
      <p className="text-fg-muted text-sm">
        That docs page doesn't exist (yet). Try the sidebar or{' '}
        <Link className="text-accent hover:underline" to="/docs/quick-start">
          jump to Quick start
        </Link>
        .
      </p>
    </div>
  )
}
