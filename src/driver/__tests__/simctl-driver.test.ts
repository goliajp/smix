import { describe, it, expect, vi } from 'vitest'
import type { Selector, A11yNode } from '../../core/index.js'
import { ExpectationFailure } from '../../core/index.js'
import { SimctlClient, type Spawner, type BinarySpawner, type SimctlDevice } from '../../sim/simctl.js'
import { SimSession } from '../../sim/session.js'
import { SimctlDriver } from '../simctl-driver.js'
import {
  RunnerClient,
  TapNotFoundError,
  RunnerTransportError,
} from '../../sim/runner-client.js'
import type { Cell } from '../../sim/cell.js'

const FAKE_UDID = 'UDID-TEST-0001'
const OK = { stdout: '', stderr: '', exitCode: 0 } as const
const ANY_SEL: Selector = { text: 'x' }

function makeDevice(overrides: Partial<SimctlDevice> = {}): SimctlDevice {
  return {
    udid: FAKE_UDID,
    name: 'iPhone 16 Pro',
    state: 'Booted',
    isAvailable: true,
    runtimeIdentifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-4',
    deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro',
    ...overrides,
  }
}

function makeDriver() {
  const spawn = vi.fn()
  const spawnBinary = vi.fn()
  const client = new SimctlClient({
    spawn: spawn as unknown as Spawner,
    spawnBinary: spawnBinary as unknown as BinarySpawner,
  })
  const session = new SimSession(client, makeDevice())
  return {
    driver: new SimctlDriver(session),
    spawn,
    spawnBinary,
    client,
    session,
  }
}

describe('SimctlDriver constructor', () => {
  it('binds session', () => {
    const { driver } = makeDriver()
    expect(driver).toBeInstanceOf(SimctlDriver)
  })
})

describe('launch', () => {
  it('parses pid and passes udid', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: 'com.example.app: 12345\n', stderr: '', exitCode: 0 })
    await expect(driver.launch('com.example.app')).resolves.toBeUndefined()
    expect(spawn).toHaveBeenCalledTimes(1)
    expect(spawn.mock.calls[0]![1]).toEqual(['simctl', 'launch', FAKE_UDID, 'com.example.app'])
  })

  it('fresh: terminate then launch', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce(OK)
    spawn.mockResolvedValueOnce({ stdout: 'com.example.app: 99\n', stderr: '', exitCode: 0 })
    await expect(driver.launch('com.example.app', { fresh: true })).resolves.toBeUndefined()
    expect(spawn).toHaveBeenCalledTimes(2)
    expect(spawn.mock.calls[0]![1]).toEqual(['simctl', 'terminate', FAKE_UDID, 'com.example.app'])
    expect(spawn.mock.calls[1]![1]).toEqual(['simctl', 'launch', FAKE_UDID, 'com.example.app'])
  })
})

describe('terminate', () => {
  it('happy', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce(OK)
    await expect(driver.terminate('com.example.app')).resolves.toBeUndefined()
    expect(spawn.mock.calls[0]![1]).toEqual(['simctl', 'terminate', FAKE_UDID, 'com.example.app'])
  })

  it('error wraps to DRIVER_ERROR with hint', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: '', stderr: 'not running', exitCode: 3 })
    let caught: unknown
    try {
      await driver.terminate('com.example.app')
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).code).toBe('DRIVER_ERROR')
    expect((caught as ExpectationFailure).message).toContain('terminate')
    expect((caught as ExpectationFailure).hint ?? '').toContain('simctl')
  })
})

describe('install', () => {
  it('happy', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce(OK)
    await expect(driver.install('/tmp/x.app')).resolves.toBeUndefined()
    expect(spawn.mock.calls[0]![1]).toEqual(['simctl', 'install', FAKE_UDID, '/tmp/x.app'])
  })

  it('error wraps', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: '', stderr: 'bad', exitCode: 2 })
    await expect(driver.install('/tmp/x.app')).rejects.toThrow(ExpectationFailure)
  })
})

describe('uninstall', () => {
  it('happy', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce(OK)
    await expect(driver.uninstall('com.example.app')).resolves.toBeUndefined()
    expect(spawn.mock.calls[0]![1]).toEqual(['simctl', 'uninstall', FAKE_UDID, 'com.example.app'])
  })

  it('error wraps', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: '', stderr: 'nope', exitCode: 1 })
    await expect(driver.uninstall('com.example.app')).rejects.toThrow(ExpectationFailure)
  })
})

describe('openUrl', () => {
  it('happy', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce(OK)
    await expect(driver.openUrl('https://example.com')).resolves.toBeUndefined()
    expect(spawn.mock.calls[0]![1]).toEqual(['simctl', 'openurl', FAKE_UDID, 'https://example.com'])
  })

  it('error wraps', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: '', stderr: 'bad', exitCode: 1 })
    await expect(driver.openUrl('https://example.com')).rejects.toThrow(ExpectationFailure)
  })
})

describe('pasteboardSet', () => {
  it('happy: stdin carries text', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce(OK)
    await expect(driver.pasteboardSet('hi')).resolves.toBeUndefined()
    expect(spawn.mock.calls[0]![1]).toEqual(['simctl', 'pbcopy', FAKE_UDID])
    expect((spawn.mock.calls[0]![2] as { stdin?: string } | undefined)?.stdin).toBe('hi')
  })

  it('error wraps', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: '', stderr: 'err', exitCode: 1 })
    await expect(driver.pasteboardSet('x')).rejects.toThrow(ExpectationFailure)
  })
})

