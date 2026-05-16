import { pathToFileURL } from 'node:url'
import { mkdir, writeFile, stat } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import type { SimctlClient } from '../../sim/simctl.js'
import { acquireCell, releaseCell, type AcquireCellOptions, type Cell } from '../../sim/cell.js'
import { SimctlDriver } from '../../driver/simctl-driver.js'
import type {
  Driver,
  LaunchOptions,
  FindOptions,
  SwipeDirection,
  KeyName,
  Permission,
  NetworkState,
  Appearance,
} from '../../driver/index.js'
import type { Selector, A11yNode, ScreenDescription, SerializedFailure } from '../../core/index.js'
import { ExpectationFailure } from '../../core/index.js'
import { run as runRegistry, resetRegistry } from '../../sdk/test.js'
import { setTraceSink, clearTraceSink, getTraceSink, appendStep, writeStepPng } from '../../sdk/trace-sink.js'
import type { StepRecord } from '../../sdk/trace-sink.js'
import type { WritableLike, CommandResult } from './list.js'

export type DeviceSelect = {
  udid?: string | undefined
  deviceName?: string | undefined
  runtimeIdentifier?: string | undefined
}

export type RunRunDeps = {
  client: SimctlClient
  file: string
  select: DeviceSelect
  grep?: string
  json?: boolean
  bail?: boolean
  out: WritableLike
  err: WritableLike
  cwd: string
}

type JsonCaseRecord = {
  name: string
  status: 'passed' | 'failed'
  durationMs: number
  error?: SerializedFailure | { name: string; message: string }
}

export async function runRunCommand(deps: RunRunDeps): Promise<CommandResult> {
  resetRegistry()

  const abs = resolve(deps.cwd, deps.file)
  try {
    await stat(abs)
  } catch {
    deps.err.write(`cannot load test file: ${abs}\n`)
    return { exitCode: 1 }
  }
  try {
    await import(pathToFileURL(abs).href)
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    deps.err.write(`cannot load test file: ${msg}\n`)
    return { exitCode: 1 }
  }

  let cell: Cell
  try {
    cell = await pickCell(deps.client, deps.select)
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    deps.err.write(`device selection failed: ${msg}\n`)
    return { exitCode: 1 }
  }

  const base = new SimctlDriver(cell)
  const traceRoot = join(deps.cwd, '.simx', 'trace')
  const tracer = new TracingDriver(base, traceRoot)

  const runStart = Date.now()
  const caseRecords: JsonCaseRecord[] = []
  const caseStartByName = new Map<string, number>()

  try {
    const result = await runRegistry({
      driver: tracer,
      ...(deps.grep !== undefined ? { grep: deps.grep } : {}),
      bail: deps.bail === true,
      onCaseStart: (name) => {
        caseStartByName.set(name, Date.now())
        const slug = slugify(name)
        tracer.beginCase(slug)
        setTraceSink({ traceRoot, caseSlug: slug })
      },
      onCaseEnd: (name, ok) => {
        clearTraceSink()
        const startedAt = caseStartByName.get(name) ?? Date.now()
        caseRecords.push({
          name,
          status: ok ? 'passed' : 'failed',
          durationMs: Date.now() - startedAt,
        })
      },
    })
    // Attach error payloads to matching failed records.
    for (const f of result.failures) {
      const rec = caseRecords.find((c) => c.name === f.name && c.status === 'failed')
      if (rec !== undefined) {
        rec.error = f.error instanceof ExpectationFailure
          ? f.error.toJSON(false)
          : { name: f.error.name, message: f.error.message }
      }
    }
    const exitCode = result.failed === 0 ? 0 : 1
    if (deps.json === true) {
      const total = result.passed + result.failed
      const report: Record<string, unknown> = {
        exitCode,
        passed: result.passed,
        failed: result.failed,
        total,
        durationMs: Date.now() - runStart,
        cases: caseRecords,
      }
      if (deps.bail === true && result.failed > 0) {
        report['bailed'] = true
      }
      deps.out.write(JSON.stringify(report) + '\n')
    } else {
      deps.out.write(`${result.passed} passed, ${result.failed} failed\n`)
      for (const f of result.failures) {
        const rendered = f.error instanceof ExpectationFailure
          ? f.error.toPrompt()
          : `${f.error.name}: ${f.error.message}`
        deps.err.write(`FAIL ${f.name}\n${rendered}\n`)
      }
    }
    return { exitCode }
  } finally {
    await releaseCell(cell).catch(() => undefined)
  }
}

export async function pickCell(
  client: SimctlClient,
  select: DeviceSelect,
): Promise<Cell> {
  if (
    select.udid !== undefined
    || select.deviceName !== undefined
    || select.runtimeIdentifier !== undefined
  ) {
    const opts: AcquireCellOptions = {}
    if (select.udid !== undefined) opts.udid = select.udid
    if (select.deviceName !== undefined) opts.deviceName = select.deviceName
    if (select.runtimeIdentifier !== undefined) opts.runtimeIdentifier = select.runtimeIdentifier
    return acquireCell(opts, { client })
  }
  // Default: prefer first booted available device to avoid DEVICE_AMBIGUOUS
  // when multiple sims are alive (common on dev machines / CI).
  const all = await client.listDevices()
  const booted = all.find((d) => d.isAvailable && d.state === 'Booted')
  if (booted !== undefined) {
    return acquireCell({ udid: booted.udid }, { client })
  }
  const available = all.find((d) => d.isAvailable)
  if (available !== undefined) {
    return acquireCell({ udid: available.udid }, { client })
  }
  return acquireCell({}, { client })
}

function slugify(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'case'
}

class TracingDriver implements Driver {
  private currentSlug: string | undefined
  private seq = 0

