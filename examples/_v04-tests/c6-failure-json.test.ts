import { test, expect } from '../../src/sdk/index.js'

// v0.4 C6 e2e fixture — deliberately mis-typed selector to drive
// ElementExpectation.toBeVisible() into its failure() branch so the
// trace-sink wiring writes failure-0001.png AND failure-0001.json
// under .simx/trace/<case>/. scripts/simx-c6-failure-json-smoke.sh
// is the canonical runner.

test('c6 failure json', async ({ app }) => {
  await app.launch('com.apple.Preferences')
  await new Promise((r) => setTimeout(r, 1500))
  await expect(app.element({ text: 'NotARealSettingsLabel' })).toBeVisible({
    timeout: 500,
  })
})
