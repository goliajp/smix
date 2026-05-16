import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  readDevSimLock,
  findDevSimLockFile,
  assertDevSimLock,
  DevSimLockViolation,
} from '../dev-lock.js'

const LOCKED_UDID = 'AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA'
const OTHER_UDID = 'BBBBBBBB-BBBB-4BBB-8BBB-BBBBBBBBBBBB'

let tmpRoot: string
let origEnv: string | undefined

beforeEach(() => {
  tmpRoot = mkdtempSync(join(tmpdir(), 'simx-dev-lock-'))
  origEnv = process.env.SIMX_DEV_LOCK
  // test-setup.ts sets SIMX_DEV_LOCK=off globally to keep legacy mocks
  // happy; this suite needs it unset to validate the assertion path.
  delete process.env.SIMX_DEV_LOCK
})

afterEach(() => {
  rmSync(tmpRoot, { recursive: true, force: true })
  if (origEnv === undefined) delete process.env.SIMX_DEV_LOCK
  else process.env.SIMX_DEV_LOCK = origEnv
})

function setupLock(udid: string): string {
  const dir = join(tmpRoot, '.simx')
  mkdirSync(dir, { recursive: true })
  writeFileSync(join(dir, 'dev-sim.txt'), `${udid}\n`)
  return tmpRoot
}

describe('dev-lock', () => {
  it('readDevSimLock returns trimmed udid when file exists', () => {
    const cwd = setupLock(LOCKED_UDID)
    expect(readDevSimLock(cwd)).toBe(LOCKED_UDID)
  })

  it('readDevSimLock returns null when no lock file', () => {
    expect(readDevSimLock(tmpRoot)).toBe(null)
  })

  it('findDevSimLockFile walks up to parent dirs', () => {
    setupLock(LOCKED_UDID)
    const nested = join(tmpRoot, 'a', 'b', 'c')
    mkdirSync(nested, { recursive: true })
    expect(findDevSimLockFile(nested)).toBe(join(tmpRoot, '.simx/dev-sim.txt'))
  })

  it('assertDevSimLock allows matching udid', () => {
    const cwd = setupLock(LOCKED_UDID)
    expect(() => assertDevSimLock(LOCKED_UDID, cwd)).not.toThrow()
  })

  it('assertDevSimLock throws DevSimLockViolation on mismatch', () => {
    const cwd = setupLock(LOCKED_UDID)
    expect(() => assertDevSimLock(OTHER_UDID, cwd)).toThrow(DevSimLockViolation)
    try {
      assertDevSimLock(OTHER_UDID, cwd)
    } catch (e) {
      expect(e).toBeInstanceOf(DevSimLockViolation)
      const v = e as DevSimLockViolation
      expect(v.attemptedUdid).toBe(OTHER_UDID)
      expect(v.lockedUdid).toBe(LOCKED_UDID)
    }
  })

  it('assertDevSimLock noop when no lock file (non-dev env)', () => {
    expect(() => assertDevSimLock(OTHER_UDID, tmpRoot)).not.toThrow()
  })

  it('assertDevSimLock respects SIMX_DEV_LOCK=off override', () => {
    const cwd = setupLock(LOCKED_UDID)
    process.env.SIMX_DEV_LOCK = 'off'
    expect(() => assertDevSimLock(OTHER_UDID, cwd)).not.toThrow()
  })
})
