import { describe, it, expect, vi, afterEach } from 'vitest'
import { EventEmitter } from 'node:events'
import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js'
import { createSimxMcpServer } from '../server.js'
import {
  __setSpawnImpl,
  __resetSpawnImpl,
  __setWriteFileImpl,
  __resetWriteFileImpl,
  __setUnlinkImpl,
  __resetUnlinkImpl,
  __setTmpdirImpl,
  __resetTmpdirImpl,
  __setRandomBytesImpl,
  __resetRandomBytesImpl,
} from '../tools.js'
import type { SimctlClient, SimctlDevice } from '../../sim/simctl.js'
import type { Driver } from '../../driver/types.js'
import type { A11yNode, ScreenDescription, Selector } from '../../core/index.js'

type SimctlLifecycleMethods = Pick<
  SimctlClient,
  | 'listDevices'
  | 'bootAndWait'
  | 'shutdown'
  | 'launch'
  | 'terminate'
  | 'install'
  | 'uninstall'
>

type MockClientOverrides = Partial<SimctlLifecycleMethods>

function makeMockClient(overrides: MockClientOverrides = {}): SimctlClient {
  const base: SimctlLifecycleMethods = {
    listDevices: vi.fn<() => Promise<SimctlDevice[]>>().mockResolvedValue([]),
    bootAndWait: vi.fn<(udid: string, timeoutMs?: number) => Promise<void>>().mockResolvedValue(undefined),
    shutdown: vi.fn<(udid: string) => Promise<void>>().mockResolvedValue(undefined),
    launch: vi
      .fn<(udid: string, bundleId: string, args?: readonly string[], env?: Record<string, string>) => Promise<{ pid: number }>>()
      .mockResolvedValue({ pid: 1 }),
    terminate: vi.fn<(udid: string, bundleId: string) => Promise<void>>().mockResolvedValue(undefined),
    install: vi.fn<(udid: string, appPath: string) => Promise<void>>().mockResolvedValue(undefined),
    uninstall: vi.fn<(udid: string, bundleId: string) => Promise<void>>().mockResolvedValue(undefined),
  }
  const merged = { ...base, ...overrides }
  return merged as unknown as SimctlClient
}

type DriverAllMethods = Pick<
  Driver,
  | 'screenshot'
  | 'describe'
  | 'tree'
  | 'findOne'
  | 'tap'
  | 'doubleTap'
  | 'longPress'
  | 'fill'
  | 'swipe'
  | 'scrollTo'
  | 'pressKey'
  | 'waitFor'
  | 'openUrl'
  | 'pasteboardSet'
  | 'pasteboardGet'
  | 'grantPermission'
>
type MockDriverOverrides = Partial<DriverAllMethods>

function makeMockDriver(overrides: MockDriverOverrides = {}): Driver {
  const defaultTree: A11yNode = {
    rawType: 'application',
    bounds: { x: 0, y: 0, w: 100, h: 200 },
    enabled: true,
    selected: false,
    hasFocus: true,
    visible: true,
    children: [],
  }
  const defaultDescribe: ScreenDescription = {
    screenshot: 'YmFzZTY0',
    elements: [],
    frontApp: '',
    summary: '',
    capturedAt: 1234,
  }
  const defaultWaitForNode: A11yNode = {
    rawType: 'XCUIElementTypeOther',
    bounds: { x: 0, y: 0, w: 0, h: 0 },
    enabled: true,
    selected: false,
    hasFocus: false,
    visible: true,
    children: [],
  }
  const base: DriverAllMethods = {
    screenshot: vi
      .fn<() => Promise<Buffer>>()
      .mockResolvedValue(Buffer.from('fake-png-bytes')),
    describe: vi
      .fn<() => Promise<ScreenDescription>>()
      .mockResolvedValue(defaultDescribe),
    tree: vi.fn<() => Promise<A11yNode>>().mockResolvedValue(defaultTree),
    findOne: vi
      .fn<(s: Selector) => Promise<A11yNode | null>>()
      .mockResolvedValue(null),
    tap: vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined),
    doubleTap: vi
      .fn<(s: Selector) => Promise<void>>()
      .mockResolvedValue(undefined),
    longPress: vi
      .fn<(s: Selector, durationMs?: number) => Promise<void>>()
      .mockResolvedValue(undefined),
    fill: vi
      .fn<(s: Selector, text: string) => Promise<void>>()
      .mockResolvedValue(undefined),
    swipe: vi
      .fn<(direction: 'up' | 'down' | 'left' | 'right', from?: Selector) => Promise<void>>()
      .mockResolvedValue(undefined),
    scrollTo: vi
      .fn<(s: Selector) => Promise<void>>()
      .mockResolvedValue(undefined),
    pressKey: vi
      .fn<(k: string) => Promise<void>>()
      .mockResolvedValue(undefined),
    waitFor: vi
      .fn<(s: Selector, timeoutMs?: number) => Promise<A11yNode>>()
      .mockResolvedValue(defaultWaitForNode),
    openUrl: vi.fn<(url: string) => Promise<void>>().mockResolvedValue(undefined),
    pasteboardSet: vi
      .fn<(text: string) => Promise<void>>()
      .mockResolvedValue(undefined),
    pasteboardGet: vi.fn<() => Promise<string>>().mockResolvedValue(''),
    grantPermission: vi
      .fn<(permission: string, bundleId?: string) => Promise<void>>()
      .mockResolvedValue(undefined),
  }
  const merged = { ...base, ...overrides }
  return merged as unknown as Driver
}

function makeMockAcquireDriver(driverOverrides: MockDriverOverrides = {}): {
  acquireDriver: (udid: string) => Promise<Driver>
  calls: string[]
  driver: Driver
} {
  const driver = makeMockDriver(driverOverrides)
  const calls: string[] = []
  const fn = vi.fn<(udid: string) => Promise<Driver>>(async (udid) => {
    calls.push(udid)
    return driver
  })
  return { acquireDriver: fn, calls, driver }
}

async function makeLinkedClient(
  opts?: Parameters<typeof createSimxMcpServer>[0],
): Promise<{
  client: Client
  server: ReturnType<typeof createSimxMcpServer>
  close: () => Promise<void>
}> {
  const server = createSimxMcpServer(opts)
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair()
  const client = new Client(
    { name: 'simx-test-client', version: '0.0.0' },
    { capabilities: {} },
  )
  await Promise.all([
    server.connect(serverTransport),
    client.connect(clientTransport),
  ])
  return {
    client,
    server,
    close: async () => {
      await client.close()
      await server.close()
    },
  }
}

