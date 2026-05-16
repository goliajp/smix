import { spawnSync } from 'node:child_process'
import { readFile, unlink } from 'node:fs/promises'
import { resolve } from 'node:path'
import { beforeAll, describe, expect, it } from 'vitest'

const WEB_ROOT = resolve(__dirname, '../..')
const TOOLS_MDX = resolve(WEB_ROOT, 'content/tools.mdx')

let mdx: string

describe('web v0.2 C2 — 27-tool generator', () => {
  beforeAll(async () => {
    try {
      await unlink(TOOLS_MDX)
    } catch {
      // ignore — file may not exist yet
    }
    const result = spawnSync('bun', ['scripts/generate-tools-page.ts'], {
      cwd: WEB_ROOT,
      encoding: 'utf8',
    })
    if (result.status !== 0) {
      throw new Error(`generator exited ${result.status}: ${result.stderr}`)
    }
    mdx = await readFile(TOOLS_MDX, 'utf8')
  })

  it('starts with the title frontmatter', () => {
    expect(mdx.startsWith('---\ntitle: MCP tools reference\n---')).toBe(true)
  })

  it('contains exactly 27 H3 tool entries', () => {
    const matches = mdx.match(/^### /gm) ?? []
    expect(matches.length).toBe(27)
  })

  it('contains exactly the 7 group H2 headers', () => {
    const groups = ['Ping', 'Lifecycle', 'Observe', 'Interaction', 'Compound', 'System', 'VLM']
    for (const g of groups) {
      expect(mdx).toContain(`## ${g}`)
    }
    const h2 = mdx.match(/^## /gm) ?? []
    expect(h2.length).toBe(7)
  })
})
