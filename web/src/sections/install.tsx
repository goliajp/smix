import { LINKS, SDKS, VERSION } from '../data/site'
import GoldenPath from '../snippets/golden-path.mdx'

export function Install() {
  return (
    <section id="install" className="border-b border-border">
      <div className="mx-auto max-w-[1180px] px-6 py-16 sm:py-20">
        <p className="eyebrow mb-6">How to start</p>
        <h2
          className="max-w-[20ch] font-mono font-500 text-fg"
          style={{ fontSize: 'clamp(26px, 3.6vw, 40px)', lineHeight: 1.1, letterSpacing: '-0.02em' }}
        >
          Pick the SDK for your harness.
        </h2>
        <p className="mt-5 max-w-[60ch] text-fg-muted">
          All four ship the same {VERSION} wire-level surface. Requires macOS with Xcode + Simulator
          for iOS, or the Android SDK with an emulator image for Android.
        </p>

        {/* Covers the gap between a version being written down here and the
            registries serving it, and comes out with the release — fact-scan
            fails both ways round now. The number below is the binding, not a
            literal — a literal is the part that goes stale. */}
        <p className="mt-5 max-w-[68ch] border border-accent bg-bg-elev px-4 py-3 text-[13px] text-fg-muted">
          <span className="font-mono font-600 text-fg">{VERSION} is not published yet.</span>{' '}
          The coordinates below are what {VERSION} will be. Until it lands, install whatever the
          registries currently serve — or build from{' '}
          <a href={LINKS.repo} target="_blank" rel="noreferrer">
            source
          </a>
          .
        </p>


        <div className="mt-10 grid gap-4 sm:grid-cols-2">
          {SDKS.map((sdk) => (
            <div key={sdk.id} className="flex flex-col border border-border bg-bg-elev">
              <div className="flex items-baseline justify-between border-b border-border px-4 py-2.5">
                <span className="font-mono text-[13px] font-600 text-fg">{sdk.name}</span>
                <span className="mono-label">{sdk.registry}</span>
              </div>
              <div className="flex grow flex-col gap-3 p-4">
                <div className="font-mono text-[12px] text-fg-dim">{sdk.coordinate}</div>
                <pre className="grow text-[12px]">
                  <code>{sdk.install}</code>
                </pre>
              </div>
            </div>
          ))}
        </div>

        <div className="mt-10 grid gap-8 lg:grid-cols-[1fr_360px] lg:items-start">
          <div>
            <span className="mono-label">golden path</span>
            <div className="mt-3">
              <GoldenPath />
            </div>
          </div>

          <div className="flex flex-col gap-3">
            <span className="mono-label">for agents</span>
            <a
              href={LINKS.mcp}
              target="_blank"
              rel="noreferrer"
              className="block border border-border bg-bg-elev p-4 no-underline transition-colors hover:border-accent"
            >
              <span className="font-mono text-[13px] font-600 text-fg">MCP setup →</span>
              <p className="mt-1.5 text-[13px] text-fg-muted">
                Drive a simulator from an agent over MCP. Bring the runner up, point your client at
                smix-mcp.
              </p>
            </a>
            <a
              href={LINKS.llms}
              target="_blank"
              rel="noreferrer"
              className="block border border-border bg-bg-elev p-4 no-underline transition-colors hover:border-accent"
            >
              <span className="font-mono text-[13px] font-600 text-fg">llms.txt →</span>
              <p className="mt-1.5 text-[13px] text-fg-muted">
                The whole verb table and selector taxonomy, generated for agents to read directly.
              </p>
            </a>
          </div>
        </div>
      </div>
    </section>
  )
}
