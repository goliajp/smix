import { Badge, GlassCard } from '@goliapkg/gds'

const LINKS = [
  {
    title: 'Quick start',
    description: 'Install the Claude Code plugin or clone the repo for local dev.',
    href: 'https://github.com/goliajp/simx#quick-start',
    cta: 'README',
  },
  {
    title: 'MCP plugin install',
    description: 'One-command install into Claude Code; 27 MCP tools auto-registered.',
    href: 'https://github.com/goliajp/simx/blob/develop/docs/plugin-install.md',
    cta: 'docs/plugin-install.md',
  },
  {
    title: 'Authoring guide for AI agents',
    description: 'The 0-shot test-authoring reference. Selectors, actions, matchers, red lines.',
    href: 'https://github.com/goliajp/simx#authoring-guide-for-ai-agents',
    cta: 'README §Authoring',
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
      </section>

      <section className="space-y-4">
        <h2 className="text-fg text-sm font-semibold tracking-wider uppercase">Start here</h2>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          {LINKS.map((l) => (
            <a
              key={l.title}
              href={l.href}
              rel="noopener noreferrer"
              target="_blank"
              className="block"
            >
              <GlassCard className="glow-card h-full">
                <div className="space-y-2 p-4">
                  <h3 className="text-fg text-sm font-semibold">{l.title}</h3>
                  <p className="text-fg-muted text-xs leading-relaxed">{l.description}</p>
                  <div className="text-accent pt-2 font-mono text-[10px]">{l.cta} →</div>
                </div>
              </GlassCard>
            </a>
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
          provider, no API key.
        </p>
      </section>

      <section className="space-y-4">
        <h2 className="text-fg text-sm font-semibold tracking-wider uppercase">
          Docs &amp; demo — coming soon
        </h2>
        <GlassCard>
          <div className="space-y-3 p-5">
            <p className="text-fg-muted text-sm leading-relaxed">
              Full documentation site lands in <strong className="text-fg">v0.2</strong> of this
              site: Authoring guide, 27-tool reference, plugin install walkthrough, examples,
              roadmap. Screencast demo lands in <strong className="text-fg">v0.3</strong>.
            </p>
            <p className="text-fg-muted text-sm leading-relaxed">
              In the meantime, the{' '}
              <a
                className="text-accent hover:underline"
                href="https://github.com/goliajp/simx#readme"
                rel="noopener noreferrer"
                target="_blank"
              >
                README on GitHub
              </a>{' '}
              already contains a complete Authoring guide for AI agents and Quick start — that's
              what Claude Code reads to write tests today.
            </p>
          </div>
        </GlassCard>
      </section>
    </div>
  )
}