describe('pasteboardGet', () => {
  it('happy', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: 'world', stderr: '', exitCode: 0 })
    await expect(driver.pasteboardGet()).resolves.toBe('world')
    expect(spawn.mock.calls[0]![1]).toEqual(['simctl', 'pbpaste', FAKE_UDID])
  })

  it('error wraps', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: '', stderr: 'err', exitCode: 1 })
    await expect(driver.pasteboardGet()).rejects.toThrow(ExpectationFailure)
  })
})

describe('grantPermission', () => {
  it('explicit bundleId, camera maps to camera', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce(OK)
    await expect(driver.grantPermission('camera', 'com.example.app')).resolves.toBeUndefined()
    expect(spawn.mock.calls[0]![1]).toEqual([
      'simctl', 'privacy', FAKE_UDID, 'grant', 'camera', 'com.example.app',
    ])
  })

  it('notifications maps to all', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce(OK)
    await expect(driver.grantPermission('notifications', 'com.example.app')).resolves.toBeUndefined()
    expect(spawn.mock.calls[0]![1]).toEqual([
      'simctl', 'privacy', FAKE_UDID, 'grant', 'all', 'com.example.app',
    ])
  })

  it('no bundleId and no prior launch throws DRIVER_ERROR with requires bundleId', async () => {
    const { driver, spawn } = makeDriver()
    let caught: unknown
    try {
      await driver.grantPermission('camera')
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).code).toBe('DRIVER_ERROR')
    expect((caught as ExpectationFailure).message).toContain('requires bundleId')
    expect(spawn).not.toHaveBeenCalled()
  })

  it('uses lastLaunchedBundleId after successful launch', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: 'com.foo: 1\n', stderr: '', exitCode: 0 })
    spawn.mockResolvedValueOnce(OK)
    await driver.launch('com.foo')
    await expect(driver.grantPermission('camera')).resolves.toBeUndefined()
    expect(spawn).toHaveBeenCalledTimes(2)
    expect(spawn.mock.calls[1]![1]).toEqual([
      'simctl', 'privacy', FAKE_UDID, 'grant', 'camera', 'com.foo',
    ])
  })
})

describe('setAppearance', () => {
  it('happy', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce(OK)
    await expect(driver.setAppearance('dark')).resolves.toBeUndefined()
    expect(spawn.mock.calls[0]![1]).toEqual(['simctl', 'ui', FAKE_UDID, 'appearance', 'dark'])
  })

  it('error wraps', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: '', stderr: 'bad', exitCode: 1 })
    await expect(driver.setAppearance('light')).rejects.toThrow(ExpectationFailure)
  })
})

describe('screenshot', () => {
  it('happy: returns PNG buffer via temp file', async () => {
    const { driver, spawn } = makeDriver()
    const png = Buffer.from([0x89, 0x50, 0x4e, 0x47])
    spawn.mockImplementationOnce(async (_cmd: string, args: readonly string[]) => {
      const { writeFile } = await import('node:fs/promises')
      await writeFile(args[5]!, png)
      return { stdout: '', stderr: '', exitCode: 0 }
    })
    const result = await driver.screenshot()
    expect(Buffer.isBuffer(result)).toBe(true)
    expect(result.equals(png)).toBe(true)
    expect(spawn.mock.calls[0]![1]!.slice(0, 5)).toEqual([
      'simctl', 'io', FAKE_UDID, 'screenshot', '--type=png',
    ])
  })

  it('error wraps', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: '', stderr: 'fail', exitCode: 164 })
    await expect(driver.screenshot()).rejects.toThrow(ExpectationFailure)
  })
})

