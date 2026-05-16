import { describe, it, expect, vi } from 'vitest'
import type { SimctlClient, SimctlDevice } from '../../sim/simctl.js'
import { runBootCommand, type BootJsonOutput } from '../commands/boot.js'

function makeDevice(o: Partial<SimctlDevice>): SimctlDevice {
  return {
    udid: 'X',
    name: 'X',
    state: 'Shutdown',
    isAvailable: true,
    runtimeIdentifier: 'r',
    deviceTypeIdentifier: 'd',
    ...o,
  }
}

function makeBufs() {
  let out = ''
  let err = ''
  return {
    out: { write: (s: string) => { out += s } },
    err: { write: (s: string) => { err += s } },
    readOut: () => out,
    readErr: () => err,
  }
}

const SAMPLE_UDID = 'A1B2C3D4-5566-4778-99AA-BBCCDDEEFF00'

describe('runBootCommand', () => {
  it('happy: UDID exact match, already booted → noop + alreadyBooted: true, exit 0', async () => {
    const device = makeDevice({ udid: SAMPLE_UDID, name: 'iPhone 17 Pro', state: 'Booted' })
    const bootAndWait = vi.fn().mockResolvedValue(undefined)
    const client = {
      listDevices: vi.fn().mockResolvedValueOnce([device]),
      bootAndWait,
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runBootCommand({
      client,
      device: SAMPLE_UDID,
      out: bufs.out,
      err: bufs.err,
    })
    expect(res).toEqual({ exitCode: 0 })
    expect(bootAndWait).not.toHaveBeenCalled()
    expect(bufs.readOut()).toContain('already booted')
    expect(bufs.readErr()).toBe('')
  })

  it('happy: UDID exact match, not booted → calls bootAndWait + alreadyBooted: false, exit 0', async () => {
    const device = makeDevice({ udid: SAMPLE_UDID, name: 'iPhone 17 Pro', state: 'Shutdown' })
    const bootAndWait = vi.fn().mockResolvedValue(undefined)
    const client = {
      listDevices: vi.fn().mockResolvedValueOnce([device]),
      bootAndWait,
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runBootCommand({
      client,
      device: SAMPLE_UDID,
      json: true,
      out: bufs.out,
      err: bufs.err,
    })
    expect(res).toEqual({ exitCode: 0 })
    expect(bootAndWait).toHaveBeenCalledTimes(1)
    expect(bootAndWait.mock.calls[0]?.[0]).toBe(SAMPLE_UDID)
    const payload = JSON.parse(bufs.readOut()) as BootJsonOutput
    expect(payload.alreadyBooted).toBe(false)
    expect(payload.durationMs).toBeGreaterThanOrEqual(0)
    expect(payload.state).toBe('Booted')
    expect(bufs.readErr()).toBe('')
  })

  it('happy: name exact match → resolves to udid, calls bootAndWait, exit 0', async () => {
    const device = makeDevice({ udid: SAMPLE_UDID, name: 'iPhone 17 Pro', state: 'Shutdown' })
    const bootAndWait = vi.fn().mockResolvedValue(undefined)
    const client = {
      listDevices: vi.fn().mockResolvedValueOnce([device]),
      bootAndWait,
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runBootCommand({
      client,
      device: 'iPhone 17 Pro',
      json: true,
      out: bufs.out,
      err: bufs.err,
    })
    expect(res).toEqual({ exitCode: 0 })
    expect(bootAndWait).toHaveBeenCalledTimes(1)
    expect(bootAndWait.mock.calls[0]?.[0]).toBe(SAMPLE_UDID)
    const payload = JSON.parse(bufs.readOut()) as BootJsonOutput
    expect(payload.udid).toBe(SAMPLE_UDID)
    expect(payload.name).toBe('iPhone 17 Pro')
  })

  it('error: device not found by UDID (合法 UUID regex 走 UDID 路径) → exit 1', async () => {
    const client = {
      listDevices: vi.fn().mockResolvedValueOnce([]),
      bootAndWait: vi.fn(),
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runBootCommand({
      client,
      device: 'FFFFFFFF-0000-0000-0000-000000000000',
      out: bufs.out,
      err: bufs.err,
    })
    expect(res).toEqual({ exitCode: 1 })
    expect(bufs.readErr()).toContain('device not found')
    expect(bufs.readErr()).toContain("udid: 'FFFFFFFF-0000-0000-0000-000000000000'")
    expect(bufs.readOut()).toBe('')
  })

  it('error: device not found by name (非 UUID regex 走 name 路径) → exit 1', async () => {
    const device = makeDevice({ udid: SAMPLE_UDID, name: 'iPhone 17 Pro', state: 'Booted' })
    const client = {
      listDevices: vi.fn().mockResolvedValueOnce([device]),
      bootAndWait: vi.fn(),
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runBootCommand({
      client,
      device: 'NonexistentDevice',
      out: bufs.out,
      err: bufs.err,
    })
    expect(res).toEqual({ exitCode: 1 })
    expect(bufs.readErr()).toContain('device not found')
    expect(bufs.readErr()).toContain("name: 'NonexistentDevice'")
  })

  it('error: name ambiguous (2+ matches) → exit 1, lists candidates', async () => {
    const devices = [
      makeDevice({ udid: 'A1B2C3D4-0000-4000-8000-000000000001', name: 'iPhone 17 Pro', state: 'Shutdown' }),
      makeDevice({ udid: 'A1B2C3D4-0000-4000-8000-000000000002', name: 'iPhone 17 Pro', state: 'Booted' }),
    ]
    const client = {
      listDevices: vi.fn().mockResolvedValueOnce(devices),
      bootAndWait: vi.fn(),
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runBootCommand({
      client,
      device: 'iPhone 17 Pro',
      out: bufs.out,
      err: bufs.err,
    })
    expect(res).toEqual({ exitCode: 1 })
    expect(bufs.readErr()).toContain('multiple devices match')
    expect(bufs.readErr()).toContain('A1B2C3D4-0000-4000-8000-000000000001')
    expect(bufs.readErr()).toContain('A1B2C3D4-0000-4000-8000-000000000002')
  })

  it('--json: outputs single-line JSON with {udid, name, state, alreadyBooted, durationMs} schema, stderr empty', async () => {
    const device = makeDevice({ udid: SAMPLE_UDID, name: 'iPhone 17 Pro', state: 'Booted' })
    const client = {
      listDevices: vi.fn().mockResolvedValueOnce([device]),
      bootAndWait: vi.fn(),
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runBootCommand({
      client,
      device: SAMPLE_UDID,
      json: true,
      out: bufs.out,
      err: bufs.err,
    })
    expect(res).toEqual({ exitCode: 0 })
    const raw = bufs.readOut()
    // single line: one '\n' at the end, no embedded newlines
    expect(raw.endsWith('\n')).toBe(true)
    expect(raw.slice(0, -1)).not.toContain('\n')
    const payload = JSON.parse(raw) as BootJsonOutput
    expect(typeof payload.udid).toBe('string')
    expect(typeof payload.name).toBe('string')
    expect(typeof payload.state).toBe('string')
    expect(typeof payload.alreadyBooted).toBe('boolean')
    expect(typeof payload.durationMs).toBe('number')
    expect(payload.udid).toBe(SAMPLE_UDID)
    expect(payload.state).toBe('Booted')
    expect(payload.alreadyBooted).toBe(true)
    expect(payload.durationMs).toBeGreaterThanOrEqual(0)
    expect(bufs.readErr()).toBe('')
  })

  it('error: bootAndWait throws timeout → exit 1 with timeout error message', async () => {
    const device = makeDevice({ udid: SAMPLE_UDID, name: 'iPhone 17 Pro', state: 'Shutdown' })
    const bootAndWait = vi
      .fn()
      .mockRejectedValueOnce(new Error('simctl bootstatus timed out after 120000ms'))
    const client = {
      listDevices: vi.fn().mockResolvedValueOnce([device]),
      bootAndWait,
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runBootCommand({
      client,
      device: SAMPLE_UDID,
      out: bufs.out,
      err: bufs.err,
    })
    expect(res).toEqual({ exitCode: 1 })
    expect(bufs.readErr()).toContain('boot failed')
    expect(bufs.readErr()).toContain('timed out')
  })

  it('--timeout 30000 透传到 bootAndWait second arg', async () => {
    const device = makeDevice({ udid: SAMPLE_UDID, name: 'iPhone 17 Pro', state: 'Shutdown' })
    const bootAndWait = vi.fn().mockResolvedValue(undefined)
    const client = {
      listDevices: vi.fn().mockResolvedValueOnce([device]),
      bootAndWait,
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runBootCommand({
      client,
      device: SAMPLE_UDID,
      timeoutMs: 30_000,
      out: bufs.out,
      err: bufs.err,
    })
    expect(res).toEqual({ exitCode: 0 })
    expect(bootAndWait).toHaveBeenCalledTimes(1)
    expect(bootAndWait.mock.calls[0]?.[1]).toBe(30_000)
  })

  it('non-json mode human output format (both booted-now and already-booted)', async () => {
    // Path 1: Shutdown → boot → "(booted in <ms>ms)"
    {
      const device = makeDevice({ udid: SAMPLE_UDID, name: 'iPhone 17 Pro', state: 'Shutdown' })
      const bootAndWait = vi.fn().mockResolvedValue(undefined)
      const client = {
        listDevices: vi.fn().mockResolvedValueOnce([device]),
        bootAndWait,
      } as unknown as SimctlClient
      const bufs = makeBufs()
      const res = await runBootCommand({
        client,
        device: SAMPLE_UDID,
        out: bufs.out,
        err: bufs.err,
      })
      expect(res).toEqual({ exitCode: 0 })
      const line = bufs.readOut().trim()
      expect(line).toMatch(
        new RegExp(`^${SAMPLE_UDID}  Booted  iPhone 17 Pro  \\(booted in \\d+ms\\)$`),
      )
    }
    // Path 2: already Booted → "(already booted)"
    {
      const device = makeDevice({ udid: SAMPLE_UDID, name: 'iPhone 17 Pro', state: 'Booted' })
      const bootAndWait = vi.fn()
      const client = {
        listDevices: vi.fn().mockResolvedValueOnce([device]),
        bootAndWait,
      } as unknown as SimctlClient
      const bufs = makeBufs()
      const res = await runBootCommand({
        client,
        device: SAMPLE_UDID,
        out: bufs.out,
        err: bufs.err,
      })
      expect(res).toEqual({ exitCode: 0 })
      expect(bufs.readOut().trim()).toBe(
        `${SAMPLE_UDID}  Booted  iPhone 17 Pro  (already booted)`,
      )
    }
  })
})
