// The wire form of `smix-error`'s failure surface.
//
// toJson() emits sorted keys and an ISO-8601 timestamp on one line. Every
// SDK emits the same bytes for the same failure, so a reader — human or
// model — sees one format whichever language raised it.

import type { A11yNode } from './A11yNode.js'

/**
 * Rust `smix_error::FailureCode`'s wire strings verbatim —
 * `crates/smix-error/tests/sdk_failure_code_parity.rs` reads this
 * declaration and fails if the two sets ever diverge.
 */
export type FailureCode =
  | 'ELEMENT_NOT_FOUND'
  | 'NOT_VISIBLE'
  | 'NOT_ENABLED'
  | 'AMBIGUOUS'
  | 'TIMEOUT'
  | 'ASSERTION_FAILED'
  | 'APP_NOT_RUNNING'
  | 'SIMULATOR_NOT_BOOTED'
  /** The touch was synthesised, and it did not land inside the element the selector matched. Distinct from element-not-found: not-found means fix the selector, missed means the element was there and the touch went elsewhere. */
  | 'TAP_MISSED'
  // The screen is described in one coordinate space and the touch would be delivered in another, so no aim can land where the tree says the element is. Distinct from tap-missed: a miss invites another attempt with a better point, and there is no better point here — whatever is passed gets recomputed against the app's frame and then read against the device's.
  | 'COORDINATE_SPACE_MISMATCH'
  | 'DRIVER_ERROR'
  // The device's capture path is under load and refusing frames for a stated window. Not a defect and not a driver error: it means "not now, try again shortly", so a caller with time left can keep waiting rather than fail.
  | 'CAPTURE_BACKPRESSURE'

export const FAILURE_CODES: readonly FailureCode[] = [
  'ELEMENT_NOT_FOUND', 'NOT_VISIBLE', 'NOT_ENABLED',
  'AMBIGUOUS', 'TIMEOUT', 'ASSERTION_FAILED',
  'APP_NOT_RUNNING', 'SIMULATOR_NOT_BOOTED', 'TAP_MISSED',
  'COORDINATE_SPACE_MISMATCH', 'DRIVER_ERROR', 'CAPTURE_BACKPRESSURE',
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
