import { describe, it, expect, vi, afterEach } from 'vitest'
import { SimctlClient, type Spawner } from '../simctl.js'
import {
  acquireSession,
  SimSession,
  DeviceNotFoundError,
  DeviceAmbiguousError,
  BootTimeoutError,
} from '../session.js'

const RUNTIME = 'com.apple.CoreSimulator.SimRuntime.iOS-26-4'

function listDevicesStdout(
  devices: Record<
    string,
    Array<{
      udid: string
      name: string
      state: string
      isAvailable: boolean
      deviceTypeIdentifier: string
    }>
  >,
): string {
  return JSON.stringify({ devices })
}

afterEach(() => {
  vi.useRealTimers()
})

describe('acquireSession', () => {
  it('happy: udid match Booted, no boot triggered', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'list', 'devices', '-j'])
      return {
        stdout: listDevicesStdout({
          [RUNTIME]: [
            {
              udid: 'AAAA-1111',
              name: 'iPhone 16 Pro',
              state: 'Booted',
              isAvailable: true,
              deviceTypeIdentifier: 'X',
            },
          ],
        }),
        stderr: '',
        exitCode: 0,
      }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    const session = await acquireSession({ udid: 'AAAA-1111' }, { client })
    expect(session).toBeInstanceOf(SimSession)
    expect(session.udid).toBe('AAAA-1111')
    expect(session.device.state).toBe('Booted')
    expect(fakeSpawn).toHaveBeenCalledTimes(1)
  })

  it('happy: deviceName+runtime selects + auto-boots', async () => {
    const calls: string[][] = []
    const fakeSpawn: Spawner = vi.fn(async (_cmd, args) => {
      calls.push([...args])
      if (args[1] === 'list') {
        return {
          stdout: listDevicesStdout({
            [RUNTIME]: [
              {
                udid: 'BB',
                name: 'iPhone 16 Pro',
                state: 'Shutdown',
                isAvailable: true,
                deviceTypeIdentifier: 'X',
              },
            ],
          }),
          stderr: '',
          exitCode: 0,
        }
      }
      return { stdout: '', stderr: '', exitCode: 0 }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    const session = await acquireSession(
      { deviceName: 'iPhone 16 Pro', runtimeIdentifier: RUNTIME },
      { client },
    )
    expect(session.udid).toBe('BB')
    expect(session.device.state).toBe('Shutdown')
    expect(calls).toEqual([
      ['simctl', 'list', 'devices', '-j'],
      ['simctl', 'bootstatus', 'BB', '-b'],
    ])
  })

  it('error: device not found', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: listDevicesStdout({}),
      stderr: '',
      exitCode: 0,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    let caught: unknown
    try {
      await acquireSession({ udid: 'NOPE' }, { client })
    } catch (err) {
      caught = err
    }
    expect(caught).toBeInstanceOf(DeviceNotFoundError)
    const e = caught as DeviceNotFoundError
    expect(e.code).toBe('DEVICE_NOT_FOUND')
    expect(e.message).toContain('NOPE')
  })

  it('error: ambiguous when multiple match and no udid', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: listDevicesStdout({
        [RUNTIME]: [
          {
            udid: 'AA',
            name: 'iPhone 16 Pro',
            state: 'Shutdown',
            isAvailable: true,
            deviceTypeIdentifier: 'X',
          },
          {
            udid: 'BB',
            name: 'iPhone 16 Pro',
            state: 'Shutdown',
            isAvailable: true,
            deviceTypeIdentifier: 'X',
          },
        ],
      }),
      stderr: '',
      exitCode: 0,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    let caught: unknown
    try {
      await acquireSession({ deviceName: 'iPhone 16 Pro' }, { client })
    } catch (err) {
      caught = err
    }
    expect(caught).toBeInstanceOf(DeviceAmbiguousError)
    const e = caught as DeviceAmbiguousError
    expect(e.code).toBe('DEVICE_AMBIGUOUS')
    const candidates = (e.details as { candidates: Array<{ udid: string }> }).candidates
    expect(candidates.map((c) => c.udid)).toEqual(['AA', 'BB'])
  })

  it('error: boot timeout', async () => {
    vi.useFakeTimers()
    const fakeSpawn: Spawner = vi.fn(async (_cmd, args) => {
      if (args[1] === 'list') {
        return {
          stdout: listDevicesStdout({
            [RUNTIME]: [
              {
                udid: 'AAAA-1111',
                name: 'iPhone 16 Pro',
                state: 'Shutdown',
                isAvailable: true,
                deviceTypeIdentifier: 'X',
              },
            ],
          }),
          stderr: '',
          exitCode: 0,
        }
      }
      return new Promise<{ stdout: string; stderr: string; exitCode: number }>(() => {})
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    const p = acquireSession({ udid: 'AAAA-1111', bootTimeoutMs: 30 }, { client })
    const settled = expect(p).rejects.toBeInstanceOf(BootTimeoutError)
    await vi.advanceTimersByTimeAsync(30)
    await settled
  })
})
