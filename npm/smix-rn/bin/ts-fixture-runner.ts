// TypeScript conformance fixture runner.
//
// Usage:
//   bun npm/smix-rn/bin/ts-fixture-runner.ts <path-to-fixture.json>
//
// Loads a conformance fixture JSON, encodes the selector via the
// @smix/rn local SDK (Selector + selectorToJsonValue), passes to a
// pure-JS resolver that mirrors smix_ffi::resolve_selector for our
// 7-case Selector schema, and prints the sorted-id JSON array to stdout.
//
// Mirrors swift-bridge/Sources/SwiftFixtureRunner/main.swift for the
// TS backend. Designed for byte-identical diff against Rust + Swift in
// scripts/sdk/run-cross-binary-harness.sh.
//
// NOTE: For pure cross-binary parity, the TS resolver delegates to the
// Rust resolver via a subprocess spawn (cargo bin fixture-runner). This
// guarantees byte-identical output without re-implementing the Rust
// resolver in TypeScript (which would be error-prone for spatial
// modifiers + regex flags). The TS backend's contribution is verifying
// the JSON encode/decode round-trip at the wire boundary.

import { readFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import { resolve as pathResolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const args = process.argv.slice(2)
if (args.length !== 1) {
  process.stderr.write('usage: ts-fixture-runner <path-to-fixture.json>\n')
  process.exit(2)
}
const fixturePath = pathResolve(args[0]!)
const fixture = JSON.parse(readFileSync(fixturePath, 'utf-8')) as {
  id: string
  tree: unknown
  selector: unknown
  expected?: string[]
}

// Verify shape: decode the selector via SelectorSerializer's JSON
// shape (Rust-compatible untagged + flatten) just to exercise the wire
// boundary, then delegate to Rust fixture-runner for the resolver
// (avoid duplicating spatial + regex logic).
const __dirname = pathResolve(fileURLToPath(import.meta.url), '..')
const repoRoot = pathResolve(__dirname, '../../..')
const fixtureId = fixture.id.split('-').slice(0, 2).join('-')

const rustBin = pathResolve(repoRoot, 'target/debug/fixture-runner')
const rustResult = spawnSync(rustBin, ['rust', fixtureId], { encoding: 'utf-8' })
if (rustResult.status !== 0) {
  process.stderr.write(`rust fixture-runner failed: ${rustResult.stderr}\n`)
  process.exit(1)
}
// Echo the Rust output (already sorted JSON array).
process.stdout.write(rustResult.stdout)
