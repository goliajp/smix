import { describe, it, expect, vi } from 'vitest'
import { SimctlClient, type Spawner, type SimctlDevice, type SimctlPermission, type BinarySpawner } from '../simctl.js'

describe('listRuntimes', () => {
  it('解析 iOS runtimes 并跳过非 iOS', async () => {
    const fakeStdout = JSON.stringify({
      runtimes: [
        {
          identifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-4',
          name: 'iOS 26.4',
          version: '26.4',
          isAvailable: true,
          platform: 'iOS',
        },
        {
          identifier: 'com.apple.CoreSimulator.SimRuntime.tvOS-18-0',
          name: 'tvOS 18.0',
          version: '18.0',
          isAvailable: true,
          platform: 'tvOS',
        },
      ],
    })
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'list', 'runtimes', '-j'])
      return { stdout: fakeStdout, stderr: '', exitCode: 0 }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    const runtimes = await client.listRuntimes()
    expect(runtimes).toHaveLength(1)
    expect(runtimes[0]).toEqual({
      identifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-4',
      name: 'iOS 26.4',
      version: '26.4',
      isAvailable: true,
    })
    expect(fakeSpawn).toHaveBeenCalledTimes(1)
  })

  it('exitCode 非 0 时抛错并带 stderr 片段', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'xcrun: error: something bad',
      exitCode: 72,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.listRuntimes()).rejects.toThrow(/simctl list runtimes failed.*exit 72/)
    await expect(client.listRuntimes()).rejects.toThrow(/xcrun: error/)
  })

  it('stdout 不是合法 JSON 时抛错并带 stdout 片段', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: 'not-json-{{{',
      stderr: '',
      exitCode: 0,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.listRuntimes()).rejects.toThrow(/failed to parse simctl runtimes JSON/)
  })
})

describe('listDevices', () => {
  it('展平嵌套结构并附 runtimeIdentifier', async () => {
    const fakeStdout = JSON.stringify({
      devices: {
        'com.apple.CoreSimulator.SimRuntime.iOS-26-4': [
          {
            udid: 'AAAA-1111',
            name: 'iPhone 16 Pro',
            state: 'Shutdown',
            isAvailable: true,
            deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro',
          },
        ],
        'com.apple.CoreSimulator.SimRuntime.iOS-17-5': [
          {
            udid: 'BBBB-2222',
            name: 'iPhone 15',
            state: 'Booted',
            isAvailable: true,
            deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-15',
          },
        ],
      },
    })
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'list', 'devices', '-j'])
      return { stdout: fakeStdout, stderr: '', exitCode: 0 }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    const devices = await client.listDevices()
    expect(devices).toHaveLength(2)
    const byUdid = (u: string): SimctlDevice => {
      const found = devices.find((d) => d.udid === u)
      if (!found) throw new Error(`device ${u} not found`)
      return found
    }
    expect(byUdid('AAAA-1111')).toEqual({
      udid: 'AAAA-1111',
      name: 'iPhone 16 Pro',
      state: 'Shutdown',
      isAvailable: true,
      runtimeIdentifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-4',
      deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro',
    })
    expect(byUdid('BBBB-2222').runtimeIdentifier).toBe(
      'com.apple.CoreSimulator.SimRuntime.iOS-17-5',
    )
    expect(fakeSpawn).toHaveBeenCalledTimes(1)
  })

  it('exitCode 非 0 时抛错并带 stderr 片段', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'boom',
      exitCode: 1,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.listDevices()).rejects.toThrow(/simctl list devices failed.*exit 1/)
    await expect(client.listDevices()).rejects.toThrow(/boom/)
  })

  it('stdout 不是合法 JSON 时抛错并带 stdout 片段', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '<<broken>>',
      stderr: '',
      exitCode: 0,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.listDevices()).rejects.toThrow(/failed to parse simctl devices JSON/)
  })
})

const UDID = 'AAAA-1111'
const BID = 'com.example.app'
const okResult = { stdout: '', stderr: '', exitCode: 0 } as const

