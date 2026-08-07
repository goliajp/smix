// Perf baseline: Smix.launchApp + tap round-trip latency
// against MockSimRuntime + MockSelectorResolver. Writes results to
// .claude/docs/perf/v7.8-baseline-ts.txt.
//
// Usage:
//   cd npm/smix-rn/examples/demo-app
//   bun perf-baseline.ts > ../../../../.claude/docs/perf/v7.8-baseline-ts.txt

import {
  bundleId,
  MockLabelsResolver,
  MockSelectorResolver,
  MockSimRuntime,
  Selector,
  Smix,
  type A11yNode,
} from '../../src/index.js'

const ITERATIONS = 1_000
const WARMUP = 100

const button: A11yNode = {
  rawType: 'button',
  identifier: 'btn-x',
  label: 'Tap me',
  bounds: { x: 100, y: 200, w: 80, h: 40 },
  enabled: true,
  visible: true,
}
const tree: A11yNode = {
  rawType: 'other',
  bounds: { x: 0, y: 0, w: 393, h: 852 },
  enabled: true,
  visible: true,
  children: [button],
}

async function setupApp() {
  const runtime = new MockSimRuntime({ snapshotResult: tree })
  const resolver = new MockSelectorResolver()
  resolver.registerHit('{"id":"btn-x"}', 'btn-x')
  const app = await Smix.launchApp(
    bundleId('dev.smix.bench'),
    runtime,
    resolver.resolve,
    new MockLabelsResolver().resolve,
  )
  return app
}

async function measureTap(): Promise<number> {
  const app = await setupApp()
  const sel = Selector.id('btn-x')
  const start = performance.now()
  await app.tap(sel)
  return performance.now() - start
}

async function main() {
  console.log('# SmixSDK TypeScript perf baseline (v7.8 c3)')
  console.log(`# Date: ${new Date().toISOString()}`)
  console.log('# Operation: Smix.launchApp + App.tap(Selector.id)')
  console.log('# Backend: MockSimRuntime + MockSelectorResolver (in-memory)')
  console.log(`# Iterations: ${ITERATIONS} (after ${WARMUP} warmup)`)
  console.log()

  // Warmup (JIT settle)
  for (let i = 0; i < WARMUP; i++) await measureTap()

  // Measure
  const samples: number[] = []
  for (let i = 0; i < ITERATIONS; i++) {
    samples.push(await measureTap())
  }
  samples.sort((a, b) => a - b)

  const median = samples[Math.floor(samples.length * 0.5)] ?? 0
  const p99 = samples[Math.floor(samples.length * 0.99)] ?? 0
  const min = samples[0] ?? 0
  const max = samples[samples.length - 1] ?? 0
  const avg = samples.reduce((a, b) => a + b, 0) / samples.length

  console.log(`min:    ${min.toFixed(3)} ms`)
  console.log(`avg:    ${avg.toFixed(3)} ms`)
  console.log(`median: ${median.toFixed(3)} ms`)
  console.log(`p99:    ${p99.toFixed(3)} ms`)
  console.log(`max:    ${max.toFixed(3)} ms`)
  console.log()
  console.log('# Regression gate:')
  console.log(`#   soft fail if median > ${(median * 1.5).toFixed(3)} ms (1.5x)`)
  console.log(`#   hard fail if median > ${(median * 3).toFixed(3)} ms (3x)`)
}

main()
