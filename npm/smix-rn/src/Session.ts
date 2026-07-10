// v1.0.3 — Session lifecycle wrapper.
//
// Session lifecycle addresses the runner-side "activation storm": pre-
// v1.0.2 runners called `.activate()` on every request whose
// `App-Activate: true` header was set, exhausting XCTest process
// arbitration on long-running gates. Sessions replace that with a
// one-shot lifecycle — open once, every subsequent request carries
// the `Session-Id` header, runner reuses its cached XCUIApplication
// binding, no per-request activation.
//
// Consumer flow:
//
// ```ts
// import { Smix, HttpRunnerClient, HttpSimRuntime, HttpSelectorResolver } from '@goliapkg/smix'
//
// const runner = new HttpRunnerClient('http://127.0.0.1:22087')
// const runtime = new HttpSimRuntime(runner)
// const resolver = new HttpSelectorResolver(runner)
// const session = await Session.open(runner, 'com.example.app', { activate: true })
// try {
//   const app = await Smix.launchApp(bundleId('com.example.app'), runtime, resolver)
//   // ... use `app` normally; every request carries Session-Id ...
// } finally {
//   await session.close()
// }
// ```

import type { HttpFetch } from './HttpRunner.js'

interface HttpRunnerLike {
  readonly baseUrl: string
  readonly fetch: HttpFetch
  setSessionId(id: string | null): void
  /** v1.0.4 §D7 — optional hook so the client can push state
   * transitions parsed from `X-Sim-Health` headers back to the
   * Session. Legacy runners omit the header; state stays `healthy`. */
  attachSessionState?(setter: (state: SessionState) => void): void
}

export interface SessionOpenOptions {
  /** If true, runner calls `.activate()` on the target once at open time. */
  activate?: boolean
}

/**
 * v1.0.4 §D7 — session-scoped sim health classification observed via
 * the runner's `X-Sim-Health` response header. Consumers subscribe
 * via {@link Session.on}.
 */
export type SessionState = 'healthy' | 'degraded' | 'cycling' | 'dead'

type StateListener = (state: SessionState) => void

/**
 * Runner-side session guard. Use `Session.open()` to acquire, always
 * pair with `session.close()` in a `finally` block.
 *
 * While the Session is open, every runner request from the same
 * `HttpRunnerClient` carries a `Session-Id` header. The runner
 * short-circuits per-request `.activate()` and reuses the session's
 * cached binding.
 */
export class Session {
  /** v1.0.4 §D7 — current classification; updated on every response
   * that carries `X-Sim-Health`. */
  private _state: SessionState = 'healthy'
  private readonly listeners = new Set<StateListener>()

  private constructor(
    private readonly runner: HttpRunnerLike,
    public readonly sessionId: string,
    public readonly activatedOnce: boolean,
    public readonly serverTimeMs: number,
  ) {}

  static async open(
    runner: HttpRunnerLike,
    bundleId: string,
    options: SessionOpenOptions = {},
  ): Promise<Session> {
    const activate = options.activate ?? false
    const res = await runner.fetch(`${runner.baseUrl}/session/open`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ bundleId, activate }),
    })
    if (!res.ok) {
      const body = await res.text()
      throw new Error(`Session.open ${bundleId}: HTTP ${res.status}: ${body}`)
    }
    const json = (await res.json()) as {
      sessionId?: string
      activatedOnce?: boolean
      serverTimeMs?: number
    }
    if (typeof json.sessionId !== 'string' || json.sessionId.length === 0) {
      throw new Error(`Session.open ${bundleId}: response missing sessionId`)
    }
    runner.setSessionId(json.sessionId)
    const session = new Session(
      runner,
      json.sessionId,
      json.activatedOnce ?? false,
      json.serverTimeMs ?? 0,
    )
    // v1.0.4 §D7 — wire the runner's X-Sim-Health parse into this
    // session's state machine. Legacy runner-clients without the
    // attachSessionState hook keep working; the session's state stays
    // 'healthy'.
    runner.attachSessionState?.((next) => session.updateState(next))
    return session
  }

  /** v1.0.4 §D7 — current sim-health classification. */
  get state(): SessionState {
    return this._state
  }

  /** v1.0.4 §D7 — subscribe to state transitions. Returns an
   * unsubscribe function. */
  on(_event: 'state', listener: StateListener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** v1.0.5 §D1 — probe `/session/list` and return `true` iff this
   * session's id is still known to the runner. Consumers wire this
   * after a state transition to `cycling`/`dead` to decide whether
   * to keep the session or reopen. */
  async stillValid(): Promise<boolean> {
    const res = await this.runner.fetch(
      `${this.runner.baseUrl}/session/list`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: '{}',
      },
    )
    if (!res.ok) return false
    const json = (await res.json()) as {
      sessions?: Array<{ sessionId?: string }>
    }
    return (json.sessions ?? []).some((s) => s.sessionId === this.sessionId)
  }

  /** v1.0.4 §D14 — instruct the runner to `terminate()` + `launch()`
   * the session's cached `XCUIApplication` in place. Preserves the
   * session id and XCUITest binding. Returns wall-clock ms. */
  async relaunchApp(): Promise<number> {
    const res = await this.runner.fetch(
      `${this.runner.baseUrl}/session/relaunch-app`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sessionId: this.sessionId }),
      },
    )
    if (!res.ok) {
      const body = await res.text()
      throw new Error(
        `Session.relaunchApp ${this.sessionId}: HTTP ${res.status}: ${body}`,
      )
    }
    const json = (await res.json()) as { ok?: boolean; wallMs?: number }
    return json.wallMs ?? 0
  }

  private updateState(next: SessionState) {
    if (next === this._state) return
    this._state = next
    for (const listener of this.listeners) {
      try {
        listener(next)
      } catch {
        // Consumer callbacks must not break the runner-response path.
      }
    }
  }

  /**
   * Ask the runner to re-issue `.activate()` on the session's cached
   * binding. Subject to a runner-side 2 s rate limit per session; when
   * rate-limited, returns `false` (no-op, session still healthy).
   *
   * Use when the client detects target-app drift (e.g. tree returned
   * empty payload, foreground steal by system UI).
   */
  async renewActivation(): Promise<boolean> {
    const res = await this.runner.fetch(
      `${this.runner.baseUrl}/session/renew-activation`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sessionId: this.sessionId }),
      },
    )
    if (!res.ok) {
      const body = await res.text()
      throw new Error(
        `Session.renewActivation ${this.sessionId}: HTTP ${res.status}: ${body}`,
      )
    }
    const json = (await res.json()) as { ok?: boolean; activated?: boolean }
    return json.activated ?? false
  }

  /**
   * Release the session. Sends `POST /session/close` (idempotent),
   * clears the `Session-Id` header from the client. Subsequent client
   * requests go via the legacy per-request rebind path (rate-limited).
   */
  async close(): Promise<void> {
    try {
      await this.runner.fetch(`${this.runner.baseUrl}/session/close`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sessionId: this.sessionId }),
      })
    } finally {
      // Clear the client header regardless of runner-side outcome.
      this.runner.setSessionId(null)
    }
  }
}
