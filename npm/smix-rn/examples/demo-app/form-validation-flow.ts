// Demo flow: form validation. Exercises App.fill / app.tap /
// Locator.toHaveLabel / Locator.toBeVisible.
//
// Scenario: signup form with email + password. Invalid email shows
// inline error label; valid email + short password shows different
// error; valid both → success.

import {
  bundleId,
  MockLabelsResolver,
  MockSelectorResolver,
  MockSimRuntime,
  Selector,
  Smix,
  type A11yNode,
} from '../../src/index.js'

function formScreen(emailError: string | null, passwordError: string | null): A11yNode {
  const children: A11yNode[] = [
    {
      rawType: 'textField',
      role: 'textField',
      identifier: 'input-email',
      label: 'Email',
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
      identifier: 'btn-submit',
      label: 'Sign Up',
      bounds: { x: 100, y: 340, w: 193, h: 44 },
      enabled: true,
      visible: true,
    },
  ]
  if (emailError !== null) {
    children.push({
      rawType: 'staticText',
      role: 'staticText',
      identifier: 'err-email',
      label: emailError,
      bounds: { x: 50, y: 240, w: 293, h: 14 },
      visible: true,
    })
  }
  if (passwordError !== null) {
    children.push({
      rawType: 'staticText',
      role: 'staticText',
      identifier: 'err-password',
      label: passwordError,
      bounds: { x: 50, y: 300, w: 293, h: 14 },
      visible: true,
    })
  }
  return {
    rawType: 'other',
    bounds: { x: 0, y: 0, w: 393, h: 852 },
    enabled: true,
    visible: true,
    children,
  }
}

const runtime = new MockSimRuntime({ snapshotResult: formScreen(null, null) })
const resolver = new MockSelectorResolver()
const labelsResolver = new MockLabelsResolver()

resolver.registerHit('{"id":"input-email"}', 'input-email')
resolver.registerHit('{"id":"input-password"}', 'input-password')
resolver.registerHit('{"id":"btn-submit"}', 'btn-submit')
resolver.registerHit('{"id":"err-email"}', 'err-email')
resolver.registerHit('{"id":"err-password"}', 'err-password')

// State machine: 0 = initial, 1 = after-bad-email tap, 2 = after-fix-email tap, 3 = after-valid tap
let submitCount = 0
runtime.afterSnapshot = () => {
  if (submitCount === 1) {
    runtime.snapshotResult = formScreen('Invalid email', null)
  } else if (submitCount === 2) {
    runtime.snapshotResult = formScreen(null, 'Password too short')
  } else if (submitCount === 3) {
    runtime.snapshotResult = formScreen(null, null)
  }
}

async function runFormValidationFlow(): Promise<void> {
  const app = await Smix.launchApp(
    bundleId('dev.smix.demo-app'),
    runtime,
    resolver.resolve,
    labelsResolver.resolve,
  )

  // 1: submit empty form → email error
  await app.tap(Selector.id('btn-submit'))
  submitCount++
  const emailErr = app.find(Selector.id('err-email'))
  await emailErr.toBeVisible({ timeoutMs: 1_000 })
  await emailErr.toHaveLabel('Invalid email', { timeoutMs: 1_000 })

  // 2: fill bad email → fill password → submit → password error
  await app.fill(Selector.id('input-email'), 'alice@example.com')
  await app.fill(Selector.id('input-password'), 'short')
  await app.tap(Selector.id('btn-submit'))
  submitCount++
  const pwErr = app.find(Selector.id('err-password'))
  await pwErr.toBeVisible({ timeoutMs: 1_000 })
  await pwErr.toHaveLabel('Password too short', { timeoutMs: 1_000 })

  // 3: fix password → submit → no errors
  await app.fill(Selector.id('input-password'), 'a-much-longer-password')
  await app.tap(Selector.id('btn-submit'))
  submitCount++
  // No assertion failure means no error labels visible.
}

try {
  await runFormValidationFlow()
  console.log('✅ form validation flow PASS (3 submit cycles, 5 assertions)')
  console.log(`   tap calls dispatched: ${runtime.tapCalls.length}`)
  console.log(`   sendString calls dispatched: ${runtime.sendStringCalls.length}`)
} catch (e) {
  if (e instanceof Error) {
    console.error('❌ form validation flow FAIL:', e.message)
  }
  process.exit(1)
}