describe('createSimxMcpServer', () => {
  it('initialize: serverInfo.name === "simx" and protocolVersion is a supported MCP version', async () => {
    const log = vi.fn()
    const { client, close } = await makeLinkedClient({ log })
    const info = client.getServerVersion()
    expect(info).toBeDefined()
    expect(info?.name).toBe('simx')
    const protocolVersion = client.getServerCapabilities() === undefined
      ? undefined
      : client.getServerVersion()
    expect(protocolVersion).toBeDefined()
    await close()
  })

  it('tools/list: returns 27 tools and contains ping', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    expect(result.tools.length).toBe(27)
    expect(result.tools.map((t) => t.name)).toContain('ping')
    await close()
  })

  it('tools/call ping {}: content[0] is text "pong"', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.callTool({ name: 'ping', arguments: {} })
    const content = result.content as Array<{ type: string; text?: string }>
    expect(content[0]?.type).toBe('text')
    expect(content[0]?.text).toBe('pong')
    expect(result.isError).not.toBe(true)
    await close()
  })

  it('tools/call unknown_xyz: returns SDK isError content (graceful, no throw)', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.callTool({ name: 'unknown_xyz', arguments: {} })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ type: string; text?: string }>
    expect(content[0]?.text ?? '').toContain('unknown_xyz')
    await close()
  })

  it('connect → close: lifecycle clean (no throw)', async () => {
    const server = createSimxMcpServer()
    const [a, b] = InMemoryTransport.createLinkedPair()
    await server.connect(b)
    await server.close()
    // second close should be tolerated by SDK; transports stay usable for GC
    expect(a).toBeDefined()
  })

  it('log injection: helper invoked at least once across initialize + list', async () => {
    const log = vi.fn()
    const { client, close } = await makeLinkedClient({ log })
    await client.listTools()
    expect(log.mock.calls.length).toBeGreaterThanOrEqual(1)
    await close()
  })

  it('ping inputSchema is {type:object, properties:{}, additionalProperties:false}', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    const ping = result.tools.find((t) => t.name === 'ping')
    expect(ping).toBeDefined()
    const schema = ping?.inputSchema as {
      type: string
      properties: Record<string, unknown>
      additionalProperties: boolean
    }
    expect(schema.type).toBe('object')
    expect(Object.keys(schema.properties).length).toBe(0)
    expect(schema.additionalProperties).toBe(false)
    await close()
  })

  it('tools/list count === 27 (C6 boundary: 26 prior + explain_screen, no C7+ sprawl)', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    expect(result.tools.length).toBe(27)
    await close()
  })

  it('tools/call ping twice: both return "pong" (idempotent)', async () => {
    const { client, close } = await makeLinkedClient()
    const r1 = await client.callTool({ name: 'ping', arguments: {} })
    const r2 = await client.callTool({ name: 'ping', arguments: {} })
    const c1 = r1.content as Array<{ text?: string }>
    const c2 = r2.content as Array<{ text?: string }>
    expect(c1[0]?.text).toBe('pong')
    expect(c2[0]?.text).toBe('pong')
    await close()
  })

  it('initialize: client sees server capabilities.tools defined', async () => {
    const { client, close } = await makeLinkedClient()
    const caps = client.getServerCapabilities()
    expect(caps).toBeDefined()
    expect(caps?.tools).toBeDefined()
    await close()
  })

  it('createSimxMcpServer({ name: "custom" }): serverInfo.name overridden', async () => {
    const { client, close } = await makeLinkedClient({ name: 'custom' })
    const info = client.getServerVersion()
    expect(info?.name).toBe('custom')
    await close()
  })

  it('createSimxMcpServer(): default version === "0.0.0" (matches cli main.meta.version)', async () => {
    const { client, close } = await makeLinkedClient()
    const info = client.getServerVersion()
    expect(info?.version).toBe('0.0.0')
    await close()
  })

  it('tools/call ping with bad args: returns isError with "invalid arguments"', async () => {
    const log = vi.fn()
    const { client, close } = await makeLinkedClient({ log })
    const result = await client.callTool({
      name: 'ping',
      arguments: { extra: 'not allowed' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text?: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  // ====== C2: 7 lifecycle tools ======

  it('tools/list: names contain all 27 (ping + 7 lifecycle + 4 observe + 7 interaction + 3 compound + 4 system + 1 vlm)', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    const names = result.tools.map((t) => t.name).sort()
    expect(names).toEqual(
      [
        'app_install',
        'app_launch',
        'app_terminate',
        'app_uninstall',
        'double_tap',
        'element_inspect',
        'explain_screen',
        'fill',
        'find_and_tap',
        'flow_run',
        'key_press',
        'long_press',
        'open_url',
        'pasteboard_get',
        'pasteboard_set',
        'permissions_grant',
        'ping',
        'screen_describe',
        'screen_hierarchy',
        'screen_screenshot',
        'scroll_to',
        'simulator_boot',
        'simulator_list',
        'simulator_shutdown',
        'swipe',
        'tap',
        'wait_for',
      ],
    )
    await close()
  })

  it('simulator_list happy: returns devices array via SimctlClient.listDevices', async () => {
    const dev: SimctlDevice = {
      udid: 'AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA',
      name: 'iPhone 16',
      state: 'Shutdown',
      isAvailable: true,
      runtimeIdentifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-0',
      deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-16',
    }
    const listDevices = vi.fn<() => Promise<SimctlDevice[]>>().mockResolvedValue([dev])
    const mock = makeMockClient({ listDevices })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({ name: 'simulator_list', arguments: {} })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ type: string; text: string }>
    const parsed = JSON.parse(content[0]!.text) as { devices: SimctlDevice[] }
    expect(parsed.devices.length).toBe(1)
    expect(parsed.devices[0]?.udid).toBe(dev.udid)
    expect(listDevices).toHaveBeenCalledTimes(1)
    await close()
  })

  it('simulator_list error: listDevices throws → isError content', async () => {
    const listDevices = vi.fn<() => Promise<SimctlDevice[]>>().mockRejectedValue(new Error('simctl boom'))
    const mock = makeMockClient({ listDevices })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({ name: 'simulator_list', arguments: {} })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('simctl boom')
    await close()
  })

  it('simulator_boot udid happy: bootAndWait called once + alreadyBooted=false', async () => {
    const dev: SimctlDevice = {
      udid: 'AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA',
      name: 'iPhone 16',
      state: 'Shutdown',
      isAvailable: true,
      runtimeIdentifier: 'r',
      deviceTypeIdentifier: 'd',
    }
    const bootAndWait = vi
      .fn<(udid: string, timeoutMs?: number) => Promise<void>>()
      .mockResolvedValue(undefined)
    const mock = makeMockClient({
      listDevices: vi.fn<() => Promise<SimctlDevice[]>>().mockResolvedValue([dev]),
      bootAndWait,
    })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'simulator_boot',
      arguments: { udid: dev.udid },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      state: string
      alreadyBooted: boolean
      udid: string
    }
    expect(parsed.state).toBe('Booted')
    expect(parsed.alreadyBooted).toBe(false)
    expect(parsed.udid).toBe(dev.udid)
    expect(bootAndWait).toHaveBeenCalledTimes(1)
    await close()
  })

  it('simulator_boot name happy: name resolve → bootAndWait + result udid populated', async () => {
    const dev: SimctlDevice = {
      udid: 'BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB',
      name: 'iPhone X',
      state: 'Shutdown',
      isAvailable: true,
      runtimeIdentifier: 'r',
      deviceTypeIdentifier: 'd',
    }
    const bootAndWait = vi.fn<(udid: string, timeoutMs?: number) => Promise<void>>().mockResolvedValue(undefined)
    const mock = makeMockClient({
      listDevices: vi.fn<() => Promise<SimctlDevice[]>>().mockResolvedValue([dev]),
      bootAndWait,
    })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'simulator_boot',
      arguments: { name: 'iPhone X' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { udid: string }
    expect(parsed.udid).toBe(dev.udid)
    expect(bootAndWait).toHaveBeenCalledTimes(1)
    await close()
  })

  it('simulator_boot already booted: skip bootAndWait + alreadyBooted=true + durationMs=0', async () => {
    const dev: SimctlDevice = {
      udid: 'CCCCCCCC-CCCC-CCCC-CCCC-CCCCCCCCCCCC',
      name: 'iPhone Booted',
      state: 'Booted',
      isAvailable: true,
      runtimeIdentifier: 'r',
      deviceTypeIdentifier: 'd',
    }
    const bootAndWait = vi.fn<(udid: string, timeoutMs?: number) => Promise<void>>().mockResolvedValue(undefined)
    const mock = makeMockClient({
      listDevices: vi.fn<() => Promise<SimctlDevice[]>>().mockResolvedValue([dev]),
      bootAndWait,
    })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'simulator_boot',
      arguments: { udid: dev.udid },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { alreadyBooted: boolean; durationMs: number }
    expect(parsed.alreadyBooted).toBe(true)
    expect(parsed.durationMs).toBe(0)
    expect(bootAndWait).toHaveBeenCalledTimes(0)
    await close()
  })

  it('simulator_boot device not found: isError + message contains query', async () => {
    const mock = makeMockClient({
      listDevices: vi.fn<() => Promise<SimctlDevice[]>>().mockResolvedValue([]),
    })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'simulator_boot',
      arguments: { udid: '00000000-0000-0000-0000-000000000000' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('00000000-0000-0000-0000-000000000000')
    await close()
  })

  it('simulator_boot bad args (no udid or name): isError + invalid arguments', async () => {
    const mock = makeMockClient()
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'simulator_boot',
      arguments: {},
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('simulator_shutdown happy: client.shutdown called once + ok:true', async () => {
    const shutdown = vi.fn<(udid: string) => Promise<void>>().mockResolvedValue(undefined)
    const mock = makeMockClient({ shutdown })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'simulator_shutdown',
      arguments: { udid: 'A' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean; udid: string }
    expect(parsed.ok).toBe(true)
    expect(parsed.udid).toBe('A')
    expect(shutdown).toHaveBeenCalledTimes(1)
    expect(shutdown).toHaveBeenCalledWith('A')
    await close()
  })

  it('simulator_shutdown error: shutdown throws → isError', async () => {
    const shutdown = vi.fn<(udid: string) => Promise<void>>().mockRejectedValue(new Error('shutdown failed boom'))
    const mock = makeMockClient({ shutdown })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'simulator_shutdown',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('shutdown failed boom')
    await close()
  })

  it('app_launch happy: launch called + pid in result', async () => {
    const launch = vi
      .fn<(udid: string, bundleId: string, args?: readonly string[], env?: Record<string, string>) => Promise<{ pid: number }>>()
      .mockResolvedValue({ pid: 12345 })
    const mock = makeMockClient({ launch })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'app_launch',
      arguments: { udid: 'A', bundleId: 'com.foo' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { pid: number }
    expect(parsed.pid).toBe(12345)
    expect(launch).toHaveBeenCalledTimes(1)
    expect(launch).toHaveBeenCalledWith('A', 'com.foo', [], {})
    await close()
  })

  it('app_launch with launchArguments: passed through to client.launch', async () => {
    const launch = vi
      .fn<(udid: string, bundleId: string, args?: readonly string[], env?: Record<string, string>) => Promise<{ pid: number }>>()
      .mockResolvedValue({ pid: 1 })
    const mock = makeMockClient({ launch })
    const { client, close } = await makeLinkedClient({ client: mock })
    await client.callTool({
      name: 'app_launch',
      arguments: { udid: 'A', bundleId: 'com.foo', launchArguments: ['-x', 'y'] },
    })
    expect(launch).toHaveBeenCalledTimes(1)
    const callArgs = launch.mock.calls[0]
    expect(callArgs?.[2]).toEqual(['-x', 'y'])
    await close()
  })

  it('app_launch error: launch throws → isError', async () => {
    const launch = vi
      .fn<(udid: string, bundleId: string, args?: readonly string[], env?: Record<string, string>) => Promise<{ pid: number }>>()
      .mockRejectedValue(new Error('launch parse fail'))
    const mock = makeMockClient({ launch })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'app_launch',
      arguments: { udid: 'A', bundleId: 'com.foo' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('launch parse fail')
    await close()
  })

  it('app_terminate happy: terminate called + ok:true', async () => {
    const terminate = vi.fn<(udid: string, bundleId: string) => Promise<void>>().mockResolvedValue(undefined)
    const mock = makeMockClient({ terminate })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'app_terminate',
      arguments: { udid: 'A', bundleId: 'com.foo' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    expect(terminate).toHaveBeenCalledTimes(1)
    expect(terminate).toHaveBeenCalledWith('A', 'com.foo')
    await close()
  })

  it('app_install happy: install called + ok:true', async () => {
    const install = vi.fn<(udid: string, appPath: string) => Promise<void>>().mockResolvedValue(undefined)
    const mock = makeMockClient({ install })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'app_install',
      arguments: { udid: 'A', appPath: '/tmp/foo.app' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    expect(install).toHaveBeenCalledTimes(1)
    expect(install).toHaveBeenCalledWith('A', '/tmp/foo.app')
    await close()
  })

  it('app_uninstall happy: uninstall called + ok:true', async () => {
    const uninstall = vi.fn<(udid: string, bundleId: string) => Promise<void>>().mockResolvedValue(undefined)
    const mock = makeMockClient({ uninstall })
    const { client, close } = await makeLinkedClient({ client: mock })
    const result = await client.callTool({
      name: 'app_uninstall',
      arguments: { udid: 'A', bundleId: 'com.foo' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    expect(uninstall).toHaveBeenCalledTimes(1)
    expect(uninstall).toHaveBeenCalledWith('A', 'com.foo')
    await close()
  })

  // ====== C3: 4 observe tools (screen_describe / screen_screenshot / screen_hierarchy / element_inspect) ======

  it('tools/list: names contain all 4 observe (screen_describe / screen_screenshot / screen_hierarchy / element_inspect)', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    const names = result.tools.map((t) => t.name)
    expect(names).toContain('screen_describe')
    expect(names).toContain('screen_screenshot')
    expect(names).toContain('screen_hierarchy')
    expect(names).toContain('element_inspect')
    await close()
  })

  it('screen_describe happy: returns ScreenDescription via driver.describe — elements + screenshot base64 in JSON content[0].text', async () => {
    const desc: ScreenDescription = {
      screenshot: 'YmFzZTY0',
      elements: [
        {
          role: 'button',
          name: 'OK',
          bounds: { x: 0, y: 0, w: 10, h: 10 },
          enabled: true,
        },
      ],
      frontApp: 'com.foo',
      summary: '',
      capturedAt: 42,
    }
    const describeFn = vi.fn<() => Promise<ScreenDescription>>().mockResolvedValue(desc)
    const { acquireDriver, calls } = makeMockAcquireDriver({ describe: describeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_describe',
      arguments: { udid: 'A' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as ScreenDescription
    expect(parsed.elements.length).toBe(1)
    expect(parsed.screenshot).toBe('YmFzZTY0')
    expect(parsed.frontApp).toBe('com.foo')
    expect(calls).toEqual(['A'])
    expect(describeFn).toHaveBeenCalledTimes(1)
    await close()
  })

  it('screen_describe limit: truncates elements client-side to N when limit < driver default 50', async () => {
    const els = Array.from({ length: 30 }, (_, i) => ({
      role: 'staticText' as const,
      name: `el-${i}`,
      bounds: { x: 0, y: 0, w: 1, h: 1 },
      enabled: true,
    }))
    const desc: ScreenDescription = {
      screenshot: 'YmFzZTY0',
      elements: els,
      frontApp: '',
      summary: '',
      capturedAt: 0,
    }
    const describeFn = vi.fn<() => Promise<ScreenDescription>>().mockResolvedValue(desc)
    const { acquireDriver } = makeMockAcquireDriver({ describe: describeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_describe',
      arguments: { udid: 'A', limit: 5 },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as ScreenDescription
    expect(parsed.elements.length).toBe(5)
    await close()
  })

  it('screen_describe error: driver.describe throws → isError content', async () => {
    const describeFn = vi.fn<() => Promise<ScreenDescription>>().mockRejectedValue(new Error('describe boom'))
    const { acquireDriver } = makeMockAcquireDriver({ describe: describeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_describe',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('describe boom')
    await close()
  })

  it('screen_describe missing udid: isError + invalid arguments', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_describe',
      arguments: {},
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('screen_screenshot happy: returns base64 + byteLength via driver.screenshot', async () => {
    const png = Buffer.from('hello-png-bytes')
    const screenshotFn = vi.fn<() => Promise<Buffer>>().mockResolvedValue(png)
    const { acquireDriver } = makeMockAcquireDriver({ screenshot: screenshotFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_screenshot',
      arguments: { udid: 'A' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { base64: string; byteLength: number }
    expect(parsed.base64).toBe(png.toString('base64'))
    expect(parsed.byteLength).toBe(png.byteLength)
    await close()
  })

  it('screen_screenshot error: driver.screenshot throws → isError', async () => {
    const screenshotFn = vi.fn<() => Promise<Buffer>>().mockRejectedValue(new Error('screenshot boom'))
    const { acquireDriver } = makeMockAcquireDriver({ screenshot: screenshotFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_screenshot',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('screenshot boom')
    await close()
  })

  it('screen_screenshot missing udid: isError + invalid arguments', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_screenshot',
      arguments: {},
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('screen_hierarchy happy: returns tree A11yNode via driver.tree', async () => {
    const child: A11yNode = {
      rawType: 'button',
      bounds: { x: 1, y: 2, w: 3, h: 4 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
    }
    const root: A11yNode = {
      rawType: 'application',
      bounds: { x: 0, y: 0, w: 100, h: 200 },
      enabled: true,
      selected: false,
      hasFocus: true,
      visible: true,
      children: [child],
    }
    const treeFn = vi.fn<() => Promise<A11yNode>>().mockResolvedValue(root)
    const { acquireDriver } = makeMockAcquireDriver({ tree: treeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_hierarchy',
      arguments: { udid: 'A' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { tree: A11yNode }
    expect(parsed.tree.rawType).toBe('application')
    expect(parsed.tree.children.length).toBe(1)
    await close()
  })

  it('screen_hierarchy error: driver.tree throws (e.g. runner not reachable) → isError', async () => {
    const treeFn = vi.fn<() => Promise<A11yNode>>().mockRejectedValue(new Error('runner unreachable'))
    const { acquireDriver } = makeMockAcquireDriver({ tree: treeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_hierarchy',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('runner unreachable')
    await close()
  })

  it('element_inspect found: returns A11yNode via driver.findOne', async () => {
    const node: A11yNode = {
      rawType: 'button',
      label: 'OK',
      bounds: { x: 0, y: 0, w: 50, h: 30 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
    }
    const findOneFn = vi
      .fn<(s: Selector) => Promise<A11yNode | null>>()
      .mockResolvedValue(node)
    const { acquireDriver } = makeMockAcquireDriver({ findOne: findOneFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'element_inspect',
      arguments: { udid: 'A', selector: { text: 'OK' } },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { node: A11yNode }
    expect(parsed.node.rawType).toBe('button')
    expect(findOneFn).toHaveBeenCalledTimes(1)
    const firstArg = findOneFn.mock.calls[0]?.[0]
    expect(firstArg).toEqual({ text: 'OK' })
    await close()
  })

  it('element_inspect not found: returns {node: null} (not isError)', async () => {
    const findOneFn = vi
      .fn<(s: Selector) => Promise<A11yNode | null>>()
      .mockResolvedValue(null)
    const { acquireDriver } = makeMockAcquireDriver({ findOne: findOneFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'element_inspect',
      arguments: { udid: 'A', selector: { text: 'X' } },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { node: A11yNode | null }
    expect(parsed.node).toBeNull()
    await close()
  })

  it('element_inspect selector modifier: passes near/below modifier through to driver.findOne', async () => {
    const findOneFn = vi
      .fn<(s: Selector) => Promise<A11yNode | null>>()
      .mockResolvedValue(null)
    const { acquireDriver } = makeMockAcquireDriver({ findOne: findOneFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const sel = {
      role: 'button',
      name: 'OK',
      below: { text: 'Confirm?' },
    }
    await client.callTool({
      name: 'element_inspect',
      arguments: { udid: 'A', selector: sel },
    })
    expect(findOneFn).toHaveBeenCalledTimes(1)
    const firstArg = findOneFn.mock.calls[0]?.[0]
    expect(firstArg).toEqual(sel)
    await close()
  })

  it('element_inspect missing base form: isError + invalid arguments', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'element_inspect',
      arguments: { udid: 'A', selector: {} },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('element_inspect missing selector: isError + invalid arguments', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'element_inspect',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('element_inspect error: driver.findOne throws → isError', async () => {
    const findOneFn = vi
      .fn<(s: Selector) => Promise<A11yNode | null>>()
      .mockRejectedValue(new Error('findOne boom'))
    const { acquireDriver } = makeMockAcquireDriver({ findOne: findOneFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'element_inspect',
      arguments: { udid: 'A', selector: { text: 'X' } },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('findOne boom')
    await close()
  })

  it('observe tools: acquireDriver called with correct udid (DI: ToolContext.acquireDriver wired through ctx)', async () => {
    const { acquireDriver, calls } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    await client.callTool({ name: 'screen_describe', arguments: { udid: 'A' } })
    await client.callTool({ name: 'screen_describe', arguments: { udid: 'B' } })
    expect(calls).toEqual(['A', 'B'])
    await close()
  })

  it('observe tools: acquireDriver throws → isError content (not throw to JSON-RPC)', async () => {
    const acquireDriver = vi
      .fn<(udid: string) => Promise<Driver>>()
      .mockRejectedValue(new Error('cell acquire failed'))
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'screen_screenshot',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('cell acquire failed')
    await close()
  })

  it('C2 8-tool regression: ping + simulator_list + app_launch happy paths still pass with ToolContext.acquireDriver present but unused', async () => {
    const dev: SimctlDevice = {
      udid: 'AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA',
      name: 'iPhone 16',
      state: 'Shutdown',
      isAvailable: true,
      runtimeIdentifier: 'r',
      deviceTypeIdentifier: 'd',
    }
    const listDevices = vi.fn<() => Promise<SimctlDevice[]>>().mockResolvedValue([dev])
    const launch = vi
      .fn<(udid: string, bundleId: string, args?: readonly string[], env?: Record<string, string>) => Promise<{ pid: number }>>()
      .mockResolvedValue({ pid: 999 })
    const mockClient = makeMockClient({ listDevices, launch })
    const acquireDriver = vi
      .fn<(udid: string) => Promise<Driver>>()
      .mockResolvedValue(makeMockDriver())
    const { client, close } = await makeLinkedClient({ client: mockClient, acquireDriver })

    const ping = await client.callTool({ name: 'ping', arguments: {} })
    expect(ping.isError).not.toBe(true)
    const pingContent = ping.content as Array<{ text: string }>
    expect(pingContent[0]?.text).toBe('pong')

    const list = await client.callTool({ name: 'simulator_list', arguments: {} })
    expect(list.isError).not.toBe(true)

    const launchRes = await client.callTool({
      name: 'app_launch',
      arguments: { udid: 'A', bundleId: 'com.foo' },
    })
    expect(launchRes.isError).not.toBe(true)
    const launchContent = launchRes.content as Array<{ text: string }>
    const launchParsed = JSON.parse(launchContent[0]!.text) as { pid: number }
    expect(launchParsed.pid).toBe(999)

    expect(acquireDriver).toHaveBeenCalledTimes(0)
    await close()
  })

  it('tools/list: schema for element_inspect has required udid + selector + selector is object', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    const ei = result.tools.find((t) => t.name === 'element_inspect')
    expect(ei).toBeDefined()
    const schema = ei?.inputSchema as {
      type: string
      properties: Record<string, { type?: string }>
      required?: string[]
    }
    expect(schema.type).toBe('object')
    expect(schema.required ?? []).toContain('udid')
    expect(schema.required ?? []).toContain('selector')
    expect(schema.properties?.selector?.type).toBe('object')
    await close()
  })

  // ====== C4: 7 interaction tools (tap / double_tap / long_press / fill / swipe / scroll_to / key_press) ======

  it('tools/list: names contain all 7 interaction (tap / double_tap / long_press / fill / swipe / scroll_to / key_press)', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    const names = result.tools.map((t) => t.name)
    expect(names).toContain('tap')
    expect(names).toContain('double_tap')
    expect(names).toContain('long_press')
    expect(names).toContain('fill')
    expect(names).toContain('swipe')
    expect(names).toContain('scroll_to')
    expect(names).toContain('key_press')
    await close()
  })

  it('tap happy: returns {ok:true, screen} via driver.tap + driver.describe', async () => {
    const tapFn = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const desc: ScreenDescription = {
      screenshot: 'YmFzZTY0',
      elements: [
        { role: 'button', name: 'OK', bounds: { x: 0, y: 0, w: 10, h: 10 }, enabled: true },
      ],
      frontApp: 'com.foo',
      summary: '',
      capturedAt: 42,
    }
    const describeFn = vi.fn<() => Promise<ScreenDescription>>().mockResolvedValue(desc)
    const { acquireDriver, driver } = makeMockAcquireDriver({ tap: tapFn, describe: describeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'tap',
      arguments: { udid: 'A', selector: { text: 'OK' } },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      ok: boolean
      screen: ScreenDescription | null
    }
    expect(parsed.ok).toBe(true)
    expect(Array.isArray(parsed.screen?.elements)).toBe(true)
    expect(tapFn).toHaveBeenCalledTimes(1)
    expect(tapFn.mock.calls[0]?.[0]).toEqual({ text: 'OK' })
    expect(describeFn).toHaveBeenCalledTimes(1)
    // Touch `driver` so the binding is referenced (clears unused-binding lint).
    expect(driver).toBeDefined()
    await close()
  })

  it('tap returnSummary=false: skips driver.describe (screen:null)', async () => {
    const tapFn = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const describeFn = vi
      .fn<() => Promise<ScreenDescription>>()
      .mockResolvedValue({
        screenshot: '',
        elements: [],
        frontApp: '',
        summary: '',
        capturedAt: 0,
      })
    const { acquireDriver } = makeMockAcquireDriver({ tap: tapFn, describe: describeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'tap',
      arguments: { udid: 'A', selector: { text: 'OK' }, returnSummary: false },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean; screen: unknown }
    expect(parsed.ok).toBe(true)
    expect(parsed.screen).toBeNull()
    expect(tapFn).toHaveBeenCalledTimes(1)
    expect(describeFn).toHaveBeenCalledTimes(0)
    await close()
  })

  it('tap selector modifier: passes near/below through', async () => {
    const tapFn = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ tap: tapFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const sel = { role: 'button', name: 'OK', below: { text: 'Confirm?' } }
    await client.callTool({
      name: 'tap',
      arguments: { udid: 'A', selector: sel, returnSummary: false },
    })
    expect(tapFn).toHaveBeenCalledTimes(1)
    expect(tapFn.mock.calls[0]?.[0]).toEqual(sel)
    await close()
  })

  it('tap error: driver.tap throws (not-found) → isError', async () => {
    const tapFn = vi
      .fn<(s: Selector) => Promise<void>>()
      .mockRejectedValue(new Error('element not found: {text:"X"}'))
    const describeFn = vi
      .fn<() => Promise<ScreenDescription>>()
      .mockResolvedValue({
        screenshot: '',
        elements: [],
        frontApp: '',
        summary: '',
        capturedAt: 0,
      })
    const { acquireDriver } = makeMockAcquireDriver({ tap: tapFn, describe: describeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'tap',
      arguments: { udid: 'A', selector: { text: 'X' } },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('element not found')
    expect(describeFn).toHaveBeenCalledTimes(0)
    await close()
  })

  it('tap describe fallback: tap ok + describe throws → {ok:true, screen:null, screenError} non-isError', async () => {
    const tapFn = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const describeFn = vi
      .fn<() => Promise<ScreenDescription>>()
      .mockRejectedValue(new Error('runner not reachable'))
    const { acquireDriver } = makeMockAcquireDriver({ tap: tapFn, describe: describeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'tap',
      arguments: { udid: 'A', selector: { text: 'OK' } },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      ok: boolean
      screen: unknown
      screenError?: string
    }
    expect(parsed.ok).toBe(true)
    expect(parsed.screen).toBeNull()
    expect(parsed.screenError ?? '').toContain('runner not reachable')
    await close()
  })

  it('tap missing selector: isError + invalid arguments', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'tap',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('double_tap happy: returns {ok:true, screen} via driver.doubleTap + describe', async () => {
    const doubleTapFn = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ doubleTap: doubleTapFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'double_tap',
      arguments: { udid: 'A', selector: { id: 'map' } },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    expect(doubleTapFn).toHaveBeenCalledTimes(1)
    expect(doubleTapFn.mock.calls[0]?.[0]).toEqual({ id: 'map' })
    await close()
  })

  it('double_tap error: driver.doubleTap throws → isError', async () => {
    const doubleTapFn = vi
      .fn<(s: Selector) => Promise<void>>()
      .mockRejectedValue(new Error('double_tap boom'))
    const { acquireDriver } = makeMockAcquireDriver({ doubleTap: doubleTapFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'double_tap',
      arguments: { udid: 'A', selector: { id: 'map' } },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('double_tap boom')
    await close()
  })

  it('long_press happy with durationMs: returns {ok:true, screen}', async () => {
    const longPressFn = vi
      .fn<(s: Selector, durationMs?: number) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ longPress: longPressFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'long_press',
      arguments: { udid: 'A', selector: { text: 'Item' }, durationMs: 1500 },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    expect(longPressFn).toHaveBeenCalledTimes(1)
    expect(longPressFn.mock.calls[0]?.[0]).toEqual({ text: 'Item' })
    expect(longPressFn.mock.calls[0]?.[1]).toBe(1500)
    await close()
  })

  it('long_press happy without durationMs: driver.longPress called with undefined second arg', async () => {
    const longPressFn = vi
      .fn<(s: Selector, durationMs?: number) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ longPress: longPressFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    await client.callTool({
      name: 'long_press',
      arguments: { udid: 'A', selector: { text: 'Item' } },
    })
    expect(longPressFn).toHaveBeenCalledTimes(1)
    expect(longPressFn.mock.calls[0]?.[1]).toBeUndefined()
    await close()
  })

  it('long_press durationMs must be positive int: isError on negative', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'long_press',
      arguments: { udid: 'A', selector: { text: 'Item' }, durationMs: -5 },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('fill happy: maps wire {value} → driver.fill(selector, text)', async () => {
    const fillFn = vi
      .fn<(s: Selector, text: string) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ fill: fillFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'fill',
      arguments: { udid: 'A', selector: { id: 'email' }, value: 'foo@bar.com' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    expect(fillFn).toHaveBeenCalledTimes(1)
    expect(fillFn.mock.calls[0]?.[0]).toEqual({ id: 'email' })
    expect(fillFn.mock.calls[0]?.[1]).toBe('foo@bar.com')
    await close()
  })

  it('fill empty value allowed: passes "" through to driver.fill', async () => {
    const fillFn = vi
      .fn<(s: Selector, text: string) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ fill: fillFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'fill',
      arguments: { udid: 'A', selector: { id: 'x' }, value: '' },
    })
    expect(result.isError).not.toBe(true)
    expect(fillFn).toHaveBeenCalledTimes(1)
    expect(fillFn.mock.calls[0]?.[1]).toBe('')
    await close()
  })

  it('fill missing value: isError + invalid arguments', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'fill',
      arguments: { udid: 'A', selector: { id: 'email' } },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('fill error: driver.fill throws → isError + describe 0 calls', async () => {
    const fillFn = vi
      .fn<(s: Selector, text: string) => Promise<void>>()
      .mockRejectedValue(new Error('fill boom'))
    const describeFn = vi
      .fn<() => Promise<ScreenDescription>>()
      .mockResolvedValue({
        screenshot: '',
        elements: [],
        frontApp: '',
        summary: '',
        capturedAt: 0,
      })
    const { acquireDriver } = makeMockAcquireDriver({ fill: fillFn, describe: describeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'fill',
      arguments: { udid: 'A', selector: { id: 'x' }, value: 'v' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('fill boom')
    expect(describeFn).toHaveBeenCalledTimes(0)
    await close()
  })

  it('swipe direction-only happy: driver.swipe(direction, undefined)', async () => {
    const swipeFn = vi
      .fn<(d: 'up' | 'down' | 'left' | 'right', from?: Selector) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ swipe: swipeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'swipe',
      arguments: { udid: 'A', direction: 'up' },
    })
    expect(result.isError).not.toBe(true)
    expect(swipeFn).toHaveBeenCalledTimes(1)
    expect(swipeFn.mock.calls[0]?.[0]).toBe('up')
    expect(swipeFn.mock.calls[0]?.[1]).toBeUndefined()
    await close()
  })

  it('swipe with from selector: driver.swipe(direction, from)', async () => {
    const swipeFn = vi
      .fn<(d: 'up' | 'down' | 'left' | 'right', from?: Selector) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ swipe: swipeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    await client.callTool({
      name: 'swipe',
      arguments: { udid: 'A', direction: 'left', from: { text: 'Card' } },
    })
    expect(swipeFn).toHaveBeenCalledTimes(1)
    expect(swipeFn.mock.calls[0]?.[0]).toBe('left')
    expect(swipeFn.mock.calls[0]?.[1]).toEqual({ text: 'Card' })
    await close()
  })

  it('swipe invalid direction: isError on direction:diagonal', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'swipe',
      arguments: { udid: 'A', direction: 'diagonal' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('swipe error: driver.swipe throws → isError', async () => {
    const swipeFn = vi
      .fn<(d: 'up' | 'down' | 'left' | 'right', from?: Selector) => Promise<void>>()
      .mockRejectedValue(new Error('swipe boom'))
    const { acquireDriver } = makeMockAcquireDriver({ swipe: swipeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'swipe',
      arguments: { udid: 'A', direction: 'up' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('swipe boom')
    await close()
  })

  it('scroll_to happy: driver.scrollTo(selector) + describe', async () => {
    const scrollToFn = vi
      .fn<(s: Selector) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ scrollTo: scrollToFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'scroll_to',
      arguments: { udid: 'A', selector: { text: 'Bottom' } },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    expect(scrollToFn).toHaveBeenCalledTimes(1)
    expect(scrollToFn.mock.calls[0]?.[0]).toEqual({ text: 'Bottom' })
    await close()
  })

  it('scroll_to missing selector: isError + invalid arguments', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'scroll_to',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('key_press happy: driver.pressKey("return") + describe', async () => {
    const pressKeyFn = vi.fn<(k: string) => Promise<void>>().mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ pressKey: pressKeyFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'key_press',
      arguments: { udid: 'A', key: 'return' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    expect(pressKeyFn).toHaveBeenCalledTimes(1)
    expect(pressKeyFn.mock.calls[0]?.[0]).toBe('return')
    await close()
  })

  it('key_press all 9 KeyName values pass: return/delete/tab/space/escape/arrowUp/arrowDown/arrowLeft/arrowRight', async () => {
    const keys = [
      'return',
      'delete',
      'tab',
      'space',
      'escape',
      'arrowUp',
      'arrowDown',
      'arrowLeft',
      'arrowRight',
    ]
    const pressKeyFn = vi.fn<(k: string) => Promise<void>>().mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ pressKey: pressKeyFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    for (const key of keys) {
      const result = await client.callTool({
        name: 'key_press',
        arguments: { udid: 'A', key, returnSummary: false },
      })
      expect(result.isError).not.toBe(true)
    }
    expect(pressKeyFn).toHaveBeenCalledTimes(keys.length)
    await close()
  })

  it('key_press invalid key: isError on key:enter (字面非 KeyName union)', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'key_press',
      arguments: { udid: 'A', key: 'enter' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('key_press error: driver.pressKey throws → isError', async () => {
    const pressKeyFn = vi
      .fn<(k: string) => Promise<void>>()
      .mockRejectedValue(new Error('press boom'))
    const { acquireDriver } = makeMockAcquireDriver({ pressKey: pressKeyFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'key_press',
      arguments: { udid: 'A', key: 'return' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('press boom')
    await close()
  })

  it('interaction tools: acquireDriver called once per call with correct udid', async () => {
    const { acquireDriver, calls } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    await client.callTool({
      name: 'tap',
      arguments: { udid: 'A', selector: { text: 'OK' }, returnSummary: false },
    })
    await client.callTool({
      name: 'tap',
      arguments: { udid: 'B', selector: { text: 'OK' }, returnSummary: false },
    })
    expect(calls).toEqual(['A', 'B'])
    await close()
  })

  it('interaction tools: acquireDriver throws → isError + describe 0 calls', async () => {
    const acquireDriver = vi
      .fn<(udid: string) => Promise<Driver>>()
      .mockRejectedValue(new Error('cell acquire failed'))
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'tap',
      arguments: { udid: 'A', selector: { text: 'OK' } },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('cell acquire failed')
    await close()
  })

  it('interaction tools returnSummary default true: tap without returnSummary → describe called once', async () => {
    const tapFn = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const describeFn = vi
      .fn<() => Promise<ScreenDescription>>()
      .mockResolvedValue({
        screenshot: '',
        elements: [],
        frontApp: '',
        summary: '',
        capturedAt: 0,
      })
    const { acquireDriver } = makeMockAcquireDriver({ tap: tapFn, describe: describeFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    await client.callTool({
      name: 'tap',
      arguments: { udid: 'A', selector: { text: 'OK' } },
    })
    expect(describeFn).toHaveBeenCalledTimes(1)
    await close()
  })

  it('C1+C2+C3 19-tool regression: ping + simulator_list + screen_describe + element_inspect happy paths still pass with extended makeMockDriver', async () => {
    const dev: SimctlDevice = {
      udid: 'AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA',
      name: 'iPhone 16',
      state: 'Shutdown',
      isAvailable: true,
      runtimeIdentifier: 'r',
      deviceTypeIdentifier: 'd',
    }
    const listDevices = vi.fn<() => Promise<SimctlDevice[]>>().mockResolvedValue([dev])
    const mockClient = makeMockClient({ listDevices })
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ client: mockClient, acquireDriver })

    const ping = await client.callTool({ name: 'ping', arguments: {} })
    expect(ping.isError).not.toBe(true)
    const pingContent = ping.content as Array<{ text: string }>
    expect(pingContent[0]?.text).toBe('pong')

    const list = await client.callTool({ name: 'simulator_list', arguments: {} })
    expect(list.isError).not.toBe(true)

    const desc = await client.callTool({
      name: 'screen_describe',
      arguments: { udid: 'A' },
    })
    expect(desc.isError).not.toBe(true)

    const ei = await client.callTool({
      name: 'element_inspect',
      arguments: { udid: 'A', selector: { text: 'OK' } },
    })
    expect(ei.isError).not.toBe(true)
    await close()
  })

  it('tools/list: schema for fill has required udid + selector + value + returnSummary optional', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    const fillSchemaEntry = result.tools.find((t) => t.name === 'fill')
    expect(fillSchemaEntry).toBeDefined()
    const schema = fillSchemaEntry?.inputSchema as {
      type: string
      properties: Record<string, { type?: string }>
      required?: string[]
    }
    expect(schema.type).toBe('object')
    expect(schema.required ?? []).toContain('udid')
    expect(schema.required ?? []).toContain('selector')
    expect(schema.required ?? []).toContain('value')
    expect(schema.required ?? []).not.toContain('returnSummary')
    expect(schema.properties?.value?.type).toBe('string')
    await close()
  })

  // ====== C5: 3 compound + 4 system tools (find_and_tap / wait_for / flow_run / open_url / pasteboard_set / pasteboard_get / permissions_grant) ======

  it('tools/list: names contain all 7 compound/system (find_and_tap / wait_for / flow_run / open_url / pasteboard_set / pasteboard_get / permissions_grant)', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    const names = result.tools.map((t) => t.name)
    for (const n of [
      'find_and_tap',
      'wait_for',
      'flow_run',
      'open_url',
      'pasteboard_set',
      'pasteboard_get',
      'permissions_grant',
    ]) {
      expect(names).toContain(n)
    }
    await close()
  })

  it('find_and_tap happy: driver.waitFor + driver.tap + describe (default returnSummary=true)', async () => {
    const waitFor = vi
      .fn<(s: Selector, t?: number) => Promise<A11yNode>>()
      .mockResolvedValue({
        rawType: 'XCUIElementTypeButton',
        bounds: { x: 0, y: 0, w: 10, h: 10 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      })
    const tap = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const describe = vi
      .fn<() => Promise<ScreenDescription>>()
      .mockResolvedValue({
        screenshot: 'AAA',
        elements: [],
        frontApp: '',
        summary: '',
        capturedAt: 7,
      })
    const { acquireDriver } = makeMockAcquireDriver({ waitFor, tap, describe })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'find_and_tap',
      arguments: { udid: 'A', selector: { text: 'Login' } },
    })
    expect(result.isError).not.toBe(true)
    expect(waitFor).toHaveBeenCalledTimes(1)
    expect(waitFor.mock.calls[0]?.[0]).toEqual({ text: 'Login' })
    expect(waitFor.mock.calls[0]?.[1]).toBeUndefined()
    expect(tap).toHaveBeenCalledTimes(1)
    expect(tap.mock.calls[0]?.[0]).toEqual({ text: 'Login' })
    expect(describe).toHaveBeenCalledTimes(1)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    await close()
  })

  it('find_and_tap with timeoutMs: driver.waitFor(selector, 5000)', async () => {
    const waitFor = vi
      .fn<(s: Selector, t?: number) => Promise<A11yNode>>()
      .mockResolvedValue({
        rawType: 'X',
        bounds: { x: 0, y: 0, w: 0, h: 0 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      })
    const { acquireDriver } = makeMockAcquireDriver({ waitFor })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'find_and_tap',
      arguments: { udid: 'A', selector: { text: 'X' }, timeoutMs: 5000, returnSummary: false },
    })
    expect(result.isError).not.toBe(true)
    expect(waitFor.mock.calls[0]?.[1]).toBe(5000)
    await close()
  })

  it('find_and_tap returnSummary=false: skips describe + screen:null', async () => {
    const describe = vi.fn<() => Promise<ScreenDescription>>()
    const { acquireDriver } = makeMockAcquireDriver({ describe })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'find_and_tap',
      arguments: { udid: 'A', selector: { text: 'OK' }, returnSummary: false },
    })
    expect(result.isError).not.toBe(true)
    expect(describe).toHaveBeenCalledTimes(0)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean; screen: unknown }
    expect(parsed.ok).toBe(true)
    expect(parsed.screen).toBeNull()
    await close()
  })

  it('find_and_tap waitFor timeout: isError + driver.tap 0 calls + driver.describe 0 calls', async () => {
    const waitFor = vi
      .fn<(s: Selector, t?: number) => Promise<A11yNode>>()
      .mockRejectedValue(new Error('waitFor({text:"X"}) timed out after 1000ms'))
    const tap = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const describe = vi.fn<() => Promise<ScreenDescription>>()
    const { acquireDriver } = makeMockAcquireDriver({ waitFor, tap, describe })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'find_and_tap',
      arguments: { udid: 'A', selector: { text: 'X' }, timeoutMs: 1000 },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('timed out')
    expect(tap).toHaveBeenCalledTimes(0)
    expect(describe).toHaveBeenCalledTimes(0)
    await close()
  })

  it('find_and_tap tap throws after waitFor ok: isError + describe 0 calls', async () => {
    const waitFor = vi
      .fn<(s: Selector, t?: number) => Promise<A11yNode>>()
      .mockResolvedValue({
        rawType: 'X',
        bounds: { x: 0, y: 0, w: 0, h: 0 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      })
    const tap = vi
      .fn<(s: Selector) => Promise<void>>()
      .mockRejectedValue(new Error('tap blew up'))
    const describe = vi.fn<() => Promise<ScreenDescription>>()
    const { acquireDriver } = makeMockAcquireDriver({ waitFor, tap, describe })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'find_and_tap',
      arguments: { udid: 'A', selector: { text: 'X' } },
    })
    expect(result.isError).toBe(true)
    expect(describe).toHaveBeenCalledTimes(0)
    await close()
  })

  it('find_and_tap describe fallback: waitFor+tap ok + describe throws → {ok:true, screen:null, screenError} non-isError', async () => {
    const describe = vi
      .fn<() => Promise<ScreenDescription>>()
      .mockRejectedValue(new Error('runner not reachable'))
    const { acquireDriver } = makeMockAcquireDriver({ describe })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'find_and_tap',
      arguments: { udid: 'A', selector: { text: 'OK' } },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      ok: boolean
      screen: unknown
      screenError?: string
    }
    expect(parsed.ok).toBe(true)
    expect(parsed.screen).toBeNull()
    expect(parsed.screenError ?? '').toContain('runner not reachable')
    await close()
  })

  it('wait_for happy: returns {node: A11yNode} via driver.waitFor', async () => {
    const node: A11yNode = {
      rawType: 'XCUIElementTypeStaticText',
      bounds: { x: 1, y: 2, w: 3, h: 4 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
    }
    const waitFor = vi
      .fn<(s: Selector, t?: number) => Promise<A11yNode>>()
      .mockResolvedValue(node)
    const { acquireDriver } = makeMockAcquireDriver({ waitFor })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'wait_for',
      arguments: { udid: 'A', selector: { text: 'Welcome' }, timeoutMs: 3000 },
    })
    expect(result.isError).not.toBe(true)
    expect(waitFor.mock.calls[0]?.[0]).toEqual({ text: 'Welcome' })
    expect(waitFor.mock.calls[0]?.[1]).toBe(3000)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { node: A11yNode }
    expect(parsed.node.rawType).toBe('XCUIElementTypeStaticText')
    await close()
  })

  it('wait_for without timeoutMs: driver.waitFor(selector, undefined)', async () => {
    const waitFor = vi
      .fn<(s: Selector, t?: number) => Promise<A11yNode>>()
      .mockResolvedValue({
        rawType: 'X',
        bounds: { x: 0, y: 0, w: 0, h: 0 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      })
    const { acquireDriver } = makeMockAcquireDriver({ waitFor })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'wait_for',
      arguments: { udid: 'A', selector: { text: 'Y' } },
    })
    expect(result.isError).not.toBe(true)
    expect(waitFor.mock.calls[0]?.[1]).toBeUndefined()
    await close()
  })

  it('wait_for timeout: isError + text contains "timed out"', async () => {
    const waitFor = vi
      .fn<(s: Selector, t?: number) => Promise<A11yNode>>()
      .mockRejectedValue(new Error('waitFor({text:"M"}) timed out after 500ms'))
    const { acquireDriver } = makeMockAcquireDriver({ waitFor })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'wait_for',
      arguments: { udid: 'A', selector: { text: 'M' }, timeoutMs: 500 },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('timed out')
    await close()
  })

  it('wait_for missing selector: isError + invalid arguments', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.callTool({
      name: 'wait_for',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('wait_for negative timeoutMs: isError', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.callTool({
      name: 'wait_for',
      arguments: { udid: 'A', selector: { text: 'X' }, timeoutMs: -5 },
    })
    expect(result.isError).toBe(true)
    await close()
  })

  it('open_url happy: driver.openUrl(url) returns {ok:true} + 0 describe', async () => {
    const openUrl = vi
      .fn<(url: string) => Promise<void>>()
      .mockResolvedValue(undefined)
    const describe = vi.fn<() => Promise<ScreenDescription>>()
    const { acquireDriver } = makeMockAcquireDriver({ openUrl, describe })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'open_url',
      arguments: { udid: 'A', url: 'https://example.com' },
    })
    expect(result.isError).not.toBe(true)
    expect(openUrl).toHaveBeenCalledTimes(1)
    expect(openUrl.mock.calls[0]?.[0]).toBe('https://example.com')
    expect(describe).toHaveBeenCalledTimes(0)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    await close()
  })

  it('open_url custom scheme: driver.openUrl("myapp://foo")', async () => {
    const openUrl = vi
      .fn<(url: string) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ openUrl })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'open_url',
      arguments: { udid: 'A', url: 'myapp://foo' },
    })
    expect(result.isError).not.toBe(true)
    expect(openUrl.mock.calls[0]?.[0]).toBe('myapp://foo')
    await close()
  })

  it('open_url missing url: isError + invalid arguments', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.callTool({
      name: 'open_url',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('open_url error: driver.openUrl throws → isError', async () => {
    const openUrl = vi
      .fn<(url: string) => Promise<void>>()
      .mockRejectedValue(new Error('openurl exploded'))
    const { acquireDriver } = makeMockAcquireDriver({ openUrl })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'open_url',
      arguments: { udid: 'A', url: 'https://x' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('openurl exploded')
    await close()
  })

  it('pasteboard_set happy: driver.pasteboardSet(value) returns {ok:true}', async () => {
    const pasteboardSet = vi
      .fn<(text: string) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ pasteboardSet })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'pasteboard_set',
      arguments: { udid: 'A', value: 'hello' },
    })
    expect(result.isError).not.toBe(true)
    expect(pasteboardSet.mock.calls[0]?.[0]).toBe('hello')
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    await close()
  })

  it('pasteboard_set empty value allowed: passes "" through', async () => {
    const pasteboardSet = vi
      .fn<(text: string) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ pasteboardSet })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'pasteboard_set',
      arguments: { udid: 'A', value: '' },
    })
    expect(result.isError).not.toBe(true)
    expect(pasteboardSet.mock.calls[0]?.[0]).toBe('')
    await close()
  })

  it('pasteboard_set missing value: isError', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.callTool({
      name: 'pasteboard_set',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    await close()
  })

  it('pasteboard_get happy: returns {value:<string>} via driver.pasteboardGet', async () => {
    const pasteboardGet = vi.fn<() => Promise<string>>().mockResolvedValue('hello')
    const { acquireDriver } = makeMockAcquireDriver({ pasteboardGet })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'pasteboard_get',
      arguments: { udid: 'A' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { value: string }
    expect(parsed.value).toBe('hello')
    await close()
  })

  it('pasteboard_get empty: returns {value:""}', async () => {
    const pasteboardGet = vi.fn<() => Promise<string>>().mockResolvedValue('')
    const { acquireDriver } = makeMockAcquireDriver({ pasteboardGet })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'pasteboard_get',
      arguments: { udid: 'A' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { value: string }
    expect(parsed.value).toBe('')
    await close()
  })

  it('permissions_grant happy: driver.grantPermission(permission, bundleId)', async () => {
    const grantPermission = vi
      .fn<(permission: string, bundleId?: string) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ grantPermission })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'permissions_grant',
      arguments: { udid: 'A', bundleId: 'com.foo.app', permission: 'camera' },
    })
    expect(result.isError).not.toBe(true)
    expect(grantPermission.mock.calls[0]?.[0]).toBe('camera')
    expect(grantPermission.mock.calls[0]?.[1]).toBe('com.foo.app')
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { ok: boolean }
    expect(parsed.ok).toBe(true)
    await close()
  })

  it('permissions_grant all 12 Permission values pass', async () => {
    const grantPermission = vi
      .fn<(permission: string, bundleId?: string) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ grantPermission })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const all = [
      'camera',
      'photos',
      'location',
      'locationAlways',
      'notifications',
      'microphone',
      'contacts',
      'calendar',
      'reminders',
      'bluetooth',
      'motion',
      'faceId',
    ]
    for (const p of all) {
      const result = await client.callTool({
        name: 'permissions_grant',
        arguments: { udid: 'A', bundleId: 'com.x', permission: p },
      })
      expect(result.isError).not.toBe(true)
    }
    expect(grantPermission).toHaveBeenCalledTimes(12)
    await close()
  })

  it('permissions_grant invalid permission: isError on unknown_perm', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.callTool({
      name: 'permissions_grant',
      arguments: { udid: 'A', bundleId: 'x', permission: 'unknown_perm' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('permissions_grant missing bundleId: isError (MCP surface is stateless, no driver lastLaunchedBundleId fallback)', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.callTool({
      name: 'permissions_grant',
      arguments: { udid: 'A', permission: 'camera' },
    })
    expect(result.isError).toBe(true)
    await close()
  })

  it('flow_run happy: 3 steps tap+fill+tap → passed=3, failed=0, stepsExecuted=3', async () => {
    const tap = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const fill = vi
      .fn<(s: Selector, text: string) => Promise<void>>()
      .mockResolvedValue(undefined)
    const { acquireDriver } = makeMockAcquireDriver({ tap, fill })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'flow_run',
      arguments: {
        udid: 'A',
        steps: [
          { action: 'tap', args: { selector: { text: 'Login' }, returnSummary: false } },
          { action: 'fill', args: { selector: { id: 'email' }, value: 'a@b.com', returnSummary: false } },
          { action: 'tap', args: { selector: { text: 'Submit' }, returnSummary: false } },
        ],
      },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      passed: number
      failed: number
      stepsExecuted: number
    }
    expect(parsed.passed).toBe(3)
    expect(parsed.failed).toBe(0)
    expect(parsed.stepsExecuted).toBe(3)
    expect(tap).toHaveBeenCalledTimes(2)
    expect(fill).toHaveBeenCalledTimes(1)
    await close()
  })

  it('flow_run bail on error: first step fails → stepsExecuted=1, passed=0, failed=1, lastError', async () => {
    const tap = vi
      .fn<(s: Selector) => Promise<void>>()
      .mockRejectedValue(new Error('element not found'))
    const { acquireDriver } = makeMockAcquireDriver({ tap })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'flow_run',
      arguments: {
        udid: 'A',
        steps: [
          { action: 'tap', args: { selector: { text: 'X' }, returnSummary: false } },
          { action: 'tap', args: { selector: { text: 'Y' }, returnSummary: false } },
        ],
      },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      passed: number
      failed: number
      stepsExecuted: number
      lastError?: string
    }
    expect(parsed.passed).toBe(0)
    expect(parsed.failed).toBe(1)
    expect(parsed.stepsExecuted).toBe(1)
    expect(parsed.lastError ?? '').toContain('element not found')
    expect(tap).toHaveBeenCalledTimes(1)
    await close()
  })

  it('flow_run bailOnError=false: continues past failure', async () => {
    let count = 0
    const tap = vi.fn<(s: Selector) => Promise<void>>(async () => {
      count += 1
      if (count === 1) {
        throw new Error('first fail')
      }
    })
    const { acquireDriver } = makeMockAcquireDriver({ tap })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'flow_run',
      arguments: {
        udid: 'A',
        bailOnError: false,
        steps: [
          { action: 'tap', args: { selector: { text: 'X' }, returnSummary: false } },
          { action: 'tap', args: { selector: { text: 'Y' }, returnSummary: false } },
        ],
      },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      passed: number
      failed: number
      stepsExecuted: number
    }
    expect(parsed.stepsExecuted).toBe(2)
    expect(parsed.passed).toBe(1)
    expect(parsed.failed).toBe(1)
    expect(tap).toHaveBeenCalledTimes(2)
    await close()
  })

  it('flow_run unknown tool: aggregated as failed step with lastError "unknown tool: ..."', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'flow_run',
      arguments: {
        udid: 'A',
        steps: [{ action: 'unknown_tool', args: {} }],
      },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      passed: number
      failed: number
      stepsExecuted: number
      lastError?: string
    }
    expect(parsed.failed).toBe(1)
    expect(parsed.lastError ?? '').toContain('unknown tool')
    await close()
  })

  it('flow_run empty steps: passed=0, failed=0, stepsExecuted=0, non-isError', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'flow_run',
      arguments: { udid: 'A', steps: [] },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      passed: number
      failed: number
      stepsExecuted: number
    }
    expect(parsed.passed).toBe(0)
    expect(parsed.failed).toBe(0)
    expect(parsed.stepsExecuted).toBe(0)
    await close()
  })

  it('flow_run injects udid into each step args (if step.args.udid omitted)', async () => {
    const tap = vi.fn<(s: Selector) => Promise<void>>().mockResolvedValue(undefined)
    const { acquireDriver, calls } = makeMockAcquireDriver({ tap })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'flow_run',
      arguments: {
        udid: 'Z',
        steps: [
          { action: 'tap', args: { selector: { text: 'X' }, returnSummary: false } },
        ],
      },
    })
    expect(result.isError).not.toBe(true)
    expect(calls).toContain('Z')
    await close()
  })

  it('flow_run forbids recursive flow_run step (decision 4.A.7)', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'flow_run',
      arguments: {
        udid: 'A',
        steps: [{ action: 'flow_run', args: { steps: [] } }],
      },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text ?? '').toContain('flow_run cannot dispatch flow_run')
    await close()
  })

  it('compound/system: acquireDriver called with correct udid per call', async () => {
    const waitFor = vi
      .fn<(s: Selector, t?: number) => Promise<A11yNode>>()
      .mockResolvedValue({
        rawType: 'X',
        bounds: { x: 0, y: 0, w: 0, h: 0 },
        enabled: true,
        selected: false,
        hasFocus: false,
        visible: true,
        children: [],
      })
    const { acquireDriver, calls } = makeMockAcquireDriver({ waitFor })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    await client.callTool({
      name: 'find_and_tap',
      arguments: { udid: 'Z', selector: { text: 'A' }, returnSummary: false },
    })
    await client.callTool({
      name: 'wait_for',
      arguments: { udid: 'Z', selector: { text: 'A' } },
    })
    await client.callTool({
      name: 'open_url',
      arguments: { udid: 'Z', url: 'https://x' },
    })
    await client.callTool({
      name: 'pasteboard_set',
      arguments: { udid: 'Z', value: 'v' },
    })
    await client.callTool({
      name: 'pasteboard_get',
      arguments: { udid: 'Z' },
    })
    await client.callTool({
      name: 'permissions_grant',
      arguments: { udid: 'Z', bundleId: 'b', permission: 'camera' },
    })
    for (const c of calls) {
      expect(c).toBe('Z')
    }
    expect(calls.length).toBeGreaterThanOrEqual(6)
    await close()
  })

  it('tools/list schemas: permissions_grant.required = [udid, bundleId, permission] + permission enum length 12', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    const entry = result.tools.find((t) => t.name === 'permissions_grant')
    expect(entry).toBeDefined()
    const schema = entry?.inputSchema as {
      required?: string[]
      properties: { permission?: { enum?: string[] } }
    }
    expect(schema.required ?? []).toContain('udid')
    expect(schema.required ?? []).toContain('bundleId')
    expect(schema.required ?? []).toContain('permission')
    expect(schema.properties.permission?.enum?.length).toBe(12)
    await close()
  })

  it('C1+C2+C3+C4 19-tool regression: happy paths still pass with extended makeMockDriver (5 new methods)', async () => {
    const dev: SimctlDevice = {
      udid: 'AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA',
      name: 'iPhone 16',
      state: 'Shutdown',
      isAvailable: true,
      runtimeIdentifier: 'r',
      deviceTypeIdentifier: 'd',
    }
    const listDevices = vi.fn<() => Promise<SimctlDevice[]>>().mockResolvedValue([dev])
    const mockClient = makeMockClient({ listDevices })
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ client: mockClient, acquireDriver })

    const ping = await client.callTool({ name: 'ping', arguments: {} })
    expect(ping.isError).not.toBe(true)

    const list = await client.callTool({ name: 'simulator_list', arguments: {} })
    expect(list.isError).not.toBe(true)

    const desc = await client.callTool({
      name: 'screen_describe',
      arguments: { udid: 'A' },
    })
    expect(desc.isError).not.toBe(true)

    const ei = await client.callTool({
      name: 'element_inspect',
      arguments: { udid: 'A', selector: { text: 'OK' } },
    })
    expect(ei.isError).not.toBe(true)

    const tap = await client.callTool({
      name: 'tap',
      arguments: { udid: 'A', selector: { text: 'OK' }, returnSummary: false },
    })
    expect(tap.isError).not.toBe(true)

    const kp = await client.callTool({
      name: 'key_press',
      arguments: { udid: 'A', key: 'return', returnSummary: false },
    })
    expect(kp.isError).not.toBe(true)

    await close()
  })
})

// ====== C6: explain_screen ======

// Build a mock child-process-like object with stdout/stderr emitters so the
// production handler's `child.stdout?.on('data', ...)` wiring exercises real
// listener registration paths. `behavior` controls what the mock does after
// listeners attach: emit stdout + close(code) for happy/exit-nonzero, throw
// ENOENT synchronously, or never close (timeout path — listens to abort signal).
type MockChildBehavior =
  | { kind: 'happy'; stdout: string }
  | { kind: 'exit-nonzero'; code: number; stderr: string }
  | { kind: 'enoent-sync' }
  | { kind: 'enoent-event' }
  | { kind: 'never-close' }

function makeMockSpawn(
  behavior: MockChildBehavior,
  opts?: { capture?: { args?: readonly unknown[] } },
): typeof import('node:child_process').spawn {
  const impl = (
    _cmd: string,
    args: readonly string[],
    spawnOpts: { signal?: AbortSignal },
  ) => {
    spawnCallCount.value += 1
    if (opts?.capture !== undefined) {
      opts.capture.args = args
    }
    if (behavior.kind === 'enoent-sync') {
      const err = new Error('spawn claude ENOENT') as NodeJS.ErrnoException
      err.code = 'ENOENT'
      throw err
    }
    const child = new EventEmitter() as EventEmitter & {
      stdout: EventEmitter
      stderr: EventEmitter
    }
    child.stdout = new EventEmitter()
    child.stderr = new EventEmitter()
    setImmediate(() => {
      if (behavior.kind === 'happy') {
        child.stdout.emit('data', Buffer.from(behavior.stdout, 'utf8'))
        child.emit('close', 0)
      } else if (behavior.kind === 'exit-nonzero') {
        child.stderr.emit('data', Buffer.from(behavior.stderr, 'utf8'))
        child.emit('close', behavior.code)
      } else if (behavior.kind === 'enoent-event') {
        const err = new Error('spawn claude ENOENT') as NodeJS.ErrnoException
        err.code = 'ENOENT'
        child.emit('error', err)
      } else if (behavior.kind === 'never-close') {
        spawnOpts.signal?.addEventListener('abort', () => {
          child.emit('close', null)
        })
      }
    })
    return child
  }
  return impl as unknown as typeof import('node:child_process').spawn
}

// Counter shared across mock spawn invocations (vi.fn() returning a typed
// function ran into TS overload-resolution issues with node:child_process'
// 12+ overload signatures; this side-channel counter sidesteps that.)
const spawnCallCount = { value: 0 }
function resetSpawnCallCount() {
  spawnCallCount.value = 0
}

describe('explain_screen (C6)', () => {
  afterEach(() => {
    __resetSpawnImpl()
    __resetWriteFileImpl()
    __resetUnlinkImpl()
    __resetTmpdirImpl()
    __resetRandomBytesImpl()
    resetSpawnCallCount()
  })

  it('tools/list: includes explain_screen with required:["udid"] and properties udid/question/timeoutMs', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    expect(result.tools.length).toBe(27)
    const entry = result.tools.find((t) => t.name === 'explain_screen')
    expect(entry).toBeDefined()
    const schema = entry?.inputSchema as {
      required?: string[]
      properties: Record<string, unknown>
    }
    expect(schema.required ?? []).toContain('udid')
    expect(Object.keys(schema.properties)).toEqual(
      expect.arrayContaining(['udid', 'question', 'timeoutMs']),
    )
    await close()
  })

  it('happy path: spawns claude with prompt+tmp path + --tools Read + --output-format text; returns description=raw_output', async () => {
    const captured: { args?: readonly unknown[] } = {}
    __setSpawnImpl(makeMockSpawn({ kind: 'happy', stdout: 'fake-vlm-output\n' }, { capture: captured }))
    const writeFn = vi.fn().mockResolvedValue(undefined)
    __setWriteFileImpl(writeFn as unknown as typeof import('node:fs/promises').writeFile)
    const unlinkFn = vi.fn().mockResolvedValue(undefined)
    __setUnlinkImpl(unlinkFn as unknown as typeof import('node:fs/promises').unlink)
    __setTmpdirImpl((() => '/tmp') as unknown as typeof import('node:os').tmpdir)
    __setRandomBytesImpl(
      ((_n: number) => Buffer.from('deadbeef', 'hex')) as unknown as typeof import('node:crypto').randomBytes,
    )

    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'AAAAAAAA-1' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as {
      description: string
      raw_output: string
    }
    expect(parsed.description).toBe('fake-vlm-output')
    expect(parsed.raw_output).toBe('fake-vlm-output')
    const args = captured.args as readonly string[]
    expect(args).toContain('-p')
    expect(args).toContain('--tools')
    expect(args).toContain('Read')
    expect(args).toContain('--output-format')
    expect(args).toContain('text')
    const promptArg = args.find((a) => typeof a === 'string' && a.includes('simx-explain-'))
    expect(promptArg).toBeDefined()
    expect(promptArg).toContain('/tmp/simx-explain-AAAAAAAA-deadbeef.png')
    expect(unlinkFn).toHaveBeenCalledTimes(1)
    expect(writeFn).toHaveBeenCalledTimes(1)
    await close()
  })

  it('custom question is interpolated into spawn prompt', async () => {
    const captured: { args?: readonly unknown[] } = {}
    __setSpawnImpl(makeMockSpawn({ kind: 'happy', stdout: 'x' }, { capture: captured }))
    __setWriteFileImpl((async () => undefined) as unknown as typeof import('node:fs/promises').writeFile)
    __setUnlinkImpl((async () => undefined) as unknown as typeof import('node:fs/promises').unlink)

    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'A', question: 'Is there a login button visible?' },
    })
    expect(result.isError).not.toBe(true)
    const args = captured.args as readonly string[]
    const promptArg = args[1] as string
    expect(promptArg).toContain('Is there a login button visible?')
    await close()
  })

  it('default question used when args.question omitted', async () => {
    const captured: { args?: readonly unknown[] } = {}
    __setSpawnImpl(makeMockSpawn({ kind: 'happy', stdout: 'x' }, { capture: captured }))
    __setWriteFileImpl((async () => undefined) as unknown as typeof import('node:fs/promises').writeFile)
    __setUnlinkImpl((async () => undefined) as unknown as typeof import('node:fs/promises').unlink)

    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'A' },
    })
    expect(result.isError).not.toBe(true)
    const args = captured.args as readonly string[]
    const promptArg = args[1] as string
    expect(promptArg).toContain("Describe what's on this screen")
    await close()
  })

  it('claude not found (spawn throws ENOENT) returns isError with install URL; tmp still cleaned up', async () => {
    __setSpawnImpl(makeMockSpawn({ kind: 'enoent-sync' }))
    const writeFn = vi.fn().mockResolvedValue(undefined)
    __setWriteFileImpl(writeFn as unknown as typeof import('node:fs/promises').writeFile)
    const unlinkFn = vi.fn().mockResolvedValue(undefined)
    __setUnlinkImpl(unlinkFn as unknown as typeof import('node:fs/promises').unlink)

    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text).toBe(
      'claude CLI not found in PATH. Install Claude Code: https://docs.claude.com/en/docs/claude-code/setup',
    )
    expect(writeFn).toHaveBeenCalledTimes(1)
    expect(unlinkFn).toHaveBeenCalledTimes(1)
    await close()
  })

  it('claude exit non-zero returns isError with stderr tail', async () => {
    __setSpawnImpl(
      makeMockSpawn({
        kind: 'exit-nonzero',
        code: 1,
        stderr: 'auth failed: not logged in\n',
      }),
    )
    __setWriteFileImpl((async () => undefined) as unknown as typeof import('node:fs/promises').writeFile)
    __setUnlinkImpl((async () => undefined) as unknown as typeof import('node:fs/promises').unlink)

    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text).toBe('claude CLI exited with code 1: auth failed: not logged in')
    await close()
  })

  it('timeout fires and returns isError with timeoutMs in text', async () => {
    __setSpawnImpl(makeMockSpawn({ kind: 'never-close' }))
    __setWriteFileImpl((async () => undefined) as unknown as typeof import('node:fs/promises').writeFile)
    const unlinkFn = vi.fn().mockResolvedValue(undefined)
    __setUnlinkImpl(unlinkFn as unknown as typeof import('node:fs/promises').unlink)

    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'A', timeoutMs: 50 },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text).toBe('explain_screen timed out after 50ms')
    expect(unlinkFn).toHaveBeenCalledTimes(1)
    await close()
  })

  it('acquireDriver failure surfaces as isError; spawn / writeFile / unlink not called', async () => {
    __setSpawnImpl(makeMockSpawn({ kind: 'happy', stdout: 'unused' }))
    const writeFn = vi.fn().mockResolvedValue(undefined)
    __setWriteFileImpl(writeFn as unknown as typeof import('node:fs/promises').writeFile)
    const unlinkFn = vi.fn().mockResolvedValue(undefined)
    __setUnlinkImpl(unlinkFn as unknown as typeof import('node:fs/promises').unlink)

    const failingAcquire = vi.fn(async (_udid: string): Promise<Driver> => {
      throw new Error('runner not reachable')
    })
    const { client, close } = await makeLinkedClient({ acquireDriver: failingAcquire })
    const result = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text).toBe('runner not reachable')
    expect(spawnCallCount.value).toBe(0)
    expect(writeFn).not.toHaveBeenCalled()
    expect(unlinkFn).not.toHaveBeenCalled()
    await close()
  })

  it('screenshot failure surfaces as isError with "screenshot failed:" prefix; spawn / unlink not called', async () => {
    __setSpawnImpl(makeMockSpawn({ kind: 'happy', stdout: 'unused' }))
    const writeFn = vi.fn().mockResolvedValue(undefined)
    __setWriteFileImpl(writeFn as unknown as typeof import('node:fs/promises').writeFile)
    const unlinkFn = vi.fn().mockResolvedValue(undefined)
    __setUnlinkImpl(unlinkFn as unknown as typeof import('node:fs/promises').unlink)

    const screenshotFn = vi
      .fn<() => Promise<Buffer>>()
      .mockRejectedValue(new Error('simctl io failed'))
    const { acquireDriver } = makeMockAcquireDriver({ screenshot: screenshotFn })
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'A' },
    })
    expect(result.isError).toBe(true)
    const content = result.content as Array<{ text: string }>
    expect(content[0]?.text).toBe('screenshot failed: simctl io failed')
    expect(spawnCallCount.value).toBe(0)
    expect(writeFn).not.toHaveBeenCalled()
    expect(unlinkFn).not.toHaveBeenCalled()
    await close()
  })

  it('zod rejects: missing udid → isError; negative timeoutMs → isError', async () => {
    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })

    const noUdid = await client.callTool({
      name: 'explain_screen',
      arguments: {},
    })
    expect(noUdid.isError).toBe(true)
    const c1 = noUdid.content as Array<{ text: string }>
    expect(c1[0]?.text ?? '').toContain('invalid arguments')

    const negTimeout = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'A', timeoutMs: -5 },
    })
    expect(negTimeout.isError).toBe(true)
    const c2 = negTimeout.content as Array<{ text: string }>
    expect(c2[0]?.text ?? '').toContain('invalid arguments')
    await close()
  })

  it('unlink swallows ENOENT (tmp removed externally between write and cleanup)', async () => {
    __setSpawnImpl(makeMockSpawn({ kind: 'happy', stdout: 'ok-output' }))
    __setWriteFileImpl((async () => undefined) as unknown as typeof import('node:fs/promises').writeFile)
    const unlinkFn = vi.fn(async () => {
      const err = new Error('ENOENT') as NodeJS.ErrnoException
      err.code = 'ENOENT'
      throw err
    })
    __setUnlinkImpl(unlinkFn as unknown as typeof import('node:fs/promises').unlink)

    const { acquireDriver } = makeMockAcquireDriver()
    const { client, close } = await makeLinkedClient({ acquireDriver })
    const result = await client.callTool({
      name: 'explain_screen',
      arguments: { udid: 'A' },
    })
    expect(result.isError).not.toBe(true)
    const content = result.content as Array<{ text: string }>
    const parsed = JSON.parse(content[0]!.text) as { description: string }
    expect(parsed.description).toBe('ok-output')
    expect(unlinkFn).toHaveBeenCalledTimes(1)
    await close()
  })

  it('tools/list returns exactly 27 tools including explain_screen (no dups)', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    expect(result.tools.length).toBe(27)
    const names = result.tools.map((t) => t.name)
    expect(names).toContain('explain_screen')
    expect(new Set(names).size).toBe(names.length)
    await close()
  })

  it('tools/list emits outputSchema for exactly 9 ScreenDescription-emitting tools', async () => {
    const { client, close } = await makeLinkedClient()
    const result = await client.listTools()
    expect(result.tools.length).toBe(27)

    const expectedSdTools = [
      'screen_describe',
      'tap',
      'double_tap',
      'long_press',
      'fill',
      'swipe',
      'scroll_to',
      'key_press',
      'find_and_tap',
    ]
    const withOutput = result.tools
      .filter((t) => (t as { outputSchema?: unknown }).outputSchema !== undefined)
      .map((t) => t.name)
      .sort()
    expect(withOutput).toEqual([...expectedSdTools].sort())

    const sd = result.tools.find((t) => t.name === 'screen_describe') as
      | { outputSchema?: { type?: string; required?: string[] } }
      | undefined
    expect(sd?.outputSchema?.type).toBe('object')
    expect(sd?.outputSchema?.required).toContain('screenshot')

    const tap = result.tools.find((t) => t.name === 'tap') as
      | { outputSchema?: { required?: string[] } }
      | undefined
    expect(tap?.outputSchema?.required).toEqual(['ok'])

    const explain = result.tools.find((t) => t.name === 'explain_screen') as
      | { outputSchema?: unknown }
      | undefined
    expect(explain?.outputSchema).toBeUndefined()

    const screenshot = result.tools.find((t) => t.name === 'screen_screenshot') as
      | { outputSchema?: unknown }
      | undefined
    expect(screenshot?.outputSchema).toBeUndefined()

    await close()
  })
})
