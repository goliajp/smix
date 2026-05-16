import { test } from '../src/sdk/index.js'

test('settings: tap General navigates to sub-page', async ({ app }) => {
  // The C2 SimxRunner XCUITest auto-launches com.apple.Preferences before
  // serving requests, so an SDK-side launch would only fight the XCUITest
  // session attachment (fresh:true terminates the app the runner is bound
  // to). C6 keeps the example minimal: tap "General" against the already-
  // running Settings root, verified by the shell wrapper's double-sided
  // probe after this test exits.
  await app.tap({ text: 'General' })
})
