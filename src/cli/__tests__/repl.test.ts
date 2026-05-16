import { describe, it, expect, vi, afterEach } from 'vitest'
import { PassThrough } from 'node:stream'
import { promises as fs } from 'node:fs'
import * as path from 'node:path'
import { runReplCommand } from '../commands/repl.js'
import { App } from '../../sdk/index.js'
import type { Driver } from '../../driver/index.js'
import type { ScreenDescription } from '../../core/index.js'
import { ExpectationFailure } from '../../core/index.js'
import { TapNotFoundError, RunnerTransportError } from '../../sim/runner-client.js'
import type { Cell } from '../../sim/cell.js'

vi.mock('../commands/repl-codegen.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../commands/repl-codegen.js')>()
  return {
    ...actual,
    validateGeneratedTs: vi.fn(async () => ({ ok: true as const })),
  }
})
import { validateGeneratedTs } from '../commands/repl-codegen.js'

function makeBufs() {
  let out = ''
  let err = ''
  return {
    output: { write: (s: string) => { out += s } },
    err: { write: (s: string) => { err += s } },
    readOut: () => out,
    readErr: () => err,
  }
}

function makeScreen(elements: ScreenDescription['elements'] = []): ScreenDescription {
  return {
    screenshot: '',
    elements,
    frontApp: 'com.apple.Preferences',
    summary: '',
    capturedAt: 1700000000000,
  }
}

function makeDriver(overrides: Partial<Driver> = {}): Driver {
  // default driver: all mutating methods resolve void; describe returns a tiny screen
  const base = {
    launch: vi.fn().mockResolvedValue(undefined),
    terminate: vi.fn().mockResolvedValue(undefined),
    install: vi.fn().mockResolvedValue(undefined),
    uninstall: vi.fn().mockResolvedValue(undefined),
    background: vi.fn().mockResolvedValue(undefined),
    foreground: vi.fn().mockResolvedValue(undefined),
    tap: vi.fn().mockResolvedValue(undefined),
    doubleTap: vi.fn().mockResolvedValue(undefined),
    longPress: vi.fn().mockResolvedValue(undefined),
    fill: vi.fn().mockResolvedValue(undefined),
    clear: vi.fn().mockResolvedValue(undefined),
    swipe: vi.fn().mockResolvedValue(undefined),
    scroll: vi.fn().mockResolvedValue(undefined),
    scrollTo: vi.fn().mockResolvedValue(undefined),
    pressKey: vi.fn().mockResolvedValue(undefined),
    hideKeyboard: vi.fn().mockResolvedValue(undefined),
    openUrl: vi.fn().mockResolvedValue(undefined),
    pasteboardSet: vi.fn().mockResolvedValue(undefined),
    pasteboardGet: vi.fn().mockResolvedValue(''),
    grantPermission: vi.fn().mockResolvedValue(undefined),
    setAppearance: vi.fn().mockResolvedValue(undefined),
    setLocale: vi.fn().mockResolvedValue(undefined),
    setNetwork: vi.fn().mockResolvedValue(undefined),
    waitFor: vi.fn().mockResolvedValue({}),
    screenshot: vi.fn().mockResolvedValue(Buffer.from([0x89, 0x50, 0x4e, 0x47])),
    tree: vi.fn().mockResolvedValue({}),
    describe: vi.fn().mockResolvedValue(
      makeScreen([
        {
          role: 'button',
          name: 'General',
          id: 'GeneralCell',
          bounds: { x: 10, y: 20, w: 100, h: 40 },
          enabled: true,
        },
      ]),
    ),
    findOne: vi.fn().mockResolvedValue(null),
    findAll: vi.fn().mockResolvedValue([]),
  } as unknown as Driver
  return Object.assign(base, overrides)
}

function makeCell(): Cell {
  const release = vi.fn().mockResolvedValue(undefined)
  return {
    id: 'cell-0001',
    udid: '7B88259F-D864-4F98-9F2C-F0424216EE64',
    runnerPort: 22087,
    traceDir: '.simx/trace',
    session: {
      release,
      // unused fields filled with stubs to satisfy structural type
      udid: '7B88259F-D864-4F98-9F2C-F0424216EE64',
      device: {} as never,
      client: {} as never,
      isAlive: async () => true,
    } as unknown as Cell['session'],
  }
}

