import type {
  Driver,
  LaunchOptions,
  FindOptions,
  SwipeDirection,
  KeyName,
  Permission,
  NetworkState,
  Appearance,
} from './types.js'
import type {
  Selector,
  A11yNode,
  ScreenDescription,
  ElementSummary,
} from '../core/index.js'
import {
  ExpectationFailure,
  isTextSelector,
  describeSelector,
  resolveSelector,
  resolveSelectorAll,
  summarizeNode,
} from '../core/index.js'
import type { SimSession } from '../sim/session.js'
import type { SimctlPermission } from '../sim/simctl.js'
import type { BinarySpawner } from '../sim/simctl.js'
import type { Cell } from '../sim/cell.js'
import {
  RunnerClient,
  TapNotFoundError,
  RunnerTransportError,
} from '../sim/runner-client.js'
import { spawn as childSpawn } from 'node:child_process'

type A11yTreeSource = { getTree(): Promise<A11yNode> }

type SimctlDriverInput = Cell | SimSession
type SimctlDriverDeps = {
  runnerClient?: RunnerClient
  /** mock injection point for path B; defaults to a child_process.spawn wrapper */
  hostHidSpawner?: BinarySpawner
  /** override default host-hid binary path (default: swift-bridge/.build/debug/simx-host-hid) */
  hostHidBin?: string
  /** optional override; defaults to runnerClient (which provides getTree() via C1 wire) */
  treeSource?: A11yTreeSource
}

const DEFAULT_HOST_HID_BIN = 'swift-bridge/.build/debug/simx-host-hid'
const WAITFOR_DEFAULT_TIMEOUT_MS = 5000
const WAITFOR_POLL_INITIAL_MS = 50
const WAITFOR_POLL_MAX_MS = 500
const WAITFOR_POLL_FACTOR = 1.5

function isCell(x: SimctlDriverInput): x is Cell {
  return (
    typeof (x as Cell).runnerPort === 'number'
    && (x as Cell).session !== undefined
  )
}

type UnsupportedKind =
  | 'HID bridge'
  | 'HID/system bridge'
  | 'not exposed by simctl'

const UNSUPPORTED_HINTS: Record<UnsupportedKind, string> = {
  'HID bridge':
    'requires HID bridge (v0.2); simctl does not expose touch/key injection',
  'HID/system bridge':
    'requires HID/system bridge (v0.2); simctl has no background/foreground subcommand',
  'not exposed by simctl':
    'not exposed by simctl in v0.1; see v1.md section 2.A for the bridge roadmap',
}

const PERMISSION_MAP: Record<Permission, SimctlPermission> = {
  camera: 'camera',
  photos: 'photos',
  location: 'location',
  locationAlways: 'location-always',
  microphone: 'microphone',
  contacts: 'contacts',
  calendar: 'calendar',
  reminders: 'reminders',
  motion: 'motion',
  // simctl privacy has no notifications/bluetooth/faceId service; fall back to 'all'
  // until the AX bridge (v0.3) lets us toggle these in-app.
  notifications: 'all',
  bluetooth: 'all',
  faceId: 'all',
}

export class SimctlDriver implements Driver {
  private readonly session: SimSession
  private readonly cell: Cell | undefined
  private readonly runnerClient: RunnerClient | undefined
  private readonly hostHidSpawner: BinarySpawner
  private readonly hostHidBin: string
  private readonly treeSource: A11yTreeSource | undefined
  private lastLaunchedBundleId: string | undefined
  private healthChecked: boolean

