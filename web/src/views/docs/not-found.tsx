import { Link } from 'react-router'

export function DocsNotFound() {
  return (
    <div>
      <div className="font-mono text-[11px] tracking-[0.16em] text-[color:var(--bad)] uppercase">
        404 · doc not found
      </div>
      <h1 className="mt-3 text-[36px] leading-[1.1] font-semibold tracking-[-0.02em]">Not found</h1>
      <p className="mt-4 text-[color:var(--fg-muted)]">
        That docs page doesn't exist. Try the sidebar or{' '}
        <Link to="/docs/quick-start">jump to Quick start</Link>.
      </p>
    </div>
  )
}
