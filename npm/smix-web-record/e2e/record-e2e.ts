// Web capture e2e: record a real click + input in headless chromium through
// the Playwright bridge, and assert the mapped IRAction. No physical device —
// a driver-layer DOM bridge (§9#1). Run with bun (it resolves the TS imports).

import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { recordWebSession } from '../src/recordWeb.js'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = `file://${join(here, 'fixture.html')}`

function fail(msg: string): never {
  console.error(`[web-e2e] ${msg}`)
  process.exit(1)
}

const actions = await recordWebSession(fixture, async (page) => {
  await page.getByTestId('go').click()
  await page.getByTestId('q').fill('smix')
})

const parsed = actions.map((a) => JSON.parse(a))

const tap = parsed.find((a) => a.kind === 'tap')
if (!tap || tap.selector.id !== 'go') fail(`expected tap(go), got ${JSON.stringify(parsed)}`)

const fill = parsed.find((a) => a.kind === 'fill')
if (!fill || fill.selector.id !== 'q' || fill.text !== 'smix') {
  fail(`expected fill(q, "smix"), got ${JSON.stringify(parsed)}`)
}

console.log('captured:', JSON.stringify(parsed))
console.log('C3-E2E-PASS')
process.exit(0)
