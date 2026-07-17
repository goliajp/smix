import { type A11yNode, findById, flatten } from './A11yNode.js'
import { ExpectationFailure } from './ExpectationFailure.js'
import { Locator, SmixNotImplementedError } from './Locator.js'
import {
  encodeSelectorJson,
  type Selector,
} from './Selector.js'
import {
  type KeyName,
  type SmixSimRuntime,
  type SwipeDirection,
} from './SimRuntime.js'
import type { LabelsResolver, SelectorResolver } from './SelectorResolver.js'

/**
 * A handle to a running app on the simulator / emulator.
 */
export class App {
  constructor(
    public readonly bundleId: string,
    public readonly runtime: SmixSimRuntime,
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

  async tap(selector: Selector, _opts: { timeoutMs?: number } = {}): Promise<void> {
    const tree = await this.runtime.snapshotTree()
    const selectorJson = encodeSelectorJson(selector)
    const treeJson = JSON.stringify(tree)

    let matches: readonly string[]
    try {
      matches = await this.resolver(treeJson, selectorJson)
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      throw new ExpectationFailure({
        code: 'unknown',
        message: `FFI resolveSelector raised: ${msg}`,
        selectorJson,
      })
    }

    const firstId = matches[0]
    const node = firstId !== undefined ? findById(tree, firstId) : null
    if (node === null) {
      throw new ExpectationFailure({
        code: 'notFound',
        message: 'no candidates after spatial + index filters',
        selectorJson,
        visibleElements: flatten(tree).slice(0, 20),
        suggestions: [
          'check `accessibilityIdentifier` is set on the target view',
          'consider relaxing to text(...) if the label is more stable than the id',
        ],
      })
    }

    await this.runtime.synthesizeTap(
      node.bounds.x + node.bounds.w / 2,
      node.bounds.y + node.bounds.h / 2,
    )
  }

  /**
   * Resolve [selector], tap to focus, then type [text].
   */
  async fill(selector: Selector, text: string): Promise<void> {
    await this.tap(selector)
    await this.runtime.sendString(text)
  }

  async pressKey(key: KeyName): Promise<void> {
    await this.runtime.pressKey(key)
  }

  async swipe(direction: SwipeDirection): Promise<void> {
    await this.runtime.swipe(direction)
  }

  async screenshot(): Promise<Uint8Array> {
    return this.runtime.screenshot()
  }

  /**
   * Tap at a normalized coordinate (0..1) — escape hatch per §9 #3.
   * Out-of-range values throw ExpectationFailure code=wrongState.
   */
  async tapAtCoord(nx: number, ny: number): Promise<void> {
    if (nx < 0 || nx > 1 || ny < 0 || ny > 1) {
      throw new ExpectationFailure({
        code: 'wrongState',
        message: `tapAtCoord arguments out of [0,1] range: nx=${nx}, ny=${ny}`,
        suggestions: ['use normalized 0..1 — for raw points, divide by viewport size'],
      })
    }
    await this.runtime.synthesizeTapAtNormalized(nx, ny)
  }

  async terminate(): Promise<void> {
    await this.runtime.terminate(this.bundleId)
  }

  async relaunch(): Promise<void> {
    await this.runtime.terminate(this.bundleId)
    await this.runtime.launch(this.bundleId)
  }

  async launchFresh(opts: {
    clearState?: boolean
    clearKeychain?: boolean
    appPath?: string
  } = {}): Promise<void> {
    await this.runtime.launchFresh({
      bundleId: this.bundleId,
      clearState: opts.clearState ?? false,
      clearKeychain: opts.clearKeychain ?? false,
      appPath: opts.appPath,
    })
  }

  // ---- sense methods ------------------------------------------------

  find(selector: Selector): Locator {
    return new Locator(this, selector)
  }

  async tree(): Promise<A11yNode> {
    return this.runtime.snapshotTree()
  }

  async systemPopups(): Promise<A11yNode[]> {
    return this.runtime.systemPopups()
  }

  async openUrl(url: string): Promise<void> {
    await this.runtime.openUrl(url)
  }
}

export { SmixNotImplementedError }
