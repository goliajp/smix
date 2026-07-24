// Web record -> generate e2e: capture a real interaction in headless chromium,
// write the IRAction, and run it through `smix authoring generate` to produce a
// flow. This closes the web leg's contribution to the cross-platform recorder:
// record -> IRAction -> generate (web has no replay runtime — native-shape
// maestro/rust is the artifact). No physical device (§9#1).

import { execFileSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { writeFileSync, readFileSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { recordWebSession } from '../src/recordWeb.js'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = `file://${join(here, 'fixture.html')}`
// repo root: npm/smix-web-record/e2e -> ../../.. is the workspace root.
const smix = join(here, '..', '..', '..', 'target', 'release', 'smix')

function fail(msg: string): never {
  console.error(`[web-gen-e2e] ${msg}`)
  process.exit(1)
}

const actions = await recordWebSession(fixture, async (page) => {
  await page.getByTestId('go').click()
  await page.getByTestId('q').fill('smix')
})
if (actions.length === 0) fail('captured no actions')

const dir = mkdtempSync(join(tmpdir(), 'smix-web-gen-'))
const events = join(dir, 'events.json')
const flow = join(dir, 'flow.yaml')
writeFileSync(events, `[${actions.join(',')}]`)

execFileSync(smix, ['authoring', 'generate', events, '--format', 'maestro', '-o', flow], {
  stdio: 'pipe',
})

const yaml = readFileSync(flow, 'utf8')
if (!yaml.includes('tapOn')) fail(`generated flow missing tapOn:\n${yaml}`)
if (!yaml.includes('inputText')) fail(`generated flow missing inputText:\n${yaml}`)

console.log('generated flow:\n' + yaml.trim())
console.log('C4-WEB-E2E-PASS')
process.exit(0)
