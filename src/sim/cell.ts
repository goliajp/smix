import type { SimctlClient } from './simctl.js'
import { acquireSession, type AcquireOptions, type SimSession } from './session.js'

/**
 * v1 runtime container: 1 booted simulator + 1 runner port + 1 trace dir.
 * v0.4-v0.6 will retrofit traceDir to `.simx/cells/<id>/`; v0.6+ adds
 * runner port allocator. C6 keeps single-cell defaults.
 */
export type Cell = {
  readonly id: string
  readonly udid: string
  readonly runnerPort: number
  readonly traceDir: string
  readonly session: SimSession
}

export type AcquireCellOptions = AcquireOptions & {
  runnerPort?: number
  traceDir?: string
  id?: string
}

export type AcquireCellDeps = {
  client: SimctlClient
}

export const DEFAULT_RUNNER_PORT = 22087
export const DEFAULT_TRACE_DIR = '.simx/trace'
export const DEFAULT_CELL_ID = 'cell-0001'

export async function acquireCell(
  opts: AcquireCellOptions,
  deps: AcquireCellDeps,
): Promise<Cell> {
  const { runnerPort, traceDir, id, ...sessionOpts } = opts
  const session = await acquireSession(sessionOpts, { client: deps.client })
  return {
    id: id ?? DEFAULT_CELL_ID,
    udid: session.udid,
    runnerPort: runnerPort ?? DEFAULT_RUNNER_PORT,
    traceDir: traceDir ?? DEFAULT_TRACE_DIR,
    session,
  }
}

export async function releaseCell(
  cell: Cell,
  opts: { shutdown?: boolean } = {},
): Promise<void> {
  await cell.session.release(opts)
}