  constructor(
    private readonly base: Driver,
    private readonly traceRoot: string,
  ) {}

  beginCase(slug: string): void {
    this.currentSlug = slug
    this.seq = 0
  }

  // Append one StepRecord to <traceRoot>/<slug>/steps.jsonl via trace-sink's
  // module singleton. Returns the seq it wrote so callers (mutating wraps)
  // can name the matching step-<NNNN>.png 1:1. Silent no-op when sink is
  // unset (lifecycle outside a case) → returns undefined; silent on fs error
  // → returns undefined. Trace-write failures must never mask the wrapped
  // action's throw. C4 only wraps mutating Driver methods; read-only
  // (tree/findOne/findAll/describe/waitFor) bypass this entirely.
  private async recordStep(
    type: string,
    args: unknown,
    ok: boolean,
    start: number,
    err?: string,
  ): Promise<number | undefined> {
    const sink = getTraceSink()
    if (sink === undefined) return undefined
    try {
      const rec: StepRecord = {
        type,
        args,
        ok,
        durationMs: Date.now() - start,
        ...(err !== undefined ? { err } : {}),
      }
      return await appendStep(sink, rec)
    } catch {
      return undefined
    }
  }

  // Capture a screenshot via base.screenshot() and persist it as
  // <traceRoot>/<slug>/step-<NNNN>.png. NNNN must match the seq just written
  // to steps.jsonl so the jsonl row and PNG file align 1:1. Best-effort:
  // base.screenshot() or writeStepPng failure is silenced because trace IO
  // must never mask the wrapped action's success/throw state. The screenshot
  // action itself skips this path (decision 3: <NNNN>-screenshot.png is the
  // sole PNG output of an explicit screenshot()).
  private async captureStepPng(seq: number): Promise<void> {
    const sink = getTraceSink()
    if (sink === undefined) return
    try {
      const png = await this.base.screenshot()
      await writeStepPng(sink, seq, png)
    } catch {
      // intentional silence
    }
  }

  async screenshot(): Promise<Buffer> {
    const start = Date.now()
    let err: string | undefined
    try {
      const png = await this.base.screenshot()
      if (this.currentSlug !== undefined) {
        this.seq += 1
        const num = String(this.seq).padStart(4, '0')
        const out = join(this.traceRoot, this.currentSlug, `${num}-screenshot.png`)
        await mkdir(dirname(out), { recursive: true })
        await writeFile(out, png)
      }
      return png
    } catch (e) {
      err = e instanceof Error ? e.message : String(e)
      throw e
    } finally {
      await this.recordStep('screenshot', {}, err === undefined, start, err)
    }
  }

  async launch(bundleId: string, opts?: LaunchOptions): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.launch(bundleId, opts) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('launch', { bundleId, opts }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async terminate(bundleId: string): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.terminate(bundleId) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('terminate', { bundleId }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async install(path: string): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.install(path) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('install', { path }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async uninstall(bundleId: string): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.uninstall(bundleId) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('uninstall', { bundleId }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async background(): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.background() }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('background', {}, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async foreground(bundleId: string): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.foreground(bundleId) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('foreground', { bundleId }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async tap(s: Selector): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.tap(s) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('tap', { selector: s }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async doubleTap(s: Selector): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.doubleTap(s) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('doubleTap', { selector: s }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async longPress(s: Selector, d?: number): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.longPress(s, d) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('longPress', { selector: s, durationMs: d }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async fill(s: Selector, t: string): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.fill(s, t) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('fill', { selector: s, text: t }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async clear(s: Selector): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.clear(s) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('clear', { selector: s }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async swipe(d: SwipeDirection, f?: Selector): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.swipe(d, f) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('swipe', { direction: d, from: f }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async scroll(s: Selector, d: 'up' | 'down'): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.scroll(s, d) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('scroll', { selector: s, direction: d }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async scrollTo(s: Selector): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.scrollTo(s) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('scrollTo', { selector: s }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async pressKey(k: KeyName): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.pressKey(k) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('pressKey', { key: k }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async hideKeyboard(): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.hideKeyboard() }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('hideKeyboard', {}, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async openUrl(url: string): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.openUrl(url) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('openUrl', { url }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async pasteboardSet(t: string): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.pasteboardSet(t) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('pasteboardSet', { text: t }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async pasteboardGet(): Promise<string> {
    const start = Date.now()
    let err: string | undefined
    try { return await this.base.pasteboardGet() }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('pasteboardGet', {}, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async grantPermission(p: Permission, bid?: string): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.grantPermission(p, bid) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('grantPermission', { permission: p, bundleId: bid }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async setAppearance(m: Appearance): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.setAppearance(m) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('setAppearance', { mode: m }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async setLocale(l: string): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.setLocale(l) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('setLocale', { locale: l }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }
  async setNetwork(n: NetworkState): Promise<void> {
    const start = Date.now()
    let err: string | undefined
    try { await this.base.setNetwork(n) }
    catch (e) { err = e instanceof Error ? e.message : String(e); throw e }
    finally {
      const seq = await this.recordStep('setNetwork', { state: n }, err === undefined, start, err)
      if (seq !== undefined) await this.captureStepPng(seq)
    }
  }

  // Read-only methods: 1-line pass-through, no step record (decision 2).
  waitFor(s: Selector, t?: number): Promise<A11yNode> { return this.base.waitFor(s, t) }
  describe(): Promise<ScreenDescription> { return this.base.describe() }
  tree(): Promise<A11yNode> { return this.base.tree() }
  findOne(s: Selector, o?: FindOptions): Promise<A11yNode | null> { return this.base.findOne(s, o) }
  findAll(s: Selector): Promise<A11yNode[]> { return this.base.findAll(s) }
}