async function feed(input: PassThrough, lines: string[]): Promise<void> {
  for (const l of lines) input.write(l)
  input.end()
}

describe('runReplCommand', () => {
  it('prints banner on startup', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const app = new App(makeDriver())
    const cell = makeCell()
    input.end()
    const res = await runReplCommand({
      app,
      cell,
      input,
      output: bufs.output,
      err: bufs.err,
    })
    expect(res.exitCode).toBe(0)
    expect(bufs.readOut()).toMatch(/simx repl — Cell cell-0001/)
    expect(bufs.readOut()).toMatch(/Type \.help for commands/)
  })

  it('dispatches launch + auto-describes top-5', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['launch com.apple.Preferences\n'])
    await p
    expect(driver.launch).toHaveBeenCalledWith('com.apple.Preferences', undefined)
    expect(bufs.readOut()).toMatch(/→ screen: com\.apple\.Preferences/)
    expect(bufs.readOut()).toMatch(/role=button name="General"/)
  })

  it('dispatches tap text=General + auto-describes', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['tap text=General\n'])
    await p
    expect(driver.tap).toHaveBeenCalledWith({ text: 'General' })
    expect(bufs.readOut()).toMatch(/→ screen:/)
  })

  it('prints hint on parser error and stays alive for next line', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['foo bar\n', '.exit\n'])
    const res = await p
    expect(res.exitCode).toBe(0)
    expect(bufs.readErr()).toMatch(/error: unknown verb 'foo'/)
  })

  it('TapNotFoundError prints hint without exit', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver({
      tap: vi.fn().mockRejectedValue(new TapNotFoundError({ text: 'X' }, 'not_found')),
    })
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['tap text=X\n', '.exit\n'])
    const res = await p
    expect(res.exitCode).toBe(0)
    expect(bufs.readErr()).toMatch(/error: tap not found/)
  })

  it('RunnerTransportError prints hint without exit', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver({
      tap: vi.fn().mockRejectedValue(new RunnerTransportError('connection refused')),
    })
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['tap text=X\n', '.exit\n'])
    const res = await p
    expect(res.exitCode).toBe(0)
    expect(bufs.readErr()).toMatch(/error: runner transport: connection refused/)
  })

  it('ExpectationFailure prints code and message without exit', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver({
      tap: vi.fn().mockRejectedValue(
        new ExpectationFailure({
          code: 'ELEMENT_NOT_FOUND',
          message: 'no such element',
        }),
      ),
    })
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['tap text=Z\n', '.exit\n'])
    const res = await p
    expect(res.exitCode).toBe(0)
    expect(bufs.readErr()).toMatch(/error: ELEMENT_NOT_FOUND: no such element/)
  })

  it('describe failure prints hint without exit', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver({
      describe: vi.fn().mockRejectedValue(new Error('describe broke')),
    })
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['tap text=General\n', '.exit\n'])
    const res = await p
    expect(res.exitCode).toBe(0)
    expect(bufs.readErr()).toMatch(/→ describe failed: describe broke/)
  })

  it('.help prints command list without calling describe', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['.help\n', '.exit\n'])
    await p
    expect(bufs.readOut()).toMatch(/launch <bundleId>/)
    expect(bufs.readOut()).toMatch(/\.exit/)
    expect(driver.describe).not.toHaveBeenCalled()
  })

  it('.exit returns exitCode 0 and releases cell', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const releaseSpy = cell.session.release as unknown as ReturnType<typeof vi.fn>
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['.exit\n'])
    const res = await p
    expect(res.exitCode).toBe(0)
    expect(releaseSpy).toHaveBeenCalledTimes(1)
  })
})

