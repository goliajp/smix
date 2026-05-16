import { test, expect } from '../../src/sdk/index.js'

// v0.4 C8 dogfood case C — extra trailing char: 'GeneralS' vs 'General'.
// Same fixture shape as the other two C8 cases. Verifies `claude -p` can
// strip a trailing character and propose 'General' from failure-0001.json
// suggestions (sim ~0.88) + visibleElements (9+ literal 'General').
test('c8 dogfood partial', async ({ app }) => {
  await app.launch('com.apple.Preferences')
  await new Promise((r) => setTimeout(r, 3000))
  await app.tap({ text: 'General' })
  await new Promise((r) => setTimeout(r, 2000))
  await expect(app.element({ text: 'GeneralS' })).toBeVisible({ timeout: 500 })
})
