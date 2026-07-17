// Locator.toHaveCount and toHaveLabel, over mocks.

import { describe, expect, test } from 'vitest'
import {
  bundleId,
  literal,
  MockLabelsResolver,
  MockSelectorResolver,
  MockSimRuntime,
  Selector,
  Smix,
  type A11yNode,
} from '../index.js'

describe('Locator.toHaveCount', () => {
  test('succeeds when match count equals expected', async () => {
    const tree: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 100, h: 100 },
      visible: true,
      children: [],
    }
    const runtime = new MockSimRuntime({ snapshotResult: tree })
    const resolver = new MockSelectorResolver()
    const labelsResolver = new MockLabelsResolver()
    labelsResolver.registerLabels('{"label":"Item"}', ['Item', 'Item', 'Item'])
    const app = await Smix.launchApp(
      bundleId('dev.smix.fixture'),
      runtime,
      resolver.resolve,
      labelsResolver.resolve,
    )
    await app.find(Selector.label('Item')).toHaveCount(3, { timeoutMs: 500 })
  })

  test('throws wrongState when count mismatch', async () => {
    const tree: A11yNode = {
      rawType: 'other',
      bounds: { x: 0, y: 0, w: 100, h: 100 },
      visible: true,
      children: [],
    }
    const runtime = new MockSimRuntime({ snapshotResult: tree })
    const resolver = new MockSelectorResolver()
    const labelsResolver = new MockLabelsResolver()
    labelsResolver.registerLabels('{"label":"Item"}', ['Item'])  // 1 match, expect 3
    const app = await Smix.launchApp(
      bundleId('dev.smix.fixture'),
      runtime,
      resolver.resolve,
      labelsResolver.resolve,
    )
    await expect(
      app.find(Selector.label('Item')).toHaveCount(3, { timeoutMs: 200 }),
    ).rejects.toMatchObject({
      name: 'ExpectationFailure',
      code: 'wrongState',
    })
  })
})

describe('Locator.toHaveLabel', () => {
  test('succeeds when matched node label equals expected', async () => {
    const node: A11yNode = {
      rawType: 'button',
      identifier: 'btn-x',
      label: 'Submit',
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
    const app = await Smix.launchApp(
      bundleId('dev.smix.fixture'),
      runtime,
      resolver.resolve,
      new MockLabelsResolver().resolve,
    )
    await app.find(Selector.id('btn-x')).toHaveLabel('Submit', { timeoutMs: 500 })
  })

  test('timeout when label never matches', async () => {
    const node: A11yNode = {
      rawType: 'button',
      identifier: 'btn-x',
      label: 'Cancel',
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
    const app = await Smix.launchApp(
      bundleId('dev.smix.fixture'),
      runtime,
      resolver.resolve,
      new MockLabelsResolver().resolve,
    )
    await expect(
      app.find(Selector.id('btn-x')).toHaveLabel('Submit', { timeoutMs: 200 }),
    ).rejects.toMatchObject({
      code: 'timeout',
    })
  })
})
