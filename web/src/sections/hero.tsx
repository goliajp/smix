import { IdleSampler } from '../components/idle-sampler'
import { LINKS } from '../data/site'
import HeroFlow from '../snippets/hero-flow.mdx'

export function Hero() {
  return (
    <section id="top" className="border-b border-border">
      <div className="mx-auto max-w-[1180px] px-6 pt-16 pb-14 sm:pt-24 sm:pb-20">
        <p className="eyebrow mb-6">What is it</p>

        <h1
          className="max-w-[16ch] font-mono font-500 text-fg"
          style={{
            fontSize: 'clamp(34px, 6vw, 68px)',
            lineHeight: 1.05,
            letterSpacing: '-0.02em',
          }}
        >
          Deterministic execution{' '}
          <span className="text-accent">+</span> observation for the AI mobile loop.
        </h1>

        <p className="mt-7 max-w-[62ch] text-fg-muted" style={{ fontSize: 'clamp(15px, 1.5vw, 19px)' }}>
          smix is the substrate an AI agent stands on to build, drive, and debug a mobile app: it
          senses the screen, acts on it, and reports failures the agent can read. Every maestro verb,
          plus a native <span className="text-fg">sense · decide · act</span> surface — iOS and
          Android at parity. Simulators and emulators by default; a physical device once you
          register it, with erase and uninstall refused on it until you say otherwise.
        </p>

        <div className="mt-9 flex flex-wrap items-center gap-3">
          <a
            href="#install"
            className="bg-accent px-5 py-2.5 font-mono text-[13px] tracking-wide text-accent-fg uppercase no-underline transition-opacity hover:opacity-90"
          >
            Install
          </a>
          <a
            href={LINKS.quickstart}
            target="_blank"
            rel="noreferrer"
            className="border border-border-strong px-5 py-2.5 font-mono text-[13px] tracking-wide text-fg uppercase no-underline transition-colors hover:border-accent hover:text-accent"
          >
            Quickstart →
          </a>
        </div>

        <div className="mt-14 grid gap-4 lg:grid-cols-2 lg:items-start">
          <div className="border border-border bg-bg-inset">
            <div className="flex items-center justify-between border-b border-border px-4 py-2">
              <span className="mono-label">examples/hello.yaml</span>
              <span className="font-mono text-[11px] text-fg-dim">golden path</span>
            </div>
            <div className="[&_pre]:border-0 [&_pre]:bg-transparent">
              <HeroFlow />
            </div>
          </div>
          <IdleSampler />
        </div>
      </div>
    </section>
  )
}