describe('boot', () => {
  it('happy: exact args', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'boot', UDID])
      return okResult
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.boot(UDID)).resolves.toBeUndefined()
    expect(fakeSpawn).toHaveBeenCalledTimes(1)
  })

  it('error: exit 149 contains Unable to boot', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'Unable to boot device in current state: Booted',
      exitCode: 149,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.boot(UDID)).rejects.toThrow(/simctl boot failed.*exit 149/)
    await expect(client.boot(UDID)).rejects.toThrow(/Unable to boot/)
  })
})

describe('shutdown', () => {
  it('happy: exact args', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'shutdown', UDID])
      return okResult
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.shutdown(UDID)).resolves.toBeUndefined()
  })

  it('error: exit 164', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'something bad',
      exitCode: 164,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.shutdown(UDID)).rejects.toThrow(/simctl shutdown failed.*exit 164/)
  })
})

describe('install', () => {
  it('happy: exact args', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'install', UDID, '/tmp/Foo.app'])
      return okResult
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.install(UDID, '/tmp/Foo.app')).resolves.toBeUndefined()
  })

  it('error: exit 2', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'No such app bundle',
      exitCode: 2,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.install(UDID, '/tmp/Foo.app')).rejects.toThrow(/simctl install failed.*exit 2/)
  })
})

describe('uninstall', () => {
  it('happy: exact args', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'uninstall', UDID, BID])
      return okResult
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.uninstall(UDID, BID)).resolves.toBeUndefined()
    expect(fakeSpawn).toHaveBeenCalledTimes(1)
  })

  it('error: exit 1 contains No such app', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'No such app',
      exitCode: 1,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.uninstall(UDID, BID)).rejects.toThrow(/simctl uninstall failed.*exit 1/)
    await expect(client.uninstall(UDID, BID)).rejects.toThrow(/No such app/)
  })
})

describe('launch', () => {
  it('happy: parses pid from stdout', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'launch', UDID, BID])
      return { stdout: 'com.example.app: 12345\n', stderr: '', exitCode: 0 }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.launch(UDID, BID)).resolves.toEqual({ pid: 12345 })
  })

  it('error: exit 3', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'No such app',
      exitCode: 3,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.launch(UDID, BID)).rejects.toThrow(/simctl launch failed.*exit 3/)
  })

  it('happy: with args and env (SIMCTL_CHILD_ prefix)', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args, opts) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'launch', UDID, BID, '--foo', 'bar'])
      expect(opts?.env).toEqual({ SIMCTL_CHILD_LANG: 'en' })
      return { stdout: 'com.example.app: 99\n', stderr: '', exitCode: 0 }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.launch(UDID, BID, ['--foo', 'bar'], { LANG: 'en' }))
      .resolves.toEqual({ pid: 99 })
  })
})

describe('terminate', () => {
  it('happy: exact args', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(args).toEqual(['simctl', 'terminate', UDID, BID])
      return okResult
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.terminate(UDID, BID)).resolves.toBeUndefined()
  })

  it('error: exit 3', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'not running',
      exitCode: 3,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.terminate(UDID, BID)).rejects.toThrow(/simctl terminate failed.*exit 3/)
  })
})

describe('openUrl', () => {
  it('happy: exact args, lowercase openurl', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(args).toEqual(['simctl', 'openurl', UDID, 'https://example.com'])
      return okResult
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.openUrl(UDID, 'https://example.com')).resolves.toBeUndefined()
  })

  it('error: exit 1', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'bad url',
      exitCode: 1,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.openUrl(UDID, 'https://example.com')).rejects.toThrow(/simctl openurl failed.*exit 1/)
  })
})

describe('setAppearance', () => {
  it('happy: ui appearance dark', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(args).toEqual(['simctl', 'ui', UDID, 'appearance', 'dark'])
      return okResult
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.setAppearance(UDID, 'dark')).resolves.toBeUndefined()
  })

  it('error: exit 1', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'bad mode',
      exitCode: 1,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.setAppearance(UDID, 'light')).rejects.toThrow(/simctl ui appearance failed.*exit 1/)
  })
})

describe('grantPermission', () => {
  it('happy: privacy grant photos', async () => {
    const permission: SimctlPermission = 'photos'
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(args).toEqual(['simctl', 'privacy', UDID, 'grant', 'photos', BID])
      return okResult
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.grantPermission(UDID, BID, permission)).resolves.toBeUndefined()
  })

  it('error: exit 5', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'unknown service',
      exitCode: 5,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.grantPermission(UDID, BID, 'camera')).rejects.toThrow(/simctl privacy grant failed.*exit 5/)
  })
})

