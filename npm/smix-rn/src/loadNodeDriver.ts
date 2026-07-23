// The production leg of the NodeDriver seam: load the real napi addon
// (`@goliapkg/smix-node`) and construct a driver aimed at a runner port. The
// import is dynamic and lazy so a platform without a prebuilt `.node` does not
// fail at install/import time — it fails clearly here, only when a caller
// actually reaches for live driving. The addon is an optionalDependency, so
// this is the single place its absence surfaces.

import type { NodeDriver } from './NodeDriver.js'

/** The runner port to drive, from `SMIX_RUNNER_PORT` or the CLI default. */
export function defaultRunnerPort(): number {
  return Number(process.env.SMIX_RUNNER_PORT) || 22087
}

/**
 * Load the real `SmixNodeDriver` and aim it at `port` (defaulting to the
 * runner port). The returned instance satisfies `NodeDriver` structurally.
 */
export async function loadNodeDriver(port?: number): Promise<NodeDriver> {
  const { SmixNodeDriver } = await import('@goliapkg/smix-node')
  return new SmixNodeDriver(port ?? defaultRunnerPort()) as unknown as NodeDriver
}
