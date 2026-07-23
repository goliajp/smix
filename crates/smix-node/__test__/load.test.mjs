import { test } from 'node:test'
import assert from 'node:assert'

// The load gate: the built .node binds to a real Node runtime and the
// driver constructs without reaching for anything. Construction only
// builds an HttpRunnerClient; it connects to nothing, so `new` on a
// never-served port must not throw.
test('the built addon loads and SmixNodeDriver constructs', async () => {
  const mod = await import('../index.js')
  assert.strictEqual(
    typeof mod.SmixNodeDriver,
    'function',
    'SmixNodeDriver must be an exported constructor',
  )
  const d = new mod.SmixNodeDriver(0)
  assert.ok(d, 'new SmixNodeDriver(0) must not throw')
})
