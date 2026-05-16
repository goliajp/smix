#!/usr/bin/env bun
import { exec as execCb, execFile as execFileCb } from 'node:child_process'
import { existsSync } from 'node:fs'
import { resolve as pathResolve } from 'node:path'
import { promisify } from 'node:util'
import { defineCommand, runMain } from 'citty'
import { SimctlClient } from '../sim/simctl.js'
import { runListCommand } from './commands/list.js'
import { runRunCommand, pickCell } from './commands/run.js'
import { runDoctorCommand, type AxpCapability } from './commands/doctor.js'
import { runReplCommand } from './commands/repl.js'
import { runBootCommand } from './commands/boot.js'
import { SimctlDriver } from '../driver/simctl-driver.js'
import { App } from '../sdk/index.js'
import { createSimxMcpServer } from '../mcp/server.js'
import { runMcpCommand } from '../mcp/runner.js'

const execFileP = promisify(execFileCb)
const execP = promisify(execCb)

const listCommand = defineCommand({
  meta: { name: 'list', description: 'List available iOS simulators' },
  async run() {
    const client = new SimctlClient()
    const res = await runListCommand({
      client,
      out: process.stdout,
      err: process.stderr,
    })
    process.exit(res.exitCode)
  },
})

const runCommand = defineCommand({
  meta: { name: 'run', description: 'Run a simx test file against a simulator' },
  args: {
    file: { type: 'positional', description: 'test file (TS)', required: true },
    udid: { type: 'string', description: 'select device by udid' },
    device: { type: 'string', description: 'select device by name' },
    runtime: { type: 'string', description: 'select device by runtime identifier' },
    grep: { type: 'string', description: 'filter case name by substring' },
    json: { type: 'boolean', description: 'output single-line JSON instead of human text', default: false },
    bail: { type: 'boolean', description: 'stop on first case failure', default: false },
  },
  async run({ args }) {
    const client = new SimctlClient()
    const res = await runRunCommand({
      client,
      file: args.file,
      select: {
        udid: args.udid,
        deviceName: args.device,
        runtimeIdentifier: args.runtime,
      },
      ...(args.grep !== undefined ? { grep: args.grep } : {}),
      json: args.json === true,
      bail: args.bail === true,
      out: process.stdout,
      err: process.stderr,
      cwd: process.cwd(),
    })
    process.exit(res.exitCode)
  },
})

const doctorCommand = defineCommand({
  meta: {
    name: 'doctor',
    description: 'Diagnose environment (Xcode, iOS runtimes, claude, bun)',
  },
  args: {
    json: { type: 'boolean', description: 'output JSON instead of human-readable', default: false },
  },
  async run({ args }) {
    const client = new SimctlClient()
    const res = await runDoctorCommand({
      probeXcode: async () => {
        const { stdout } = await execFileP('xcodebuild', ['-version'])
        return stdout.split('\n')[0] ?? ''
      },
      probeRuntimes: async () => {
        const rs = await client.listRuntimes()
        const items = rs.filter((r) => r.isAvailable).map((r) => ({
          identifier: r.identifier,
          name: r.name,
          version: r.version,
          isAvailable: r.isAvailable,
        }))
        return { count: items.length, items }
      },
      probeClaude: async () => {
        let path: string | undefined
        try {
          // command -v is POSIX, doesn't require a `which` binary on the host
          const { stdout } = await execP('command -v claude', { shell: '/bin/sh' })
          const p = stdout.toString().trim()
          path = p.length > 0 ? p : undefined
        } catch {
          path = undefined
        }
        if (path === undefined) {
          return { path: undefined, loggedIn: false }
        }
        try {
          const { stdout } = await execFileP(
            'claude',
            ['-p', 'ping', '--tools', '', '--output-format', 'text'],
            { timeout: 30_000 },
          )
          const body = stdout.toString().trim()
          if (body.length === 0) {
            return {
              path,
              loggedIn: false,
              failureDetail: 'empty stdout from `claude -p ping`',
            }
          }
          return { path, loggedIn: true }
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err)
          return { path, loggedIn: false, failureDetail: msg.slice(0, 200) }
        }
      },
      probeBun: () => process.versions.bun,
      probeHidChannels: async () => {
        const bin = pathResolve(
          process.cwd(),
          'swift-bridge/.build/debug/simx-host-hid',
        )
        if (!existsSync(bin)) {
          throw new Error(
            `simx-host-hid not built at ${bin} ` +
              '(run: swift build --package-path swift-bridge)',
          )
        }
        const { stdout } = await execFileP(bin, ['probe'])
        const parsed = JSON.parse(stdout.toString().trim()) as {
          ok: boolean
          channels: Array<{ name: string; available: boolean }>
        }
        const find = (n: string): boolean =>
          parsed.channels.find((c) => c.name === n)?.available === true
        return { digitizer: find('digitizer'), indigo9: find('indigo9') }
      },
      probeAxp: async (): Promise<AxpCapability> => {
        const bin = pathResolve(
          process.cwd(),
          'swift-bridge/.build/debug/simx-host-hid',
        )
        if (!existsSync(bin)) {
          throw new Error(
            `simx-host-hid not built at ${bin} ` +
              '(run: swift build --package-path swift-bridge)',
          )
        }
        const { stdout } = await execFileP(bin, ['axp-probe'])
        const raw = stdout.toString().trim()
        const parsed: unknown = JSON.parse(raw)
        if (!isAxpCapability(parsed)) {
          throw new Error(
            `simx-host-hid axp-probe returned malformed JSON: ${raw.slice(0, 200)}`,
          )
        }
        return parsed
      },
      json: args.json === true,
      out: process.stdout,
      err: process.stderr,
    })
    process.exit(res.exitCode)
  },
})

