import type { SimctlClient, SimctlDevice } from './simctl.js'
import { assertDevSimLock } from './dev-lock.js'

export type AcquireOptions = {
  udid?: string
  deviceName?: string
  runtimeIdentifier?: string
  autoBoot?: boolean
  bootTimeoutMs?: number
}

export type SessionDeps = {
  client: SimctlClient
}

export type SessionErrorCode =
  | 'DEVICE_NOT_FOUND'
  | 'DEVICE_AMBIGUOUS'
  | 'BOOT_TIMEOUT'
  | 'BOOT_FAILED'

export class SessionError extends Error {
  readonly code: SessionErrorCode
  readonly details: Readonly<Record<string, unknown>>

  constructor(
    code: SessionErrorCode,
    message: string,
    details: Record<string, unknown>,
  ) {
    super(message)
    this.name = 'SessionError'
    this.code = code
    this.details = Object.freeze({ ...details })
    // Preserve instanceof across the subclass chain (TS class extending Error quirk).
    Object.setPrototypeOf(this, new.target.prototype)
  }
}

export class DeviceNotFoundError extends SessionError {
  constructor(message: string, details: Record<string, unknown>) {
    super('DEVICE_NOT_FOUND', message, details)
    this.name = 'DeviceNotFoundError'
  }
}

export class DeviceAmbiguousError extends SessionError {
  constructor(message: string, details: Record<string, unknown>) {
    super('DEVICE_AMBIGUOUS', message, details)
    this.name = 'DeviceAmbiguousError'
  }
}

export class BootTimeoutError extends SessionError {
  constructor(message: string, details: Record<string, unknown>) {
    super('BOOT_TIMEOUT', message, details)
    this.name = 'BootTimeoutError'
  }
}

export class BootFailedError extends SessionError {
  constructor(message: string, details: Record<string, unknown>) {
    super('BOOT_FAILED', message, details)
    this.name = 'BootFailedError'
  }
}

export class SimSession {
  readonly udid: string
  readonly device: SimctlDevice
  readonly client: SimctlClient

  constructor(client: SimctlClient, device: SimctlDevice) {
    this.client = client
    this.device = device
    this.udid = device.udid
  }

  // Default: don't shutdown — sims are commonly shared with the Xcode UI.
  async release(opts: { shutdown?: boolean } = {}): Promise<void> {
    if (opts.shutdown === true) {
      await this.client.shutdown(this.udid)
    }
  }

  async isAlive(): Promise<boolean> {
    const list = await this.client.listDevices()
    return list.find((d) => d.udid === this.udid)?.state === 'Booted'
  }
}

export async function acquireSession(
  opts: AcquireOptions,
  deps: SessionDeps,
): Promise<SimSession> {
  const { client } = deps
  const all = await client.listDevices()
  const candidates = filterDevices(all, opts)

  if (candidates.length === 0) {
    throw new DeviceNotFoundError(
      `device not found: ${describeQuery(opts)}`,
      { query: opts },
    )
  }
  if (candidates.length > 1 && opts.udid === undefined) {
    throw new DeviceAmbiguousError(
      `multiple devices match: ${candidates.length} candidates; pass udid to disambiguate`,
      {
        candidates: candidates.map((d) => ({
          udid: d.udid,
          name: d.name,
          runtimeIdentifier: d.runtimeIdentifier,
          state: d.state,
        })),
      },
    )
  }

  const device = candidates[0]!

  // Enforce dev sim lock — refuse to acquire any sim other than the
  // one declared in .simx/dev-sim.txt unless SIMX_DEV_LOCK=off is set.
  // Prevents accidentally grabbing the user's own simulators during
  // SDK/CLI/MCP-driven flows.
  assertDevSimLock(device.udid)

  if (device.state === 'Booted') {
    return new SimSession(client, device)
  }

  if (opts.autoBoot === false) {
    throw new BootFailedError(
      `device ${device.udid} not booted and autoBoot=false`,
      { udid: device.udid },
    )
  }

  const timeoutMs = opts.bootTimeoutMs ?? 60_000
  try {
    await client.bootAndWait(device.udid, timeoutMs)
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    if (/timed out after/.test(message)) {
      throw new BootTimeoutError(
        `bootstatus timed out after ${timeoutMs}ms for udid ${device.udid}`,
        { udid: device.udid, timeoutMs },
      )
    }
    throw new BootFailedError(
      `boot failed for udid ${device.udid}: ${message}`,
      { udid: device.udid, cause: message },
    )
  }
  return new SimSession(client, device)
}

function filterDevices(
  all: readonly SimctlDevice[],
  opts: AcquireOptions,
): SimctlDevice[] {
  if (opts.udid !== undefined) {
    return all.filter((d) => d.udid === opts.udid && d.isAvailable)
  }
  return all.filter((d) => {
    if (!d.isAvailable) return false
    if (opts.deviceName !== undefined && d.name !== opts.deviceName) return false
    if (opts.runtimeIdentifier !== undefined && d.runtimeIdentifier !== opts.runtimeIdentifier) return false
    return true
  })
}

function describeQuery(opts: AcquireOptions): string {
  const parts: string[] = []
  if (opts.udid !== undefined) parts.push(`udid: '${opts.udid}'`)
  if (opts.deviceName !== undefined) parts.push(`deviceName: '${opts.deviceName}'`)
  if (opts.runtimeIdentifier !== undefined) parts.push(`runtimeIdentifier: '${opts.runtimeIdentifier}'`)
  return parts.length === 0 ? 'no filter' : `{ ${parts.join(', ')} }`
}
