// Wire tests for the full act/sense API.

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

describe('App.fill / pressKey', () => {
  test('fill = tap-to-focus + sendString', async () => {
    const field: A11yNode = {
      rawType: 'textField',
      identifier: 'input-username',
      label: 'Username',
      bounds: { x: 50, y: 200, w: 200, h: 40 },
      enabled: true,
      visible: true,
    }
    const tree: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 393, h: 852 },
      visible: true,
      children: [field],
    }
    const runtime = new MockSimRuntime({ snapshotResult: tree })
    const resolver = new MockSelectorResolver()
    resolver.registerHit('{"id":"input-username"}', 'input-username')
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)

    await app.fill(Selector.id('input-username'), 'alice')

    expect(runtime.tapCalls.length).toBe(1)
    expect(runtime.tapCalls[0]?.x).toBeCloseTo(150, 1)
    expect(runtime.tapCalls[0]?.y).toBeCloseTo(220, 1)
    expect(runtime.sendStringCalls).toEqual(['alice'])
  })

  test('pressKey delegates to runtime', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await app.pressKey('return')
    await app.pressKey('delete')
    expect(runtime.pressKeyCalls).toEqual(['return', 'delete'])
  })

  test('fill failure surfaces ExpectationFailure (no sendString fired)', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await expect(app.fill(Selector.id('missing'), 'text')).rejects.toBeInstanceOf(ExpectationFailure)
    expect(runtime.sendStringCalls).toEqual([])
  })
})

describe('App.swipe / screenshot / systemPopups / openUrl', () => {
  test('swipe delegates with 4 directions', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await app.swipe('down')
    await app.swipe('up')
    await app.swipe('left')
    await app.swipe('right')
    expect(runtime.swipeCalls).toEqual(['down', 'up', 'left', 'right'])
  })

  test('screenshot returns runtime bytes', async () => {
    const expected = new Uint8Array([0x89, 0x50, 0x4e, 0x47])
    const runtime = new MockSimRuntime()
    runtime.screenshotResult = expected
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    const out = await app.screenshot()
    expect(out).toEqual(expected)
    expect(runtime.screenshotCalls).toBe(1)
  })

  test('systemPopups returns runtime list', async () => {
    const alert: A11yNode = {
      rawType: 'alert',
      identifier: 'alert-permission',
      bounds: { x: 0, y: 0, w: 200, h: 100 },
    }
    const runtime = new MockSimRuntime()
    runtime.systemPopupsResult = [alert]
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    const popups = await app.systemPopups()
    expect(popups.length).toBe(1)
    expect(popups[0]?.identifier).toBe('alert-permission')
  })

  test('openUrl forwards to runtime', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await app.openUrl('smix://deep/link?id=42')
    expect(runtime.openUrlCalls).toEqual(['smix://deep/link?id=42'])
  })
})

describe('App.launchFresh', () => {
  test('clearState + clearKeychain', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await app.launchFresh({ clearState: true, clearKeychain: true })
    expect(runtime.launchFreshCalls.length).toBe(1)
    const call = runtime.launchFreshCalls[0]!
    expect(call.bundleId).toBe('dev.smix.fixture')
    expect(call.clearState).toBe(true)
    expect(call.clearKeychain).toBe(true)
    expect(call.appPath).toBeUndefined()
  })

  test('with appPath override', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await app.launchFresh({ appPath: '/tmp/Reinstall.apk' })
    expect(runtime.launchFreshCalls[0]?.appPath).toBe('/tmp/Reinstall.apk')
  })

  test('defaults are false', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await app.launchFresh()
    const call = runtime.launchFreshCalls[0]!
    expect(call.clearState).toBe(false)
    expect(call.clearKeychain).toBe(false)
  })
})

describe('App.tapAtCoord', () => {
  test('valid range', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await app.tapAtCoord(0.5, 0.5)
    expect(runtime.tapAtNormalizedCalls.length).toBe(1)
    expect(runtime.tapAtNormalizedCalls[0]).toEqual({ nx: 0.5, ny: 0.5 })
  })

  test('edge values 0,0 and 1,1', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await app.tapAtCoord(0, 0)
    await app.tapAtCoord(1, 1)
    expect(runtime.tapAtNormalizedCalls.length).toBe(2)
  })

  test('out of range nx throws wrongState', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await expect(app.tapAtCoord(1.5, 0.5)).rejects.toMatchObject({
      name: 'ExpectationFailure',
      code: 'wrongState',
    })
    expect(runtime.tapAtNormalizedCalls).toEqual([])
  })

  test('negative ny throws wrongState', async () => {
    const runtime = new MockSimRuntime()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, new MockSelectorResolver().resolve)
    await expect(app.tapAtCoord(0.5, -0.1)).rejects.toMatchObject({
      code: 'wrongState',
    })
  })
})