function isStringArray(v: unknown): v is string[] {
  return Array.isArray(v) && v.every((x) => typeof x === 'string')
}

function isResolvedMissing(v: unknown): v is { resolved: string[]; missing: string[] } {
  if (typeof v !== 'object' || v === null) return false
  const o = v as Record<string, unknown>
  return isStringArray(o.resolved) && isStringArray(o.missing)
}

function isAxpCapability(v: unknown): v is AxpCapability {
  if (typeof v !== 'object' || v === null) return false
  const o = v as Record<string, unknown>
  const fw = o.framework
  if (typeof fw !== 'object' || fw === null) return false
  const fwo = fw as Record<string, unknown>
  if (typeof fwo.path !== 'string' || typeof fwo.loaded !== 'boolean') return false
  if (!isResolvedMissing(o.classes)) return false
  if (!isResolvedMissing(o.selectors)) return false
  const si = o.sharedInstance
  if (typeof si !== 'object' || si === null) return false
  const sio = si as Record<string, unknown>
  if (typeof sio.available !== 'boolean') return false
  return true
}

const replCommand = defineCommand({
  meta: {
    name: 'repl',
    description: 'Interactive shell: one-line SDK calls + auto screen describe per step',
  },
  args: {
    udid: { type: 'string', description: 'select device by udid' },
    device: { type: 'string', description: 'select device by name' },
    runtime: { type: 'string', description: 'select device by runtime identifier' },
  },
  async run({ args }) {
    const client = new SimctlClient()
    const cell = await pickCell(client, {
      udid: args.udid,
      deviceName: args.device,
      runtimeIdentifier: args.runtime,
    })
    const driver = new SimctlDriver(cell)
    const app = new App(driver)
    const res = await runReplCommand({
      app,
      cell,
      input: process.stdin,
      output: process.stdout,
      err: process.stderr,
    })
    process.exit(res.exitCode)
  },
})

const bootCommand = defineCommand({
  meta: { name: 'boot', description: 'Boot an iOS simulator (idempotent; blocks until booted)' },
  args: {
    device: {
      type: 'positional',
      description: 'device UDID (UUID format) or exact device name',
      required: true,
    },
    json: {
      type: 'boolean',
      description: 'output single-line JSON instead of human text',
      default: false,
    },
    timeout: { type: 'string', description: 'boot timeout in ms (default 120000)' },
  },
  async run({ args }) {
    const client = new SimctlClient()
    let timeoutMs: number | undefined
    if (args.timeout !== undefined) {
      const n = Number(args.timeout)
      if (!Number.isFinite(n) || n <= 0) {
        process.stderr.write(
          `invalid --timeout: '${args.timeout}' (must be positive integer ms)\n`,
        )
        process.exit(1)
      }
      timeoutMs = n
    }
    const res = await runBootCommand({
      client,
      device: args.device,
      json: args.json === true,
      ...(timeoutMs !== undefined ? { timeoutMs } : {}),
      out: process.stdout,
      err: process.stderr,
    })
    process.exit(res.exitCode)
  },
})

const mcpCommand = defineCommand({
  meta: {
    name: 'mcp',
    description: 'Run stdio MCP server for AI agent integration',
  },
  args: {},
  async run() {
    const server = createSimxMcpServer({
      name: 'simx',
      version: '0.0.0',
      client: new SimctlClient(),
    })
    const res = await runMcpCommand({ server })
    process.exit(res.exitCode)
  },
})

const main = defineCommand({
  meta: { name: 'simx', version: '0.0.0', description: 'AI-native iOS Simulator automation' },
  subCommands: {
    list: listCommand,
    run: runCommand,
    doctor: doctorCommand,
    repl: replCommand,
    boot: bootCommand,
    mcp: mcpCommand,
  },
})

await runMain(main)
