import { LINKS, VERSION } from '../data/site'

const COLS: { heading: string; links: { label: string; href: string }[] }[] = [
  {
    heading: 'For agents',
    links: [
      { label: 'llms.txt', href: LINKS.llms },
      { label: 'MCP setup', href: LINKS.mcp },
      { label: 'AI assertions', href: LINKS.aiAssertions },
    ],
  },
  {
    heading: 'Docs',
    links: [
      { label: 'ai-guide', href: LINKS.aiGuide },
      { label: 'Quickstart', href: LINKS.quickstart },
      { label: 'Selectors', href: LINKS.selectors },
      { label: 'Verb parity', href: LINKS.verbParity },
    ],
  },
  {
    heading: 'Project',
    links: [
      { label: 'GitHub', href: LINKS.repo },
      { label: 'Dashboard (live-sim panel)', href: LINKS.dashboard },
      { label: 'examples/hello.yaml', href: LINKS.hello },
    ],
  },
]

export function Footer() {
  return (
    <footer className="bg-bg-inset">
      <div className="mx-auto max-w-[1180px] px-6 py-12">
        <div className="grid gap-8 sm:grid-cols-[1.4fr_1fr_1fr_1.2fr]">
          <div>
            <div className="font-mono text-[17px] font-600 text-fg">smix</div>
            <p className="mt-3 max-w-[34ch] text-[13px] text-fg-muted">
              The deterministic execution and observation substrate of the AI mobile-app dev/debug
              loop. A Claude Code sub-product.
            </p>
          </div>
          {COLS.map((col) => (
            <nav key={col.heading} aria-label={col.heading}>
              <div className="mono-label">{col.heading}</div>
              <ul className="mt-3 flex flex-col gap-2">
                {col.links.map((l) => (
                  <li key={l.label}>
                    <a
                      href={l.href}
                      target="_blank"
                      rel="noreferrer"
                      className="font-mono text-[13px] text-fg-muted hover:text-accent"
                    >
                      {l.label}
                    </a>
                  </li>
                ))}
              </ul>
            </nav>
          ))}
        </div>
        <div className="mt-10 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-5">
          <span className="font-mono text-[11px] tracking-wide text-fg-dim">
            simulator only · iOS + Android · v{VERSION}
          </span>
          <span className="font-mono text-[11px] text-fg-dim">sense · decide · act</span>
        </div>
      </div>
    </footer>
  )
}
