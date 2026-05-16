import type { Driver } from '../driver/types.js'
import { SimctlDriver } from '../driver/simctl-driver.js'
import { acquireCell } from '../sim/cell.js'
import type { SimctlClient } from '../sim/simctl.js'

export type AcquireDriver = (udid: string) => Promise<Driver>

// Per-process udid → Driver factory backed by acquireCell + SimctlDriver.
// Caches Driver instances by udid so repeated tool calls reuse the same runner
// binding. The cache never evicts in v0.6 (MCP server lifetime is bound to the
// Claude Code session); eviction / multi-cell pooling is a v1.1 concern.
export function defaultAcquireDriver(client: SimctlClient): AcquireDriver {
  const cache = new Map<string, Promise<Driver>>()
  return (udid: string): Promise<Driver> => {
    const cached = cache.get(udid)
    if (cached !== undefined) return cached
    const p = acquireCell({ udid }, { client }).then(
      (cell) => new SimctlDriver(cell) as Driver,
    )
    cache.set(udid, p)
    return p
  }
}
