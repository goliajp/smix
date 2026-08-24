// Single source of the version + external links used across the page.
// VERSION traces to the workspace Cargo.toml (`version = "8.0.0"`); the SDK
// coordinates trace to README.md's Install section + the brief. Links point
// at GitHub `HEAD` (the repo default branch) so they resolve regardless of
// how this static bundle is deployed.

export const VERSION = '8.0.0'

const REPO = 'https://github.com/goliajp/smix'
const BLOB = `${REPO}/blob/HEAD`
const TREE = `${REPO}/tree/HEAD`

export const LINKS = {
  repo: REPO,
  llms: `${BLOB}/llms.txt`,
  aiGuide: `${TREE}/docs/ai-guide`,
  quickstart: `${BLOB}/docs/ai-guide/01-quickstart.md`,
  selectors: `${BLOB}/docs/ai-guide/03-selectors.md`,
  mcp: `${BLOB}/docs/ai-guide/11-mcp.md`,
  cli: `${BLOB}/docs/ai-guide/05-cli.md`,
  aiAssertions: `${BLOB}/docs/ai-guide/10-ai-assertions.md`,
  verbParity: `${BLOB}/docs/ai-guide/verb-parity.md`,
  hello: `${BLOB}/examples/hello.yaml`,
  dashboard: `${TREE}/dashboard`,
} as const

// SDK install coordinates — all four ship the same 8.0.0 release surface.
export const SDKS = [
  {
    id: 'rust',
    name: 'Rust CLI + SDK',
    registry: 'crates.io',
    coordinate: 'smix-cli',
    install: 'cargo install smix-cli --locked',
    lang: 'bash',
  },
  {
    id: 'ts',
    name: 'TypeScript / Node / Bun',
    registry: 'npm',
    coordinate: '@goliapkg/smix',
    install: 'npm install @goliapkg/smix@8.0.0',
    lang: 'bash',
  },
  {
    id: 'kotlin',
    name: 'Kotlin / Java',
    registry: 'Maven Central',
    coordinate: 'jp.golia.smix:smix-sdk',
    install: 'implementation("jp.golia.smix:smix-sdk:8.0.0")',
    lang: 'kotlin',
  },
  {
    id: 'swift',
    name: 'Swift Package Manager',
    registry: 'SwiftPM',
    coordinate: 'goliajp/smix @ swift-v8.0.0',
    install: '.package(url: "https://github.com/goliajp/smix", from: "8.0.0")',
    lang: 'swift',
  },
] as const
