import { test, expect } from '../../src/sdk/index.js'
import { ExpectationFailure } from '../../src/core/index.js'

// v0.4 C3 e2e fixture — deliberately typo'd selector 'Genral' (drop 'e')
// against Settings root where 'General' cell exists. The matcher's failure()
// branch must populate ExpectationFailure.suggestions with 'General' as the
// top entry. The catch block emits a single-line JSON envelope on stdout so
// scripts/simx-c3-suggestions-smoke.sh can jq-parse it without depending on
// trace-sink failure.json (lands in C6).
test('c3 suggestions', async ({ app }) => {
  await app.launch('com.apple.Preferences')
  await new Promise((r) => setTimeout(r, 2000))
  // Tap into the General sub-page first. Settings root on iOS 26.4 dev sim
  // puts a large Apple Account banner before General in the snapshot tree;
  // the driver caps collectVisibleSummaries at 50 nodes (DFS pre-order),
  // and the General cell sits at index ~56 — just past the cutoff. On the
  // General sub-page itself, 'General' is the navigation title at DFS
  // index 13 (well within 50), giving the typo 'Genral' a stable target.
  await app.tap({ text: 'General' })
  await new Promise((r) => setTimeout(r, 1500))
  try {
    await expect(app.element({ text: 'Genral' })).toBeVisible({ timeout: 500 })
  } catch (e) {
    if (e instanceof ExpectationFailure) {
      console.log('__C3_SUGGESTIONS_JSON__ ' + JSON.stringify(e.toJSON()))
    }
    throw e
  }
})
