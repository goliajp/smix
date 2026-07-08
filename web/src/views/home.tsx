import { Link } from 'react-router'

// Editorial home. Cream bg, body in Inter, ALL-CAPS small-mono labels
// with red dot prefix, minimal warm-tan rules. The hero is a metadata
// block above a title and a body paragraph; numbers (perf) go below in
// a rule-separated grid.

type DocLink = { slug: string; title: string; desc: string }

const DOCS: DocLink[] = [
  {
    slug: 'quick-start',
    title: 'Quick start',
    desc: 'Install the plugin or clone the repo. Boot a sim, run the example, see green in under a minute.',
  },
  {
    slug: 'plugin-install',
    title: 'Plugin install',
    desc: 'One command into Claude Code. 27 MCP tools auto-registered. No API keys.',
  },
  {
    slug: 'authoring',
    title: 'Authoring guide',
    desc: 'For AI agents writing tests. Selectors, actions, matchers, the AI-readable failure contract.',
  },
  {
    slug: 'tools',
    title: 'MCP tools',
    desc: 'All 27 tools: lifecycle, observe, interaction, compound, system, vlm.',
  },
  {
    slug: 'examples',
    title: 'Examples',
    desc: 'Working `.test.ts` files: login tap, text-selector, screenshot-only.',
  },
]

const PERF_HISTORY = [
  { v: 'v1.0', step: '~730 ms', ratio: '~2.2×' },
  { v: 'v1.1', step: '313 ms', ratio: '~5.2×' },
  { v: 'v1.2.4', step: '242 ms', ratio: '6.73×', current: true },
]

function Dot({ accent }: { accent?: boolean }) {
  return (
    <span
      aria-hidden
      className={
        'inline-block h-1.5 w-1.5 rounded-full ' +
        (accent ? 'bg-[color:var(--accent)]' : 'bg-[color:var(--fg-dim)]')
      }
    />
  )
}

