import { Link } from 'react-router'

function Dot() {
  return (
    <span aria-hidden className="inline-block h-1.5 w-1.5 rounded-full bg-[color:var(--accent)]" />
  )
}

export function HomeView() {
  return (
    <div className="mx-auto max-w-[1180px] px-8 pt-10 pb-20">
      <section className="pb-12">
        <div className="flex items-center gap-2 font-mono text-[11px] tracking-[0.16em] text-[color:var(--accent)] uppercase">
          <Dot />
          <span>smix</span>
          <span className="text-[color:var(--fg-dim)]">·</span>
          <span className="text-[color:var(--fg-muted)]">dashboard</span>
        </div>

        <h1 className="mt-5 text-[44px] leading-[1.05] font-semibold tracking-[-0.025em] lg:text-[56px]">
          AI-native UI automation for the{' '}
          <span className="text-[color:var(--accent)]">iOS Simulator</span> and{' '}
          <span className="text-[color:var(--accent)]">Android emulator</span>.
        </h1>

        <p className="mt-6 max-w-[60ch] text-[color:var(--fg)] opacity-90">
          This is the smix live observation panel. Watch running simulators stream in real time
          while flows execute.
        </p>

        <div className="mt-7 flex flex-wrap items-center gap-3">
          <Link
            to="/live"
            className="border border-[color:var(--accent)] bg-[color:var(--accent)] px-4 py-2 font-mono text-[12px] tracking-[0.16em] text-[color:var(--accent-fg)] uppercase no-underline hover:opacity-90"
          >
            open live panel →
          </Link>
          <a
            className="border border-[color:var(--border-strong)] px-4 py-2 font-mono text-[12px] tracking-[0.16em] text-[color:var(--fg-muted)] uppercase no-underline hover:text-[color:var(--accent)]"
            href="https://github.com/goliajp/smix"
            rel="noopener noreferrer"
            target="_blank"
          >
            github ↗
          </a>
        </div>
      </section>
    </div>
  )
}
