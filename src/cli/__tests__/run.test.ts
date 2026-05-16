import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mkdtemp, rm, readFile, readdir, writeFile, stat } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runRunCommand } from '../commands/run.js'
import { resetRegistry } from '../../sdk/test.js'
import type { SimctlClient, SimctlDevice, SimctlPermission } from '../../sim/simctl.js'

const SDK_TEST_PATH = new URL('../../sdk/test.js', import.meta.url).pathname
const SDK_INDEX_PATH = new URL('../../sdk/index.js', import.meta.url).pathname

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

type FakeClient = SimctlClient & {
  __launchCalls: Array<{ udid: string; bundleId: string }>
  __screenshotCalls: Array<{ udid: string }>
}

function makeFakeClient(opts: {
  devices: SimctlDevice[]
  screenshotPng?: Buffer
  shouldFailScreenshot?: boolean
  shouldFailLaunch?: boolean
}): FakeClient {
  const launchCalls: Array<{ udid: string; bundleId: string }> = []
  const screenshotCalls: Array<{ udid: string }> = []
  // Real PNG magic prefix so v0.4 C5 acceptance (xxd -p -l 4 == 89504e47)
  // does not have to special-case fake screenshots. The fill byte is preserved
  // so existing >=1000 byte checks still hold.
  const png = opts.screenshotPng ?? Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    Buffer.alloc(1500, 0x89),
  ])
  return {
    listDevices: vi.fn().mockResolvedValue(opts.devices),
    launch: vi.fn().mockImplementation(async (udid: string, bundleId: string) => {
      launchCalls.push({ udid, bundleId })
      if (opts.shouldFailLaunch) throw new Error('launch fail')
      return { pid: 1 }
    }),
    terminate: vi.fn().mockResolvedValue(undefined),
    install: vi.fn().mockResolvedValue(undefined),
    uninstall: vi.fn().mockResolvedValue(undefined),
    openUrl: vi.fn().mockResolvedValue(undefined),
    pasteboardGet: vi.fn().mockResolvedValue(''),
    pasteboardSet: vi.fn().mockResolvedValue(undefined),
    grantPermission: vi.fn().mockResolvedValue(undefined),
    setAppearance: vi.fn().mockResolvedValue(undefined),
    bootAndWait: vi.fn().mockResolvedValue(undefined),
    boot: vi.fn().mockResolvedValue(undefined),
    shutdown: vi.fn().mockResolvedValue(undefined),
    listRuntimes: vi.fn().mockResolvedValue([]),
    screenshot: vi.fn().mockImplementation(async (udid: string) => {
      screenshotCalls.push({ udid })
      if (opts.shouldFailScreenshot) throw new Error('screenshot fail')
      return png
    }),
    get __launchCalls() { return launchCalls },
    get __screenshotCalls() { return screenshotCalls },
  } as unknown as FakeClient
}

let tmpRoot: string

beforeEach(async () => {
  resetRegistry()
  tmpRoot = await mkdtemp(join(tmpdir(), 'simx-c6-'))
})

afterEach(async () => {
  resetRegistry()
  if (tmpRoot) {
    await rm(tmpRoot, { recursive: true, force: true })
  }
})

async function writeTest(name: string, body: string): Promise<string> {
  const file = join(tmpRoot, `${name}.test.ts`)
  await writeFile(file, body)
  return file
}

