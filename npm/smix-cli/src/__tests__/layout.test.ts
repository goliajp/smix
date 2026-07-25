import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { TARGETS } from '../resolve.js'

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..')
const readJson = (p: string): Record<string, unknown> =>
  JSON.parse(readFileSync(join(root, p), 'utf8')) as Record<string, unknown>

describe('package layout', () => {
  it('depends on exactly the platform packages the target table names', () => {
    const main = readJson('package.json')
    const deps = main['optionalDependencies'] as Record<string, string>
    expect(Object.keys(deps).sort()).toEqual(
      TARGETS.map((t) => `@goliapkg/smix-cli-${t.suffix}`).sort(),
    )
    // One version for the whole set: a launcher resolving a subpackage
    // built from different sources is the failure this pins.
    const versions = new Set(Object.values(deps))
    expect(versions.size).toBe(1)
    expect([...versions][0]).toBe(main['version'])
  })

  it('installs both executables, because the MCP server is one of them', () => {
    const bin = readJson('package.json')['bin'] as Record<string, string>
    expect(Object.keys(bin).sort()).toEqual(['smix', 'smix-mcp'])
  })

  it('each platform package admits only the machine it was built for', () => {
    for (const t of TARGETS) {
      const pkg = readJson(join('npm', t.suffix, 'package.json'))
      expect(pkg['name']).toBe(`@goliapkg/smix-cli-${t.suffix}`)
      expect(pkg['os']).toEqual([t.platform])
      expect(pkg['cpu']).toEqual([t.arch])
      // The binaries must be listed, or `npm pack` ships an empty package
      // that installs cleanly and provides no command.
      expect(pkg['files']).toEqual(['smix', 'smix-mcp'])
    }
  })

  it('the CI job builds the same triples this package resolves to', () => {
    const ci = readFileSync(join(root, '..', '..', '.github/workflows/ci.yml'), 'utf8')
    const job = ci.slice(ci.indexOf('cli-prebuild:'))
    expect(job).not.toBe('')
    for (const t of TARGETS) {
      expect(job).toContain(t.triple)
    }
  })
})
