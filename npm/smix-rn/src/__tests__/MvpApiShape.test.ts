// v7.5 c1 — MVP shape JVM vitest tests.
//
// Mirror Kotlin MvpApiShapeTest + Swift MvpApiShapeTests. Verifies
// the SDK exposes the right TypeScript types + constructors and that
// stub surface throws SmixNotImplementedError with explicit stage
// markers.

import { describe, expect, test } from 'vitest'
import {
  App,
  ExpectationFailure,
  FAILURE_CODES,
  literal,
  MockSelectorResolver,
  MockSimRuntime,
  Selector,
  Smix,
  SmixNotImplementedError,
  appPath,
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

// v7.6 c1 update: toHaveLabel + toHaveCount stubs replaced with real
// wires via dedicated FFI resolve_selector_count / resolve_selector_labels.
// All SDK API stubs from v7.5 c1 are now wired.

describe('Smix.launchApp wire (v7.5 c1)', () => {
  test('launchApp(.bundleId) calls runtime.launch', async () => {
    const runtime = new MockSimRuntime()
    await Smix.launchApp(bundleId('dev.smix.target'), runtime, new MockSelectorResolver().resolve)
    expect(runtime.launchCalls).toEqual(['dev.smix.target'])
  })

  test('launchApp(.appPath) dispatches to launchFromPath (mirror Swift v7.2 c5)', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(appPath('/tmp/Foo.apk'), runtime, new MockSelectorResolver().resolve)
    expect(runtime.launchFromPathCalls).toEqual(['/tmp/Foo.apk'])
    expect(runtime.launchCalls).toEqual([])
    expect(app.bundleId).toBe('/tmp/Foo.apk')
  })

  test('relaunch issues terminate + launch', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.target'), runtime, new MockSelectorResolver().resolve)
    await app.relaunch()
    expect(runtime.launchCalls).toEqual(['dev.smix.target', 'dev.smix.target'])
    expect(runtime.terminateCalls).toEqual(['dev.smix.target'])
  })
})