  constructor(input: SimctlDriverInput, deps?: SimctlDriverDeps) {
    if (isCell(input)) {
      this.cell = input
      this.session = input.session
      this.runnerClient = deps?.runnerClient
        ?? new RunnerClient({ port: input.runnerPort })
    } else {
      this.cell = undefined
      this.session = input
      this.runnerClient = deps?.runnerClient
    }
    this.hostHidSpawner = deps?.hostHidSpawner ?? defaultHostHidSpawner
    this.hostHidBin = deps?.hostHidBin ?? DEFAULT_HOST_HID_BIN
    // treeSource defaults to runnerClient (RunnerClient implements A11yTreeSource
    // structurally via getTree()). Tests may inject a fake.
    this.treeSource = deps?.treeSource ?? this.runnerClient
    this.lastLaunchedBundleId = undefined
    this.healthChecked = false
  }

  async launch(bundleId: string, opts?: LaunchOptions): Promise<void> {
    if (opts?.fresh === true) {
      // simctl terminate exits non-zero when nothing is running; that's harmless here.
      try {
        await this.session.client.terminate(this.session.udid, bundleId)
      } catch {
        /* ignore */
      }
    }
    await this.wrap('launch', () =>
      this.session.client.launch(
        this.session.udid,
        bundleId,
        opts?.args ?? [],
        opts?.env ?? {},
      ),
    )
    this.lastLaunchedBundleId = bundleId
  }

  async terminate(bundleId: string): Promise<void> {
    await this.wrap('terminate', () =>
      this.session.client.terminate(this.session.udid, bundleId),
    )
  }

  async install(path: string): Promise<void> {
    await this.wrap('install', () =>
      this.session.client.install(this.session.udid, path),
    )
  }

  async uninstall(bundleId: string): Promise<void> {
    await this.wrap('uninstall', () =>
      this.session.client.uninstall(this.session.udid, bundleId),
    )
  }

  async openUrl(url: string): Promise<void> {
    await this.wrap('openUrl', () =>
      this.session.client.openUrl(this.session.udid, url),
    )
  }

  async pasteboardSet(text: string): Promise<void> {
    await this.wrap('pasteboardSet', () =>
      this.session.client.pasteboardSet(this.session.udid, text),
    )
  }

  async pasteboardGet(): Promise<string> {
    return this.wrap('pasteboardGet', () =>
      this.session.client.pasteboardGet(this.session.udid),
    )
  }

  async grantPermission(permission: Permission, bundleId?: string): Promise<void> {
    const target = bundleId ?? this.lastLaunchedBundleId
    if (target === undefined) {
      throw new ExpectationFailure({
        code: 'DRIVER_ERROR',
        message: 'Driver.grantPermission() requires bundleId before any app is launched',
        hint: 'pass bundleId explicitly, or call launch(bundleId) first',
      })
    }
    await this.wrap('grantPermission', () =>
      this.session.client.grantPermission(
        this.session.udid,
        target,
        PERMISSION_MAP[permission],
      ),
    )
  }

  async setAppearance(mode: Appearance): Promise<void> {
    await this.wrap('setAppearance', () =>
      this.session.client.setAppearance(this.session.udid, mode),
    )
  }

  async screenshot(): Promise<Buffer> {
    return this.wrap('screenshot', () =>
      this.session.client.screenshot(this.session.udid),
    )
  }

  async describe(): Promise<ScreenDescription> {
    return this.wrap('describe', async () => {
      const png = await this.session.client.screenshot(this.session.udid)
      // v0.4 C1: try to enrich with tree summaries; on any tree-source failure
      // (no Cell / sparse-tree / transport / unconfigured runner) fall back to
      // `elements: []` — describe() must never throw on a missing tree; that
      // would break the matcher.ts safeDescribe contract (it expects a usable
      // ScreenDescription even in degraded mode).
      let elements: ElementSummary[] = []
      try {
        if (this.treeSource !== undefined) {
          const tree = await this.treeSource.getTree()
          elements = collectVisibleSummaries(tree, 50)
        }
      } catch {
        elements = []
      }
      return {
        screenshot: png.toString('base64'),
        elements,
        frontApp: this.lastLaunchedBundleId ?? '',
        summary: '',
        capturedAt: Date.now(),
      }
    })
  }

