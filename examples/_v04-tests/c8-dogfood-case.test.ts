import { test, expect } from '../../src/sdk/index.js'

// v0.4 C8 dogfood case B — wrong case: lowercase 'general' vs Settings cell
// 'General'. Same fixture shape as c8-dogfood-typo; this case verifies
// `claude -p` recognises case-sensitivity when fed failure-0001.json
// (suggestions[0] ranks 'General' top by edit-distance sim ~0.86).
test('c8 dogfood case', async ({ app }) => {
  await app.launch('com.apple.Preferences')
  await new Promise((r) => setTimeout(r, 3000))
  await app.tap({ text: 'General' })
  await new Promise((r) => setTimeout(r, 2000))
  await expect(app.element({ text: 'general' })).toBeVisible({ timeout: 500 })
})
