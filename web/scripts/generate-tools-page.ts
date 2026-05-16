import { writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'

import { ALL_TOOLS } from '../../src/mcp/tools'

type GroupName = 'Ping' | 'Lifecycle' | 'Observe' | 'Interaction' | 'Compound' | 'System' | 'VLM'

const NAME_TO_GROUP: Record<string, GroupName> = {
  ping: 'Ping',
  simulator_list: 'Lifecycle',
  simulator_boot: 'Lifecycle',
  simulator_shutdown: 'Lifecycle',
  app_launch: 'Lifecycle',
  app_terminate: 'Lifecycle',
  app_install: 'Lifecycle',
  app_uninstall: 'Lifecycle',
  screen_describe: 'Observe',
  screen_screenshot: 'Observe',
  screen_hierarchy: 'Observe',
  element_inspect: 'Observe',
  tap: 'Interaction',
  double_tap: 'Interaction',
  long_press: 'Interaction',
  fill: 'Interaction',
  swipe: 'Interaction',
  scroll_to: 'Interaction',
  key_press: 'Interaction',
  find_and_tap: 'Compound',
  wait_for: 'Compound',
  flow_run: 'Compound',
  open_url: 'System',
  pasteboard_set: 'System',
  pasteboard_get: 'System',
  permissions_grant: 'System',
  explain_screen: 'VLM',
}

const GROUP_ORDER: readonly GroupName[] = [
  'Ping',
  'Lifecycle',
  'Observe',
  'Interaction',
  'Compound',
  'System',
  'VLM',
]

export default async function generateToolsPage(): Promise<string> {
  const buckets: Record<GroupName, typeof ALL_TOOLS[number][]> = {
    Ping: [],
    Lifecycle: [],
    Observe: [],
    Interaction: [],
    Compound: [],
    System: [],
    VLM: [],
  }

  for (const tool of ALL_TOOLS) {
    const group = NAME_TO_GROUP[tool.name]
    if (!group) {
      throw new Error(`unmapped tool: ${tool.name} (add to NAME_TO_GROUP in generate-tools-page.ts)`)
    }
    buckets[group].push(tool)
  }

  const lines: string[] = []
  lines.push('---')
  lines.push('title: MCP tools reference')
  lines.push('---')
  lines.push('')
  lines.push('# MCP tools reference')
  lines.push('')
  lines.push(
    'simx exposes **27 MCP tools** to Claude Code. This page is generated at build time from `src/mcp/tools.ts` — the source of truth.',
  )
  lines.push('')

  for (const group of GROUP_ORDER) {
    const tools = buckets[group]
    if (tools.length === 0) continue
    lines.push(`## ${group}`)
    lines.push('')
    for (const tool of tools) {
      lines.push(`### ${tool.name}`)
      lines.push('')
      // MDX treats `{...}` as JSX expressions and `<...>` as JSX elements in
      // prose. Escape both so descriptions like "Foo { bar }" or "<tool name>"
      // pass through verbatim.
      lines.push(tool.description.replace(/[{}<>]/g, (c) => '\\' + c))
      lines.push('')
      lines.push('```json')
      lines.push(JSON.stringify(tool.inputJsonSchema, null, 2))
      lines.push('```')
      lines.push('')
    }
  }

  return lines.join('\n')
}

if (import.meta.main) {
  const out = await generateToolsPage()
  const outPath = resolve(import.meta.dirname, '../content/tools.mdx')
  await writeFile(outPath, out, 'utf8')
  console.log(`wrote ${outPath} (${out.length} chars, ${ALL_TOOLS.length} tools)`)
}