  async tap(selector: Selector): Promise<void> {
    // Path A (v0.2 C6 compat): plain text-only selector → runner /tap by label.
    // Path B (v0.3 C6 new): everything else → resolveSelector → host-hid by coord.
    if (this.isPlainTextSelector(selector)) {
      return this.tapViaRunner(selector)
    }
    return this.tapViaResolver(selector)
  }

  // ----- v0.3 C6: tap path A (plain text-only via runner /tap) ---------------
  private async tapViaRunner(selector: { text: string }): Promise<void> {
    if (this.runnerClient === undefined) {
      throw new ExpectationFailure({
        code: 'DRIVER_ERROR',
        message: 'Driver.tap() requires a RunnerClient; construct SimctlDriver with a Cell',
        selector,
        hint: 'use acquireCell(...) instead of acquireSession(...) when building the driver',
      })
    }
    if (!this.healthChecked) {
      const alive = await this.runnerClient.health()
      if (!alive) {
        throw new ExpectationFailure({
          code: 'DRIVER_ERROR',
          message: `SimxRunner not reachable on port ${this.cell?.runnerPort ?? '?'}`,
          selector,
          hint: 'start the runner first: bash scripts/simx-runner-tap-smoke.sh (or simx-runner-health.sh)',
        })
      }
      this.healthChecked = true
    }
    try {
      await this.runnerClient.tap({ text: selector.text })
    } catch (e) {
      if (e instanceof TapNotFoundError) {
        throw new ExpectationFailure({
          code: 'ELEMENT_NOT_FOUND',
          message: `element not found by runner: ${describeSelector(selector)}`,
          selector,
          hint: 'verify the text label matches an XCUIElement label or identifier (NSPredicate label==% OR identifier==%)',
        })
      }
      if (e instanceof RunnerTransportError) {
        throw new ExpectationFailure({
          code: 'DRIVER_ERROR',
          message: `Driver.tap() transport failure: ${e.message}`,
          selector,
          hint: `runner port=${this.cell?.runnerPort ?? '?'} unreachable or returned non-2xx; check xcodebuild test log (expected port 22087)`,
        })
      }
      throw e
    }
  }

  // ----- v0.3 C6: tap path B (resolveSelector → host-hid by coord) ----------
  private async tapViaResolver(selector: Selector): Promise<void> {
    const tree = await this.tree()
    const node = resolveSelector(tree, selector)
    if (node === null) {
      throw new ExpectationFailure({
        code: 'ELEMENT_NOT_FOUND',
        message: `element not found: ${describeSelector(selector)}`,
        selector,
        visibleElements: collectVisibleSummaries(tree, 10),
        hint: 'matched 0 nodes in the current a11y tree; check selector or wait for the screen to settle',
      })
    }
    const root = tree.bounds
    if (
      root.w <= 0 ||
      root.h <= 0 ||
      node.bounds.w <= 0 ||
      node.bounds.h <= 0
    ) {
      throw new ExpectationFailure({
        code: 'ELEMENT_NOT_FOUND',
        message: `matched node has empty / offscreen frame: ${describeSelector(selector)}`,
        selector,
        hint: 'node bounds w*h == 0; element may be offscreen or hidden',
      })
    }
    const cx = node.bounds.x + node.bounds.w / 2
    const cy = node.bounds.y + node.bounds.h / 2
    const nx = cx / root.w
    const ny = cy / root.h
    if (nx <= 0 || nx >= 1 || ny <= 0 || ny >= 1) {
      throw new ExpectationFailure({
        code: 'ELEMENT_NOT_FOUND',
        message: `matched node centroid out of app frame: ${describeSelector(selector)}`,
        selector,
        hint: `centroid (nx=${nx.toFixed(3)}, ny=${ny.toFixed(3)}) outside (0,1); element offscreen`,
      })
    }
    const args = [
      'tap',
      '--udid', this.session.udid,
      '--x', nx.toFixed(4),
      '--y', ny.toFixed(4),
    ]
    let result: { stdout: Buffer; stderr: string; exitCode: number }
    try {
      result = await this.hostHidSpawner(this.hostHidBin, args)
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      throw new ExpectationFailure({
        code: 'DRIVER_ERROR',
        message: `host-hid spawn failed: ${msg}`,
        selector,
        hint: `check ${this.hostHidBin} exists and is executable (swift build -c debug --product simx-host-hid)`,
      })
    }
    const stdoutStr = result.stdout.toString().trim()
    let parsed: unknown
    try {
      parsed = JSON.parse(stdoutStr)
    } catch {
      throw new ExpectationFailure({
        code: 'DRIVER_ERROR',
        message: `host-hid tap returned non-JSON: ${stdoutStr.slice(0, 200)}`,
        selector,
        hint: 'check swift-bridge/.build/debug/simx-host-hid is up to date',
      })
    }
    if (
      typeof parsed !== 'object' ||
      parsed === null ||
      (parsed as Record<string, unknown>).ok !== true
    ) {
      const preview = (() => {
        try { return JSON.stringify(parsed) } catch { return String(parsed) }
      })().slice(0, 200)
      throw new ExpectationFailure({
        code: 'DRIVER_ERROR',
        message: `host-hid tap failed: ${preview}`,
        selector,
        hint: 'host-hid digitizer path rejected the tap; see simx-host-hid stderr',
      })
    }
  }

