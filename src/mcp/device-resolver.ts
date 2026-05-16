import type { SimctlDevice } from '../sim/simctl.js'

// Strict UUID v4 form (8-4-4-4-12 hex) — avoids misclassifying device names
// that contain '-' (e.g. 'iPad Pro 11-inch (M4)') as UDIDs.
export const UUID_RE = /^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$/

export function resolveDevice(
  all: readonly SimctlDevice[],
  query: string,
): SimctlDevice[] {
  if (UUID_RE.test(query)) {
    return all.filter((d) => d.udid === query && d.isAvailable)
  }
  return all.filter((d) => d.name === query && d.isAvailable)
}

export function describeQuery(query: string): string {
  return UUID_RE.test(query) ? `udid: '${query}'` : `name: '${query}'`
}
