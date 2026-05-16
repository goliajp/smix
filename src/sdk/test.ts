import { App } from './app.js'
import { StubDriver } from '../driver/index.js'
import type { Driver } from '../driver/index.js'
import { ExpectationFailure } from '../core/index.js'

export type TestContext = {
  app: App
}

export type TestFn = (ctx: TestContext) => Promise<void> | void

type TestCase = {
  name: string
  fn: TestFn
}

const registry: TestCase[] = []

/**
 * Register a test. Mirrors Vitest/Jest/Playwright shape so AI doesn't
 * have to learn anything new — but registration is in-memory only,
 * dispatched by run().
 */
export function test(name: string, fn: TestFn): void {
  registry.push({ name, fn })
}

export type RunOptions = {
  driver?: Driver
  /** filter by case name substring */
  grep?: string
  /** stop the main loop on the first case failure */
  bail?: boolean
  /** invoked before each case body */
  onCaseStart?: (name: string) => void | Promise<void>
  /** invoked after each case body, ok=false if it threw */
  onCaseEnd?: (name: string, ok: boolean) => void | Promise<void>
}

export type RunResult = {
  passed: number
  failed: number
  failures: Array<{ name: string; error: ExpectationFailure | Error }>
}

/**
 * Execute every registered test against the provided driver
 * (or a stub if none is given). Sequential by design — parallelism
 * across simulators is a CLI concern, not a registry concern.
 */
export async function run(opts: RunOptions = {}): Promise<RunResult> {
  const driver = opts.driver ?? new StubDriver()
  const app = new App(driver)
  const ctx: TestContext = { app }
  const result: RunResult = { passed: 0, failed: 0, failures: [] }

  const cases = opts.grep
    ? registry.filter((c) => c.name.includes(opts.grep!))
    : registry

  for (const c of cases) {
    await opts.onCaseStart?.(c.name)
    try {
      await c.fn(ctx)
      result.passed++
      await opts.onCaseEnd?.(c.name, true)
    } catch (err) {
      result.failed++
      result.failures.push({
        name: c.name,
        error: err instanceof Error ? err : new Error(String(err)),
      })
      await opts.onCaseEnd?.(c.name, false)
      if (opts.bail === true) break
    }
  }
  return result
}

/** Test discovery hook for the CLI to inspect what's registered. */
export function listTests(): ReadonlyArray<{ name: string }> {
  return registry.map((c) => ({ name: c.name }))
}

/** Clear registry between runs; useful for watch mode. */
export function resetRegistry(): void {
  registry.length = 0
}
