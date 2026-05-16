import type { SimctlClient, SimctlDevice } from '../../sim/simctl.js'
import type { CommandResult, WritableLike } from './list.js'

export type BootCommandDeps = {
  client: Pick<SimctlClient, 'listDevices' | 'bootAndWait'>
  device: string
  json?: boolean
  timeoutMs?: number
  out: WritableLike
  err: WritableLike
}

export type BootJsonOutput = {
  udid: string
  name: string
  state: string
  alreadyBooted: boolean
  durationMs: number
}

// Strict UUID v4 form (8-4-4-4-12 hex) — avoids misclassifying device names
// that contain '-' (e.g. 'iPad Pro 11-inch (M4)') as UDIDs.
const UUID_RE = /^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$/

const DEFAULT_TIMEOUT_MS = 120_000

export async function runBootCommand(deps: BootCommandDeps): Promise<CommandResult> {
  try {
    const all = await deps.client.listDevices()
    const candidates = resolveDevice(all, deps.device)
    if (candidates.length === 0) {
      deps.err.write(`device not found: ${describeQuery(deps.device)}\n`)
      return { exitCode: 1 }
    }
    if (candidates.length > 1) {
      deps.err.write(
        `multiple devices match name '${deps.device}': ${candidates.length} candidates\n`,
      )
      for (const d of candidates) {
        deps.err.write(`  ${d.udid}  ${d.state}  ${d.name}\n`)
      }
      return { exitCode: 1 }
    }
    const dev = candidates[0]!
    const timeoutMs = deps.timeoutMs ?? DEFAULT_TIMEOUT_MS
    const alreadyBooted = dev.state === 'Booted'
    const start = Date.now()
    if (!alreadyBooted) {
      try {
        await deps.client.bootAndWait(dev.udid, timeoutMs)
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        deps.err.write(`boot failed for ${dev.udid}: ${msg}\n`)
        return { exitCode: 1 }
      }
    }
    const durationMs = alreadyBooted ? 0 : Date.now() - start
    const finalState = alreadyBooted ? dev.state : 'Booted'
    if (deps.json === true) {
      const payload: BootJsonOutput = {
        udid: dev.udid,
        name: dev.name,
        state: finalState,
        alreadyBooted,
        durationMs,
      }
      deps.out.write(JSON.stringify(payload) + '\n')
    } else {
      const tail = alreadyBooted ? '(already booted)' : `(booted in ${durationMs}ms)`
      deps.out.write(`${dev.udid}  ${finalState}  ${dev.name}  ${tail}\n`)
    }
    return { exitCode: 0 }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    deps.err.write(`simx boot failed: ${msg}\n`)
    return { exitCode: 1 }
  }
}

function resolveDevice(
  all: readonly SimctlDevice[],
  query: string,
): SimctlDevice[] {
  if (UUID_RE.test(query)) {
    return all.filter((d) => d.udid === query && d.isAvailable)
  }
  return all.filter((d) => d.name === query && d.isAvailable)
}

function describeQuery(query: string): string {
  return UUID_RE.test(query) ? `udid: '${query}'` : `name: '${query}'`
}