describe('describe', () => {
  it('returns base64 screenshot and empty frontApp before launch', async () => {
    const { driver, spawn } = makeDriver()
    const png = Buffer.from([0x89, 0x50])
    spawn.mockImplementationOnce(async (_cmd: string, args: readonly string[]) => {
      const { writeFile } = await import('node:fs/promises')
      await writeFile(args[5]!, png)
      return { stdout: '', stderr: '', exitCode: 0 }
    })
    const start = Date.now()
    const result = await driver.describe()
    expect(result.screenshot).toBe(png.toString('base64'))
    expect(result.elements).toEqual([])
    expect(result.frontApp).toBe('')
    expect(result.summary).toBe('')
    expect(result.capturedAt).toBeGreaterThanOrEqual(start)
  })

  it('frontApp filled after launch', async () => {
    const { driver, spawn } = makeDriver()
    spawn.mockResolvedValueOnce({ stdout: 'com.foo: 1\n', stderr: '', exitCode: 0 })
    spawn.mockImplementationOnce(async (_cmd: string, args: readonly string[]) => {
      const { writeFile } = await import('node:fs/promises')
      await writeFile(args[5]!, Buffer.alloc(8))
      return { stdout: '', stderr: '', exitCode: 0 }
    })
    await driver.launch('com.foo')
    const result = await driver.describe()
    expect(result.frontApp).toBe('com.foo')
  })

  // ------------------------------------------------------------------
  // v0.4 C1: describe().elements is filled from the a11y tree (top 50)
  // ------------------------------------------------------------------

  function makeFlatTreeForDescribe(
    children: A11yNode[],
  ): A11yNode {
    return {
      rawType: 'application',
      bounds: { x: 0, y: 0, w: 400, h: 800 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children,
    }
  }

  function stubScreenshot(spawn: ReturnType<typeof vi.fn>): void {
    spawn.mockImplementationOnce(async (_cmd: string, args: readonly string[]) => {
      const { writeFile } = await import('node:fs/promises')
      await writeFile(args[5]!, Buffer.from([0x89, 0x50]))
      return { stdout: '', stderr: '', exitCode: 0 }
    })
  }

  it('v0.4 C1: fills elements from tree (visible+enabled, top 50 in DFS pre-order)', async () => {
    // 60 visible+enabled cells → top 50 only, DFS pre-order matches construction order
    const cells: A11yNode[] = []
    for (let i = 0; i < 60; i += 1) {
      cells.push({
        rawType: 'cell',
        label: `Cell-${i}`,
        bounds: { x: 0, y: 100 + i * 10, w: 400, h: 10 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      })
    }
    const tree = makeFlatTreeForDescribe(cells)
    const { driver, spawn } = makeDriverForDescribe(tree)
    stubScreenshot(spawn)
    const result = await driver.describe()
    expect(result.elements.length).toBe(50)
    // root (application) is visible+enabled too → included as first DFS entry
    expect(result.elements[0]!.role).toBeDefined()
    // last entry should reflect DFS pre-order truncation at 50; first 49 cells
    // come after root → element[49].name === 'Cell-48'
    expect(result.elements[49]!.name).toBe('Cell-48')
  })

  it('v0.4 C1: elements excludes visible=false nodes', async () => {
    const tree = makeFlatTreeForDescribe([
      {
        rawType: 'cell',
        label: 'Visible',
        bounds: { x: 0, y: 100, w: 400, h: 44 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      },
      {
        rawType: 'cell',
        label: 'Hidden',
        bounds: { x: 0, y: 200, w: 400, h: 44 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: false,
        children: [],
      },
    ])
    const { driver, spawn } = makeDriverForDescribe(tree)
    stubScreenshot(spawn)
    const result = await driver.describe()
    const names = result.elements.map((e) => e.name).filter(Boolean)
    expect(names).toContain('Visible')
    expect(names).not.toContain('Hidden')
  })

  it('v0.4 C1: elements excludes enabled=false nodes', async () => {
    const tree = makeFlatTreeForDescribe([
      {
        rawType: 'cell',
        label: 'OnCell',
        bounds: { x: 0, y: 100, w: 400, h: 44 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      },
      {
        rawType: 'cell',
        label: 'OffCell',
        bounds: { x: 0, y: 200, w: 400, h: 44 },
        enabled: false,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      },
    ])
    const { driver, spawn } = makeDriverForDescribe(tree)
    stubScreenshot(spawn)
    const result = await driver.describe()
    const names = result.elements.map((e) => e.name).filter(Boolean)
    expect(names).toContain('OnCell')
    expect(names).not.toContain('OffCell')
  })

  it('v0.4 C1: treeSource failure degrades to empty elements (does not throw)', async () => {
    const transportErr = new RunnerTransportError('runner /tree fetch failed: ECONNREFUSED')
    const { driver, spawn } = makeDriverForDescribe(transportErr)
    stubScreenshot(spawn)
    const start = Date.now()
    const result = await driver.describe()
    expect(result.elements).toEqual([])
    expect(result.screenshot).toBe(Buffer.from([0x89, 0x50]).toString('base64'))
    expect(result.capturedAt).toBeGreaterThanOrEqual(start)
  })

  it('v0.4 C1: treeSource undefined (no Cell) degrades to empty elements', async () => {
    // makeDriver() builds SimctlDriver(session) without a Cell — treeSource is undefined
    const { driver, spawn } = makeDriver()
    stubScreenshot(spawn)
    const result = await driver.describe()
    expect(result.elements).toEqual([])
    expect(result.frontApp).toBe('')
  })

  it('v0.4 C1: elements are summarized via summarizeNode (role / name / id / bounds preserved)', async () => {
    const tree = makeFlatTreeForDescribe([
      {
        rawType: 'cell',
        role: 'button',
        identifier: 'general-cell',
        label: 'General',
        bounds: { x: 10, y: 200, w: 380, h: 44 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      },
    ])
    const { driver, spawn } = makeDriverForDescribe(tree)
    stubScreenshot(spawn)
    const result = await driver.describe()
    // root application node is the first DFS entry; the General cell is second
    const general = result.elements.find((e) => e.id === 'general-cell')
    expect(general).toBeDefined()
    expect(general!.role).toBe('button')
    expect(general!.name).toBe('General')
    expect(general!.bounds).toEqual({ x: 10, y: 200, w: 380, h: 44 })
    expect(general!.enabled).toBe(true)
  })
})

/**
 * v0.4 C1 helper: makeDriver wired with a fake treeSource for describe() tests.
 * Pass an A11yNode to inject a tree, or an Error to make getTree() throw.
 */
function makeDriverForDescribe(treeOrError: A11yNode | Error) {
  const spawn = vi.fn()
  const spawnBinary = vi.fn()
  const client = new SimctlClient({
    spawn: spawn as unknown as Spawner,
    spawnBinary: spawnBinary as unknown as BinarySpawner,
  })
  const session = new SimSession(client, makeDevice())
  const cell: Cell = {
    id: 'cell-0001',
    udid: session.udid,
    runnerPort: 22087,
    traceDir: '.simx/trace',
    session,
  }
  const getTree = vi.fn().mockImplementation(async () => {
    if (treeOrError instanceof Error) throw treeOrError
    return treeOrError
  })
  const runnerClient = {
    health: vi.fn().mockResolvedValue(true),
    tap: vi.fn().mockResolvedValue(undefined),
    getTree,
  } as unknown as RunnerClient
  return {
    driver: new SimctlDriver(cell, { runnerClient }),
    cell,
    getTree,
    spawn,
  }
}

function makeDriverWithRunner(opts?: {
  tap?: (selector: { text: string }) => Promise<void>
  health?: () => Promise<boolean>
}) {
  const spawn = vi.fn()
  const spawnBinary = vi.fn()
  const client = new SimctlClient({
    spawn: spawn as unknown as Spawner,
    spawnBinary: spawnBinary as unknown as BinarySpawner,
  })
  const session = new SimSession(client, makeDevice())
  const cell: Cell = {
    id: 'cell-0001',
    udid: session.udid,
    runnerPort: 22087,
    traceDir: '.simx/trace',
    session,
  }
  const runnerTap = vi
    .fn()
    .mockImplementation(opts?.tap ?? (async () => undefined))
  const runnerHealth = vi
    .fn()
    .mockImplementation(opts?.health ?? (async () => true))
  const runnerClient = {
    health: runnerHealth,
    tap: runnerTap,
  } as unknown as RunnerClient
  return {
    driver: new SimctlDriver(cell, { runnerClient }),
    cell,
    runnerTap,
    runnerHealth,
  }
}

describe('tap', () => {
  it('text happy: posts to runner with selector', async () => {
    const { driver, runnerTap, runnerHealth } = makeDriverWithRunner()
    await expect(driver.tap({ text: 'General' })).resolves.toBeUndefined()
    expect(runnerHealth).toHaveBeenCalledTimes(1)
    expect(runnerTap).toHaveBeenCalledTimes(1)
    expect(runnerTap.mock.calls[0]![0]).toEqual({ text: 'General' })
  })

  it('pre-flight health fail: throws DRIVER_ERROR with hint pointing to runner script', async () => {
    const { driver, runnerTap } = makeDriverWithRunner({
      health: async () => false,
    })
    let caught: unknown
    try {
      await driver.tap({ text: 'General' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('DRIVER_ERROR')
    expect(err.hint ?? '').toContain('simx-runner')
    expect(runnerTap).not.toHaveBeenCalled()
  })

  it('runner returns 404: throws ELEMENT_NOT_FOUND with selector echoed', async () => {
    const { driver } = makeDriverWithRunner({
      tap: async (sel) => {
        throw new TapNotFoundError(sel, '{"ok":false,"error":"not_found"}')
      },
    })
    let caught: unknown
    try {
      await driver.tap({ text: 'Nope' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('ELEMENT_NOT_FOUND')
    expect(err.selector).toEqual({ text: 'Nope' })
  })

  it('RunnerTransportError: throws DRIVER_ERROR mentioning runner port', async () => {
    const { driver } = makeDriverWithRunner({
      tap: async () => {
        throw new RunnerTransportError('fetch failed: ECONNREFUSED', {
          cause: new Error('ECONNREFUSED'),
        })
      },
    })
    let caught: unknown
    try {
      await driver.tap({ text: 'General' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('DRIVER_ERROR')
    expect(err.hint ?? '').toContain('22087')
  })
})

// ============================================================
// v0.3 C6: tree / findOne / findAll / waitFor + tap path B
// ============================================================

function makeRect(x: number, y: number, w: number, h: number) {
  return { x, y, w, h }
}

function makeNode(
  overrides: Partial<A11yNode> & { rawType?: string } = {},
): A11yNode {
  return {
    rawType: overrides.rawType ?? 'other',
    bounds: overrides.bounds ?? makeRect(0, 0, 0, 0),
    enabled: overrides.enabled ?? true,
    selected: overrides.selected ?? false,
    hasFocus: overrides.hasFocus ?? false,
    visible: overrides.visible ?? true,
    children: overrides.children ?? [],
    ...overrides,
  } as A11yNode
}

/** dev sim Settings root-shaped tree: 3 cells, identifier on General */
function makeSettingsTree(): A11yNode {
  return makeNode({
    rawType: 'application',
    bounds: makeRect(0, 0, 400, 800),
    children: [
      makeNode({
        rawType: 'cell',
        label: 'Apple Account',
        bounds: makeRect(0, 100, 400, 80),
      }),
      makeNode({
        rawType: 'cell',
        label: 'General',
        identifier: 'general',
        bounds: makeRect(0, 200, 400, 44),
      }),
      makeNode({
        rawType: 'cell',
        label: 'Display & Brightness',
        bounds: makeRect(0, 260, 400, 44),
      }),
    ],
  })
}

function makeDriverWithDeps(opts?: {
  tree?: A11yNode | (() => Promise<A11yNode>)
  treeError?: Error
  hostHidResult?: { stdout: Buffer; stderr: string; exitCode: number }
  hostHidImpl?: BinarySpawner
}) {
  const spawn = vi.fn()
  const spawnBinary = vi.fn()
  const client = new SimctlClient({
    spawn: spawn as unknown as Spawner,
    spawnBinary: spawnBinary as unknown as BinarySpawner,
  })
  const session = new SimSession(client, makeDevice())
  const cell: Cell = {
    id: 'cell-0001',
    udid: session.udid,
    runnerPort: 22087,
    traceDir: '.simx/trace',
    session,
  }
  const getTree = vi.fn().mockImplementation(async () => {
    if (opts?.treeError) throw opts.treeError
    if (typeof opts?.tree === 'function') return await (opts.tree as () => Promise<A11yNode>)()
    return opts?.tree ?? makeSettingsTree()
  })
  const runnerClient = {
    health: vi.fn().mockResolvedValue(true),
    tap: vi.fn().mockResolvedValue(undefined),
    getTree,
  } as unknown as RunnerClient
  const defaultOk = {
    stdout: Buffer.from(JSON.stringify({ ok: true, path: 'digitizer' })),
    stderr: '',
    exitCode: 0,
  }
  const hostHidSpawner = vi
    .fn()
    .mockImplementation(opts?.hostHidImpl ?? (async () => opts?.hostHidResult ?? defaultOk))
  return {
    driver: new SimctlDriver(cell, {
      runnerClient,
      hostHidSpawner: hostHidSpawner as unknown as BinarySpawner,
      hostHidBin: '/fake/host-hid',
    }),
    cell,
    getTree,
    hostHidSpawner,
    runnerClient,
  }
}

describe('tree (v0.3 C6)', () => {
  it('passes through RunnerClient.getTree', async () => {
    const tree = makeSettingsTree()
    const { driver, getTree } = makeDriverWithDeps({ tree })
    await expect(driver.tree()).resolves.toBe(tree)
    expect(getTree).toHaveBeenCalledTimes(1)
  })

  it('rethrows RunnerTransportError without extra wrapping', async () => {
    const transportErr = new RunnerTransportError('runner /tree fetch failed: ECONNREFUSED')
    const { driver } = makeDriverWithDeps({ treeError: transportErr })
    await expect(driver.tree()).rejects.toBe(transportErr)
  })

  it('without tree source: throws DRIVER_ERROR with hint about Cell', async () => {
    const { driver } = makeDriver() // no runnerClient → treeSource undefined
    let caught: unknown
    try {
      await driver.tree()
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('DRIVER_ERROR')
    expect(err.hint ?? '').toMatch(/acquireCell|RunnerClient\.getTree|Cell/)
  })
})

describe('findOne (v0.3 C6)', () => {
  it('text selector hit', async () => {
    const { driver } = makeDriverWithDeps()
    const node = await driver.findOne({ text: 'Apple Account' })
    expect(node).not.toBeNull()
    expect(node?.label).toBe('Apple Account')
  })

  it('id selector hit', async () => {
    const { driver } = makeDriverWithDeps()
    const node = await driver.findOne({ id: 'general' })
    expect(node).not.toBeNull()
    expect(node?.identifier).toBe('general')
  })

  it('role+name hit', async () => {
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({ rawType: 'button', label: 'OK', bounds: makeRect(0, 0, 80, 44), role: 'button' }),
      ],
    })
    const { driver } = makeDriverWithDeps({ tree })
    const node = await driver.findOne({ role: 'button', name: 'OK' })
    expect(node).not.toBeNull()
    expect(node?.label).toBe('OK')
  })

  it('miss returns null (not throws)', async () => {
    const { driver } = makeDriverWithDeps()
    await expect(driver.findOne({ text: 'Nope' })).resolves.toBeNull()
  })

  it('spatial modifier (below) hit', async () => {
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({ rawType: 'staticText', label: 'Header', bounds: makeRect(0, 0, 400, 44) }),
        makeNode({ rawType: 'cell', label: 'Cell', bounds: makeRect(0, 100, 400, 44) }),
      ],
    })
    const { driver } = makeDriverWithDeps({ tree })
    const node = await driver.findOne({ text: 'Cell', below: { text: 'Header' } })
    expect(node).not.toBeNull()
    expect(node?.label).toBe('Cell')
  })
})

describe('findAll (v0.3 C6)', () => {
  it('multi hit: DFS pre-order length 3', async () => {
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({ rawType: 'staticText', label: 'X', bounds: makeRect(0, 0, 80, 20) }),
        makeNode({ rawType: 'staticText', label: 'X', bounds: makeRect(0, 100, 80, 20) }),
        makeNode({ rawType: 'staticText', label: 'X', bounds: makeRect(0, 200, 80, 20) }),
      ],
    })
    const { driver } = makeDriverWithDeps({ tree })
    const nodes = await driver.findAll({ text: 'X' })
    expect(nodes).toHaveLength(3)
  })

  it('0 hit returns []', async () => {
    const { driver } = makeDriverWithDeps()
    await expect(driver.findAll({ text: 'Nope' })).resolves.toEqual([])
  })

  it('index modifier (first: true) silently ignored → still all matches', async () => {
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({ rawType: 'staticText', label: 'X', bounds: makeRect(0, 0, 80, 20) }),
        makeNode({ rawType: 'staticText', label: 'X', bounds: makeRect(0, 100, 80, 20) }),
        makeNode({ rawType: 'staticText', label: 'X', bounds: makeRect(0, 200, 80, 20) }),
      ],
    })
    const { driver } = makeDriverWithDeps({ tree })
    const nodes = await driver.findAll({ text: 'X', first: true })
    expect(nodes).toHaveLength(3)
  })
})

describe('waitFor (v0.3 C6)', () => {
  it('immediate hit: resolves on first poll without setTimeout', async () => {
    const { driver, getTree } = makeDriverWithDeps()
    const node = await driver.waitFor({ text: 'General' }, 5000)
    expect(node.label).toBe('General')
    expect(getTree).toHaveBeenCalledTimes(1)
  })

  it('polls until hit (sequence: miss → hit)', async () => {
    let callIdx = 0
    const emptyTree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [],
    })
    const populatedTree = makeSettingsTree()
    const { driver, getTree } = makeDriverWithDeps({
      tree: async () => {
        callIdx += 1
        return callIdx === 1 ? emptyTree : populatedTree
      },
    })
    const node = await driver.waitFor({ text: 'General' }, 5000)
    expect(node.label).toBe('General')
    expect(getTree).toHaveBeenCalledTimes(2)
  })

  it('timeout: rejects with TIMEOUT after deadline, hint mentions describe', async () => {
    const empty = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [],
    })
    const { driver } = makeDriverWithDeps({ tree: empty })
    let caught: unknown
    try {
      await driver.waitFor({ text: 'General' }, 50)
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('TIMEOUT')
    expect(err.hint ?? '').toContain('describe')
  })

  it('single-shot (timeout=0): one findOne, no poll, rejects immediately on miss', async () => {
    const empty = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [],
    })
    const { driver, getTree } = makeDriverWithDeps({ tree: empty })
    let caught: unknown
    try {
      await driver.waitFor({ text: 'General' }, 0)
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).code).toBe('TIMEOUT')
    expect(getTree).toHaveBeenCalledTimes(1)
  })
})

