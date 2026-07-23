// TS-driving end-to-end: the smix TS SDK drives a real iOS Simulator through
// the real napi addon, against a real runner. No loopback, no mock — the whole
// chain (smix-rn -> loadNodeDriver -> @goliapkg/smix-node .node -> smix-runner-
// client wire -> runner -> Preferences on the sim) is exercised.
//
// Run by scripts/release/ts-driving-e2e.sh after `smix runner up <udid>
// --bundle com.apple.Preferences`. Prints TS-DRIVE-E2E-PASS and exits 0 on
// success; any thrown step exits non-zero (unhandled rejection).
//
// The target ids are the locale-independent com.apple.settings.* identifiers
// verified device-green in the v2.8-C5 stress corpus, not screen text.

import { Smix, Selector, bundleId, loadNodeDriver, flatten, HttpSimRuntime } from '@goliapkg/smix'

const PORT = Number(process.env.SMIX_RUNNER_PORT) || 22087
const TOP = 'com.apple.settings.primaryAppleAccount'
const GENERAL = 'com.apple.settings.general'

function fail(msg) {
  console.error(`[ts-e2e] ${msg}`)
  process.exit(1)
}

const driver = await loadNodeDriver(PORT)
const runtime = new HttpSimRuntime(`http://127.0.0.1:${PORT}`)

// 1. Sense: the real .node returns a real Preferences tree.
const tree0 = JSON.parse(await driver.snapshotTree())
if (tree0.rawType !== 'application') fail(`snapshotTree top is ${tree0.rawType}, not application`)
if (!Array.isArray(tree0.children) || tree0.children.length === 0) fail('Preferences tree is empty')

// 2. Launch through the napi session (openSession + launchApp on the sim).
const app = await Smix.launchApp(bundleId('com.apple.Preferences'), runtime.resolver, { driver })

// 3. Resolve + assert the main-screen rows are visible on the real tree.
await app.find(Selector.id(TOP)).toBeVisible({ timeoutMs: 8000 })
await app.find(Selector.id(GENERAL)).toBeVisible({ timeoutMs: 8000 })

// 4. Act: tap General. A tap that misses (a scrim, the wrong element) fails
//    the runner's tap-hit-verdict and throws — reaching here means a real hit.
await app.tap(Selector.id(GENERAL))

// 5. Navigation happened: poll until the main-screen top row is gone (we are on
//    the General sub-screen). Proves the tap did something, not just resolved.
let navigated = false
for (let i = 0; i < 20; i++) {
  const tree = JSON.parse(await app.snapshotTree())
  if (!flatten(tree).some((n) => n.identifier === TOP)) {
    navigated = true
    break
  }
  await new Promise((r) => setTimeout(r, 250))
}
if (!navigated) fail(`still on the main screen after tapping ${GENERAL} — navigation did not happen`)

console.log('TS-DRIVE-E2E-PASS')
process.exit(0)
