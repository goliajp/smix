import { Badge, GlassCard } from '@goliapkg/gds'
import { Link } from 'react-router'

const LINKS = [
  {
    title: 'Quick start',
    description: 'Install the Claude Code plugin or clone the repo for local dev.',
    to: '/docs/quick-start',
    cta: '/docs/quick-start',
  },
  {
    title: 'MCP plugin install',
    description: 'One-command install into Claude Code; 27 MCP tools auto-registered.',
    to: '/docs/plugin-install',
    cta: '/docs/plugin-install',
  },
  {
    title: 'Authoring guide for AI agents',
    description: 'The 0-shot test-authoring reference. Selectors, actions, matchers, red lines.',
    to: '/docs/authoring',
    cta: '/docs/authoring',
  },
]

const TOOL_GROUPS = [
  { label: 'lifecycle', count: 7 },
  { label: 'observe', count: 4 },
  { label: 'interaction', count: 7 },
  { label: 'compound', count: 3 },
  { label: 'system', count: 4 },
  { label: 'vlm', count: 1 },
  { label: 'ping', count: 1 },
]

export function HomeView() {
  return (
    <div className="space-y-12">
      <section className="space-y-4">
        <div className="flex items-center gap-3">
          <h1
            className="text-fg text-3xl font-bold tracking-tight"
            style={{ textShadow: '0 0 24px var(--gds-accent, #3b82f6)' }}
          >
            simx
          </h1>
          <Badge color="info">v1.0</Badge>
          <Badge>MIT</Badge>
        </div>
        <p className="text-fg max-w-2xl text-lg leading-relaxed">
          AI-native iOS Simulator automation. <strong>27 MCP tools</strong> for Claude Code, iOS
          17/18/26 simulators, real HID injection + accessibility tree read, ergonomic test DSL.
        </p>
        <p className="text-fg-muted max-w-2xl text-sm">
          Designed for Claude Code subscribers — no API keys, no multi-provider VLM abstraction, no
          real-device path. The agent reads the screen via private CoreSimulator symbols (dlsym),
          taps via Indigo HID, and writes assertions in a Playwright-shaped DSL.
        </p>
        <div className="pt-2">
          <Link
            to="/docs/quick-start"
            className="bg-accent/15 hover:bg-accent/25 text-accent inline-flex items-center gap-2 rounded-md px-4 py-2 text-sm font-semibold transition-colors"
          >
            Read the docs →
          </Link>
        </div>
      </section>

      <section className="space-y-4">
        <h2 className="text-fg text-sm font-semibold tracking-wider uppercase">Start here</h2>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          {LINKS.map((l) => (
            <Link key={l.title} to={l.to} className="block">
              <GlassCard className="glow-card h-full">
                <div className="space-y-2 p-4">
                  <h3 className="text-fg text-sm font-semibold">{l.title}</h3>
                  <p className="text-fg-muted text-xs leading-relaxed">{l.description}</p>
                  <div className="text-accent pt-2 font-mono text-[10px]">{l.cta} →</div>
                </div>
              </GlassCard>
            </Link>
          ))}
        </div>
      </section>

      <section className="space-y-4">
        <h2 className="text-fg text-sm font-semibold tracking-wider uppercase">27 MCP tools</h2>
        <div className="flex flex-wrap gap-2">
          {TOOL_GROUPS.map((g) => (
            <Badge key={g.label}>
              {g.label} · {g.count}
            </Badge>
          ))}
        </div>
        <p className="text-fg-muted text-xs">
          Including <code className="text-fg-muted">explain_screen</code> which shells out to your
          local <code className="text-fg-muted">claude</code> CLI for VLM grounding — no third
          provider, no API key. See the{' '}
          <Link to="/docs/tools" className="text-accent hover:underline">
            full 27-tool reference
          </Link>
          .
        </p>
      </section>
    </div>
  )
}
