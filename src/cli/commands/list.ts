import type { SimctlClient } from '../../sim/simctl.js'

export type WritableLike = { write: (chunk: string) => void }

export type CommandResult = { exitCode: number }

export type RunListDeps = {
  client: Pick<SimctlClient, 'listDevices'>
  out: WritableLike
  err: WritableLike
}

export async function runListCommand(deps: RunListDeps): Promise<CommandResult> {
  try {
    const devices = await deps.client.listDevices()
    for (const d of devices) {
      if (!d.isAvailable) continue
      deps.out.write(`${d.udid}  ${d.state}  ${d.name}  ${d.runtimeIdentifier}\n`)
    }
    return { exitCode: 0 }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    deps.err.write(`simx list failed: ${msg}\n`)
    return { exitCode: 1 }
  }
}
