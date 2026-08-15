// A handle to a running app. Live driving (tap/fill/swipe/...) and live
// sense (snapshotTree) are pending the napi axis: the TS SDK has no FFI
// path, so with the fictional HTTP wire deleted there is no way to reach a
// simulator from here yet. These methods throw SmixNotImplementedError
// rather than posting to a route the runner never served. What does work is
// the resolver seam: given a treeJson, `resolver` / `labelsResolver` resolve
// selectors against the runner's served /select/resolve* routes.

import { type A11yNode, flatten } from './A11yNode.js'
import { ExpectationFailure } from './ExpectationFailure.js'
import { Locator, SmixNotImplementedError } from './Locator.js'
import type { NodeDriver, NodeSession } from './NodeDriver.js'
import { type Selector, encodeSelectorJson } from './Selector.js'
import type { LabelsResolver, SelectorResolver } from './SelectorResolver.js'
import type { SystemPopup } from './SystemPopup.js'

type KeyName = 'return' | 'delete' | 'space' | 'tab' | 'escape' | 'enter'
type SwipeDirection = 'up' | 'down' | 'left' | 'right'

/**
 * A handle to a running app on the simulator / emulator.
 */
export class App {
  constructor(
    public readonly bundleId: string,
    /** The napi driver (act + sense); a mock in tests, the real addon in prod. */
    public readonly driver: NodeDriver,
    /** The napi session this app was launched in (lifecycle verbs). */
    public readonly session: NodeSession,
    public readonly resolver: SelectorResolver,
    /**
     * Wraps `resolve_selector_labels`, backing toHaveCount and
     * toHaveLabel. The default throws rather than guessing: there is no
     * sensible answer without a resolver, and a silent empty list would
     * read as "no matches".
     */
    public readonly labelsResolver: LabelsResolver = async () => {
      throw new Error('labelsResolver not configured; pass one to Smix.launchApp or App constructor')
    },
  ) {}

  // ---- act methods --------------------------------------------------

  /**
   * Snapshot the tree, resolve `selector` against it, and return the first
   * matching id. Selectors resolve to an id on this side; only the id crosses
   * the napi boundary. No match throws ELEMENT_NOT_FOUND with the visible
   * elements so the failure is AI-readable.
   */
  private async resolveFirstOrThrow(selector: Selector): Promise<string> {
    const treeJson = await this.driver.snapshotTree()
    const selectorJson = encodeSelectorJson(selector)
    const ids = await this.resolver(treeJson, selectorJson)
    const first = ids[0]
    if (first === undefined) {
      const tree = JSON.parse(treeJson) as A11yNode
      throw new ExpectationFailure({
        code: 'ELEMENT_NOT_FOUND',
        message: `no element matched selector ${selectorJson}`,
        selectorJson,
        visibleElements: flatten(tree).slice(0, 20),
      })
    }
    return first
  }

  async tap(selector: Selector, _opts: { timeoutMs?: number } = {}): Promise<void> {
    await this.driver.tapById(await this.resolveFirstOrThrow(selector))
  }

  async fill(selector: Selector, text: string): Promise<void> {
    const id = await this.resolveFirstOrThrow(selector)
    await this.driver.tapById(id)
    await this.driver.inputText(text)
  }

  async pressKey(key: KeyName): Promise<void> {
    await this.driver.pressKey(key === 'enter' ? 'return' : key)
  }

  async swipe(direction: SwipeDirection): Promise<void> {
    await this.driver.swipe(direction)
  }

  async screenshot(): Promise<Uint8Array> {
    // No runner wire route yet — singled out to a later checkpoint (v2.md
    // decision, 2026-07-24). The blocker is the wire, not napi.
    throw new SmixNotImplementedError('wire', 'App.screenshot')
  }

  /** Tap at a normalized coordinate (0..1) — escape hatch per §9 #3. */
  async tapAtCoord(nx: number, ny: number): Promise<void> {
    if (nx < 0 || nx > 1 || ny < 0 || ny > 1) {
      throw new ExpectationFailure({
        code: 'ASSERTION_FAILED',
        message: `tapAtCoord expects normalized 0..1 coordinates, got (${nx}, ${ny})`,
      })
    }
    await this.driver.tapAtCoord(nx, ny)
  }

  /**
   * Swipe between two normalized points (0..1) — escape hatch per §9 #3,
   * authorised on the same grounds as tapAtCoord. Two points, because a
   * swipe is a path: tap's single-point shape does not describe one.
   */
  async swipeAtCoord(from: [number, number], to: [number, number]): Promise<void> {
    for (const [label, p] of [['from', from], ['to', to]] as const) {
      const [x, y] = p
      if (x < 0 || x > 1 || y < 0 || y > 1) {
        throw new ExpectationFailure({
          code: 'ASSERTION_FAILED',
          message: `swipeAtCoord expects normalized 0..1 coordinates, got ${label}=(${x}, ${y})`,
        })
      }
    }
    await this.driver.swipeAtCoord(from[0], from[1], to[0], to[1])
  }

  async terminate(): Promise<void> {
    await this.session.terminateApp()
  }

  async relaunch(): Promise<void> {
    await this.session.relaunchApp()
  }

  async launchFresh(_opts: {
    clearState?: boolean
    clearKeychain?: boolean
    appPath?: string
  } = {}): Promise<void> {
    // Clearing state / installing an appPath is host-side (simctl), with no
    // runner wire — singled out to a later checkpoint. The blocker is the
    // host, not napi.
    throw new SmixNotImplementedError('host', 'App.launchFresh')
  }

  // ---- sense methods ------------------------------------------------

  find(selector: Selector): Locator {
    return new Locator(this, selector)
  }

  /**
   * The tree seam Locator polls. Pending napi: without an FFI path there is
   * no live simulator to snapshot. Lands with the napi axis.
   */
  async snapshotTree(): Promise<A11yNode> {
    return JSON.parse(await this.driver.snapshotTree()) as A11yNode
  }

  async tree(): Promise<A11yNode> {
    return this.snapshotTree()
  }

  async systemPopups(): Promise<SystemPopup[]> {
    return JSON.parse(await this.driver.systemPopups()) as SystemPopup[]
  }

  async openUrl(_url: string): Promise<void> {
    // No runner wire route yet — singled out to a later checkpoint. The
    // blocker is the wire, not napi.
    throw new SmixNotImplementedError('wire', 'App.openUrl')
  }
}

export { SmixNotImplementedError }
