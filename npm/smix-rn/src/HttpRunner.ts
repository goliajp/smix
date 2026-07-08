// v7.5 c3 — HTTP-based SmixSimRuntime + SelectorResolver implementations.
//
// Per R-7.5-B HTTP-runtime decision: TS SDK communicates with smix-runner
// via fetch JSON-RPC-ish API (one POST per call). Default endpoint
// matches existing smix-runner-client conventions; consumers override
// via base URL constructor param.
//
// Wire contract (mirrors smix-runner-client v3+):
//   POST /sim/launch         { bundleId }                                  → 200
//   POST /sim/terminate      { bundleId }                                  → 200
//   POST /sim/launch-fresh   { bundleId, clearState, clearKeychain, appPath? } → 200
//   POST /sim/launch-from-path { appPath }                                 → 200
//   POST /sim/screenshot     {}                                            → 200 { png: base64 }
//   POST /sim/system-popups  {}                                            → 200 { nodes: A11yNode[] }
//   POST /sim/open-url       { url }                                       → 200
//   POST /a11y/snapshot      {}                                            → 200 { tree: A11yNode }
//   POST /input/tap          { x, y }                                      → 200
//   POST /input/tap-normalized { nx, ny }                                  → 200
//   POST /input/swipe        { direction }                                 → 200
//   POST /input/send-string  { text }                                      → 200
//   POST /input/press-key    { key }                                       → 200
//   POST /select/resolve     { treeJson, selectorJson }                    → 200 { ids: string[] }
//
// v7.7 c1 contract additions (server impl deferred):
//   POST /select/resolve-count   { treeJson, selectorJson }                → 200 { count: u32 }
//   POST /select/resolve-labels  { treeJson, selectorJson }                → 200 { labels: string[] }
//
// The exact endpoint shapes are negotiated with smix-runner-server in
// follow-up Swift work; for v7.5-v7.7 c1 we wire the TS-side fetch +
// decode and a MockHttpRunnerClient stub that lets vitest exercise the
// runtime without booting a real server.

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
) => Promise<{ ok: boolean; status: number; json(): Promise<unknown>; text(): Promise<string> }>

/**
 * SmixSimRuntime + SelectorResolver wrapper around the smix-runner HTTP
 * server. Used in production RN/Expo test targets via fetch.
 */
export class HttpSimRuntime implements SmixSimRuntime {
  readonly resolver: SelectorResolver
  /**
   * v7.7 c1 — labels resolver wrapping Rust `resolve_selector_labels`
   * via smix-runner-server `/select/resolve-labels` endpoint. Used by
   * `Locator.toHaveCount` (count = labels.length) + Locator.toHaveLabel.
   * Pass directly to App constructor or Smix.launchApp.
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
   * v7.7 c1 — direct match-count via dedicated FFI fn (avoid allocating
   * the full ids list when callers only need count). Mirrors Rust
   * `resolve_selector_count`. Locator.toHaveCount can prefer this over
   * `labelsResolver` if it needs only count, not the labels themselves.
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