describe('runRunCommand', () => {
  it('happy: single passing test writes trace png', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'hi',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('hi', async ({ app }) => {\n` +
        `  await app.launch('com.example')\n` +
        `  await app.screenshot()\n` +
        `})\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res).toEqual({ exitCode: 0 })
    expect(bufs.readOut()).toContain('1 passed, 0 failed')
    const pngPath = join(tmpRoot, '.simx', 'trace', 'hi', '0001-screenshot.png')
    const png = await readFile(pngPath)
    expect(png.length).toBeGreaterThanOrEqual(1000)
    // v0.4 C5: launch's captureStepPng also triggers one screenshot call, in
    // addition to the explicit app.screenshot(). Expect at least 2.
    expect(client.__screenshotCalls.length).toBeGreaterThanOrEqual(2)
  })

  it('happy: two tests → two case slug directories', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'multi',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('first case', async ({ app }) => { await app.screenshot() })\n` +
        `test('second case', async ({ app }) => { await app.screenshot() })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res).toEqual({ exitCode: 0 })
    expect(bufs.readOut()).toContain('2 passed, 0 failed')
    await stat(join(tmpRoot, '.simx', 'trace', 'first-case', '0001-screenshot.png'))
    await stat(join(tmpRoot, '.simx', 'trace', 'second-case', '0001-screenshot.png'))
  })

  it('fail: ExpectationFailure rendered via toPrompt', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'fail',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `import { ExpectationFailure } from '${SDK_INDEX_PATH}'\n` +
        `test('boom', async () => {\n` +
        `  throw new ExpectationFailure({ code: 'ELEMENT_NOT_FOUND', message: 'x', hint: 'add a11y id' })\n` +
        `})\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res).toEqual({ exitCode: 1 })
    expect(bufs.readOut()).toContain('0 passed, 1 failed')
    expect(bufs.readErr()).toContain('ELEMENT_NOT_FOUND')
    expect(bufs.readErr()).toContain('add a11y id')
  })

  it('device selection: --udid picks exactly that device', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
      makeDevice({ udid: 'BBB', state: 'Booted', isAvailable: true }),
      makeDevice({ udid: 'CCC', state: 'Shutdown', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'pick',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('pick', async ({ app }) => { await app.launch('com.x') })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: { udid: 'BBB' },
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(0)
    expect(client.__launchCalls[0]?.udid).toBe('BBB')
  })

  it('device selection: default picks first booted available', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'SHUT', state: 'Shutdown', isAvailable: true }),
      makeDevice({ udid: 'BOOT1', state: 'Booted', isAvailable: true }),
      makeDevice({ udid: 'BOOT2', state: 'Booted', isAvailable: true }),
      makeDevice({ udid: 'UNAVAIL', state: 'Shutdown', isAvailable: false }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'default',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('default pick', async ({ app }) => { await app.launch('com.x') })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(0)
    expect(client.__launchCalls[0]?.udid).toBe('BOOT1')
  })

  it('matcher failure writes failure-0001.png to trace dir', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    // Stub global fetch so the auto-constructed RunnerClient.getTree() returns
    // an empty (but valid) A11yNode — findOne resolves to null → toBeVisible's
    // pollFor exits cleanly to the failure() branch instead of bubbling a
    // RunnerTransportError. driver.describe() also calls /tree but tolerates
    // an empty tree via collectVisibleSummaries (elements stays []).
    const emptyTree = {
      rawType: 'application',
      bounds: { x: 0, y: 0, w: 390, h: 844 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
    }
    const originalFetch = globalThis.fetch
    globalThis.fetch = (async () =>
      new Response(JSON.stringify(emptyTree), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      })) as unknown as typeof fetch
    try {
      const file = await writeTest(
        'matcher-fail',
        `import { test, expect } from '${SDK_INDEX_PATH}'\n` +
          `test('matcher fail', async ({ app }) => {\n` +
          `  await expect(app.element({ text: 'NotReal' })).toBeVisible({ timeout: 0 })\n` +
          `})\n`,
      )
      const bufs = makeBufs()
      const res = await runRunCommand({
        client,
        file,
        select: {},
        out: bufs.out,
        err: bufs.err,
        cwd: tmpRoot,
      })
      expect(res.exitCode).toBe(1)
      expect(bufs.readOut()).toContain('0 passed, 1 failed')
      const pngPath = join(tmpRoot, '.simx', 'trace', 'matcher-fail', 'failure-0001.png')
      const png = await readFile(pngPath)
      expect(png.length).toBeGreaterThanOrEqual(100)
    } finally {
      globalThis.fetch = originalFetch
    }
  })

  it('steps.jsonl: records launch + screenshot in order', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'steps-happy',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('hi', async ({ app }) => {\n` +
        `  await app.launch('com.example')\n` +
        `  await app.screenshot()\n` +
        `})\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res).toEqual({ exitCode: 0 })
    const jsonlPath = join(tmpRoot, '.simx', 'trace', 'hi', 'steps.jsonl')
    const txt = await readFile(jsonlPath, 'utf8')
    const lines = txt.split('\n').filter((l) => l.length > 0)
    expect(lines).toHaveLength(2)
    const launch = JSON.parse(lines[0] ?? '') as Record<string, unknown>
    const shot = JSON.parse(lines[1] ?? '') as Record<string, unknown>
    expect(launch['type']).toBe('launch')
    expect(launch['ok']).toBe(true)
    expect(launch['seq']).toBe(1)
    expect(typeof launch['ts']).toBe('number')
    expect(typeof launch['duration_ms']).toBe('number')
    const launchArgs = launch['args'] as Record<string, unknown>
    expect(launchArgs['bundleId']).toBe('com.example')
    expect(shot['type']).toBe('screenshot')
    expect(shot['ok']).toBe(true)
    expect(shot['seq']).toBe(2)
  })

  it('steps.jsonl: action throw records ok:false err and still rethrows', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices, shouldFailScreenshot: true })
    const file = await writeTest(
      'steps-throw',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('boom', async ({ app }) => {\n` +
        `  await app.screenshot()\n` +
        `})\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res).toEqual({ exitCode: 1 })
    const jsonlPath = join(tmpRoot, '.simx', 'trace', 'boom', 'steps.jsonl')
    const txt = await readFile(jsonlPath, 'utf8')
    const lines = txt.split('\n').filter((l) => l.length > 0)
    expect(lines).toHaveLength(1)
    const obj = JSON.parse(lines[0] ?? '') as Record<string, unknown>
    expect(obj['type']).toBe('screenshot')
    expect(obj['ok']).toBe(false)
    expect(typeof obj['err']).toBe('string')
    expect(String(obj['err'])).toContain('screenshot fail')
    expect(typeof obj['duration_ms']).toBe('number')
  })

  it('steps.jsonl: read-only methods do not create steps.jsonl', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'steps-readonly',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('readonly', async ({ app }) => {\n` +
        // ElementHandle construction does not invoke any Driver method.
        `  app.element({ text: 'x' })\n` +
        `})\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res).toEqual({ exitCode: 0 })
    const jsonlPath = join(tmpRoot, '.simx', 'trace', 'readonly', 'steps.jsonl')
    const exists = await stat(jsonlPath).then(() => true).catch(() => false)
    expect(exists).toBe(false)
  })

  it('step-png: launch + tap writes step-0001.png + step-0002.png matching jsonl seq', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    // Stub fetch: GET /health → 200, POST /tap → 200. Both routes the SDK
    // tap path A relies on. The tap response shape is the minimal envelope
    // RunnerClient.tap accepts as success.
    const originalFetch = globalThis.fetch
    globalThis.fetch = (async (input: unknown) => {
      const url = typeof input === 'string' ? input : String(input)
      if (url.includes('/health')) {
        return new Response('ok', { status: 200 })
      }
      if (url.includes('/tap')) {
        return new Response(JSON.stringify({ ok: true }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      }
      return new Response('{}', { status: 200, headers: { 'Content-Type': 'application/json' } })
    }) as unknown as typeof fetch
    try {
      const file = await writeTest(
        'launch-tap',
        `import { test } from '${SDK_INDEX_PATH}'\n` +
          `test('lt', async ({ app }) => {\n` +
          `  await app.launch('com.example')\n` +
          `  await app.tap({ text: 'Foo' })\n` +
          `})\n`,
      )
      const bufs = makeBufs()
      const res = await runRunCommand({
        client,
        file,
        select: {},
        out: bufs.out,
        err: bufs.err,
        cwd: tmpRoot,
      })
      expect(res).toEqual({ exitCode: 0 })
      const traceDir = join(tmpRoot, '.simx', 'trace', 'lt')
      const step1 = await readFile(join(traceDir, 'step-0001.png'))
      const step2 = await readFile(join(traceDir, 'step-0002.png'))
      expect(step1.subarray(0, 4).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47]))).toBe(true)
      expect(step2.subarray(0, 4).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47]))).toBe(true)
      const jsonl = await readFile(join(traceDir, 'steps.jsonl'), 'utf8')
      const lines = jsonl.split('\n').filter((l) => l.length > 0)
      expect(lines).toHaveLength(2)
      const l1 = JSON.parse(lines[0] ?? '') as Record<string, unknown>
      const l2 = JSON.parse(lines[1] ?? '') as Record<string, unknown>
      expect(l1['type']).toBe('launch')
      expect(l1['seq']).toBe(1)
      expect(l2['type']).toBe('tap')
      expect(l2['seq']).toBe(2)
      const entries = await readdir(traceDir)
      const stepPngs = entries.filter((n) => /^step-\d+\.png$/.test(n))
      expect(stepPngs.sort()).toEqual(['step-0001.png', 'step-0002.png'])
      expect(entries.some((n) => /-screenshot\.png$/.test(n))).toBe(false)
    } finally {
      globalThis.fetch = originalFetch
    }
  })

  it('step-png: explicit screenshot action does NOT write step-NNNN.png', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'shot-only',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('s', async ({ app }) => { await app.screenshot() })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res).toEqual({ exitCode: 0 })
    const traceDir = join(tmpRoot, '.simx', 'trace', 's')
    await stat(join(traceDir, '0001-screenshot.png'))
    const stepExists = await stat(join(traceDir, 'step-0001.png')).then(() => true).catch(() => false)
    expect(stepExists).toBe(false)
    const jsonl = await readFile(join(traceDir, 'steps.jsonl'), 'utf8')
    const lines = jsonl.split('\n').filter((l) => l.length > 0)
    expect(lines).toHaveLength(1)
    const obj = JSON.parse(lines[0] ?? '') as Record<string, unknown>
    expect(obj['type']).toBe('screenshot')
    expect(obj['seq']).toBe(1)
  })

  it('step-png: action throw still attempts step-png write (best-effort)', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices, shouldFailLaunch: true })
    const file = await writeTest(
      'launch-fail',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('lf', async ({ app }) => { await app.launch('com.example') })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res).toEqual({ exitCode: 1 })
    const traceDir = join(tmpRoot, '.simx', 'trace', 'lf')
    const jsonl = await readFile(join(traceDir, 'steps.jsonl'), 'utf8')
    const lines = jsonl.split('\n').filter((l) => l.length > 0)
    expect(lines).toHaveLength(1)
    const obj = JSON.parse(lines[0] ?? '') as Record<string, unknown>
    expect(obj['type']).toBe('launch')
    expect(obj['ok']).toBe(false)
    expect(String(obj['err'])).toContain('launch fail')
    // best-effort: base.screenshot still succeeds in the finally block.
    const step1 = await readFile(join(traceDir, 'step-0001.png'))
    expect(step1.subarray(0, 4).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47]))).toBe(true)
  })

  it('error: file not found', async () => {
    const client = makeFakeClient({ devices: [] })
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file: '/nonexistent/xx.test.ts',
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(1)
    expect(bufs.readErr()).toContain('cannot load test file')
  })
})

describe('C5: --grep + --json + --bail (v0.5 C5)', () => {
  it('device selection: --device picks by name', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', name: 'iPhone 15', state: 'Booted', isAvailable: true }),
      makeDevice({ udid: 'BBB', name: 'iPhone 16 Pro', state: 'Booted', isAvailable: true }),
      makeDevice({ udid: 'CCC', name: 'iPad', state: 'Shutdown', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'pick-name',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('pick by name', async ({ app }) => { await app.launch('com.x') })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: { deviceName: 'iPhone 16 Pro' },
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(0)
    expect(client.__launchCalls[0]?.udid).toBe('BBB')
  })

  it('--grep filters case name by substring', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'grep-substr',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('foo bar', async ({ app }) => { await app.screenshot() })\n` +
        `test('baz qux', async ({ app }) => { await app.screenshot() })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      grep: 'foo',
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(0)
    expect(bufs.readOut()).toContain('1 passed, 0 failed')
    expect(bufs.readOut()).not.toContain('2 passed')
    // Only 'foo bar' case ran → its trace slug exists, 'baz qux' does not
    await stat(join(tmpRoot, '.simx', 'trace', 'foo-bar', '0001-screenshot.png'))
    const bazExists = await stat(join(tmpRoot, '.simx', 'trace', 'baz-qux'))
      .then(() => true)
      .catch(() => false)
    expect(bazExists).toBe(false)
  })

  it('--grep 0 matches → 0 passed 0 failed exit 0', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'grep-zero',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('alpha', async ({ app }) => { await app.screenshot() })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      grep: 'nonexistent',
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(0)
    expect(bufs.readOut()).toContain('0 passed, 0 failed')
  })

  it('--json happy → single-line valid JSON to stdout, stderr empty', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'json-happy',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('h', async ({ app }) => { await app.screenshot() })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      json: true,
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(0)
    expect(bufs.readErr()).toBe('')
    const raw = bufs.readOut()
    // single-line JSON + trailing newline
    expect(raw.endsWith('\n')).toBe(true)
    expect(raw.split('\n').filter((l) => l.length > 0)).toHaveLength(1)
    const parsed = JSON.parse(raw) as Record<string, unknown>
    expect(parsed['exitCode']).toBe(0)
    expect(parsed['passed']).toBe(1)
    expect(parsed['failed']).toBe(0)
    expect(parsed['total']).toBe(1)
    expect(typeof parsed['durationMs']).toBe('number')
    const cases = parsed['cases'] as Array<Record<string, unknown>>
    expect(cases).toHaveLength(1)
    expect(cases[0]?.['name']).toBe('h')
    expect(cases[0]?.['status']).toBe('passed')
    expect(typeof cases[0]?.['durationMs']).toBe('number')
    expect(parsed['bailed']).toBeUndefined()
  })

  it('--json fail → cases[0].error contains ExpectationFailure.toJSON schema', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'json-fail',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `import { ExpectationFailure } from '${SDK_INDEX_PATH}'\n` +
        `test('boom', async () => {\n` +
        `  throw new ExpectationFailure({ code: 'ELEMENT_NOT_FOUND', message: 'missing', hint: 'add a11y id' })\n` +
        `})\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      json: true,
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(1)
    expect(bufs.readErr()).toBe('')
    const parsed = JSON.parse(bufs.readOut()) as Record<string, unknown>
    expect(parsed['exitCode']).toBe(1)
    expect(parsed['failed']).toBe(1)
    const cases = parsed['cases'] as Array<Record<string, unknown>>
    expect(cases).toHaveLength(1)
    expect(cases[0]?.['status']).toBe('failed')
    const err = cases[0]?.['error'] as Record<string, unknown>
    expect(err['code']).toBe('ELEMENT_NOT_FOUND')
    expect(typeof err['message']).toBe('string')
    expect(err['hint']).toBe('add a11y id')
  })

  it('--bail stops on first fail', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'bail-stop',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('pass1', async ({ app }) => { await app.screenshot() })\n` +
        `test('fail-mid', async () => { throw new Error('mid') })\n` +
        `test('pass2', async ({ app }) => { await app.screenshot() })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      bail: true,
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(1)
    expect(bufs.readOut()).toContain('1 passed, 1 failed')
    // pass2 was never invoked → no slug dir
    const pass2Exists = await stat(join(tmpRoot, '.simx', 'trace', 'pass2'))
      .then(() => true)
      .catch(() => false)
    expect(pass2Exists).toBe(false)
  })

  it('--bail + --json + --grep combined', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'bail-json-grep',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('a', async ({ app }) => { await app.screenshot() })\n` +
        `test('b-fail', async () => { throw new Error('b') })\n` +
        `test('c-fail', async () => { throw new Error('c') })\n` +
        `test('d', async ({ app }) => { await app.screenshot() })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      grep: 'fail',
      bail: true,
      json: true,
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(1)
    const parsed = JSON.parse(bufs.readOut()) as Record<string, unknown>
    const cases = parsed['cases'] as Array<unknown>
    expect(cases).toHaveLength(1)
    expect(parsed['bailed']).toBe(true)
    expect(parsed['total']).toBe(1)
    expect(parsed['failed']).toBe(1)
    expect(parsed['passed']).toBe(0)
  })

  it('non-json mode byte-identical regression', async () => {
    const devices: SimctlDevice[] = [
      makeDevice({ udid: 'AAA', state: 'Booted', isAvailable: true }),
    ]
    const client = makeFakeClient({ devices })
    const file = await writeTest(
      'non-json-byte',
      `import { test } from '${SDK_INDEX_PATH}'\n` +
        `test('h', async ({ app }) => { await app.screenshot() })\n`,
    )
    const bufs = makeBufs()
    const res = await runRunCommand({
      client,
      file,
      select: {},
      out: bufs.out,
      err: bufs.err,
      cwd: tmpRoot,
    })
    expect(res.exitCode).toBe(0)
    expect(bufs.readOut()).toBe('1 passed, 0 failed\n')
  })
})