describe('.history/.undo/.redo (v0.5 C2)', () => {
  it('.history on fresh REPL prints "no history yet"', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['.history\n', '.exit\n'])
    await p
    expect(bufs.readOut()).toMatch(/no history yet/)
  })

  it('.history after 1 action lists that entry with seq 1', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['launch com.apple.Preferences\n', '.history\n', '.exit\n'])
    await p
    expect(bufs.readOut()).toMatch(/history \(1 entries\):/)
    expect(bufs.readOut()).toMatch(/1\. launch com\.apple\.Preferences/)
  })

  it('.history after 2 actions lists both in seq ascending order', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      'tap text=General\n',
      '.history\n',
      '.exit\n',
    ])
    await p
    const out = bufs.readOut()
    expect(out).toMatch(/history \(2 entries\):/)
    const idx1 = out.indexOf('1. launch com.apple.Preferences')
    const idx2 = out.indexOf('2. tap text=General')
    expect(idx1).toBeGreaterThan(-1)
    expect(idx2).toBeGreaterThan(idx1)
  })

  it('parser error does not enter history', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['foo bar\n', '.history\n', '.exit\n'])
    await p
    expect(bufs.readErr()).toMatch(/error: unknown verb 'foo'/)
    expect(bufs.readOut()).toMatch(/no history yet/)
  })

  it('dispatch error does not enter history (TapNotFoundError)', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver({
      tap: vi.fn().mockRejectedValue(new TapNotFoundError({ text: 'X' }, 'not_found')),
    })
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['tap text=X\n', '.history\n', '.exit\n'])
    await p
    expect(bufs.readErr()).toMatch(/error: tap not found/)
    expect(bufs.readOut()).toMatch(/no history yet/)
  })

  it('.undo on empty stack prints hint to stdout (no err)', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['.undo\n', '.exit\n'])
    await p
    expect(bufs.readOut()).toMatch(/nothing to undo/)
    expect(bufs.readErr()).not.toMatch(/nothing to undo/)
  })

  it('action + .undo: history becomes empty', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.undo\n',
      '.history\n',
      '.exit\n',
    ])
    await p
    const out = bufs.readOut()
    expect(out).toMatch(/← undo: launch com\.apple\.Preferences/)
    expect(out).toMatch(/history \(1 entries\):/)
    expect(out).toMatch(/\(redo\) launch com\.apple\.Preferences/)
  })

  it('action + .undo + .redo: history shows it active again', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.undo\n',
      '.redo\n',
      '.history\n',
      '.exit\n',
    ])
    await p
    const out = bufs.readOut()
    expect(out).toMatch(/← undo: launch com\.apple\.Preferences/)
    expect(out).toMatch(/→ redo: launch com\.apple\.Preferences/)
    expect(out).toMatch(/history \(1 entries\):/)
    expect(out).toMatch(/1\. launch com\.apple\.Preferences/)
    expect(out).not.toMatch(/\(redo\) launch/)
  })

  it('new action after .undo clears redo-stack', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.undo\n',
      'tap text=General\n',
      '.redo\n',
      '.exit\n',
    ])
    await p
    // after new action, redoStack should be cleared, so .redo prints "nothing to redo"
    expect(bufs.readOut()).toMatch(/nothing to redo/)
  })

  it('seq is globally monotonic (does not reuse on undo)', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      'tap text=General\n',
      '.undo\n',
      'tap text=About\n',
      '.history\n',
      '.exit\n',
    ])
    await p
    const out = bufs.readOut()
    // After: launch(seq=1), tap General(seq=2), .undo (G→redo cleared by next), tap About(seq=3, redoStack cleared)
    // .history shows 2 active entries with seq 1 and seq 3
    expect(out).toMatch(/history \(2 entries\):/)
    expect(out).toMatch(/1\. launch com\.apple\.Preferences/)
    expect(out).toMatch(/3\. tap text=About/)
    expect(out).not.toMatch(/2\. tap text=General/)
  })
})

