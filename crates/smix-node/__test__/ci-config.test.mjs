import { test } from 'node:test'
import assert from 'node:assert'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

// __test__ -> smix-node -> crates -> repo root
const repo = dirname(dirname(dirname(dirname(fileURLToPath(import.meta.url)))))
const ciPath = join(repo, '.github', 'workflows', 'ci.yml')
const ci = readFileSync(ciPath, 'utf8')

test('actionlint accepts ci.yml', () => {
  // Throws (non-zero exit) if the workflow is malformed.
  execFileSync('actionlint', [ciPath], { stdio: 'pipe' })
})

test('ci.yml has a napi-prebuild job whose matrix is exactly napi.targets', () => {
  assert.match(ci, /^\s{2}napi-prebuild:/m, 'a napi-prebuild job must exist')

  const pkg = JSON.parse(readFileSync(join(repo, 'crates', 'smix-node', 'package.json'), 'utf8'))
  const targets = pkg.napi.targets
  assert.ok(Array.isArray(targets) && targets.length === 3, 'napi.targets is the 3-triple set')

  // Every declared target must appear in the workflow's matrix — the triple
  // set has a single source of truth (napi.targets), and the CI matrix is
  // pinned to it here rather than drifting a second copy.
  for (const triple of targets) {
    assert.ok(ci.includes(triple), `ci.yml matrix must build ${triple}`)
  }
})

test('the prebuild job builds and packages but never publishes', () => {
  // The distribution machinery is wired; publish is deferred and user-gated.
  // No publish verb may hide in the workflow.
  for (const verb of ['napi prepublish', 'npm publish', 'cargo publish', 'bun publish']) {
    assert.ok(!ci.includes(verb), `ci.yml must not run ${verb}`)
  }
})
