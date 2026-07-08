// v7.5 c3 — Locator poll loop tests, mirror Kotlin LocatorMockTest +
// Swift LocatorMockTests.

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

describe('Locator.toBeVisible', () => {
  test('succeeds when initially visible', async () => {
    const node: A11yNode = {
      rawType: 'button',
      identifier: 'btn-x',
      label: 'x',
      bounds: { x: 0, y: 0, w: 10, h: 10 },
      visible: true,
    }
    const tree: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 100, h: 100 },
      visible: true,
      children: [node],
    }
    const runtime = new MockSimRuntime({ snapshotResult: tree })
    const resolver = new MockSelectorResolver()
    resolver.registerHit('{"id":"btn-x"}', 'btn-x')
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)
    await app.find(Selector.id('btn-x')).toBeVisible({ timeoutMs: 500 })
  })

  test('timeout when never matches', async () => {
    const tree: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 100, h: 100 },
      visible: true,
      children: [],
    }
    const runtime = new MockSimRuntime({ snapshotResult: tree })
    const resolver = new MockSelectorResolver()
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)
    await expect(
      app.find(Selector.id('btn-missing')).toBeVisible({ timeoutMs: 300 }),
    ).rejects.toMatchObject({
      name: 'ExpectationFailure',
      code: 'timeout',
    })
  })

  test('wrongState when matched but hidden', async () => {
    const node: A11yNode = {
      rawType: 'button',
      identifier: 'btn-y',
      label: 'y',
      bounds: { x: 0, y: 0, w: 10, h: 10 },
      visible: false,
    }
    const tree: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 100, h: 100 },
      visible: true,
      children: [node],
    }
    const runtime = new MockSimRuntime({ snapshotResult: tree })
    const resolver = new MockSelectorResolver()
    resolver.registerHit('{"id":"btn-y"}', 'btn-y')
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)
    await expect(
      app.find(Selector.id('btn-y')).toBeVisible({ timeoutMs: 200 }),
    ).rejects.toMatchObject({
      name: 'ExpectationFailure',
      code: 'wrongState',
    })
  })
})

describe('Locator.toContainText', () => {
  test('succeeds when label matches', async () => {
    const node: A11yNode = {
      rawType: 'staticText',
      identifier: 'msg',
      label: 'Welcome back, Alice!',
      bounds: { x: 0, y: 0, w: 100, h: 24 },
      visible: true,
    }
    const tree: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 100, h: 100 },
      children: [node],
    }
    const runtime = new MockSimRuntime({ snapshotResult: tree })
    const resolver = new MockSelectorResolver()
    resolver.registerHit('{"id":"msg"}', 'msg')
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)
    await app.find(Selector.id('msg')).toContainText('Alice', { timeoutMs: 500 })
  })

  test('timeout when never matches', async () => {
    const node: A11yNode = {
      rawType: 'staticText',
      identifier: 'msg',
      label: 'Hello, World',
      bounds: { x: 0, y: 0, w: 100, h: 24 },
      visible: true,
    }
    const tree: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 100, h: 100 },
      children: [node],
    }
    const runtime = new MockSimRuntime({ snapshotResult: tree })
    const resolver = new MockSelectorResolver()
    resolver.registerHit('{"id":"msg"}', 'msg')
    const app = await Smix.launchApp(bundleId('dev.smix.fixture'), runtime, resolver.resolve)
    await expect(
      app.find(Selector.id('msg')).toContainText('Goodbye', { timeoutMs: 200 }),
    ).rejects.toMatchObject({ code: 'timeout' })
  })
})

// v7.6 c1 update: toHaveLabel + toHaveCount stubs replaced — see
// LocatorToHave.test.ts for the c1 replacement tests.
