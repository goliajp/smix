import { describe, it, expect, vi } from 'vitest'
import type { SimctlClient, SimctlDevice } from '../../sim/simctl.js'
import { runListCommand } from '../commands/list.js'

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

describe('runListCommand', () => {
  it('happy: prints one line per available device', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({
        udid: 'A',
        state: 'Booted',
        name: 'iPhone 17 Pro',
        runtimeIdentifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-4',
      }),
      makeDevice({
        udid: 'B',
        state: 'Shutdown',
        name: 'iPad',
        runtimeIdentifier: 'com.apple.CoreSimulator.SimRuntime.iOS-18-4',
      }),
    ]
    const client = { listDevices: vi.fn().mockResolvedValueOnce(devices) } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runListCommand({ client, out: bufs.out, err: bufs.err })
    expect(res).toEqual({ exitCode: 0 })
    const lines = bufs.readOut().split('\n').filter(Boolean)
    expect(lines).toHaveLength(2)
    expect(lines[0]).toBe('A  Booted  iPhone 17 Pro  com.apple.CoreSimulator.SimRuntime.iOS-26-4')
    expect(lines[1]).toBe('B  Shutdown  iPad  com.apple.CoreSimulator.SimRuntime.iOS-18-4')
  })

  it('filters out isAvailable=false', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'A', state: 'Booted', name: 'OK', isAvailable: true }),
      makeDevice({ udid: 'B', state: 'Shutdown', name: 'HIDDEN', isAvailable: false }),
    ]
    const client = { listDevices: vi.fn().mockResolvedValueOnce(devices) } as unknown as SimctlClient
    const bufs = makeBufs()
    await runListCommand({ client, out: bufs.out, err: bufs.err })
    expect(bufs.readOut()).not.toContain('HIDDEN')
    expect(bufs.readOut()).toContain('OK')
  })

  it('listDevices error → exitCode 1 + stderr', async () => {
    const client = {
      listDevices: vi.fn().mockRejectedValueOnce(new Error('simctl failed: exit 1')),
    } as unknown as SimctlClient
    const bufs = makeBufs()
    const res = await runListCommand({ client, out: bufs.out, err: bufs.err })
    expect(res).toEqual({ exitCode: 1 })
    expect(bufs.readErr()).toContain('simctl failed')
    expect(bufs.readOut()).toBe('')
  })
})
