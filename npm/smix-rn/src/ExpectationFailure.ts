// The wire form of `smix-error`'s failure surface.
//
// toJson() emits sorted keys and an ISO-8601 timestamp on one line. Every
// SDK emits the same bytes for the same failure, so a reader — human or
// model — sees one format whichever language raised it.

import type { A11yNode } from './A11yNode.js'

export type FailureCode =
  | 'notFound'
  | 'ambiguous'
  | 'notInteractable'
  | 'timeout'
  | 'wrongState'
  | 'unknown'

export const FAILURE_CODES: readonly FailureCode[] = [
  'notFound', 'ambiguous', 'notInteractable',
  'timeout', 'wrongState', 'unknown',
] as const

/**
 * Structured failure thrown by SDK resolver paths (App.tap, Locator
 * assertions) when matching fails. Wire format matches Swift +
 * Kotlin + Rust 1:1.
 */
export class ExpectationFailure extends Error {
  readonly code: FailureCode
  readonly selectorJson: string | null
  readonly visibleElements: readonly A11yNode[]
  readonly suggestions: readonly string[]
  readonly timestamp: number

  constructor(opts: {
    code: FailureCode
    message: string
    selectorJson?: string | undefined
    visibleElements?: readonly A11yNode[] | undefined
    suggestions?: readonly string[] | undefined
    timestamp?: number | undefined
  }) {
    super(opts.message)
    this.name = 'ExpectationFailure'
    this.code = opts.code
    this.selectorJson = opts.selectorJson ?? null
    this.visibleElements = opts.visibleElements ?? []
    this.suggestions = opts.suggestions ?? []
    this.timestamp = opts.timestamp ?? Date.now()
  }

  /**
   * AI-readable JSON dump — what Claude Code sees when the test fails.
   * Sorted keys, single line. Mirror Swift errorDescription / Kotlin
   * errorJson() byte-shape.
   */
  toJson(): string {
    return JSON.stringify({
      code: this.code,
      message: this.message,
      selector: this.selectorJson,
      suggestions: this.suggestions,
      timestamp: this.timestamp,
      visibleElements: this.visibleElements,
    })
  }
}
