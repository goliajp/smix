import { ElementHandle } from './element.js'
import { ElementExpectation, ScreenExpectation } from './matchers.js'
import type { ScreenDescription } from '../core/index.js'

/**
 * Custom expect, deliberately not Vitest/Jest based.
 *
 * Why not reuse: their matchers throw AssertionError with text diff,
 * which is opaque to AI agents. Our matchers throw ExpectationFailure
 * with structured visibleElements + screenshot + suggestions — paste
 * one of these back to a coding agent and it can self-correct.
 *
 * Surface (intentionally narrow):
 *   expect(app.element({...})).toBeVisible()
 *   expect(app.element({...})).toHaveText('...')
 *   expect(await app.describe()).toMatchSnapshot('name')
 */
export function expect(handle: ElementHandle): ElementExpectation
export function expect(screen: ScreenDescription): ScreenExpectation
export function expect(target: ElementHandle | ScreenDescription): unknown {
  if (target instanceof ElementHandle) {
    return new ElementExpectation(target)
  }
  return new ScreenExpectation(target)
}
