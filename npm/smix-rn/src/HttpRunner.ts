// The HTTP-backed SmixSimRuntime and SelectorResolver: one POST per call
// against a running smix-runner.
//
// KNOWN DEFECT — the endpoints below are not the ones the runner serves.
// Of the 16 called here only /select/resolve, /select/resolve-count and
// /select/resolve-labels exist; the other 13 answer 404. The runner takes
// /session/launch-app where this sends /sim/launch, /tree for
// /a11y/snapshot, /tap for /input/tap, /swipe-once for /input/swipe, and
// serves no screenshot route at all (screenshots are taken out of band).
// Every test here injects a mock client, so nothing compared these strings
// against the runner's route table. Correcting the wire is a v2 breaking
// change, tracked with the rest of them.

import { type A11yNode } from './A11yNode.js'
import type {
  KeyName,
  LaunchFreshCall,
  SmixSimRuntime,
  SwipeDirection,
} from './SimRuntime.js'
import type { LabelsResolver, SelectorResolver } from './SelectorResolver.js'

/**
 * Minimal HTTP client surface — overridable for testing. fetch-compatible
 * signature so consumers can pass globalThis.fetch directly.
 */
export type HttpFetch = (
  input: string,
  init: { method: 'POST'; headers: Record<string, string>; body: string },
) => Promise<{
  ok: boolean
  status: number
  json(): Promise<unknown>
  text(): Promise<string>
  /** v1.0.4 §D7 — optional; used to parse `X-Sim-Health` response header. */
  headers?: { get(name: string): string | null }
}>

/**
 * SmixSimRuntime + SelectorResolver wrapper around the smix-runner HTTP
 * server. Used in production RN/Expo test targets via fetch.
 */
export class HttpSimRuntime implements SmixSimRuntime {
  readonly resolver: SelectorResolver
  /**
   * Backs toHaveCount (count = labels.length) and toHaveLabel. Pass it to
   * the App constructor or to Smix.launchApp.
   */
  readonly labelsResolver: LabelsResolver

  /**
   * v1.0.3 — session id sent as `Session-Id` header on every request
   * when non-null. Set via {@link setSessionId}; when set, the runner
   * short-circuits per-request `.activate()` and reuses the cached
   * XCUIApplication binding. Typically managed by the {@link Session}
   * class — direct callers rarely need to touch this.
   */
  private sessionId: string | null = null

  /**
   * v1.0.4 §D7 — setter installed by {@link Session.open} so this
   * client can push `X-Sim-Health` header transitions back to the
   * session's state machine. `null` when no session is open (legacy
   * per-request path).
   */
  private sessionStateSetter: ((state: SessionState) => void) | null = null

  /**
   * v1.0.3 — public alias for the wrapped fetch implementation.
   * Consumed by the {@link Session} class which drives `/session/*`
   * routes through the same transport.
   */
  get fetch(): HttpFetch {
    return this.fetchImpl
  }

  constructor(
    public readonly baseUrl: string,
    public readonly fetchImpl: HttpFetch = globalThis.fetch as unknown as HttpFetch,
  ) {
    this.resolver = async (treeJson, selectorJson) => {
      const r = await this.post('/select/resolve', { treeJson, selectorJson })
      return (r as { ids: readonly string[] }).ids
    }
    this.labelsResolver = async (treeJson, selectorJson) => {
      const r = await this.post('/select/resolve-labels', { treeJson, selectorJson })
      return (r as { labels: readonly string[] }).labels
    }
  }

  /**
   * v1.0.3 — attach / clear the `Session-Id` header on every subsequent
   * request. Called by {@link Session.open} / {@link Session.close};
   * consumers who manage sessions manually can call this directly.
   */
  setSessionId(id: string | null): void {
    this.sessionId = id
  }

  /**
   * v1.0.4 §D7 — register a callback the client invokes on every
   * response that carries `X-Sim-Health`. Session.open() wires this
   * so consumers can subscribe via `session.on('state', ...)`.
   */
  attachSessionState(setter: (state: SessionState) => void): void {
    this.sessionStateSetter = setter
  }

