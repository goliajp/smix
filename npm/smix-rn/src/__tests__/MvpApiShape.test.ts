// API-shape tests. Verifies the SDK exposes the right TypeScript types +
// constructors, and that the live driving / sense surface — pending the
// napi axis — throws SmixNotImplementedError rather than posting to a
// route the runner never served.

import { describe, expect, test } from 'vitest'
import {
  App,
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
  test('selector base cases exhaustive', () => {
    const cases: Selector[] = [
      Selector.id('btn-login'),
      Selector.text(literal('Sign In')),
      Selector.label('Settings'),
      Selector.role('button'),
      Selector.role('button', literal('Submit')),
    ]
    expect(cases.length).toBe(5)
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
    expect(roles.length).toBe(28)
  })
})

describe('FailureCode', () => {
  test('6 cases exposed', () => {
    expect(FAILURE_CODES.length).toBe(6)
    const expected = new Set<FailureCode>([
      'notFound', 'ambiguous', 'notInteractable',
      'timeout', 'wrongState', 'unknown',
    ])
    expect(new Set(FAILURE_CODES)).toEqual(expected)
  })

  test('toJson() emits stable keys', () => {
    const failure = new ExpectationFailure({
      code: 'notFound',
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

  test('toJson() camelCase FailureCode raw value', () => {
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