export function HomeView() {
  return (
    <div className="mx-auto max-w-[1180px] px-8 pt-10 pb-20">
      {/* ─── hero ─── */}
      <section className="grid grid-cols-12 gap-8 pb-12">
        <div className="col-span-12 lg:col-span-8">
          {/* label */}
          <div className="flex items-center gap-2 font-mono text-[11px] tracking-[0.16em] text-[color:var(--accent)] uppercase">
            <Dot accent />
            <span>smix</span>
            <span className="text-[color:var(--fg-dim)]">·</span>
            <span className="text-[color:var(--fg-muted)]">v1.2.4 · 2026-05-17</span>
          </div>

          {/* title */}
          <h1 className="mt-5 text-[44px] leading-[1.05] font-semibold tracking-[-0.025em] lg:text-[56px]">
            iOS Simulator automation,{' '}
            <span className="text-[color:var(--accent)]">written by AI.</span>
          </h1>

          {/* body */}
          <p className="mt-6 max-w-[60ch] text-[color:var(--fg)] opacity-90">
            smix is an iOS Simulator automation tool built for <strong>Claude Code</strong>{' '}
            subscribers. Real HID tap via <code>CoreSimulator</code> dlsym, accessibility tree via
            XCUITest, a Playwright-shaped DSL, 27 MCP tools. No API keys, no real-device path, no
            multi-provider VLM abstraction.
          </p>

          {/* CTA + install */}
          <div className="mt-7 flex flex-wrap items-center gap-3">
            <Link
              to="/docs/quick-start"
              className="border border-[color:var(--accent)] bg-[color:var(--accent)] px-4 py-2 font-mono text-[12px] tracking-[0.16em] text-[color:var(--accent-fg)] uppercase no-underline hover:opacity-90"
            >
              quick start →
            </Link>
            <code className="border border-[color:var(--border-strong)] bg-[color:var(--bg-elev)] px-3 py-2 text-[13px]">
              <span className="text-[color:var(--fg-dim)]">$ </span>
              bun add -D smix
            </code>
          </div>
        </div>

        {/* sidecar metadata */}
        <aside className="col-span-12 lg:col-span-4">
          <div className="border-l border-[color:var(--border)] pl-5">
            <dl className="space-y-3 font-mono text-[12px]">
              <div className="flex items-baseline gap-3">
                <dt className="w-24 text-[color:var(--fg-dim)] uppercase">vs maestro</dt>
                <dd className="text-[color:var(--accent)]">6.73× faster</dd>
              </div>
              <div className="flex items-baseline gap-3">
                <dt className="w-24 text-[color:var(--fg-dim)] uppercase">per step</dt>
                <dd>242 ms</dd>
              </div>
              <div className="flex items-baseline gap-3">
                <dt className="w-24 text-[color:var(--fg-dim)] uppercase">failures</dt>
                <dd>0 / 180</dd>
              </div>
              <div className="flex items-baseline gap-3">
                <dt className="w-24 text-[color:var(--fg-dim)] uppercase">mcp tools</dt>
                <dd>27</dd>
              </div>
              <div className="flex items-baseline gap-3">
                <dt className="w-24 text-[color:var(--fg-dim)] uppercase">iOS</dt>
                <dd>17 · 18 · 26</dd>
              </div>
              <div className="flex items-baseline gap-3">
                <dt className="w-24 text-[color:var(--fg-dim)] uppercase">license</dt>
                <dd>MIT</dd>
              </div>
            </dl>
          </div>
        </aside>
      </section>

      <hr className="rule-strong" />

      {/* ─── bench ─── */}
      <section className="py-12">
        <div className="flex items-center gap-2 font-mono text-[11px] tracking-[0.16em] text-[color:var(--accent)] uppercase">
          <Dot accent />
          <span>bench</span>
          <span className="text-[color:var(--fg-dim)]">·</span>
          <span className="text-[color:var(--fg-muted)]">30-step complex flow</span>
        </div>
        <h2 className="mt-3 max-w-[28ch] text-[28px] leading-[1.15] font-semibold tracking-[-0.02em]">
          6.73× faster than maestro on the same workflow.
        </h2>
        <p className="mt-3 max-w-[60ch] text-[color:var(--fg-muted)]">
          Same 30 user-perceptible actions on both tools (3 Path B taps, 11 fills, 8 clears, 1
          pressKey delete, 9 assertVisible / expect.toBeVisible). iOS 26.4 simulator. 0 step
          failures across 6 iterations.
        </p>

        <div className="mt-7 overflow-hidden border border-[color:var(--border)]">
          <table className="w-full border-collapse text-[13px]">
            <thead>
              <tr className="bg-[color:var(--bg-inset)]">
                <th className="border-b border-[color:var(--border)] px-4 py-2.5 text-left font-mono text-[11px] font-medium tracking-[0.16em] text-[color:var(--fg-muted)] uppercase">
                  metric
                </th>
                <th className="border-b border-[color:var(--border)] px-4 py-2.5 text-left font-mono text-[11px] font-medium tracking-[0.16em] text-[color:var(--fg-muted)] uppercase">
                  smix v1.2.4
                </th>
                <th className="border-b border-[color:var(--border)] px-4 py-2.5 text-left font-mono text-[11px] font-medium tracking-[0.16em] text-[color:var(--fg-muted)] uppercase">
                  maestro 2.2.0
                </th>
                <th className="border-b border-[color:var(--border)] px-4 py-2.5 text-left font-mono text-[11px] font-medium tracking-[0.16em] text-[color:var(--fg-muted)] uppercase">
                  ratio
                </th>
              </tr>
            </thead>
            <tbody className="font-mono">
              <tr>
                <td className="border-b border-[color:var(--border)] px-4 py-3 text-[color:var(--fg-muted)]">
                  per-iter P50
                </td>
                <td className="border-b border-[color:var(--border)] px-4 py-3">7,268 ms</td>
                <td className="border-b border-[color:var(--border)] px-4 py-3 text-[color:var(--fg-muted)]">
                  48,918 ms
                </td>
                <td className="border-b border-[color:var(--border)] px-4 py-3 text-[color:var(--accent)]">
                  6.73×
                </td>
              </tr>
              <tr>
                <td className="border-b border-[color:var(--border)] px-4 py-3 text-[color:var(--fg-muted)]">
                  per-step average
                </td>
                <td className="border-b border-[color:var(--border)] px-4 py-3">242 ms</td>
                <td className="border-b border-[color:var(--border)] px-4 py-3 text-[color:var(--fg-muted)]">
                  1,630 ms
                </td>
                <td className="border-b border-[color:var(--border)] px-4 py-3 text-[color:var(--accent)]">
                  6.73×
                </td>
              </tr>
              <tr>
                <td className="px-4 py-3 text-[color:var(--fg-muted)]">step failures</td>
                <td className="px-4 py-3">0 / 90</td>
                <td className="px-4 py-3 text-[color:var(--fg-muted)]">0 / 90</td>
                <td className="px-4 py-3 text-[color:var(--fg-dim)]">—</td>
              </tr>
            </tbody>
          </table>
        </div>

        {/* perf history */}
        <div className="mt-6 grid grid-cols-1 gap-3 sm:grid-cols-3">
          {PERF_HISTORY.map((h) => (
            <div
              key={h.v}
              className={
                'border p-4 ' +
                (h.current
                  ? 'border-[color:var(--accent)] bg-[color:var(--accent-soft)]'
                  : 'border-[color:var(--border)]')
              }
            >
              <div className="font-mono text-[11px] tracking-[0.16em] text-[color:var(--fg-muted)] uppercase">
                {h.v}
                {h.current ? (
                  <span className="ml-2 text-[color:var(--accent)]">· current</span>
                ) : null}
              </div>
              <div className="mt-2 font-mono text-[18px] font-semibold">
                {h.step}
                <span className="text-[color:var(--fg-muted)]"> / step</span>
              </div>
              <div className="mt-1 font-mono text-[12px] text-[color:var(--accent)]">
                {h.ratio} maestro
              </div>
            </div>
          ))}
        </div>
      </section>

      <hr className="rule" />

      {/* ─── docs index ─── */}
      <section className="py-12">
        <div className="flex items-center gap-2 font-mono text-[11px] tracking-[0.16em] text-[color:var(--accent)] uppercase">
          <Dot accent />
          <span>documentation</span>
        </div>
        <h2 className="mt-3 text-[28px] leading-[1.15] font-semibold tracking-[-0.02em]">
          Start here.
        </h2>

        <div className="mt-7 grid grid-cols-1 gap-0 border border-[color:var(--border)] md:grid-cols-2">
          {DOCS.map((d, i) => (
            <Link
              key={d.slug}
              to={`/docs/${d.slug}`}
              className={
                'group block border-[color:var(--border)] p-5 no-underline hover:bg-[color:var(--bg-elev)] ' +
                (i % 2 === 0 ? 'md:border-r' : '') +
                (i < DOCS.length - 2
                  ? ' border-b'
                  : i === DOCS.length - 2
                    ? ' border-b md:border-b-0'
                    : ' border-b md:border-b-0')
              }
            >
              <div className="flex items-start justify-between gap-3">
                <h3 className="text-[16px] font-semibold text-[color:var(--fg)] group-hover:text-[color:var(--accent)]">
                  {d.title}
                </h3>
                <span className="font-mono text-[color:var(--fg-dim)] group-hover:text-[color:var(--accent)]">
                  →
                </span>
              </div>
              <p className="mt-2 text-[13.5px] leading-relaxed text-[color:var(--fg-muted)]">
                {d.desc}
              </p>
              <div className="mt-3 font-mono text-[11px] tracking-[0.1em] text-[color:var(--fg-dim)] group-hover:text-[color:var(--accent)]">
                /docs/{d.slug}
              </div>
            </Link>
          ))}
        </div>
      </section>

      <hr className="rule" />

      {/* ─── does / never does ─── */}
      <section className="py-12">
        <div className="flex items-center gap-2 font-mono text-[11px] tracking-[0.16em] text-[color:var(--accent)] uppercase">
          <Dot accent />
          <span>shape</span>
        </div>
        <h2 className="mt-3 text-[28px] leading-[1.15] font-semibold tracking-[-0.02em]">
          What smix does. What it doesn't.
        </h2>

        <div className="mt-7 grid grid-cols-1 gap-10 md:grid-cols-2">
          <div>
            <div className="font-mono text-[11px] tracking-[0.16em] text-[color:var(--accent)] uppercase">
              + does
            </div>
            <ul className="mt-3 space-y-2.5 text-[14px] leading-relaxed text-[color:var(--fg)]">
              <li>Runs on iOS Simulator only (iOS 17 / 18 / 26).</li>
              <li>
                Talks to <code>CoreSimulator</code> + Indigo HID via dlsym in its own process.
              </li>
              <li>
                Reads the accessibility tree via XCUITest snapshot (cached, instrumented per stage).
              </li>
              <li>Exposes 27 MCP tools for Claude Code — no API keys, no extra config.</li>
              <li>Writes AI-readable failure JSON (visibleElements + suggestions + screenshot).</li>
            </ul>
          </div>

          <div>
            <div className="font-mono text-[11px] tracking-[0.16em] text-[color:var(--bad)] uppercase">
              − never does
            </div>
            <ul className="mt-3 space-y-2.5 text-[14px] leading-relaxed text-[color:var(--fg)]">
              <li>Touch real devices.</li>
              <li>
                Patch Apple binaries or inject across processes (Springboard, the target app).
              </li>
              <li>
                Use a multi-provider VLM abstraction. AI runs through the local <code>claude</code>{' '}
                CLI.
              </li>
              <li>
                Listen on <code>0.0.0.0</code> — the runner port is bound to <code>127.0.0.1</code>{' '}
                only (compliance gate, v1.2 C1).
              </li>
              <li>Expose xpath / pixel-coord selectors to the test DSL.</li>
            </ul>
          </div>
        </div>
      </section>
    </div>
  )
}