  private isPlainTextSelector(s: Selector): s is { text: string } {
    if (!isTextSelector(s)) return false
    if (typeof s.text !== 'string') return false
    const mods: ReadonlyArray<keyof Selector> = [
      'near', 'below', 'above', 'leftOf', 'rightOf', 'inside', 'nth', 'first', 'last',
    ]
    return mods.every((k) => (s as Record<string, unknown>)[k] === undefined)
  }

  // -- Not yet implemented (handed off to v0.2 HID bridge) --
  doubleTap(_s: Selector): Promise<void> { return this.notSupported('doubleTap', 'HID bridge') }
  longPress(_s: Selector, _d?: number): Promise<void> { return this.notSupported('longPress', 'HID bridge') }
  fill(_s: Selector, _t: string): Promise<void> { return this.notSupported('fill', 'HID bridge') }
  clear(_s: Selector): Promise<void> { return this.notSupported('clear', 'HID bridge') }
  swipe(_d: SwipeDirection, _f?: Selector): Promise<void> { return this.notSupported('swipe', 'HID bridge') }
  scroll(_s: Selector, _d: 'up' | 'down'): Promise<void> { return this.notSupported('scroll', 'HID bridge') }
  scrollTo(_s: Selector): Promise<void> { return this.notSupported('scrollTo', 'HID bridge') }
  pressKey(_k: KeyName): Promise<void> { return this.notSupported('pressKey', 'HID bridge') }
  hideKeyboard(): Promise<void> { return this.notSupported('hideKeyboard', 'HID bridge') }

  // -- v0.3 C6 AX bridge: real implementations (transport = RunnerClient.getTree + resolveSelector) --
  async tree(): Promise<A11yNode> {
    if (this.treeSource === undefined) {
      throw new ExpectationFailure({
        code: 'DRIVER_ERROR',
        message: 'Driver.tree() requires a tree source; construct SimctlDriver with a Cell',
        hint: 'use acquireCell(...) so RunnerClient.getTree is bound',
      })
    }
    return this.treeSource.getTree()
  }

  async findOne(selector: Selector, _opts?: FindOptions): Promise<A11yNode | null> {
    const tree = await this.tree()
    return resolveSelector(tree, selector)
  }

  async findAll(selector: Selector): Promise<A11yNode[]> {
    const tree = await this.tree()
    return resolveSelectorAll(tree, selector)
  }

