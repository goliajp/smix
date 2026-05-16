import { test, expect } from '../../src/sdk/index.js'

// v0.4 C8 dogfood case A — single-char typo: drop 'e' from 'General'.
// Fixture first taps into the Settings General sub-page (sub-page top-50
// visibleElements contain 9+ English 'General' tokens; Settings root is
// sparse + leaks zh-Hans labels), then asserts a deliberately mis-typed
// selector so trace-sink lands .simx/trace/c8-dogfood-typo/failure-0001.json.
// scripts/simx-c8-dogfood-smoke.sh feeds that JSON to `claude -p` and
// asserts the stdout proposes 'General' via regex /general/i.
test('c8 dogfood typo', async ({ app }) => {
  await app.launch('com.apple.Preferences')
  // 3000ms post-launch: c3 used 2000ms in single-case smoke; in the c8
  // single-runner-3-case path we cold-restart Settings between cases via
  // simctl terminate, so first General cell render can lag — give it room.
  await new Promise((r) => setTimeout(r, 3000))
  await app.tap({ text: 'General' })
  await new Promise((r) => setTimeout(r, 2000))
  await expect(app.element({ text: 'Genral' })).toBeVisible({ timeout: 500 })
})
