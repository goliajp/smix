#!/usr/bin/env bun
// v0.6 C8 — MCP client e2e smoke. Spawns `bun src/cli/index.ts mcp` as a
// stdio child process, drives 20 tools through the SDK Client API, and emits
// one JSON line to stdout: {passed_tools, failed_tools, total, ...}.
//
// PASS = passed_tools.length >= 18 (cold plan §出口验收 line 51).
// Excluded by design: ping (sample), simulator_shutdown (destructive),
// app_install / app_uninstall (no fixture), permissions_grant (state side-effect),
// explain_screen (slow ~120s; C6 smoke covers happy path), fill (driver
// NotImplemented).

import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { ListToolsResultSchema } from '@modelcontextprotocol/sdk/types.js'
import { spawnSync } from 'node:child_process'
import { readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'

type ToolCall = { name: string; arguments: Record<string, unknown> }
type Outcome = { name: string; ok: true } | { name: string; ok: false; error: string }

const ROOT = process.cwd()

function resolveUdid(): string {
  const devFile = join(ROOT, '.simx/dev-sim.txt')
  if (existsSync(devFile)) {
    const v = readFileSync(devFile, 'utf8').trim()
    if (v) return v
  }
  const list = spawnSync('xcrun', ['simctl', 'list', 'devices', '-j'], { encoding: 'utf8' })
  const j = JSON.parse(list.stdout || '{}') as {
    devices?: Record<string, Array<{ udid: string; state: string; isAvailable: boolean }>>
  }
  for (const arr of Object.values(j.devices ?? {})) {
    for (const d of arr) {
      if (d.isAvailable && d.state === 'Booted') return d.udid
    }
  }
  throw new Error('no booted iOS simulator')
}

function checkRunner(): boolean {
  const r = spawnSync('curl', ['-fsS', '-m', '1', 'http://127.0.0.1:22087/health'], { encoding: 'utf8' })
  return r.status === 0 && (r.stdout ?? '').includes('"ok"')
}

async function main(): Promise<void> {
  const udid = resolveUdid()
  const runnerStarted = checkRunner()

  const transport = new StdioClientTransport({
    command: 'bun',
    args: ['src/cli/index.ts', 'mcp'],
    cwd: ROOT,
  })
  const client = new Client({ name: 'mcp-smoke', version: '0.0.0' }, { capabilities: {} })
  await client.connect(transport)

  // Use raw request for tools/list to avoid SDK Client.listTools() caching the
  // outputSchema validators (which would then reject callTool results that lack
  // structuredContent — server v0.6 returns content[] only by design).
  const listed = await client.request(
    { method: 'tools/list' },
    ListToolsResultSchema,
  )
  const toolsListed = listed.tools.length

  // 20 tool e2e flow. Order is intentional:
  //   lifecycle launch → observe → interact → compound/system → cleanup.
  // Locale: Settings labels in zh-Hans match `通用` / `关于` (sim runs Chinese).
  // Falls back to English on systems where the sim is English-localized; the
  // 5 HID-bridge tools (double_tap / long_press / swipe / scroll_to / key_press)
  // are always failed by SimctlDriver irrespective of locale.
  const settingsGeneral = '通用'
  const settingsAbout = '关于本机'
  const flow: ToolCall[] = [
    { name: 'simulator_list', arguments: {} },
    { name: 'simulator_boot', arguments: { udid } },
    { name: 'app_launch', arguments: { udid, bundleId: 'com.apple.Preferences' } },
    { name: 'screen_describe', arguments: { udid } },
    { name: 'screen_screenshot', arguments: { udid } },
    { name: 'screen_hierarchy', arguments: { udid } },
    { name: 'element_inspect', arguments: { udid, selector: { text: settingsGeneral } } },
    { name: 'tap', arguments: { udid, selector: { text: settingsGeneral } } },
    { name: 'wait_for', arguments: { udid, selector: { text: settingsAbout }, timeoutMs: 5000 } },
    { name: 'find_and_tap', arguments: { udid, selector: { text: settingsAbout } } },
    { name: 'double_tap', arguments: { udid, selector: { text: settingsGeneral } } },
    { name: 'long_press', arguments: { udid, selector: { text: settingsGeneral }, durationMs: 300 } },
    { name: 'swipe', arguments: { udid, direction: 'up' } },
    { name: 'scroll_to', arguments: { udid, selector: { text: settingsAbout } } },
    { name: 'key_press', arguments: { udid, key: 'return' } },
    { name: 'pasteboard_set', arguments: { udid, value: 'simx-c8-e2e' } },
    { name: 'pasteboard_get', arguments: { udid } },
    { name: 'open_url', arguments: { udid, url: 'https://example.com' } },
    { name: 'flow_run', arguments: { udid, steps: [{ action: 'screen_describe', args: { udid } }] } },
    { name: 'app_terminate', arguments: { udid, bundleId: 'com.apple.Preferences' } },
  ]

  const passed: Outcome[] = []
  const failed: Outcome[] = []
  for (const call of flow) {
    try {
      const res = (await client.callTool({ name: call.name, arguments: call.arguments })) as {
        isError?: boolean
        content?: Array<{ text?: string }>
      }
      if (res.isError === true) {
        const text = (res.content?.[0]?.text ?? '').slice(0, 200)
        failed.push({ name: call.name, ok: false, error: text })
      } else {
        passed.push({ name: call.name, ok: true })
      }
    } catch (e) {
      failed.push({ name: call.name, ok: false, error: (e as Error).message.slice(0, 200) })
    }
  }

  await transport.close()

  const out = {
    passed_tools: passed,
    failed_tools: failed,
    total: flow.length,
    tools_listed: toolsListed,
    runner_started: runnerStarted ? 'yes' : 'no',
    dev_sim_udid: udid,
  }
  console.log(JSON.stringify(out))
  process.exit(passed.length >= 18 ? 0 : 1)
}

main().catch((e) => {
  console.error(`[mcp-smoke] fatal: ${(e as Error).message}`)
  process.exit(2)
})