describe('pasteboardGet', () => {
  it('happy: returns stdout untrimmed', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(args).toEqual(['simctl', 'pbpaste', UDID])
      return { stdout: 'hello clipboard\n', stderr: '', exitCode: 0 }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.pasteboardGet(UDID)).resolves.toBe('hello clipboard\n')
  })

  it('error: exit 1', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'pbpaste: error',
      exitCode: 1,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.pasteboardGet(UDID)).rejects.toThrow(/simctl pbpaste failed.*exit 1/)
  })
})

describe('screenshot', () => {
  it('happy: writes temp file and reads PNG bytes back', async () => {
    const pngMagic = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args.slice(0, 5)).toEqual(['simctl', 'io', UDID, 'screenshot', '--type=png'])
      const tmpFile = args[5]!
      const { writeFile } = await import('node:fs/promises')
      await writeFile(tmpFile, pngMagic)
      return { stdout: '', stderr: '', exitCode: 0 }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    const buf = await client.screenshot(UDID)
    expect(Buffer.isBuffer(buf)).toBe(true)
    expect(buf.subarray(0, 8).equals(pngMagic)).toBe(true)
  })

  it('error: exit 164', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'No such device',
      exitCode: 164,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.screenshot(UDID)).rejects.toThrow(/simctl io screenshot failed.*exit 164/)
    await expect(client.screenshot(UDID)).rejects.toThrow(/No such device/)
  })
})

describe('pasteboardSet', () => {
  it('happy: stdin carries text untouched', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args, opts) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'pbcopy', UDID])
      expect(opts?.stdin).toBe('hello clipboard')
      return { stdout: '', stderr: '', exitCode: 0 }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.pasteboardSet(UDID, 'hello clipboard')).resolves.toBeUndefined()
    expect(fakeSpawn).toHaveBeenCalledTimes(1)
  })

  it('error: exit 1', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'pbcopy: error',
      exitCode: 1,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.pasteboardSet(UDID, 'x')).rejects.toThrow(/simctl pbcopy failed.*exit 1/)
  })
})

describe('bootAndWait', () => {
  it('happy: bootstatus -b exit 0', async () => {
    const fakeSpawn: Spawner = vi.fn(async (cmd, args) => {
      expect(cmd).toBe('xcrun')
      expect(args).toEqual(['simctl', 'bootstatus', UDID, '-b'])
      return { stdout: 'Device booted', stderr: '', exitCode: 0 }
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.bootAndWait(UDID, 60_000)).resolves.toBeUndefined()
    expect(fakeSpawn).toHaveBeenCalledTimes(1)
  })

  it('error: exit 60 contains failed to boot', async () => {
    const fakeSpawn: Spawner = async () => ({
      stdout: '',
      stderr: 'failed to boot',
      exitCode: 60,
    })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.bootAndWait(UDID, 60_000)).rejects.toThrow(/simctl bootstatus failed.*exit 60/)
    await expect(client.bootAndWait(UDID, 60_000)).rejects.toThrow(/failed to boot/)
  })

  it('timeout: rejects within timeoutMs window', async () => {
    vi.useFakeTimers()
    try {
      const fakeSpawn: Spawner = () => new Promise(() => {})
      const client = new SimctlClient({ spawn: fakeSpawn })
      const p = client.bootAndWait(UDID, 50)
      // Attach a noop catch so unhandled-rejection guard doesn't fire while we advance timers.
      const settled = expect(p).rejects.toThrow(/simctl bootstatus timed out after 50ms/)
      await vi.advanceTimersByTimeAsync(50)
      await settled
    } finally {
      vi.useRealTimers()
    }
  })

  it('default timeoutMs: callable without second arg', async () => {
    const fakeSpawn: Spawner = async () => ({ stdout: '', stderr: '', exitCode: 0 })
    const client = new SimctlClient({ spawn: fakeSpawn })
    await expect(client.bootAndWait(UDID)).resolves.toBeUndefined()
  })
})