  /**
   * Counts matches without building the id list, for callers that want a
   * count and nothing else.
   */
  async resolveCount(treeJson: string, selectorJson: string): Promise<number> {
    const r = await this.post('/select/resolve-count', { treeJson, selectorJson })
    return (r as { count: number }).count
  }

  async launch(bundleId: string): Promise<void> {
    await this.post('/sim/launch', { bundleId })
  }

  async terminate(bundleId: string): Promise<void> {
    await this.post('/sim/terminate', { bundleId })
  }

  async snapshotTree(): Promise<A11yNode> {
    const r = await this.post('/a11y/snapshot', {})
    return (r as { tree: A11yNode }).tree
  }

  async synthesizeTap(x: number, y: number): Promise<void> {
    await this.post('/input/tap', { x, y })
  }

  async sendString(text: string): Promise<void> {
    await this.post('/input/send-string', { text })
  }

  async pressKey(key: KeyName): Promise<void> {
    await this.post('/input/press-key', { key })
  }

  async swipe(direction: SwipeDirection): Promise<void> {
    await this.post('/input/swipe', { direction })
  }

  async screenshot(): Promise<Uint8Array> {
    const r = await this.post('/sim/screenshot', {}) as { png: string }
    return base64ToBytes(r.png)
  }

  async systemPopups(): Promise<A11yNode[]> {
    const r = await this.post('/sim/system-popups', {}) as { nodes: A11yNode[] }
    return r.nodes
  }

  async openUrl(url: string): Promise<void> {
    await this.post('/sim/open-url', { url })
  }

  async launchFresh(opts: {
    bundleId: string
    clearState: boolean
    clearKeychain: boolean
    appPath?: string | undefined
  }): Promise<void> {
    const payload: LaunchFreshCall = {
      bundleId: opts.bundleId,
      clearState: opts.clearState,
      clearKeychain: opts.clearKeychain,
      appPath: opts.appPath,
    }
    await this.post('/sim/launch-fresh', payload)
  }

  async launchFromPath(appPath: string): Promise<void> {
    await this.post('/sim/launch-from-path', { appPath })
  }

  async synthesizeTapAtNormalized(nx: number, ny: number): Promise<void> {
    await this.post('/input/tap-normalized', { nx, ny })
  }

  private async post(path: string, body: unknown): Promise<unknown> {
    const url = `${this.baseUrl}${path}`
    const headers: Record<string, string> = {
      'content-type': 'application/json',
    }
    if (this.sessionId !== null) {
      headers['session-id'] = this.sessionId
    }
    const resp = await this.fetchImpl(url, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
    })
    // v1.0.4 §D7 — parse X-Sim-Health header and forward to the
    // attached session state setter (if any). Runs BEFORE the
    // ok-check so state transitions are visible on error paths too.
    const simHealthHeader = resp.headers?.get?.('x-sim-health')
    if (simHealthHeader && this.sessionStateSetter) {
      const normalized = simHealthHeader.trim().toLowerCase()
      if (
        normalized === 'healthy' ||
        normalized === 'degraded' ||
        normalized === 'cycling' ||
        normalized === 'dead'
      ) {
        this.sessionStateSetter(normalized as SessionState)
      }
    }
    if (!resp.ok) {
      const text = await resp.text()
      throw new Error(`smix-runner ${path} → HTTP ${resp.status}: ${text}`)
    }
    // 200 with no body returns undefined; tolerate empty response
    try {
      return await resp.json()
    } catch {
      return undefined
    }
  }
}

/** v1.0.4 §D7 — re-export from Session.ts so this file's use compiles. */
export type SessionState = 'healthy' | 'degraded' | 'cycling' | 'dead'

function base64ToBytes(b64: string): Uint8Array {
  if (typeof globalThis.atob === 'function') {
    const bin = globalThis.atob(b64)
    const out = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
    return out
  }
  // Node fallback
  return Uint8Array.from(Buffer.from(b64, 'base64'))
}
