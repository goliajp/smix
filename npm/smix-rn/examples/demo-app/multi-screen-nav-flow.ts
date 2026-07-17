// Demo flow: multi-screen navigation. Exercises App.openUrl
// (deep link) / app.tapAtCoord (§9 #3 escape hatch) / app.pressKey /
// app.relaunch.
//
// Scenario: navigate from home → settings → profile → relaunch back to
// home. Tests lifecycle methods + escape hatch.

import {
  bundleId,
  MockLabelsResolver,
  MockSelectorResolver,
  MockSimRuntime,
  Selector,
  Smix,
  type A11yNode,
} from '../../src/index.js'

const HOME: A11yNode = {
  rawType: 'other',
  identifier: 'screen-home',
  bounds: { x: 0, y: 0, w: 393, h: 852 },
  visible: true,
  children: [
    {
      rawType: 'staticText',
      identifier: 'title-home',
      label: 'Home',
      bounds: { x: 0, y: 50, w: 393, h: 30 },
      visible: true,
    },
    {
      rawType: 'button',
      identifier: 'btn-settings',
      label: 'Settings',
      bounds: { x: 100, y: 200, w: 193, h: 44 },
      visible: true,
    },
  ],
}

const SETTINGS: A11yNode = {
  rawType: 'other',
  identifier: 'screen-settings',
  bounds: { x: 0, y: 0, w: 393, h: 852 },
  visible: true,
  children: [
    {
      rawType: 'staticText',
      identifier: 'title-settings',
      label: 'Settings',
      bounds: { x: 0, y: 50, w: 393, h: 30 },
      visible: true,
    },
    {
      rawType: 'button',
      identifier: 'btn-profile',
      label: 'Profile',
      bounds: { x: 100, y: 200, w: 193, h: 44 },
      visible: true,
    },
  ],
}

const PROFILE: A11yNode = {
  rawType: 'other',
  identifier: 'screen-profile',
  bounds: { x: 0, y: 0, w: 393, h: 852 },
  visible: true,
  children: [
    {
      rawType: 'staticText',
      identifier: 'title-profile',
      label: 'Profile',
      bounds: { x: 0, y: 50, w: 393, h: 30 },
      visible: true,
    },
  ],
}

let currentScreen: A11yNode = HOME
const runtime = new MockSimRuntime({ snapshotResult: HOME })
const resolver = new MockSelectorResolver()
const labelsResolver = new MockLabelsResolver()

resolver.registerHit('{"id":"btn-settings"}', 'btn-settings')
resolver.registerHit('{"id":"btn-profile"}', 'btn-profile')
resolver.registerHit('{"id":"title-home"}', 'title-home')
resolver.registerHit('{"id":"title-settings"}', 'title-settings')
resolver.registerHit('{"id":"title-profile"}', 'title-profile')

runtime.afterSnapshot = () => {
  runtime.snapshotResult = currentScreen
}

async function runMultiScreenNavFlow(): Promise<void> {
  const app = await Smix.launchApp(
    bundleId('dev.smix.demo-app'),
    runtime,
    resolver.resolve,
    labelsResolver.resolve,
  )

  // 1: home screen visible
  await app.find(Selector.id('title-home')).toBeVisible({ timeoutMs: 500 })

  // 2: tap Settings → settings screen
  await app.tap(Selector.id('btn-settings'))
  currentScreen = SETTINGS
  await app.find(Selector.id('title-settings')).toBeVisible({ timeoutMs: 500 })

  // 3: tap Profile → profile screen
  await app.tap(Selector.id('btn-profile'))
  currentScreen = PROFILE
  await app.find(Selector.id('title-profile')).toBeVisible({ timeoutMs: 500 })

  // 4: escape hatch — tapAtCoord at top-left (would be back button)
  await app.tapAtCoord(0.05, 0.05)

  // 5: deep link to home via openUrl
  await app.openUrl('demo-app://home')
  currentScreen = HOME

  // 6: pressKey escape (simulates hardware back)
  await app.pressKey('escape')

  // 7: relaunch — terminate + launch
  await app.relaunch()
  currentScreen = HOME
  await app.find(Selector.id('title-home')).toBeVisible({ timeoutMs: 500 })
}

try {
  await runMultiScreenNavFlow()
  console.log('✅ multi-screen nav flow PASS (7 navigation steps)')
  console.log(`   tap calls: ${runtime.tapCalls.length}`)
  console.log(`   tapAtNormalized calls: ${runtime.tapAtNormalizedCalls.length}`)
  console.log(`   openUrl calls: ${runtime.openUrlCalls.length}`)
  console.log(`   pressKey calls: ${runtime.pressKeyCalls.length}`)
  console.log(`   launch calls (incl. relaunch): ${runtime.launchCalls.length}`)
  console.log(`   terminate calls: ${runtime.terminateCalls.length}`)
} catch (e) {
  if (e instanceof Error) {
    console.error('❌ multi-screen nav flow FAIL:', e.message)
  }
  process.exit(1)
}
