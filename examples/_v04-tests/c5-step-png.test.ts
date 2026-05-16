import { test } from '../../src/sdk/index.js'

// v0.4 C5 e2e fixture — drive 2 non-screenshot mutating actions (launch +
// tap) so .simx/trace/<slug>/ ends with step-0001.png + step-0002.png plus
// a matching 2-row steps.jsonl. The smoke script reads both jsonl and the
// PNG files; the example itself is a success path and does not catch /
// assert. Path A (plain text-only selector) routes to runner /tap, same
// shape as the v0.4 C4 e2e fixture.
test('c5 step png', async ({ app }) => {
  await app.launch('com.apple.Preferences')
  await new Promise((r) => setTimeout(r, 1500))
  await app.tap({ text: 'General' })
  await new Promise((r) => setTimeout(r, 500))
})
