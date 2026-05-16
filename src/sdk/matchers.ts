import type { ElementHandle } from './element.js'
import {
  ExpectationFailure,
  summarizeNode,
  type ScreenDescription,
  type A11yNode,
  type ElementSummary,
} from '../core/index.js'
import {
  getTraceSink,
  writeFailurePng,
  writeFailureJson,
  reserveFailureSeq,
  type FailurePayload,
} from './trace-sink.js'

const DEFAULT_TIMEOUT = 5_000

type ExpectOptions = {
  timeout?: number
}

/**
 * Matchers for element handles. Each matcher:
 *  - polls until success or timeout
 *  - on failure throws ExpectationFailure with current visible elements
 *    and a screenshot embedded — ready to feed back to an AI agent.
 */
export class ElementExpectation {
  constructor(private readonly handle: ElementHandle) {}

  async toBeVisible(opts?: ExpectOptions): Promise<void> {
    const node = await this.pollFor(
      async () => {
        const found = await this.handle.resolve()
        return found && found.visible ? found : null
      },
      opts?.timeout ?? DEFAULT_TIMEOUT,
    )
    if (!node) {
      throw await this.failure({
        code: 'NOT_VISIBLE',
        message: `Expected element to be visible.`,
      })
    }
  }

  async toBeHidden(opts?: ExpectOptions): Promise<void> {
    const stillVisible = await this.pollFor(
      async () => {
        const found = await this.handle.resolve()
        return found && found.visible ? found : null
      },
      opts?.timeout ?? DEFAULT_TIMEOUT,
      /* invert */ true,
    )
    if (stillVisible) {
      throw await this.failure({
        code: 'ASSERTION_FAILED',
        message: `Expected element to be hidden, but it was visible.`,
      })
    }
  }

  async toHaveText(expected: string | RegExp, opts?: ExpectOptions): Promise<void> {
    const node = await this.pollFor(
      async () => {
        const found = await this.handle.resolve()
        if (!found) return null
        return matchText(found, expected) ? found : null
      },
      opts?.timeout ?? DEFAULT_TIMEOUT,
    )
    if (!node) {
      const current = await this.handle.resolve()
      throw await this.failure({
        code: 'ASSERTION_FAILED',
        message: current
          ? `Expected text ${formatExpected(expected)}, got ${JSON.stringify(visibleText(current))}.`
          : `Expected text ${formatExpected(expected)}, but element was not found.`,
      })
    }
  }

  async toBeEnabled(opts?: ExpectOptions): Promise<void> {
    const node = await this.pollFor(
      async () => {
        const found = await this.handle.resolve()
        return found && found.enabled ? found : null
      },
      opts?.timeout ?? DEFAULT_TIMEOUT,
    )
    if (!node) {
      throw await this.failure({
        code: 'NOT_ENABLED',
        message: `Expected element to be enabled.`,
      })
    }
  }

  async toHaveCount(expected: number, opts?: ExpectOptions): Promise<void> {
    const matches = await this.pollFor(
      async () => {
        const all = await this.handle.resolveAll()
        return all.length === expected ? all : null
      },
      opts?.timeout ?? DEFAULT_TIMEOUT,
    )
    if (!matches) {
      const actual = (await this.handle.resolveAll()).length
      throw await this.failure({
        code: 'ASSERTION_FAILED',
        message: `Expected ${expected} matching element(s), got ${actual}.`,
      })
    }
  }

  // ---- helpers ----

  private async pollFor<T>(
    probe: () => Promise<T | null>,
    timeoutMs: number,
    invert = false,
  ): Promise<T | null> {
    const deadline = Date.now() + timeoutMs
    let interval = 50
    while (true) {
      const result = await probe()
      if (invert ? result === null : result !== null) return result
      if (Date.now() >= deadline) return invert ? result : null
      await sleep(Math.min(interval, deadline - Date.now()))
      interval = Math.min(interval * 1.5, 500)
    }
  }

  private async failure(init: { code: 'NOT_VISIBLE' | 'NOT_ENABLED' | 'ASSERTION_FAILED'; message: string }): Promise<ExpectationFailure> {
    const screen = await safeDescribe(this.handle)
    const visibleElements = screen?.elements ?? []
    const suggestions = buildSuggestions(this.handle, visibleElements)
    const failure: ConstructorParameters<typeof ExpectationFailure>[0] = {
      code: init.code,
      message: init.message,
      selector: this.handle.selector,
      visibleElements,
      suggestions,
    }
    if (screen?.screenshot) failure.screenshot = screen.screenshot
    // Construct once; reuse for JSON (toJSON) and the return path.
    // ExpectationFailure ctor has no IO / no external side effects, so
    // moving construction above the sink block is behaviourally identical.
    const failureObj = new ExpectationFailure(failure)

    // v0.4 C2 + C6: when a CLI-side trace sink is active, persist both the
    // failure screenshot (failure-<NNNN>.png) and the serialized failure
    // payload (failure-<NNNN>.json) under .simx/trace/<case>/. They share
    // <NNNN> within the case (seqByCase single source of truth) so AI
    // consumers (claude -p ... --image failure-NNNN.png) can pair them by
    // suffix. Sink absence is the steady-state silent path — the in-memory
    // ExpectationFailure.screenshot field still carries base64 for callers
    // that consume it directly.
    const sink = getTraceSink()
    if (sink !== undefined) {
      let pngSeq: number | undefined
      let pngWritten = false
      if (screen?.screenshot !== undefined && screen.screenshot.length > 0) {
        try {
          const result = await writeFailurePng(sink, screen.screenshot)
          if (result !== null) {
            pngSeq = result.seq
            pngWritten = true
          }
        } catch {
          // PNG fs error; fall through to JSON-only write below.
        }
      }
      if (pngSeq === undefined) {
        try {
          pngSeq = reserveFailureSeq(sink)
        } catch {
          pngSeq = undefined
        }
      }
      if (pngSeq !== undefined) {
        try {
          const payload: FailurePayload = {
            ...failureObj.toJSON(false),
            ...(pngWritten
              ? { screenshot_path: `failure-${String(pngSeq).padStart(4, '0')}.png` }
              : {}),
          }
          await writeFailureJson(sink, pngSeq, payload)
        } catch {
          // JSON fs error must never mask the matcher throw below.
        }
      }
    }
    return failureObj
  }
}

