import { readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'

const LOCK_FILE_RELATIVE = '.simx/dev-sim.txt'

export function findDevSimLockFile(cwd: string = process.cwd()): string | null {
  let dir = cwd
  // Walk up to root looking for .simx/dev-sim.txt. Stops at filesystem root.
  while (true) {
    const candidate = join(dir, LOCK_FILE_RELATIVE)
    if (existsSync(candidate)) return candidate
    const parent = join(dir, '..')
    if (parent === dir) return null
    dir = parent
  }
}

export function readDevSimLock(cwd: string = process.cwd()): string | null {
  const file = findDevSimLockFile(cwd)
  if (file === null) return null
  try {
    const value = readFileSync(file, 'utf8').trim()
    return value || null
  } catch {
    return null
  }
}

export class DevSimLockViolation extends Error {
  readonly attemptedUdid: string
  readonly lockedUdid: string
  constructor(attemptedUdid: string, lockedUdid: string) {
    super(
      `dev sim lock violation: attempted to acquire ${attemptedUdid} but .simx/dev-sim.txt locks to ${lockedUdid}. ` +
        `Set SIMX_DEV_LOCK=off to override (e.g. when intentionally operating another sim).`,
    )
    this.name = 'DevSimLockViolation'
    this.attemptedUdid = attemptedUdid
    this.lockedUdid = lockedUdid
  }
}

export function assertDevSimLock(udid: string, cwd: string = process.cwd()): void {
  if (process.env.SIMX_DEV_LOCK === 'off') return
  const locked = readDevSimLock(cwd)
  if (locked === null) return
  if (udid !== locked) throw new DevSimLockViolation(udid, locked)
}
