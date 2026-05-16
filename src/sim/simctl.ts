import { execFile, spawn as childSpawn } from 'node:child_process'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

export type SimctlRuntime = {
  identifier: string
  name: string
  version: string
  isAvailable: boolean
}

export type SimctlDevice = {
  udid: string
  name: string
  state: string
  isAvailable: boolean
  runtimeIdentifier: string
  deviceTypeIdentifier: string
}

export type SimctlPermission =
  | 'all'
  | 'calendar'
  | 'camera'
  | 'contacts-limited'
  | 'contacts'
  | 'location'
  | 'location-always'
  | 'photos-add'
  | 'photos'
  | 'media-library'
  | 'microphone'
  | 'motion'
  | 'reminders'
  | 'siri'

export type SpawnOptions = {
  stdin?: string
  env?: Record<string, string>
}

export type Spawner = (
  cmd: string,
  args: readonly string[],
  opts?: SpawnOptions,
) => Promise<{ stdout: string; stderr: string; exitCode: number }>

export type BinarySpawner = (
  cmd: string,
  args: readonly string[],
) => Promise<{ stdout: Buffer; stderr: string; exitCode: number }>

export type SimctlClientOptions = {
  spawn?: Spawner
  spawnBinary?: BinarySpawner
}

const MAX_DETAIL_CHARS = 200

export class SimctlClient {
  private readonly spawn: Spawner
  private readonly spawnBinary: BinarySpawner

  constructor(opts: SimctlClientOptions = {}) {
    this.spawn = opts.spawn ?? defaultSpawn
    this.spawnBinary = opts.spawnBinary ?? defaultSpawnBinary
  }

  async listRuntimes(): Promise<SimctlRuntime[]> {
    const parsed = await this.runJson<{ runtimes: RawRuntime[] }>(
      ['simctl', 'list', 'runtimes', '-j'],
      'list runtimes',
      'runtimes',
    )
    return parsed.runtimes
      .filter((r) => r.platform === 'iOS')
      .map((r) => ({
        identifier: r.identifier,
        name: r.name,
        version: r.version,
        isAvailable: r.isAvailable,
      }))
  }

  async listDevices(): Promise<SimctlDevice[]> {
    const parsed = await this.runJson<{ devices: Record<string, RawDevice[]> }>(
      ['simctl', 'list', 'devices', '-j'],
      'list devices',
      'devices',
    )
    const out: SimctlDevice[] = []
    for (const [runtimeIdentifier, devs] of Object.entries(parsed.devices)) {
      for (const d of devs) {
        out.push({
          udid: d.udid,
          name: d.name,
          state: d.state,
          isAvailable: d.isAvailable,
          runtimeIdentifier,
          deviceTypeIdentifier: d.deviceTypeIdentifier,
        })
      }
    }
    return out
  }

  async boot(udid: string): Promise<void> {
    await this.runText(['simctl', 'boot', udid], 'boot')
  }

  async shutdown(udid: string): Promise<void> {
    await this.runText(['simctl', 'shutdown', udid], 'shutdown')
  }

  async install(udid: string, appPath: string): Promise<void> {
    await this.runText(['simctl', 'install', udid, appPath], 'install')
  }

  async uninstall(udid: string, bundleId: string): Promise<void> {
    await this.runText(['simctl', 'uninstall', udid, bundleId], 'uninstall')
  }

  async launch(
    udid: string,
    bundleId: string,
    args: readonly string[] = [],
    env: Record<string, string> = {},
  ): Promise<{ pid: number }> {
    // simctl reads env vars prefixed with SIMCTL_CHILD_ and propagates the suffix to the launched app
    const childEnv: Record<string, string> = {}
    for (const [k, v] of Object.entries(env)) {
      childEnv[`SIMCTL_CHILD_${k}`] = v
    }
    const launchArgs = ['simctl', 'launch', udid, bundleId, ...args]
    const opts: SpawnOptions | undefined =
      Object.keys(childEnv).length > 0 ? { env: childEnv } : undefined
    const stdout = await this.runText(launchArgs, 'launch', opts)
    const match = stdout.match(/^([^:]+):\s+(\d+)\b/m)
    if (!match || match[2] === undefined) {
      throw new Error(`failed to parse simctl launch output: ${truncate(stdout)}`)
    }
    return { pid: Number(match[2]) }
  }

  async terminate(udid: string, bundleId: string): Promise<void> {
    await this.runText(['simctl', 'terminate', udid, bundleId], 'terminate')
  }

  async openUrl(udid: string, url: string): Promise<void> {
    await this.runText(['simctl', 'openurl', udid, url], 'openurl')
  }

  async setAppearance(udid: string, mode: 'light' | 'dark'): Promise<void> {
    await this.runText(['simctl', 'ui', udid, 'appearance', mode], 'ui appearance')
  }

  async grantPermission(
    udid: string,
    bundleId: string,
    permission: SimctlPermission,
  ): Promise<void> {
    await this.runText(
      ['simctl', 'privacy', udid, 'grant', permission, bundleId],
      'privacy grant',
    )
  }

  async pasteboardGet(udid: string): Promise<string> {
    return this.runText(['simctl', 'pbpaste', udid], 'pbpaste')
  }

