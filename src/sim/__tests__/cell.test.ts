import { describe, it, expect, vi } from 'vitest'
import {
  acquireCell,
  releaseCell,
  DEFAULT_RUNNER_PORT,
  DEFAULT_TRACE_DIR,
  DEFAULT_CELL_ID,
} from '../cell.js'
import { SimSession, DeviceNotFoundError } from '../session.js'
import type { SimctlClient, SimctlDevice } from '../simctl.js'

const RUNTIME = 'com.apple.CoreSimulator.SimRuntime.iOS-26-4'

function makeDevice(overrides: Partial<SimctlDevice> = {}): SimctlDevice {
  return {
    udid: 'X',
    name: 'iPhone 16 Pro',
    state: 'Booted',
    isAvailable: true,
    runtimeIdentifier: RUNTIME,
    deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro',
    ...overrides,
  }
}

function makeFakeClient(devices: SimctlDevice[]): SimctlClient {
  return {
    listDevices: vi.fn().mockResolvedValue(devices),
    bootAndWait: vi.fn().mockResolvedValue(undefined),
    boot: vi.fn().mockResolvedValue(undefined),
    shutdown: vi.fn().mockResolvedValue(undefined),
  } as unknown as SimctlClient
}

describe('acquireCell', () => {
  it('defaults: udid passthrough, runnerPort=22087, traceDir=.simx/trace, id=cell-0001', async () => {
    const client = makeFakeClient([makeDevice({ udid: 'X' })])
    const cell = await acquireCell({ udid: 'X' }, { client })
    expect(cell.id).toBe(DEFAULT_CELL_ID)
    expect(cell.udid).toBe('X')
    expect(cell.runnerPort).toBe(DEFAULT_RUNNER_PORT)
    expect(cell.traceDir).toBe(DEFAULT_TRACE_DIR)
    expect(DEFAULT_RUNNER_PORT).toBe(22087)
    expect(DEFAULT_TRACE_DIR).toBe('.simx/trace')
    expect(DEFAULT_CELL_ID).toBe('cell-0001')
  })

  it('errors propagate from acquireSession (DEVICE_NOT_FOUND)', async () => {
    const client = makeFakeClient([])
    await expect(acquireCell({ udid: 'X' }, { client })).rejects.toBeInstanceOf(
      DeviceNotFoundError,
    )
  })

  it('with custom runnerPort', async () => {
    const client = makeFakeClient([makeDevice({ udid: 'X' })])
    const cell = await acquireCell(
      { udid: 'X', runnerPort: 33000 },
      { client },
    )
    expect(cell.runnerPort).toBe(33000)
  })

  it('cell.session is SimSession instance with same udid', async () => {
    const client = makeFakeClient([makeDevice({ udid: 'X' })])
    const cell = await acquireCell({ udid: 'X' }, { client })
    expect(cell.session).toBeInstanceOf(SimSession)
    expect(cell.session.udid).toBe('X')
  })

  it('releaseCell delegates to session.release()', async () => {
    const client = makeFakeClient([makeDevice({ udid: 'X' })])
    const cell = await acquireCell({ udid: 'X' }, { client })
    const spy = vi.spyOn(cell.session, 'release').mockResolvedValue(undefined)
    await releaseCell(cell)
    expect(spy).toHaveBeenCalledTimes(1)
  })
})
