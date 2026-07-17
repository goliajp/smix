// Demo flow: list scroll. Exercises App.swipe / Locator.toHaveCount /
// fluent chaining (Selector.id().below()).
//
// Scenario: scrollable list of items;swipe up to reveal more;assert
// total count + that specific item becomes visible after scroll.

import {
  bundleId,
  MockLabelsResolver,
  MockSelectorResolver,
  MockSimRuntime,
  Selector,
  Smix,
  type A11yNode,
} from '../../src/index.js'

function makeList(visibleStart: number, visibleCount: number, totalItems: number): A11yNode {
  const items: A11yNode[] = []
  for (let i = visibleStart; i < visibleStart + visibleCount && i < totalItems; i++) {
    items.push({
      rawType: 'cell',
      role: 'cell',
      identifier: `item-${i}`,
      label: `Item ${i}`,
      text: `Item ${i}`,
      bounds: { x: 0, y: 100 + (i - visibleStart) * 60, w: 393, h: 60 },
      enabled: true,
      visible: true,
    })
  }
  return {
    rawType: 'other',
    bounds: { x: 0, y: 0, w: 393, h: 852 },
    enabled: true,
    visible: true,
    children: [
      {
        rawType: 'scrollView',
        role: 'scrollView',
        identifier: 'scroll-list',
        bounds: { x: 0, y: 100, w: 393, h: 600 },
        enabled: true,
        visible: true,
        children: items,
      },
    ],
  }
}

const TOTAL = 20
let visibleStart = 0
const VISIBLE_PAGE = 5

const runtime = new MockSimRuntime({ snapshotResult: makeList(visibleStart, VISIBLE_PAGE, TOTAL) })
const resolver = new MockSelectorResolver()
const labelsResolver = new MockLabelsResolver()

// Pre-register hits for all 20 items (simulates Rust resolver behavior)
for (let i = 0; i < TOTAL; i++) {
  resolver.registerHit(`{"id":"item-${i}"}`, `item-${i}`)
}

// Labels resolver returns count = total items (not just visible) per
// Playwright "find all" semantics for toHaveCount.
const allLabels = Array.from({ length: TOTAL }, (_, i) => `Item ${i}`)
labelsResolver.registerLabels('{"role":"cell"}', allLabels)

runtime.afterSnapshot = () => {
  runtime.snapshotResult = makeList(visibleStart, VISIBLE_PAGE, TOTAL)
}

async function runListScrollFlow(): Promise<void> {
  const app = await Smix.launchApp(
    bundleId('dev.smix.demo-app'),
    runtime,
    resolver.resolve,
    labelsResolver.resolve,
  )

  // 1: assert initial count via labels resolver — 20 total cells
  await app.find(Selector.role('cell')).toHaveCount(TOTAL, { timeoutMs: 1_000 })

  // 2: assert visible items 0-4 by id
  const item0 = app.find(Selector.id('item-0'))
  await item0.toBeVisible({ timeoutMs: 500 })
  await item0.toContainText('Item 0', { timeoutMs: 500 })

  // 3: swipe up 3 times to advance visible range
  for (let page = 0; page < 3; page++) {
    await app.swipe('up')
    visibleStart += VISIBLE_PAGE
  }

  // 4: assert item-15 now in view
  const item15 = app.find(Selector.id('item-15'))
  await item15.toBeVisible({ timeoutMs: 500 })

  // 5: total count still 20 (count semantics return all matches not just visible)
  await app.find(Selector.role('cell')).toHaveCount(TOTAL, { timeoutMs: 500 })
}

try {
  await runListScrollFlow()
  console.log('✅ list scroll flow PASS (5 assertions across scroll states)')
  console.log(`   swipe calls dispatched: ${runtime.swipeCalls.length}`)
  console.log(`   final visibleStart: ${visibleStart}`)
} catch (e) {
  if (e instanceof Error) {
    console.error('❌ list scroll flow FAIL:', e.message)
  }
  process.exit(1)
}
