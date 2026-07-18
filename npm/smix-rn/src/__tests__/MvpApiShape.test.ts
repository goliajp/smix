// API-shape tests. Verifies the SDK exposes the right TypeScript types +
// constructors, and that the live driving / sense surface — pending the
// napi axis — throws SmixNotImplementedError rather than posting to a
// route the runner never served.

import { describe, expect, test } from 'vitest'
import {
  App,
  encodeSelectorJson,
  ExpectationFailure,
  FAILURE_CODES,
  literal,
  MockSelectorResolver,
  Selector,
  Smix,
  SmixNotImplementedError,
  bundleId,
  type A11yRole,
  type FailureCode,
} from '../index.js'

describe('Selector base cases', () => {
  // Each constructor is asserted by the wire JSON it encodes to — a
  // previous version built the five selectors and asserted the array
  // had five elements, which a broken encoder passes.
  test('every base constructor encodes its wire discriminator', () => {
    const byWire: Array<[Selector, Record<string, unknown>]> = [
      [Selector.id('btn-login'), { id: 'btn-login' }],
      [Selector.text(literal('Sign In')), { text: 'Sign In' }],
      [Selector.label('Settings'), { label: 'Settings' }],
      [Selector.role('button'), { role: 'button' }],
      [Selector.role('button', literal('Submit')), { role: 'button', name: 'Submit' }],
    ]
    for (const [sel, wire] of byWire) {
      expect(JSON.parse(encodeSelectorJson(sel))).toEqual(wire)
    }
  })

  test('Selector.id encodes as {"id":...}', () => {
    const sel = Selector.id('btn-login')
    expect(sel.data.kind).toBe('id')
    if (sel.data.kind === 'id') expect(sel.data.id).toBe('btn-login')
  })
})

describe('A11yRole', () => {
  test('camelCase wire values exposed as type union', () => {
    const roles: A11yRole[] = [
      'button', 'link', 'textField', 'secureTextField', 'searchField',
      'switch', 'toggle', 'checkBox', 'radio', 'image', 'staticText',
      'tab', 'tabBar', 'navigationBar', 'cell', 'alert', 'dialog',
      'slider', 'progressBar', 'picker', 'menu', 'menuItem',
      'scrollView', 'segmentedControl', 'table', 'collectionView',
      'webView', 'keyboard',
    ]
    // The list's value is the type annotation above — each string must
    // typecheck as A11yRole. The runtime assertion is only that the
    // fixture has no duplicate entries; asserting its length proved
    // nothing (the old `toBe(28)` passed with 28 wrong strings).
    expect(new Set(roles).size).toBe(roles.length)
  })
})

describe('FailureCode', () => {
  // The vocabulary is Rust's — smix_error::FailureCode. Rust is the
  // source; crates/smix-error/tests/sdk_failure_code_parity.rs reads
  // this SDK's declaration and fails if the two ever diverge.
  test('9 cases exposed, matching the Rust wire vocabulary', () => {
    expect(FAILURE_CODES.length).toBe(9)
    const expected = new Set<FailureCode>([
      'ELEMENT_NOT_FOUND', 'NOT_VISIBLE', 'NOT_ENABLED',
      'AMBIGUOUS', 'TIMEOUT', 'ASSERTION_FAILED',
      'APP_NOT_RUNNING', 'SIMULATOR_NOT_BOOTED', 'DRIVER_ERROR',
    ])
    expect(new Set(FAILURE_CODES)).toEqual(expected)
  })

  test('toJson() emits stable keys', () => {
    const failure = new ExpectationFailure({
      code: 'ELEMENT_NOT_FOUND',
      message: 'no candidates',
      selectorJson: '{"id":"btn"}',
      suggestions: ['check id'],
      timestamp: 1_780_000_000_000,
    })
    const json = failure.toJson()
    for (const key of ['code', 'message', 'selector', 'visibleElements', 'suggestions', 'timestamp']) {
      const count = json.split(`"${key}":`).length - 1
      expect(count, `key '${key}' should appear exactly once`).toBe(1)
    }
  })

  test('toJson() emits the Rust wire FailureCode value', () => {
    for (const code of FAILURE_CODES) {
      const f = new ExpectationFailure({ code, message: '' })
      expect(f.toJson()).toContain(`"${code}"`)
    }
  })
})

describe('pending-napi surface throws SmixNotImplementedError', () => {
  test('Smix.launchApp throws napi', async () => {
    await expect(
      Smix.launchApp(bundleId('dev.smix.target'), new MockSelectorResolver().resolve),
    ).rejects.toBeInstanceOf(SmixNotImplementedError)
  })

  test('App act + sense methods throw napi', async () => {
    const app = new App('dev.smix.target', new MockSelectorResolver().resolve)
    await expect(app.tap(Selector.id('x'))).rejects.toBeInstanceOf(SmixNotImplementedError)
    await expect(app.fill(Selector.id('x'), 't')).rejects.toBeInstanceOf(SmixNotImplementedError)
    await expect(app.snapshotTree()).rejects.toBeInstanceOf(SmixNotImplementedError)
    await expect(app.tree()).rejects.toBeInstanceOf(SmixNotImplementedError)
    await expect(app.systemPopups()).rejects.toBeInstanceOf(SmixNotImplementedError)
  })

  test('Locator assertions throw napi at the tree seam', async () => {
    const app = new App('dev.smix.target', new MockSelectorResolver().resolve)
    await expect(
      app.find(Selector.id('x')).toBeVisible({ timeoutMs: 50 }),
    ).rejects.toBeInstanceOf(SmixNotImplementedError)
  })
})
