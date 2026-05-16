import { test, expect } from '../../src/sdk/index.js'

// v0.4 C2 e2e fixture — deliberately mis-typed selector to drive
// ElementExpectation.toBeVisible() into its failure() branch so the
// trace-sink wiring writes failure-0001.png under .simx/trace/<case>/.
// scripts/simx-c2-matcher-failure-smoke.sh is the canonical runner.

test('c2 matcher failure', async ({ app }) => {
  await app.launch('com.apple.Preferences')
  // Settle: let Settings root render so describe()'s tree + screenshot
  // capture something non-empty before the matcher polls.
  await new Promise((r) => setTimeout(r, 1500))
  await expect(app.element({ text: 'NotARealSettingsLabel' })).toBeVisible({
    timeout: 500,
  })
})
