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
  MockNodeDriver,
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
  // The count is not hard-coded here. It used to be, and adding
  // TAP_MISSED to the vocabulary left this asserting 9 against 10 — a
  // second copy of the list that Rust's sdk_failure_code_parity gate
  // could not see, because that gate reads FAILURE_CODES, not this
  // test. So this checks the shape of FAILURE_CODES (no duplicates, a
  // couple of anchors present) and leaves the count to parity.
  test('the vocabulary is a set and carries its anchors', () => {
    expect(new Set(FAILURE_CODES).size).toBe(FAILURE_CODES.length)
    for (const code of ['ELEMENT_NOT_FOUND', 'TAP_MISSED', 'DRIVER_ERROR'] as FailureCode[]) {
      expect(FAILURE_CODES).toContain(code)
    }
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

describe('the driving surface works through the node seam; only host/wire gaps throw', () => {
  const makeApp = () => {
    const driver = new MockNodeDriver()
    return new App('dev.smix.target', driver, driver.session, new MockSelectorResolver().resolve)
  }

  test('Smix.launchApp drives through the node driver and returns an App', async () => {
    const app = await Smix.launchApp(bundleId('dev.smix.target'), {
      driver: new MockNodeDriver(),
      resolver: new MockSelectorResolver().resolve,
    })
    expect(app).toBeInstanceOf(App)
  })

  test('retired act + sense methods no longer throw SmixNotImplementedError', async () => {
    const app = makeApp()
    // tap with an empty resolver fails ELEMENT_NOT_FOUND — a real failure,
    // NOT SmixNotImplementedError. That distinction is the whole point.
    await expect(app.tap(Selector.id('x'))).rejects.not.toBeInstanceOf(SmixNotImplementedError)
    await expect(app.snapshotTree()).resolves.toBeDefined()
    await expect(app.tree()).resolves.toBeDefined()
    await expect(app.systemPopups()).resolves.toBeDefined()
  })

  test('screenshot / openUrl / launchFresh still throw, re-labeled off napi', async () => {
    const app = makeApp()
    const gaps: Array<[() => Promise<unknown>, string]> = [
      [() => app.screenshot(), 'wire'],
      [() => app.openUrl('https://x'), 'wire'],
      [() => app.launchFresh(), 'host'],
    ]
    for (const [call, stage] of gaps) {
      await expect(call()).rejects.toSatisfy(
        (e: unknown) => e instanceof SmixNotImplementedError && e.stage === stage,
      )
    }
  })

  test('Locator polls the real tree seam rather than throwing napi', async () => {
    const app = makeApp()
    await expect(
      app.find(Selector.id('x')).toBeVisible({ timeoutMs: 50 }),
    ).rejects.not.toBeInstanceOf(SmixNotImplementedError)
  })
})
