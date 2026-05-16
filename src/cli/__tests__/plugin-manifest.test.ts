import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

interface PluginManifest {
  name: string
  description: string
  version: string
  mcpServers: {
    simx: {
      command: string
      args: string[]
    }
  }
}

function readManifest(): PluginManifest {
  const raw = readFileSync(
    resolve(process.cwd(), '.claude-plugin/plugin.json'),
    'utf-8',
  )
  return JSON.parse(raw) as PluginManifest
}

describe('plugin manifest (.claude-plugin/plugin.json) (v0.7 C5)', () => {
  it('plugin.json exists at .claude-plugin/plugin.json and is valid JSON', () => {
    const m = readManifest()
    expect(m.name).toBe('simx')
    expect(m.version).toMatch(/^\d+\.\d+\.\d+$/)
  })

  it('plugin manifest embeds mcpServers.simx with bun + ${CLAUDE_PLUGIN_ROOT} args pattern', () => {
    const m = readManifest()
    expect(m.mcpServers.simx.command).toBe('bun')
    const arg0 = m.mcpServers.simx.args[0]
    expect(arg0).toBeDefined()
    expect(arg0!).toContain('${CLAUDE_PLUGIN_ROOT}')
    expect(arg0!).toContain('src/cli/index.ts')
    expect(m.mcpServers.simx.args[1]).toBe('mcp')
  })

  it('plugin manifest description mentions 27 tools to match v0.6 C6 baseline', () => {
    const m = readManifest()
    expect(m.description).toContain('27 tools')
    expect(m.description.length).toBeGreaterThanOrEqual(20)
  })

  it('plugin manifest name matches package.json name and src/cli/index.ts mcp serverInfo name', () => {
    const m = readManifest()
    const pkgRaw = readFileSync(resolve(process.cwd(), 'package.json'), 'utf-8')
    const pkg = JSON.parse(pkgRaw) as { name: string }
    const cliSrc = readFileSync(
      resolve(process.cwd(), 'src/cli/index.ts'),
      'utf-8',
    )
    const match = cliSrc.match(
      /createSimxMcpServer\(\{\s*name:\s*'([^']+)'/,
    )
    expect(match).not.toBeNull()
    const serverInfoName = match![1]!
    expect(m.name).toBe('simx')
    expect(pkg.name).toBe(m.name)
    expect(serverInfoName).toBe(m.name)
  })
})
