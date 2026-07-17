// The HTTP-backed SelectorResolver + session client: one POST per call
// against a running smix-runner. Every route it sends is one the runner
// serves — /select/resolve{,-count,-labels} for resolution and /session/*
// (driven by the Session class through this client's fetch + Session-Id
// plumbing). Live driving and sense (launch / tap / snapshot) are not here;
// they are pending the napi axis.

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
 * SelectorResolver + session client over the smix-runner HTTP server.
 * Resolves selectors against a treeJson supplied by the caller and carries
 * the `Session-Id` header for the {@link Session} lifecycle. Used in
 * production RN/Expo test targets via fetch.
 */
export class HttpSimRuntime {
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