describe('.save as <name> (v0.5 C3)', () => {
  const createdFiles: string[] = []
  const cwd = process.cwd()

  afterEach(async () => {
    for (const f of createdFiles.splice(0)) {
      try {
        await fs.rm(f)
      } catch {
        // ignore
      }
    }
    vi.mocked(validateGeneratedTs).mockReset()
    vi.mocked(validateGeneratedTs).mockResolvedValue({ ok: true as const })
  })

  it('.save on empty buffer prints hint and writes no file', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, ['.save as empty-flow\n', '.exit\n'])
    await p
    expect(bufs.readOut()).toMatch(/nothing to save \(history is empty\)/)
    await expect(
      fs.access(path.resolve(cwd, 'examples', 'empty-flow.test.ts')),
    ).rejects.toThrow()
  })

  it('.save after single action writes file and prints ok msg', async () => {
    const target = path.resolve(cwd, 'examples', 'unit-c3-single.test.ts')
    createdFiles.push(target)
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.save as unit-c3-single\n',
      '.exit\n',
    ])
    await p
    expect(bufs.readOut()).toMatch(
      /→ saved: examples\/unit-c3-single\.test\.ts \(tsc ok\)/,
    )
    const body = await fs.readFile(target, 'utf8')
    expect(body).toContain("await app.launch('com.apple.Preferences')")
  })

  it('.save after multi-action sequence preserves order', async () => {
    const target = path.resolve(cwd, 'examples', 'unit-c3-multi.test.ts')
    createdFiles.push(target)
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      'tap text=General\n',
      '.save as unit-c3-multi\n',
      '.exit\n',
    ])
    await p
    const body = await fs.readFile(target, 'utf8')
    expect(body).toContain("await app.launch('com.apple.Preferences')")
    expect(body).toContain("await app.tap({ text: 'General' })")
    expect(body.indexOf('launch')).toBeLessThan(body.indexOf('tap'))
  })

  it('.save does not clear undoStack (decision 10)', async () => {
    const target = path.resolve(cwd, 'examples', 'unit-c3-keep.test.ts')
    createdFiles.push(target)
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.save as unit-c3-keep\n',
      '.history\n',
      '.exit\n',
    ])
    await p
    const out = bufs.readOut()
    expect(out).toMatch(/history \(1 entries\):/)
    expect(out).toMatch(/1\. launch com\.apple\.Preferences/)
  })

  it('refuses to overwrite an existing file', async () => {
    const target = path.resolve(cwd, 'examples', 'unit-c3-exists.test.ts')
    createdFiles.push(target)
    await fs.writeFile(target, '// pre-existing dummy\n', 'utf8')
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.save as unit-c3-exists\n',
      '.exit\n',
    ])
    await p
    expect(bufs.readOut()).toMatch(
      /error: examples\/unit-c3-exists\.test\.ts already exists; pick another name/,
    )
    // file content unchanged
    const body = await fs.readFile(target, 'utf8')
    expect(body).toBe('// pre-existing dummy\n')
  })

  it('rejects bad name with uppercase', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.save as Bad-Name\n',
      '.exit\n',
    ])
    await p
    expect(bufs.readOut()).toMatch(/error: bad name 'Bad-Name'/)
    await expect(
      fs.access(path.resolve(cwd, 'examples', 'Bad-Name.test.ts')),
    ).rejects.toThrow()
  })

  it('rejects bad name with path separator', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.save as bad/path\n',
      '.exit\n',
    ])
    await p
    expect(bufs.readOut()).toMatch(/error: bad name 'bad\/path'/)
  })

  it('parser-error path for bare .save (no arg) goes to err stream', async () => {
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.save\n',
      '.exit\n',
    ])
    await p
    expect(bufs.readErr()).toMatch(/error: \.save requires 'as <name>'/)
  })

  it('prints warning when validateGeneratedTs reports tsc errors', async () => {
    const target = path.resolve(cwd, 'examples', 'unit-c3-tscbad.test.ts')
    createdFiles.push(target)
    vi.mocked(validateGeneratedTs).mockResolvedValueOnce({
      ok: false,
      output:
        "examples/unit-c3-tscbad.test.ts(3,3): error TS2322: Type 'string' is not assignable to type 'number'.\n",
    })
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.save as unit-c3-tscbad\n',
      '.exit\n',
    ])
    await p
    const out = bufs.readOut()
    expect(out).toMatch(
      /warning: examples\/unit-c3-tscbad\.test\.ts saved but tsc reported errors:/,
    )
    expect(out).toMatch(/error TS2322/)
    expect(out).toMatch(/\(file kept on disk for inspection\)/)
    // file kept on disk
    await expect(fs.access(target)).resolves.toBeUndefined()
  })

  it('generated file uses LF line endings only', async () => {
    const target = path.resolve(cwd, 'examples', 'unit-c3-lf.test.ts')
    createdFiles.push(target)
    const bufs = makeBufs()
    const input = new PassThrough()
    const driver = makeDriver()
    const app = new App(driver)
    const cell = makeCell()
    const p = runReplCommand({ app, cell, input, output: bufs.output, err: bufs.err })
    await feed(input, [
      'launch com.apple.Preferences\n',
      '.save as unit-c3-lf\n',
      '.exit\n',
    ])
    await p
    const body = await fs.readFile(target, 'utf8')
    expect(body).not.toMatch(/\r/)
    expect(body.endsWith('\n')).toBe(true)
  })
})
