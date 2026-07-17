// A handle to a running app. Live driving (tap/fill/swipe/...) and live
// sense (snapshotTree) are pending the napi axis: the TS SDK has no FFI
// path, so with the fictional HTTP wire deleted there is no way to reach a
// simulator from here yet. These methods throw SmixNotImplementedError
// rather than posting to a route the runner never served. What does work is
// the resolver seam: given a treeJson, `resolver` / `labelsResolver` resolve
// selectors against the runner's served /select/resolve* routes.

import { type A11yNode } from './A11yNode.js'
import { Locator, SmixNotImplementedError } from './Locator.js'
import { type Selector } from './Selector.js'
import type { LabelsResolver, SelectorResolver } from './SelectorResolver.js'

type KeyName = 'return' | 'delete' | 'space' | 'tab' | 'escape' | 'enter'
type SwipeDirection = 'up' | 'down' | 'left' | 'right'

/**
 * A handle to a running app on the simulator / emulator.
 */
export class App {
  constructor(
    public readonly bundleId: string,
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

  // ---- act methods (pending napi) -----------------------------------

  async tap(_selector: Selector, _opts: { timeoutMs?: number } = {}): Promise<void> {
    throw new SmixNotImplementedError('napi', 'App.tap')
  }

  async fill(_selector: Selector, _text: string): Promise<void> {
    throw new SmixNotImplementedError('napi', 'App.fill')
  }

  async pressKey(_key: KeyName): Promise<void> {
    throw new SmixNotImplementedError('napi', 'App.pressKey')
  }

  async swipe(_direction: SwipeDirection): Promise<void> {
    throw new SmixNotImplementedError('napi', 'App.swipe')
  }

  async screenshot(): Promise<Uint8Array> {
    throw new SmixNotImplementedError('napi', 'App.screenshot')
  }

  /** Tap at a normalized coordinate (0..1) — escape hatch per §9 #3. */
  async tapAtCoord(_nx: number, _ny: number): Promise<void> {
    throw new SmixNotImplementedError('napi', 'App.tapAtCoord')
  }

  async terminate(): Promise<void> {
    throw new SmixNotImplementedError('napi', 'App.terminate')
  }

  async relaunch(): Promise<void> {
    throw new SmixNotImplementedError('napi', 'App.relaunch')
  }

  async launchFresh(_opts: {
    clearState?: boolean
    clearKeychain?: boolean
    appPath?: string
  } = {}): Promise<void> {
    throw new SmixNotImplementedError('napi', 'App.launchFresh')
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
    throw new SmixNotImplementedError('napi', 'App.snapshotTree')
  }

  async tree(): Promise<A11yNode> {
    return this.snapshotTree()
  }

  async systemPopups(): Promise<A11yNode[]> {
    throw new SmixNotImplementedError('napi', 'App.systemPopups')
  }

  async openUrl(_url: string): Promise<void> {
    throw new SmixNotImplementedError('napi', 'App.openUrl')
  }
}

export { SmixNotImplementedError }
