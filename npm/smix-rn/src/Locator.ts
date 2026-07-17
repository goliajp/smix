// 250ms tick over a millisecond deadline. On timeout, throws
// ExpectationFailure with the last tree's first-20 visible elements
// populated for AI agent diagnosis.

import { findById, flatten, type A11yNode } from './A11yNode.js'
import type { App } from './App.js'
import { ExpectationFailure } from './ExpectationFailure.js'
import {
  encodeSelectorJson,
  type Selector,
} from './Selector.js'

const POLL_INTERVAL_MS = 250
const DEFAULT_TIMEOUT_MS = 5_000

export class Locator {
  constructor(
    public readonly app: App,
    public readonly selector: Selector,
  ) {}

  /**
   * Resolve [selector] until at least one match exists AND that match
   * is `visible=true`. Polls every 250ms until [timeoutMs] expires.
   */
  async toBeVisible(opts: { timeoutMs?: number } = {}): Promise<void> {
    return this.pollUntil({
      timeoutMs: opts.timeoutMs ?? DEFAULT_TIMEOUT_MS,
      wrongStatePredicate: node => node !== null && node.visible !== true,
      successPredicate: node => node !== null && node.visible === true,
      timeoutMessage: 'element never became visible',
    })
  }

  /**
   * Resolve [selector] until a match exists AND `text.includes(needle)`
   * on label/title/text fields.
   */
  async toContainText(text: string, opts: { timeoutMs?: number } = {}): Promise<void> {
    return this.pollUntil({
      timeoutMs: opts.timeoutMs ?? DEFAULT_TIMEOUT_MS,
      wrongStatePredicate: () => false,
      successPredicate: node =>
        node !== null && (
          node.label?.includes(text) === true ||
          node.title?.includes(text) === true ||
          node.text?.includes(text) === true
        ),
      timeoutMessage: `element text never contained "${text}"`,
    })
  }

  /**
   * Resolve [selector] until a match exists and its label equals
   * [label]. Reads the label off the snapshot node rather than the
   * labels resolver, which answers about the whole match set.
   */
  async toHaveLabel(label: string, opts: { timeoutMs?: number } = {}): Promise<void> {
    return this.pollUntil({
      timeoutMs: opts.timeoutMs ?? DEFAULT_TIMEOUT_MS,
      wrongStatePredicate: () => false,
      successPredicate: node => node !== null && node.label === label,
      timeoutMessage: `element label never equaled "${label}"`,
    })
  }

  /**
   * Resolve [selector] until the labels resolver returns exactly [n]
   * matches. Goes through the labels resolver so the count ignores any
   * index modifier — counting asks about all matches, as in Playwright.
   */
  async toHaveCount(n: number, opts: { timeoutMs?: number } = {}): Promise<void> {
    const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS
    const deadline = Date.now() + timeoutMs
    let lastCount = 0
    let lastTree: A11yNode | null = null
    const selectorJson = encodeSelectorJson(this.selector)
    while (true) {
      const tree = await this.app.runtime.snapshotTree()
      lastTree = tree
      let labels: readonly string[]
      try {
        labels = await this.app.labelsResolver(JSON.stringify(tree), selectorJson)
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e)
        throw new ExpectationFailure({
          code: 'unknown',
          message: `FFI resolveSelectorLabels raised during poll: ${msg}`,
          selectorJson,
        })
      }
      lastCount = labels.length
      if (lastCount === n) return
      if (Date.now() >= deadline) break
      await delay(POLL_INTERVAL_MS)
    }
    throw new ExpectationFailure({
      code: 'wrongState',
      message: `expected ${n} matches, last poll saw ${lastCount}`,
      selectorJson,
      visibleElements: lastTree !== null ? flatten(lastTree).slice(0, 20) : [],
      suggestions: [
        'verify the selector matches the expected count in the current tree',
        'consider extending the timeout if render is slow',
      ],
    })
  }

  /**
   * Generic 250ms poll loop. On each tick, resolve [selector] via the
   * app's runtime + resolver, then check predicates.
   *
   * - [successPredicate] returns true → return immediately
   * - [wrongStatePredicate] returns true → throw wrongState
   * - Otherwise, continue polling.
   *
   * On timeout, throws timeout with the last tree's first-20 nodes.
   */
  private async pollUntil(opts: {
    timeoutMs: number
    wrongStatePredicate: (node: A11yNode | null) => boolean
    successPredicate: (node: A11yNode | null) => boolean
    timeoutMessage: string
  }): Promise<void> {
    const deadline = Date.now() + opts.timeoutMs
    let lastTree: A11yNode | null = null

    while (true) {
      const tree = await this.app.runtime.snapshotTree()
      lastTree = tree
      const selectorJson = encodeSelectorJson(this.selector)
      let matches: readonly string[]
      try {
        matches = await this.app.resolver(JSON.stringify(tree), selectorJson)
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e)
        throw new ExpectationFailure({
          code: 'unknown',
          message: `FFI resolveSelector raised during poll: ${msg}`,
          selectorJson,
        })
      }
      const firstId = matches[0]
      const node = firstId !== undefined ? findById(tree, firstId) : null

      if (opts.wrongStatePredicate(node)) {
        throw new ExpectationFailure({
          code: 'wrongState',
          message: 'element matched but predicate failed (e.g. not visible)',
          selectorJson,
          visibleElements: node !== null ? [node, ...flatten(tree).slice(0, 19)] : flatten(tree).slice(0, 20),
        })
      }
      if (opts.successPredicate(node)) return

      if (Date.now() >= deadline) break
      await delay(POLL_INTERVAL_MS)
    }

    throw new ExpectationFailure({
      code: 'timeout',
      message: opts.timeoutMessage,
      selectorJson: encodeSelectorJson(this.selector),
      visibleElements: lastTree !== null ? flatten(lastTree).slice(0, 20) : [],
      suggestions: [
        'consider extending the timeout if the UI takes longer to settle',
        'verify the selector matches when manually inspected (use app.tree())',
      ],
    })
  }
}

function delay(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms))
}

/**
 * Compile-time-known unimplemented surface (distinct from
 * ExpectationFailure which is runtime resolution failure).
 */
export class SmixNotImplementedError extends Error {
  constructor(public readonly stage: string, public readonly api: string) {
    super(`${api} not implemented yet (lands ${stage})`)
    this.name = 'SmixNotImplementedError'
  }
}