/**
 * Matchers operating on a captured ScreenDescription, primarily for
 * visual regression snapshots. Visual diffing is intentionally image-
 * based, not stringified — text snapshots are a poor fit for screens.
 */
export class ScreenExpectation {
  constructor(private readonly screen: ScreenDescription) {}

  async toMatchSnapshot(_name: string): Promise<void> {
    // v0 stub. Real impl: load baseline PNG, diff with pixelmatch,
    // fail with ExpectationFailure carrying both baseline + actual.
    throw new ExpectationFailure({
      code: 'DRIVER_ERROR',
      message: `toMatchSnapshot() not implemented in v0.`,
      hint: 'Visual regression lands in v1 alongside the recording pipeline.',
    })
  }
}

// ---- module-private helpers ----

function matchText(node: A11yNode, expected: string | RegExp): boolean {
  const candidates = [node.text, node.label, node.value].filter(
    (v): v is string => typeof v === 'string',
  )
  if (expected instanceof RegExp) {
    return candidates.some((c) => expected.test(c))
  }
  return candidates.some((c) => c === expected)
}

function visibleText(node: A11yNode): string {
  return node.text ?? node.label ?? node.value ?? ''
}

function formatExpected(p: string | RegExp): string {
  return p instanceof RegExp ? p.toString() : JSON.stringify(p)
}

async function safeDescribe(handle: ElementHandle): Promise<ScreenDescription | null> {
  try {
    return await handle.driver.describe()
  } catch {
    return null
  }
}

/**
 * v0.4 C3: multifield (name + text) edit-distance ranking.
 *  - threshold: strict > 0.5 (short typo'd targets need headroom)
 *  - per element, the higher-scoring field wins ('name' preferred on tie)
 *  - global sort: similarity desc → field name>text → DFS pre-order index asc
 *  - top 3 truncation
 *
 * Message format embeds similarity and matched field name so AI consumers
 * can both human-read and regex-parse `(similarity 0.XX, field name|text)`.
 */
type SuggestionCandidate = {
  readonly summary: ElementSummary
  readonly field: 'name' | 'text'
  readonly value: string
  readonly score: number
  readonly index: number
}

const SUGGESTION_THRESHOLD = 0.5
const SUGGESTION_TOP_N = 3

function buildSuggestions(handle: ElementHandle, visible: ElementSummary[]): string[] {
  const target = extractTargetString(handle)
  if (target === undefined) return []
  const lower = target.toLowerCase()
  const candidates: SuggestionCandidate[] = []
  for (let i = 0; i < visible.length; i++) {
    const el = visible[i]!
    let best: { field: 'name' | 'text'; value: string; score: number } | undefined
    if (typeof el.name === 'string' && el.name.length > 0) {
      const s = similarity(el.name.toLowerCase(), lower)
      if (s > SUGGESTION_THRESHOLD) best = { field: 'name', value: el.name, score: s }
    }
    if (typeof el.text === 'string' && el.text.length > 0) {
      const s = similarity(el.text.toLowerCase(), lower)
      if (s > SUGGESTION_THRESHOLD && (best === undefined || s > best.score)) {
        best = { field: 'text', value: el.text, score: s }
      }
    }
    if (best !== undefined) {
      candidates.push({ summary: el, field: best.field, value: best.value, score: best.score, index: i })
    }
  }
  candidates.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score
    if (a.field !== b.field) return a.field === 'name' ? -1 : 1
    return a.index - b.index
  })
  return candidates
    .slice(0, SUGGESTION_TOP_N)
    .map(
      (c) =>
        `Did you mean ${JSON.stringify(c.value)}? (similarity ${c.score.toFixed(2)}, field ${c.field})`,
    )
}

function extractTargetString(handle: ElementHandle): string | undefined {
  const s = handle.selector as Record<string, unknown>
  if (typeof s.text === 'string') return s.text
  if (typeof s.label === 'string') return s.label
  if (typeof s.name === 'string') return s.name
  return undefined
}

function similarity(a: string, b: string): number {
  if (a === b) return 1
  const longer = a.length >= b.length ? a : b
  const shorter = a.length >= b.length ? b : a
  if (longer.length === 0) return 1
  const dist = editDistance(longer, shorter)
  return (longer.length - dist) / longer.length
}

function editDistance(a: string, b: string): number {
  const dp: number[] = Array.from({ length: b.length + 1 }, (_, i) => i)
  for (let i = 1; i <= a.length; i++) {
    let prev = i - 1
    dp[0] = i
    for (let j = 1; j <= b.length; j++) {
      const tmp = dp[j] ?? 0
      dp[j] = a[i - 1] === b[j - 1]
        ? prev
        : 1 + Math.min(prev, dp[j] ?? 0, dp[j - 1] ?? 0)
      prev = tmp
    }
  }
  return dp[b.length] ?? 0
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, Math.max(0, ms)))
}
