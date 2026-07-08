// v7.5 c1 — App.tap mock-based unit tests.
//
// Mirror Kotlin AppTapMockTest + Swift AppTapMockTests. Verifies wire
// pipeline (snapshot → SelectorResolver → tap synthesize) end-to-end
// against MockSimRuntime + MockSelectorResolver.

import { describe, expect, test } from 'vitest'
import {
  bundleId,
  ExpectationFailure,
  literal,
  MockSelectorResolver,
  MockSimRuntime,
  Selector,
  Smix,
  type A11yNode,
} from '../index.js'

describe('App.tap notFound path', () => {
  test('empty resolver result yields notFound', async () => {
    const runtime = new MockSimRuntime()
    const resolver = new MockSelectorResolver()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)
    await expect(app.tap(Selector.id('btn-missing'))).rejects.toMatchObject({
      name: 'ExpectationFailure',
      code: 'notFound',
    })
  })
})

describe('App.tap hit path', () => {
  test('id hit synthesizes at bounds center', async () => {
    const button: A11yNode = {
      rawType: 'button',
      identifier: 'btn-login',
      label: 'Sign In',
      bounds: { x: 100, y: 200, w: 80, h: 40 },
      enabled: true,
      visible: true,
    }
    const root: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 393, h: 852 },
      enabled: true,
      visible: true,
      children: [button],
    }
    const runtime = new MockSimRuntime({ snapshotResult: root })
    const resolver = new MockSelectorResolver()
    resolver.registerHit('{"id":"btn-login"}', 'btn-login')
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)

    await app.tap(Selector.id('btn-login'))

    expect(runtime.tapCalls.length).toBe(1)
    expect(runtime.tapCalls[0]?.x).toBeCloseTo(140, 1)
    expect(runtime.tapCalls[0]?.y).toBeCloseTo(220, 1)
  })

  test('label hit synthesizes at center', async () => {
    const button: A11yNode = {
      rawType: 'button',
      identifier: 'btn-foo',
      label: 'Submit',
      bounds: { x: 50, y: 600, w: 100, h: 50 },
      visible: true,
    }
    const root: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 393, h: 852 },
      visible: true,
      children: [button],
    }
    const runtime = new MockSimRuntime({ snapshotResult: root })
    const resolver = new MockSelectorResolver()
    resolver.registerHit('{"label":"Submit"}', 'btn-foo')
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)

    await app.tap(Selector.label('Submit'))
    expect(runtime.tapCalls.length).toBe(1)
    expect(runtime.tapCalls[0]?.x).toBeCloseTo(100, 1)
  })
})

describe('App.tap passes correct selectorJson', () => {
  test('Rust-compatible untagged shape', async () => {
    const root: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 393, h: 852 },
      visible: true,
      children: [],
    }
    const runtime = new MockSimRuntime({ snapshotResult: root })
    const resolver = new MockSelectorResolver()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)

    await expect(app.tap(Selector.id('btn-x'))).rejects.toBeInstanceOf(ExpectationFailure)

    expect(resolver.calls.length).toBe(1)
    expect(resolver.calls[0]?.selectorJson).toBe('{"id":"btn-x"}')
  })
})

describe('App.tap resolver error surfaces as ExpectationFailure', () => {
  test('throws unknown when resolver raises', async () => {
    const root: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 393, h: 852 },
      visible: true,
      children: [],
    }
    const runtime = new MockSimRuntime({ snapshotResult: root })
    const resolver = new MockSelectorResolver()
    resolver.throwOnNext = new Error('simulated FFI parse error')
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)

    await expect(app.tap(Selector.id('anything'))).rejects.toMatchObject({
      name: 'ExpectationFailure',
      code: 'unknown',
    })
  })
})
