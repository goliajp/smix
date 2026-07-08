// Realistic login flow exercising the full SDK surface against
// MockSimRuntime + MockSelectorResolver.
//
// In production this would use HttpSimRuntime + HttpSimRuntime.resolver
// — the test code below stays identical.

import {
  bundleId,
  literal,
  MockLabelsResolver,
  MockSelectorResolver,
  MockSimRuntime,
  Selector,
  Smix,
  type A11yNode,
} from '../../src/index.js'

// ---- Stage the UI tree ------------------------------------------------

const loginScreen: A11yNode = {
  rawType: 'other',
  bounds: { x: 0, y: 0, w: 393, h: 852 },
  enabled: true,
  visible: true,
  children: [
    {
      rawType: 'textField',
      role: 'textField',
      identifier: 'input-username',
      label: 'Username',
      bounds: { x: 50, y: 200, w: 293, h: 40 },
      enabled: true,
      visible: true,
    },
    {
      rawType: 'secureTextField',
      role: 'secureTextField',
      identifier: 'input-password',
      label: 'Password',
      bounds: { x: 50, y: 260, w: 293, h: 40 },
      enabled: true,
      visible: true,
    },
    {
      rawType: 'button',
      role: 'button',
      identifier: 'btn-login',
      label: 'Sign In',
      bounds: { x: 100, y: 340, w: 193, h: 44 },
      enabled: true,
      visible: true,
    },
  ],
}

const welcomeScreen: A11yNode = {
  rawType: 'other',
  bounds: { x: 0, y: 0, w: 393, h: 852 },
  enabled: true,
  visible: true,
  children: [
    {
      rawType: 'staticText',
      role: 'staticText',
      identifier: 'welcome-msg',
      label: 'Welcome back, alice',
      text: 'Welcome back, alice',
      bounds: { x: 50, y: 100, w: 293, h: 24 },
      enabled: true,
      visible: true,
    },
  ],
}

// ---- Wire up the mock runtime + resolvers ----------------------------

const runtime = new MockSimRuntime({ snapshotResult: loginScreen })
const resolver = new MockSelectorResolver()
const labelsResolver = new MockLabelsResolver()

resolver.registerHit('{"id":"input-username"}', 'input-username')
resolver.registerHit('{"id":"input-password"}', 'input-password')
resolver.registerHit('{"id":"btn-login"}', 'btn-login')
resolver.registerHit('{"id":"welcome-msg"}', 'welcome-msg')

// Simulate UI transition: after btn-login tap, swap snapshot to welcome screen.
let didLogin = false
runtime.afterSnapshot = () => {
  if (didLogin) runtime.snapshotResult = welcomeScreen
}

// ---- Run the flow -----------------------------------------------------

async function runLoginFlow(): Promise<void> {
  const app = await Smix.launchApp(
    bundleId('com.example.app'),
    runtime,
    resolver.resolve,
    labelsResolver.resolve,
  )

  // Step 1+2: fill username + password
  await app.fill(Selector.id('input-username'), 'alice')
  await app.fill(Selector.id('input-password'), 'secret')

  // Step 3: tap Sign In, then swap UI to welcome screen
  await app.tap(Selector.id('btn-login'))
  didLogin = true

  // Step 4: assert welcome message visible
  const welcome = app.find(Selector.id('welcome-msg'))
  await welcome.toBeVisible({ timeoutMs: 1_000 })
  await welcome.toContainText('alice', { timeoutMs: 1_000 })
}

// ---- Run + report -----------------------------------------------------

try {
  await runLoginFlow()
  console.log('✅ login flow PASS (4 steps, mock sim)')
  console.log(`   1. fill username → ${JSON.stringify(runtime.sendStringCalls[0])}`)
  console.log(`   2. fill password → ${JSON.stringify(runtime.sendStringCalls[1])}`)
  console.log(`   3. tap "Sign In" → ${runtime.tapCalls.length} taps dispatched`)
  console.log('   4. assert welcome.visible + contains "alice" → PASS')
} catch (e) {
  if (e instanceof Error) {
    console.error('❌ login flow FAIL:', e.message)
    // ExpectationFailure has toJson() for AI-readable diagnosis
    const ef = e as { toJson?: () => string }
    if (typeof ef.toJson === 'function') {
      console.error('   AI-readable JSON:', ef.toJson())
    }
  }
  process.exit(1)
}
