import { mkdir, writeFile, appendFile } from 'node:fs/promises'
import { join } from 'node:path'
import type { SerializedFailure } from '../core/error.js'

/**
 * Matcher-side trace sink registry (v0.4 C2).
 *
 * Module-private singleton because matchers are invoked from user test
 * code via `expect()`, which has no place to thread a per-run sink
 * handle. The CLI run command sets the sink at case start and clears
 * it at case end; matchers consult it on failure to drop a PNG into
 * the per-case trace directory. No registered sink = silent no-op
 * (matchers still throw with the screenshot field populated in-memory).
 */
export type MatcherTraceSink = {
  readonly traceRoot: string
  readonly caseSlug: string
}

let activeSink: MatcherTraceSink | undefined
const seqByCase = new Map<string, number>()
const stepSeqByCase = new Map<string, number>()

export function setTraceSink(sink: MatcherTraceSink): void {
  activeSink = sink
  // Reset per-case counters on each set so a re-run of the same slug
  // restarts numbering from failure-0001.png and steps.jsonl seq=1.
  seqByCase.set(sink.caseSlug, 0)
  stepSeqByCase.set(sink.caseSlug, 0)
}

export function clearTraceSink(): void {
  activeSink = undefined
}

export function getTraceSink(): MatcherTraceSink | undefined {
  return activeSink
}

/**
 * Write the matcher-failure screenshot to `<traceRoot>/<caseSlug>/failure-<NNNN>.png`.
 *
 * Returns `{path, seq}` for the file written, or null if there is no
 * screenshot to write. The caller (matchers.ts) uses `seq` to write a
 * sibling failure-<NNNN>.json with the same NNNN so AI consumers can
 * pair the two by suffix. Throws on fs errors — callers must wrap to
 * keep the underlying matcher failure path unaffected.
 */
export async function writeFailurePng(
  sink: MatcherTraceSink,
  pngBase64: string | undefined,
): Promise<{ path: string; seq: number } | null> {
  if (pngBase64 === undefined || pngBase64.length === 0) return null
  const next = (seqByCase.get(sink.caseSlug) ?? 0) + 1
  seqByCase.set(sink.caseSlug, next)
  const num = String(next).padStart(4, '0')
  const dir = join(sink.traceRoot, sink.caseSlug)
  const out = join(dir, `failure-${num}.png`)
  await mkdir(dir, { recursive: true })
  const bytes = Buffer.from(pngBase64, 'base64')
  await writeFile(out, bytes)
  return { path: out, seq: next }
}

/**
 * A single Driver action record written to steps.jsonl.
 * Schema is wire-stable; see docs/design.md §steps.jsonl wire schema (added C4).
 */
export type StepRecord = {
  readonly type: string
  readonly args: unknown
  readonly ok: boolean
  readonly durationMs: number
  readonly err?: string
}

// Shallow recursive sanitization: RegExp → {__regex, flags}; arrays / plain
// objects recurse one level. Primitives untouched. Functions become '[fn]'
// and Buffers become {__buffer_bytes: <length>} so JSON.stringify produces
// AI-readable output instead of the default lossy form ({} for RegExp,
// large hex blob for Buffer).
function sanitizeArgs(v: unknown): unknown {
  if (v instanceof RegExp) return { __regex: v.source, flags: v.flags }
  if (typeof v === 'function') return '[fn]'
  if (Buffer.isBuffer(v)) return { __buffer_bytes: v.length }
  if (Array.isArray(v)) return v.map(sanitizeArgs)
  if (v !== null && typeof v === 'object') {
    const out: Record<string, unknown> = {}
    for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
      out[k] = sanitizeArgs(val)
    }
    return out
  }
  return v
}

/**
 * Append one JSON line to `<traceRoot>/<caseSlug>/steps.jsonl`.
 * Callers (TracingDriver) silence fs errors so trace-write failures
 * never mask the underlying action's throw.
 */
export async function appendStep(
  sink: MatcherTraceSink,
  record: StepRecord,
): Promise<number> {
  const next = (stepSeqByCase.get(sink.caseSlug) ?? 0) + 1
  stepSeqByCase.set(sink.caseSlug, next)
  const line: Record<string, unknown> = {
    ts: Date.now(),
    seq: next,
    type: record.type,
    args: sanitizeArgs(record.args),
    ok: record.ok,
    duration_ms: record.durationMs,
  }
  if (record.err !== undefined) {
    line['err'] = record.err
  }
  const dir = join(sink.traceRoot, sink.caseSlug)
  const out = join(dir, 'steps.jsonl')
  await mkdir(dir, { recursive: true })
  await appendFile(out, JSON.stringify(line) + '\n')
  return next
}

/**
 * Write a per-step screenshot PNG to `<traceRoot>/<caseSlug>/step-<NNNN>.png`.
 *
 * `seq` must come from the corresponding `appendStep` call's return value so
 * that the jsonl row's `seq` field and this PNG file's NNNN suffix align 1:1.
 * Callers (TracingDriver) silence fs errors so trace-write failures never
 * mask the wrapped action's throw. Returns the absolute path written.
 */
export async function writeStepPng(
  sink: MatcherTraceSink,
  seq: number,
  png: Buffer | Uint8Array,
): Promise<string> {
  const num = String(seq).padStart(4, '0')
  const dir = join(sink.traceRoot, sink.caseSlug)
  const out = join(dir, `step-${num}.png`)
  await mkdir(dir, { recursive: true })
  await writeFile(out, png)
  return out
}

/**
 * Payload written to <traceRoot>/<caseSlug>/failure-<NNNN>.json by writeFailureJson.
 *
 * Equivalent to SerializedFailure (the wire form emitted by
 * ExpectationFailure.toJSON(false)) plus an optional screenshot_path field
 * that points at the sibling failure-<NNNN>.png. Decision 2: trace-disk JSON
 * never embeds the base64 screenshot — the PNG file is the carrier.
 */
export type FailurePayload = SerializedFailure & {
  readonly screenshot_path?: string
}

/**
 * Reserve a new failure seq for the current case without writing a file.
 * Used by matchers.ts when the screenshot is unavailable so the JSON file
 * is still numbered consistently with the case's seqByCase counter
 * (decision 5). Increments the same `seqByCase` Map that writeFailurePng
 * uses, so PNG and JSON never collide on NNNN within a case.
 */
export function reserveFailureSeq(sink: MatcherTraceSink): number {
  const next = (seqByCase.get(sink.caseSlug) ?? 0) + 1
  seqByCase.set(sink.caseSlug, next)
  return next
}

/**
 * Write the matcher-failure JSON payload to
 * `<traceRoot>/<caseSlug>/failure-<NNNN>.json`.
 *
 * `seq` must come from a prior writeFailurePng return value or
 * reserveFailureSeq() call so this JSON file and its sibling
 * failure-<NNNN>.png share the same NNNN suffix within the case. Pretty-
 * printed with 2-space indent so AI consumers can `cat` the file and feed
 * it to claude -p with the sibling PNG via --image. Callers (matchers.ts)
 * silence fs errors so trace-write failures never mask the matcher's
 * throw. Returns the absolute path written.
 */
export async function writeFailureJson(
  sink: MatcherTraceSink,
  seq: number,
  payload: FailurePayload,
): Promise<string> {
  const num = String(seq).padStart(4, '0')
  const dir = join(sink.traceRoot, sink.caseSlug)
  const out = join(dir, `failure-${num}.json`)
  await mkdir(dir, { recursive: true })
  await writeFile(out, JSON.stringify(payload, null, 2))
  return out
}