  async waitFor(selector: Selector, timeoutMs?: number): Promise<A11yNode> {
    const total = timeoutMs ?? WAITFOR_DEFAULT_TIMEOUT_MS
    // single-shot path (timeout=0 in FindOptions semantics, no poll)
    if (total <= 0) {
      const node = await this.findOne(selector)
      if (node !== null) return node
      throw new ExpectationFailure({
        code: 'TIMEOUT',
        message: `waitFor(${describeSelector(selector)}) timed out after ${total}ms`,
        selector,
        hint: 'call app.describe() to inspect the current screen; selector may be wrong',
      })
    }
    const deadline = Date.now() + total
    let interval = WAITFOR_POLL_INITIAL_MS
    // eslint-disable-next-line no-constant-condition
    while (true) {
      const node = await this.findOne(selector)
      if (node !== null) return node
      if (Date.now() >= deadline) {
        throw new ExpectationFailure({
          code: 'TIMEOUT',
          message: `waitFor(${describeSelector(selector)}) timed out after ${total}ms`,
          selector,
          hint: 'call app.describe() to inspect the current screen; selector may be wrong',
        })
      }
      const remaining = deadline - Date.now()
      await new Promise((resolve) => setTimeout(resolve, Math.min(interval, remaining)))
      interval = Math.min(interval * WAITFOR_POLL_FACTOR, WAITFOR_POLL_MAX_MS)
    }
  }

  // -- Not yet implemented (no simctl subcommand) --
  background(): Promise<void> { return this.notSupported('background', 'HID/system bridge') }
  foreground(_b: string): Promise<void> { return this.notSupported('foreground', 'HID/system bridge') }
  setLocale(_l: string): Promise<void> { return this.notSupported('setLocale', 'not exposed by simctl') }
  setNetwork(_n: NetworkState): Promise<void> { return this.notSupported('setNetwork', 'not exposed by simctl') }

  private async wrap<T>(op: string, fn: () => Promise<T>): Promise<T> {
    try {
      return await fn()
    } catch (err) {
      if (err instanceof ExpectationFailure) throw err
      const message = err instanceof Error ? err.message : String(err)
      throw new ExpectationFailure({
        code: 'DRIVER_ERROR',
        message: `Driver.${op}() failed: ${message}`,
        hint: `simctl call failed; check simulator state and bundle/path validity (op=${op})`,
      })
    }
  }

  private notSupported(op: string, kind: UnsupportedKind): Promise<never> {
    return Promise.reject(
      new ExpectationFailure({
        code: 'DRIVER_ERROR',
        message: `Driver.${op}() not supported by SimctlDriver`,
        hint: UNSUPPORTED_HINTS[kind],
      }),
    )
  }
}

/**
 * DFS pre-order collect up to `limit` enabled+visible nodes and project them
 * to ElementSummary. Used by tapViaResolver to populate visibleElements when
 * a selector has 0 matches — AI-readable failure prompt requirement.
 */
function collectVisibleSummaries(tree: A11yNode, limit: number): ElementSummary[] {
  const out: ElementSummary[] = []
  walk(tree)
  return out
  function walk(n: A11yNode): void {
    if (out.length >= limit) return
    if (n.enabled === true && n.visible === true) out.push(summarizeNode(n))
    for (const c of n.children) {
      if (out.length >= limit) return
      walk(c)
    }
  }
}

/** Default host-hid spawner: stream stdout into a Buffer (binary-safe). */
const defaultHostHidSpawner: BinarySpawner = (cmd, args) =>
  new Promise((resolve) => {
    const child = childSpawn(cmd, [...args])
    const stdoutChunks: Buffer[] = []
    let stderr = ''
    child.stdout.on('data', (chunk: Buffer) => {
      stdoutChunks.push(chunk)
    })
    child.stderr.on('data', (chunk: Buffer) => {
      stderr += chunk.toString()
    })
    child.on('close', (code) => {
      resolve({
        stdout: Buffer.concat(stdoutChunks),
        stderr,
        exitCode: code ?? 1,
      })
    })
  })
