import { LINKS } from '../data/site'

type Layer = {
  key: string
  name: string
  role: string
  color: string
  detail: string
  primitives: string[]
}

// The three-layer reaction chain — CLAUDE.md §12.1. sense + act are flat
// smix-core capabilities; decide sits at the driver boundary.
const LAYERS: Layer[] = [
  {
    key: 'sense',
    name: 'Sense',
    role: 'core capability',
    color: 'var(--sense)',
    detail:
      'Read the screen accessibility-first: the a11y tree, then Vision OCR, then a fenced AI tier. Core owns it — never buried in a driver.',
    primitives: ['/tree', '/find', '/system-popups', 'ocrText'],
  },
  {
    key: 'decide',
    name: 'Decide',
    role: 'driver boundary',
    color: 'var(--decide)',
    detail:
      'Runtime-specific knowledge is baked into the driver; where there is none, the driver stays transparent and the agent decides with core capabilities.',
    primitives: ['runtime-specific → baked', 'otherwise → transparent'],
  },
  {
    key: 'act',
    name: 'Act',
    role: 'core capability',
    color: 'var(--act)',
    detail:
      'Drive the screen through the same flat core surface as sensing — real HID-level events, never a hidden driver-only path.',
    primitives: ['/tap', '/fill', '/clear', '/pressKey', '/swipe', '/scroll'],
  },
]

type Card = {
  title: string
  body: string
  meta: string
  href?: string
}

const CARDS: Card[] = [
  {
    title: 'AI-readable failures',
    body: 'Every miss is structured, not a stack trace: it carries the visible elements on screen and suggested fixes, so an agent can recover without a human reading logs.',
    meta: 'ExpectationFailure · to_prompt',
    href: LINKS.aiGuide,
  },
  {
    title: 'OCR as a second sense',
    body: 'When the accessibility tree drops a label — a common React Native gap — Vision OCR reads the pixels instead. Sensing is a fallback chain, not a single strategy.',
    meta: 'a11y tree → Vision OCR',
  },
  {
    title: 'Fenced AI-assertion tier',
    body: 'assertCondition and extractWithAI send a screenshot to a local claude CLI and get a structured verdict back. Opt-in and non-deterministic — fenced out of the deterministic sense path by design.',
    meta: 'assertCondition · extractWithAI',
    href: LINKS.aiAssertions,
  },
  {
    title: 'MCP driving surface',
    body: 'Hand the whole simulator to an agent over MCP — launch, describe, tap, fill, press, assert — for the exploratory loop before a flow is worth writing down.',
    meta: 'smix_describe · smix_tap · smix_assert_visible',
    href: LINKS.mcp,
  },
  {
    title: 'True animation-idle',
    body: 'waitForAnimationToEnd samples the screen with a frame-diff and returns the moment it goes still — no fixed sleep. A still screen no longer pays a flat 400ms, and a long animation is no longer silently cut off.',
    meta: 'frame-diff · not a sleep',
  },
  {
    title: 'A phone is reachable only once you say so',
    body: 'Simulators, emulators and physical devices are all addressable — but a physical one must be registered by hand first, so smix never reaches whatever happens to be plugged in. Wiping it takes a second, per-device opt-in. Where a phone has no equivalent of a simulator verb, you get an error naming the gap rather than a silent no-op.',
    meta: 'register · allow-destructive · no silent degrade',
    href: LINKS.cli,
  },
  {
    title: 'Selectors stay semantic',
    body: 'No xpath, no coordinates on the selector surface — a flow names what a human would name. The sole escape hatches are the authorized native tapAtCoord and swipeAtCoord, for the screens that carry no accessibility semantics at all.',
    meta: 'tapAtCoord / swipeAtCoord',
    href: LINKS.selectors,
  },
]

export function Capabilities() {
  return (
    <section id="capabilities" className="border-b border-border">
      <div className="mx-auto max-w-[1180px] px-6 py-16 sm:py-20">
        <p className="eyebrow mb-6">What it does</p>
        <h2
          className="max-w-[20ch] font-mono font-500 text-fg"
          style={{ fontSize: 'clamp(26px, 3.6vw, 40px)', lineHeight: 1.1, letterSpacing: '-0.02em' }}
        >
          One reaction chain: sense, decide, act.
        </h2>
        <p className="mt-5 max-w-[60ch] text-fg-muted">
          Every screen behaviour smix handles runs the same three layers. Sensing and acting are flat
          core capabilities; deciding lives at the driver boundary. It is an invariant of the
          architecture, not a convention.
        </p>

        {/* Three-layer diagram — colour encodes the real layer. */}
        <div className="mt-10 border border-border">
          {LAYERS.map((layer, i) => (
            <div
              key={layer.key}
              className={
                'grid gap-4 p-5 sm:grid-cols-[200px_1fr] sm:gap-8 sm:p-6 ' +
                (i < LAYERS.length - 1 ? 'border-b border-border' : '')
              }
              style={{ borderLeft: `3px solid ${layer.color}` }}
            >
              <div>
                <div className="flex items-baseline gap-3">
                  <span
                    className="font-mono text-[19px] font-600"
                    style={{ color: layer.color }}
                  >
                    {layer.name}
                  </span>
                  <span className="mono-label">{layer.role}</span>
                </div>
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {layer.primitives.map((p) => (
                    <code
                      key={p}
                      className="border border-border bg-bg-inset px-2 py-0.5 font-mono text-[11px] text-fg-muted"
                    >
                      {p}
                    </code>
                  ))}
                </div>
              </div>
              <p className="text-fg-muted">{layer.detail}</p>
            </div>
          ))}
        </div>

        {/* Capability cards. */}
        <div className="mt-4 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {CARDS.map((card) => {
            const inner = (
              <>
                <h3 className="font-mono text-[15px] font-600 text-fg">{card.title}</h3>
                <p className="mt-3 text-[14px] leading-relaxed text-fg-muted">{card.body}</p>
                <p className="mt-4 font-mono text-[11px] tracking-wide text-fg-dim">{card.meta}</p>
              </>
            )
            return card.href ? (
              <a
                key={card.title}
                href={card.href}
                target="_blank"
                rel="noreferrer"
                className="block border border-border bg-bg-elev p-5 no-underline transition-colors hover:border-accent"
              >
                {inner}
              </a>
            ) : (
              <div key={card.title} className="border border-border bg-bg-elev p-5">
                {inner}
              </div>
            )
          })}
        </div>
      </div>
    </section>
  )
}
