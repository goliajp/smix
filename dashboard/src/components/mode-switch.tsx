import { type JSX } from 'react'

export type ViewMode = 'grid' | 'single'

// Segmented mono-uppercase mode switch sitting inline in the LiveView
// status bar. Active half fills with accent (matches the LIVE marker on
// the left so eye flow stays warm), inactive half is dim until hover.
export function ModeSwitch({
  mode,
  onModeChange,
}: {
  mode: ViewMode
  onModeChange: (m: ViewMode) => void
}): JSX.Element {
  const options: ViewMode[] = ['grid', 'single']
  return (
    <div
      role="group"
      aria-label="view mode"
      className="inline-flex divide-x divide-[color:var(--border-strong)] border border-[color:var(--border-strong)] bg-[color:var(--bg)]"
    >
      {options.map((m) => {
        const active = m === mode
        return (
          <button
            key={m}
            type="button"
            onClick={() => onModeChange(m)}
            aria-pressed={active}
            className={
              'px-3 py-1 font-mono text-[10.5px] tracking-[0.18em] uppercase ' +
              (active
                ? 'bg-[color:var(--accent)] text-[color:var(--accent-fg)]'
                : 'text-[color:var(--fg-muted)] hover:text-[color:var(--accent)]')
            }
          >
            {m}
          </button>
        )
      })}
    </div>
  )
}
