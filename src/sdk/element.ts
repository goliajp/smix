import type { Driver, FindOptions } from '../driver/index.js'
import type { Selector, A11yNode } from '../core/index.js'

/**
 * A deferred reference to a UI element. Resolves lazily by re-querying
 * on each await — so tests stay correct even if the screen changes
 * between matchers.
 */
export class ElementHandle {
  constructor(
    readonly driver: Driver,
    readonly selector: Selector,
  ) {}

  /** Resolve once; returns null if the element is not present. */
  async resolve(opts?: FindOptions): Promise<A11yNode | null> {
    return this.driver.findOne(this.selector, opts)
  }

  /** Resolve all matching nodes (for count assertions). */
  async resolveAll(): Promise<A11yNode[]> {
    return this.driver.findAll(this.selector)
  }
}