  // simctl pbcopy reads the pasteboard text from stdin; see `simctl help pbcopy`
  async pasteboardSet(udid: string, text: string): Promise<void> {
    await this.runText(['simctl', 'pbcopy', udid], 'pbcopy', { stdin: text })
  }

  // `simctl bootstatus <udid> -b` both boots the device (if needed) and blocks
  // until boot completes; no need for a polling loop.
  async bootAndWait(udid: string, timeoutMs: number = 60_000): Promise<void> {
    const work = this.spawn('xcrun', ['simctl', 'bootstatus', udid, '-b'])
    const timeout = new Promise<never>((_resolve, reject) => {
      const t = setTimeout(() => reject(new BootstatusTimeoutMarker(timeoutMs)), timeoutMs)
      t.unref?.()
    })
    let result
    try {
      result = await Promise.race([work, timeout])
    } catch (err) {
      if (err instanceof BootstatusTimeoutMarker) {
        throw new Error(`simctl bootstatus timed out after ${err.timeoutMs}ms`)
      }
      throw err
    }
    if (result.exitCode !== 0) {
      throw new Error(
        `simctl bootstatus failed (exit ${result.exitCode}): ${truncate(result.stderr)}`,
      )
    }
  }

  // On iOS 26 / Xcode 26 `simctl io screenshot -` no longer streams to stdout
  // (it tries to write a file literally named "-"). Use a temp file and read back.
  async screenshot(udid: string): Promise<Buffer> {
    const dir = await mkdtemp(join(tmpdir(), 'simx-shot-'))
    const file = join(dir, 'screenshot.png')
    try {
      const { stderr, exitCode } = await this.spawn(
        'xcrun',
        ['simctl', 'io', udid, 'screenshot', '--type=png', file],
      )
      if (exitCode !== 0) {
        throw new Error(
          `simctl io screenshot failed (exit ${exitCode}): ${truncate(stderr)}`,
        )
      }
      return await readFile(file)
    } finally {
      await rm(dir, { recursive: true, force: true }).catch(() => undefined)
    }
  }

  private async runText(
    args: readonly string[],
    op: string,
    opts?: SpawnOptions,
  ): Promise<string> {
    const { stdout, stderr, exitCode } = await this.spawn('xcrun', args, opts)
    if (exitCode !== 0) {
      throw new Error(
        `simctl ${op} failed (exit ${exitCode}): ${truncate(stderr)}`,
      )
    }
    return stdout
  }

  private async runJson<T>(
    args: readonly string[],
    execOp: string,
    parseOp: string,
  ): Promise<T> {
    const stdout = await this.runText(args, execOp)
    try {
      return JSON.parse(stdout) as T
    } catch {
      throw new Error(
        `failed to parse simctl ${parseOp} JSON: ${truncate(stdout)}`,
      )
    }
  }
}

type RawRuntime = {
  identifier: string
  name: string
  version: string
  isAvailable: boolean
  platform: string
}

type RawDevice = {
  udid: string
  name: string
  state: string
  isAvailable: boolean
  deviceTypeIdentifier: string
}

class BootstatusTimeoutMarker {
  constructor(readonly timeoutMs: number) {}
}

function truncate(s: string): string {
  if (s.length <= MAX_DETAIL_CHARS) return s
  return s.slice(0, MAX_DETAIL_CHARS) + '…'
}

const defaultSpawn: Spawner = (cmd, args, opts) => {
  if (opts?.stdin === undefined) {
    return new Promise((resolve) => {
      execFile(cmd, [...args], { maxBuffer: 64 * 1024 * 1024 }, (err, stdout, stderr) => {
        const exitCode = err && typeof (err as NodeJS.ErrnoException & { code?: number }).code === 'number'
          ? ((err as NodeJS.ErrnoException & { code?: number }).code ?? 1)
          : err
            ? 1
            : 0
        resolve({ stdout: String(stdout), stderr: String(stderr), exitCode })
      })
    })
  }
  return new Promise((resolve) => {
    const child = childSpawn(cmd, [...args])
    let stdout = ''
    let stderr = ''
    child.stdout.on('data', (chunk: Buffer) => {
      stdout += chunk.toString()
    })
    child.stderr.on('data', (chunk: Buffer) => {
      stderr += chunk.toString()
    })
    child.on('close', (code) => {
      resolve({ stdout, stderr, exitCode: code ?? 1 })
    })
    child.stdin.write(opts.stdin ?? '')
    child.stdin.end()
  })
}

const defaultSpawnBinary: BinarySpawner = (cmd, args) =>
  new Promise((resolve) => {
    const child = childSpawn(cmd, [...args])
    const stdoutChunks: Buffer[] = []
    let stderr = ''
    child.stdout.on('data', (chunk: Buffer) => {
      stdoutChunks.push(chunk)
    })
    child.stderr.on('data', (chunk: Buffer) => {
      stderr += chunk.toString()
    })
    child.on('close', (code) => {
      resolve({
        stdout: Buffer.concat(stdoutChunks),
        stderr,
        exitCode: code ?? 1,
      })
    })
  })
