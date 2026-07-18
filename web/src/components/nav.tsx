import { LINKS, VERSION } from '../data/site'
import { ThemeToggle } from './theme-toggle'

export function Nav() {
  return (
    <header className="sticky top-0 z-50 border-b border-border bg-bg/85 backdrop-blur">
      <div className="mx-auto flex max-w-[1180px] items-center justify-between px-6 py-3">
        <a href="#top" className="flex items-baseline gap-3 no-underline">
          <span className="font-mono text-[19px] font-600 tracking-tight text-fg">smix</span>
          <span className="hidden font-mono text-[11px] tracking-wider text-fg-dim uppercase sm:inline">
            sense · decide · act
          </span>
        </a>
        <nav className="flex items-center gap-4 sm:gap-5">
          <span className="chip hidden sm:inline-flex">v{VERSION}</span>
          <a href="#capabilities" className="hidden font-mono text-[12px] text-fg-muted hover:text-fg md:inline">
            capabilities
          </a>
          <a href="#maestro" className="hidden font-mono text-[12px] text-fg-muted hover:text-fg md:inline">
            vs maestro
          </a>
          <a href="#install" className="hidden font-mono text-[12px] text-fg-muted hover:text-fg md:inline">
            install
          </a>
          <a
            href={LINKS.repo}
            className="font-mono text-[12px] text-fg-muted hover:text-fg"
            target="_blank"
            rel="noreferrer"
          >
            GitHub
          </a>
          <ThemeToggle />
        </nav>
      </div>
    </header>
  )
}
