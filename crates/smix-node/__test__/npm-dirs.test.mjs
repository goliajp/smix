import { test } from 'node:test'
import assert from 'node:assert'
import { execFileSync } from 'node:child_process'
import { existsSync, readdirSync, readFileSync, rmSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const crate = dirname(dirname(fileURLToPath(import.meta.url)))
const npmDir = join(crate, 'npm')

// The prebuild distribution structure: `napi.targets` drives one per-platform
// subpackage per triple. Regenerate from scratch so the test reflects the
// declared targets, not a stale run.
function regen() {
  rmSync(npmDir, { recursive: true, force: true })
  execFileSync('bunx', ['napi', 'create-npm-dirs'], { cwd: crate, stdio: 'pipe' })
}

const EXPECTED = {
  'darwin-arm64': { os: 'darwin', cpu: 'arm64' },
  'darwin-x64': { os: 'darwin', cpu: 'x64' },
  'linux-x64-gnu': { os: 'linux', cpu: 'x64', libc: 'glibc' },
}

test('create-npm-dirs produces exactly the three declared per-platform subpackages', () => {
  regen()
  const dirs = readdirSync(npmDir).sort()
  assert.deepStrictEqual(dirs, Object.keys(EXPECTED).sort())

  for (const [short, want] of Object.entries(EXPECTED)) {
    const pkg = JSON.parse(readFileSync(join(npmDir, short, 'package.json'), 'utf8'))
    assert.strictEqual(pkg.name, `@goliapkg/smix-node-${short}`, `name for ${short}`)
    assert.deepStrictEqual(pkg.os, [want.os], `os for ${short}`)
    assert.deepStrictEqual(pkg.cpu, [want.cpu], `cpu for ${short}`)
    if (want.libc) assert.deepStrictEqual(pkg.libc, [want.libc], `libc for ${short}`)
  }
})

test('artifacts maps the host .node into its subpackage', () => {
  // build must have produced the host .node first (the checkpoint chains it);
  // artifacts copies it into npm/darwin-arm64/. --output-dir . points at the
  // crate root where `napi build` left the .node (the default ./artifacts is
  // the CI-download location, absent locally).
  execFileSync('bunx', ['napi', 'artifacts', '--output-dir', '.'], { cwd: crate, stdio: 'pipe' })
  assert.ok(
    existsSync(join(npmDir, 'darwin-arm64', 'smix-node.darwin-arm64.node')),
    'host darwin-arm64 .node must land in its subpackage',
  )
})
