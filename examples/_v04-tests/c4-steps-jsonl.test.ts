import { test } from '../../src/sdk/index.js'

// v0.4 C4 e2e fixture — drive at least 2 mutating actions (launch + tap)
// to populate .simx/trace/<slug>/steps.jsonl. The smoke script reads the
// jsonl file and asserts line count + field shape; the example itself is
// a success path and does not catch / assert.
test('c4 steps jsonl', async ({ app }) => {
  await app.launch('com.apple.Preferences')
  await new Promise((r) => setTimeout(r, 1500))
  // Path A (plain text-only selector) routes to runner /tap, no resolver
  // lookup required. v0.3 C6 acceptance confirms this works on dev sim
  // even when the sparse-tree regression hides nodes from describe().
  await app.tap({ text: 'General' })
  await new Promise((r) => setTimeout(r, 500))
})