// ============================================================
// v0.4 C7: SimctlDriver.waitFor exponential backoff (50 → 500ms, factor 1.5)
//
// Replaces v0.3 C6 fixed 200ms poll. Curve matches src/sdk/matchers.ts
// pollFor (intentionally duplicated, not factored — see plan-hot decision 3).
// All cases use vi.useFakeTimers so CI < 50ms wall regardless of timeouts;
// pattern matches src/sim/__tests__/simctl.test.ts:449-461.
// ============================================================
describe('waitFor backoff (v0.4 C7)', () => {
  function emptyTree(): A11yNode {
    return makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [],
    })
  }

  it('exponential backoff: 50 → 75 → 112 → 168 → ... getTree call count grows by formula', async () => {
    vi.useFakeTimers()
    try {
      const { driver, getTree } = makeDriverWithDeps({ tree: emptyTree() })
      const pending = driver.waitFor({ text: 'Nope' }, 10_000).catch((e: unknown) => e)
      // probe#1 runs synchronously inside the loop (no sleep before it).
      await vi.advanceTimersByTimeAsync(0)
      expect(getTree).toHaveBeenCalledTimes(1)
      // interval[0] = 50
      await vi.advanceTimersByTimeAsync(50)
      expect(getTree).toHaveBeenCalledTimes(2)
      // interval[1] = 75
      await vi.advanceTimersByTimeAsync(75)
      expect(getTree).toHaveBeenCalledTimes(3)
      // interval[2] = 112.5 → setTimeout uses 112.5; advance 113 to clear it
      await vi.advanceTimersByTimeAsync(113)
      expect(getTree).toHaveBeenCalledTimes(4)
      // interval[3] = 168.75 → advance 169
      await vi.advanceTimersByTimeAsync(169)
      expect(getTree).toHaveBeenCalledTimes(5)
      // Drain to deadline.
      await vi.advanceTimersByTimeAsync(10_000)
      await pending
    } finally {
      vi.useRealTimers()
    }
  })

  it('immediate hit under fake timers: resolves with single getTree call', async () => {
    vi.useFakeTimers()
    try {
      const { driver, getTree } = makeDriverWithDeps()
      const node = await driver.waitFor({ text: 'General' }, 5000)
      expect(node.label).toBe('General')
      expect(getTree).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('deadline truncation: total=80ms, last sleep clipped to remaining budget', async () => {
    vi.useFakeTimers()
    try {
      const { driver, getTree } = makeDriverWithDeps({ tree: emptyTree() })
      const pending = driver.waitFor({ text: 'Nope' }, 80).catch((e: unknown) => e)
      await vi.advanceTimersByTimeAsync(0)
      expect(getTree).toHaveBeenCalledTimes(1)
      // probe#2 at t=50
      await vi.advanceTimersByTimeAsync(50)
      expect(getTree).toHaveBeenCalledTimes(2)
      // remaining = 30; next sleep clipped to min(75, 30) = 30; probe#3 at t=80
      await vi.advanceTimersByTimeAsync(30)
      expect(getTree).toHaveBeenCalledTimes(3)
      const caught = await pending
      expect(caught).toBeInstanceOf(ExpectationFailure)
      expect((caught as ExpectationFailure).code).toBe('TIMEOUT')
    } finally {
      vi.useRealTimers()
    }
  })

  it('tiny timeout=10ms: probe#1 misses, deadline triggers within first interval window', async () => {
    vi.useFakeTimers()
    try {
      const { driver, getTree } = makeDriverWithDeps({ tree: emptyTree() })
      const pending = driver.waitFor({ text: 'Nope' }, 10).catch((e: unknown) => e)
      await vi.advanceTimersByTimeAsync(0)
      expect(getTree).toHaveBeenCalledTimes(1)
      // sleep is min(50, 10) = 10; after 10ms, probe#2 → deadline → throw
      await vi.advanceTimersByTimeAsync(10)
      const caught = await pending
      expect(caught).toBeInstanceOf(ExpectationFailure)
      expect((caught as ExpectationFailure).code).toBe('TIMEOUT')
      expect(getTree).toHaveBeenCalledTimes(2)
    } finally {
      vi.useRealTimers()
    }
  })

  it('cap at 500ms: after step 6 the interval stays at 500 (not 569)', async () => {
    vi.useFakeTimers()
    try {
      const { driver, getTree } = makeDriverWithDeps({ tree: emptyTree() })
      const pending = driver.waitFor({ text: 'Nope' }, 60_000).catch((e: unknown) => e)
      // probe#1 at t=0
      await vi.advanceTimersByTimeAsync(0)
      expect(getTree).toHaveBeenCalledTimes(1)
      // intervals 50/75/112.5/168.75/253.125/379.6875 — advance just past each.
      await vi.advanceTimersByTimeAsync(50)
      expect(getTree).toHaveBeenCalledTimes(2)
      await vi.advanceTimersByTimeAsync(75)
      expect(getTree).toHaveBeenCalledTimes(3)
      await vi.advanceTimersByTimeAsync(113)
      expect(getTree).toHaveBeenCalledTimes(4)
      await vi.advanceTimersByTimeAsync(169)
      expect(getTree).toHaveBeenCalledTimes(5)
      await vi.advanceTimersByTimeAsync(254)
      expect(getTree).toHaveBeenCalledTimes(6)
      await vi.advanceTimersByTimeAsync(380)
      expect(getTree).toHaveBeenCalledTimes(7)
      // Sleep between probe#7 and probe#8 = min(500, remaining) = 500
      // (not 569.53 — cap kicked in). Each setTimeout fires at virtual time
      // = (registration time) + (floor of delay). Cumulative virtual time
      // after the advances above = 1041; probe#7's setTimeout was registered
      // at virtual t≈1037 with delay 500 → fires at t=1537. Advance 495
      // brings us to t=1536 (just under) → no new probe; advance 2 more
      // (t=1538) crosses the boundary → probe#8 fires. If the cap were
      // missing the interval would be 569.5 → boundary t≈1606 → advance(495)
      // would also not fire it, but advance(2) to t=1538 would still be
      // pre-boundary (probe#8 wouldn't fire) — that's the regression signal.
      await vi.advanceTimersByTimeAsync(495)
      expect(getTree).toHaveBeenCalledTimes(7)
      await vi.advanceTimersByTimeAsync(2)
      expect(getTree).toHaveBeenCalledTimes(8)
      // Drain.
      await vi.advanceTimersByTimeAsync(60_000)
      await pending
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('tap path B — non-text selector → resolveSelector → host-hid (v0.3 C6)', () => {
  it('id selector hit: spawns host-hid with normalized centroid coords', async () => {
    const { driver, hostHidSpawner } = makeDriverWithDeps()
    await expect(driver.tap({ id: 'general' })).resolves.toBeUndefined()
    expect(hostHidSpawner).toHaveBeenCalledTimes(1)
    const args = hostHidSpawner.mock.calls[0]![1] as string[]
    expect(args[0]).toBe('tap')
    expect(args).toContain('--udid')
    expect(args).toContain('--x')
    expect(args).toContain('--y')
    // General cell at (0,200,400,44), root (0,0,400,800)
    // centroid = (200, 222), nx=0.5, ny=0.2775
    const xIdx = args.indexOf('--x') + 1
    const yIdx = args.indexOf('--y') + 1
    expect(parseFloat(args[xIdx]!)).toBeCloseTo(0.5, 3)
    expect(parseFloat(args[yIdx]!)).toBeCloseTo(0.2775, 3)
  })

  it('role+name selector hit: host-hid called', async () => {
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({
          rawType: 'cell',
          label: 'General',
          bounds: makeRect(0, 200, 400, 44),
          role: 'cell',
        }),
      ],
    })
    const { driver, hostHidSpawner } = makeDriverWithDeps({ tree })
    await expect(driver.tap({ role: 'cell', name: 'General' })).resolves.toBeUndefined()
    expect(hostHidSpawner).toHaveBeenCalledTimes(1)
  })

  it('text + modifier (below) selector hit: host-hid called', async () => {
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({ rawType: 'staticText', label: 'Header', bounds: makeRect(0, 0, 400, 44) }),
        makeNode({ rawType: 'cell', label: 'General', bounds: makeRect(0, 100, 400, 44) }),
      ],
    })
    const { driver, hostHidSpawner } = makeDriverWithDeps({ tree })
    await expect(
      driver.tap({ text: 'General', below: { text: 'Header' } }),
    ).resolves.toBeUndefined()
    expect(hostHidSpawner).toHaveBeenCalledTimes(1)
  })

  it('label selector hit: host-hid called', async () => {
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({ rawType: 'cell', label: 'General', bounds: makeRect(0, 200, 400, 44) }),
      ],
    })
    const { driver, hostHidSpawner } = makeDriverWithDeps({ tree })
    await expect(driver.tap({ label: 'General' })).resolves.toBeUndefined()
    expect(hostHidSpawner).toHaveBeenCalledTimes(1)
  })

  it('regex text selector hit: host-hid called (path B, not plain text)', async () => {
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({ rawType: 'cell', label: 'General', bounds: makeRect(0, 200, 400, 44) }),
      ],
    })
    const { driver, hostHidSpawner } = makeDriverWithDeps({ tree })
    await expect(driver.tap({ text: /^Gen/ })).resolves.toBeUndefined()
    expect(hostHidSpawner).toHaveBeenCalledTimes(1)
  })

  it('0 hit: rejects ELEMENT_NOT_FOUND with visibleElements, host-hid NOT called', async () => {
    const { driver, hostHidSpawner } = makeDriverWithDeps()
    let caught: unknown
    try {
      await driver.tap({ id: 'nope-not-in-tree' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('ELEMENT_NOT_FOUND')
    expect(err.selector).toEqual({ id: 'nope-not-in-tree' })
    expect(err.visibleElements).toBeDefined()
    expect((err.visibleElements ?? []).length).toBeGreaterThan(0)
    expect(hostHidSpawner).not.toHaveBeenCalled()
  })

  it('host-hid returns ok=false: rejects DRIVER_ERROR with hint mentioning host-hid', async () => {
    const { driver } = makeDriverWithDeps({
      hostHidResult: {
        stdout: Buffer.from(JSON.stringify({ ok: false, error: 'hid_failed' })),
        stderr: '',
        exitCode: 0,
      },
    })
    let caught: unknown
    try {
      await driver.tap({ id: 'general' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('DRIVER_ERROR')
    expect(err.hint ?? '').toContain('host-hid')
  })

  it('host-hid returns non-JSON: rejects DRIVER_ERROR with hint about simx-host-hid', async () => {
    const { driver } = makeDriverWithDeps({
      hostHidResult: {
        stdout: Buffer.from('garbage not json'),
        stderr: '',
        exitCode: 0,
      },
    })
    let caught: unknown
    try {
      await driver.tap({ id: 'general' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('DRIVER_ERROR')
    expect(err.message).toContain('non-JSON')
  })

  it('host-hid spawn throws: rejects DRIVER_ERROR with hint pointing to swift build', async () => {
    const { driver } = makeDriverWithDeps({
      hostHidImpl: async () => {
        throw new Error('ENOENT: simx-host-hid not found')
      },
    })
    let caught: unknown
    try {
      await driver.tap({ id: 'general' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('DRIVER_ERROR')
    expect(err.message).toContain('host-hid spawn failed')
    expect(err.hint ?? '').toContain('swift build')
  })

  it('candidate has empty frame: rejects ELEMENT_NOT_FOUND with offscreen hint', async () => {
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({
          rawType: 'cell',
          identifier: 'zero',
          label: 'Zero',
          bounds: makeRect(0, 0, 0, 0),
        }),
      ],
    })
    const { driver, hostHidSpawner } = makeDriverWithDeps({ tree })
    let caught: unknown
    try {
      await driver.tap({ id: 'zero' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    const err = caught as ExpectationFailure
    expect(err.code).toBe('ELEMENT_NOT_FOUND')
    expect((err.message + ' ' + (err.hint ?? '')).toLowerCase()).toMatch(/empty|offscreen/)
    expect(hostHidSpawner).not.toHaveBeenCalled()
  })

  it('centroid out of (0,1) bounds: rejects ELEMENT_NOT_FOUND', async () => {
    // root bounds (0,0,400,800); candidate centroid at (500, 400) → nx=1.25
    const tree = makeNode({
      rawType: 'application',
      bounds: makeRect(0, 0, 400, 800),
      children: [
        makeNode({
          rawType: 'cell',
          identifier: 'off',
          label: 'Off',
          bounds: makeRect(450, 350, 100, 100),
        }),
      ],
    })
    const { driver, hostHidSpawner } = makeDriverWithDeps({ tree })
    let caught: unknown
    try {
      await driver.tap({ id: 'off' })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).code).toBe('ELEMENT_NOT_FOUND')
    expect(hostHidSpawner).not.toHaveBeenCalled()
  })

  it('getTree transport error: rethrows', async () => {
    const transportErr = new RunnerTransportError('runner /tree fetch failed: ECONNREFUSED')
    const { driver, hostHidSpawner } = makeDriverWithDeps({ treeError: transportErr })
    await expect(driver.tap({ id: 'general' })).rejects.toBe(transportErr)
    expect(hostHidSpawner).not.toHaveBeenCalled()
  })

  it('passes session UDID via --udid arg to host-hid', async () => {
    const { driver, hostHidSpawner } = makeDriverWithDeps()
    await driver.tap({ id: 'general' })
    const args = hostHidSpawner.mock.calls[0]![1] as string[]
    const udidIdx = args.indexOf('--udid') + 1
    expect(args[udidIdx]).toBe(FAKE_UDID)
  })
})

describe('unimplemented methods', () => {
  const cases: Array<{
    name: string
    hintFragment: string
    invoke: (d: SimctlDriver) => Promise<unknown>
  }> = [
    { name: 'doubleTap', hintFragment: 'HID bridge', invoke: (d) => d.doubleTap(ANY_SEL) },
    { name: 'longPress', hintFragment: 'HID bridge', invoke: (d) => d.longPress(ANY_SEL) },
    { name: 'fill', hintFragment: 'HID bridge', invoke: (d) => d.fill(ANY_SEL, 'x') },
    { name: 'clear', hintFragment: 'HID bridge', invoke: (d) => d.clear(ANY_SEL) },
    { name: 'swipe', hintFragment: 'HID bridge', invoke: (d) => d.swipe('up') },
    { name: 'scroll', hintFragment: 'HID bridge', invoke: (d) => d.scroll(ANY_SEL, 'up') },
    { name: 'scrollTo', hintFragment: 'HID bridge', invoke: (d) => d.scrollTo(ANY_SEL) },
    { name: 'pressKey', hintFragment: 'HID bridge', invoke: (d) => d.pressKey('return') },
    { name: 'hideKeyboard', hintFragment: 'HID bridge', invoke: (d) => d.hideKeyboard() },
    { name: 'background', hintFragment: 'HID/system bridge', invoke: (d) => d.background() },
    { name: 'foreground', hintFragment: 'HID/system bridge', invoke: (d) => d.foreground('com.foo') },
    { name: 'setLocale', hintFragment: 'not exposed by simctl', invoke: (d) => d.setLocale('en_US') },
    { name: 'setNetwork', hintFragment: 'not exposed by simctl', invoke: (d) => d.setNetwork('offline') },
  ]

  it.each(cases)('$name throws DRIVER_ERROR with hint', async ({ name, hintFragment, invoke }) => {
    const { driver, spawn, spawnBinary } = makeDriver()
    let caught: unknown
    try {
      await invoke(driver)
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).code).toBe('DRIVER_ERROR')
    expect((caught as ExpectationFailure).message).toContain(name)
    expect((caught as ExpectationFailure).hint ?? '').toContain(hintFragment)
    expect(spawn).not.toHaveBeenCalled()
    expect(spawnBinary).not.toHaveBeenCalled()
  })
})
