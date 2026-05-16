import type { A11yNode } from '../core/screen.js'
import { type Role, elementTypeNameToRole } from '../core/role.js'

export class TapNotFoundError extends Error {
  readonly selector: { text: string }
  readonly body: string
  constructor(selector: { text: string }, body: string) {
    super(`tap not_found for selector ${JSON.stringify(selector)}`)
    this.name = 'TapNotFoundError'
    this.selector = selector
    this.body = body
    Object.setPrototypeOf(this, new.target.prototype)
  }
}

export class RunnerTransportError extends Error {
  readonly status: number | undefined
  override readonly cause: unknown
  constructor(message: string, opts: { status?: number; cause?: unknown } = {}) {
    super(message)
    this.name = 'RunnerTransportError'
    this.status = opts.status
    this.cause = opts.cause
    Object.setPrototypeOf(this, new.target.prototype)
  }
}

export type RunnerClientOptions = {
  port: number
  host?: string
  fetchImpl?: typeof fetch
}

/**
 * Thin HTTP client over the SimxRunner XCUITest server (C2). C6 uses
 * only GET /health + POST /tap; future checkpoints add /screenshot,
 * /locate, etc. against this same client.
 */
export class RunnerClient {
  private readonly base: string
  private readonly fetchImpl: typeof fetch

  constructor(opts: RunnerClientOptions) {
    const host = opts.host ?? '127.0.0.1'
    this.base = `http://${host}:${opts.port}`
    this.fetchImpl = opts.fetchImpl ?? fetch
  }

  async health(): Promise<boolean> {
    try {
      const res = await this.fetchImpl(`${this.base}/health`)
      return res.ok && res.status === 200
    } catch {
      return false
    }
  }

  /**
   * Only `{ text: string }` base selectors are supported in C6.
   * Caller (SimctlDriver) is responsible for rejecting other forms;
   * RunnerClient stays narrow so it doesn't grow a selector compiler.
   */
  async tap(selector: { text: string }): Promise<void> {
    const body = JSON.stringify({ selector })
    let res: Response
    try {
      res = await this.fetchImpl(`${this.base}/tap`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
      })
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      throw new RunnerTransportError(`runner /tap fetch failed: ${msg}`, { cause: e })
    }
    if (res.status === 404) {
      const text = await res.text().catch(() => '')
      throw new TapNotFoundError(selector, text)
    }
    if (!res.ok) {
      const text = await res.text().catch(() => '')
      throw new RunnerTransportError(
        `runner /tap returned status ${res.status}: ${text.slice(0, 200)}`,
        { status: res.status },
      )
    }
  }

  /**
   * GET /tree — fetch the frontmost app's a11y tree as A11yNode.
   * v0.3 C1: backed by XCUIElementSnapshot via the runner UITest closure.
   * SDK / Driver does not consume this yet — `driver.tree()` stays a stub
   * until C6/C7. Throws RunnerTransportError on non-200 / malformed JSON /
   * fetch failure (no selector-not-found variant — C1 only surfaces
   * transport-level errors; selector resolution is C4+).
   */
  async getTree(): Promise<A11yNode> {
    let res: Response
    try {
      res = await this.fetchImpl(`${this.base}/tree`)
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      throw new RunnerTransportError(`runner /tree fetch failed: ${msg}`, { cause: e })
    }
    if (res.status === 500) {
      const text = await res.text().catch(() => '')
      throw new RunnerTransportError(
        `runner /tree returned 500 (snapshot unavailable): ${text.slice(0, 200)}`,
        { status: 500 },
      )
    }
    if (!res.ok) {
      const text = await res.text().catch(() => '')
      throw new RunnerTransportError(
        `runner /tree returned status ${res.status}: ${text.slice(0, 200)}`,
        { status: res.status },
      )
    }
    let json: unknown
    try {
      json = await res.json()
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      throw new RunnerTransportError(`runner /tree returned non-JSON body: ${msg}`, { cause: e })
    }
    if (!isA11yNode(json)) {
      const preview = (() => {
        try {
          return JSON.stringify(json).slice(0, 200)
        } catch {
          return String(json).slice(0, 200)
        }
      })()
      throw new RunnerTransportError(`runner /tree returned malformed payload: ${preview}`)
    }
    fillRolesInPlace(json)
    return json
  }
}

/**
 * Walk the tree once and assign `role` from `rawType`. In-place to avoid
 * a full deep clone (trees can be ~200 nodes; clone would be O(N) extra).
 * Always derived from rawType — any pre-existing `role` on the wire is
 * overwritten (Swift wire deliberately omits role; this guards future
 * upstream impls from leaking stale values).
 *
 * exactOptionalPropertyTypes: `node.role = undefined` is not assignable;
 * unknown rawType uses `delete` to preserve the invariant "role is set ⇔
 * rawType is in KNOWN_ROLES".
 */
function fillRolesInPlace(node: A11yNode): void {
  const r = elementTypeNameToRole(node.rawType)
  if (r !== undefined) {
    node.role = r
  } else {
    delete (node as { role?: Role }).role
  }
  for (const c of node.children) fillRolesInPlace(c)
}

// Narrow type guard — keeps RunnerClient zero-dep. Optional fields
// (identifier/label/value/text/role) are not required at this layer; C3 will
// fold a stricter post-processor (a11y-tree-normalize.ts) over the result
// when the Swift/TS protocol is frozen.
function isA11yNode(v: unknown): v is A11yNode {
  if (typeof v !== 'object' || v === null) return false
  const o = v as Record<string, unknown>
  if (typeof o.rawType !== 'string') return false
  if (typeof o.enabled !== 'boolean') return false
  if (typeof o.selected !== 'boolean') return false
  if (typeof o.hasFocus !== 'boolean') return false
  if (typeof o.visible !== 'boolean') return false
  const b = o.bounds
  if (typeof b !== 'object' || b === null) return false
  const bb = b as Record<string, unknown>
  if (
    typeof bb.x !== 'number' ||
    typeof bb.y !== 'number' ||
    typeof bb.w !== 'number' ||
    typeof bb.h !== 'number'
  ) {
    return false
  }
  if (!Array.isArray(o.children)) return false
  for (const c of o.children) {
    if (!isA11yNode(c)) return false
  }
  return true
}
