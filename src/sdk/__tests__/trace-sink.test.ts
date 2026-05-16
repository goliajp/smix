import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtemp, rm, readFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  setTraceSink,
  clearTraceSink,
  appendStep,
  writeStepPng,
  writeFailureJson,
  type StepRecord,
  type FailurePayload,
} from '../trace-sink.js'

let tmpRoot: string

beforeEach(async () => {
  tmpRoot = await mkdtemp(join(tmpdir(), 'simx-c4-trace-'))
})

afterEach(async () => {
  clearTraceSink()
  if (tmpRoot) {
    await rm(tmpRoot, { recursive: true, force: true })
  }
})

function readLines(path: string): Promise<string[]> {
  return readFile(path, 'utf8').then((s) => s.split('\n').filter((l) => l.length > 0))
}

describe('trace-sink appendStep', () => {
  it('writes one line to <traceRoot>/<slug>/steps.jsonl with full schema', async () => {
    const sink = { traceRoot: tmpRoot, caseSlug: 'demo' }
    setTraceSink(sink)
    const rec: StepRecord = {
      type: 'launch',
      args: { bundleId: 'com.example' },
      ok: true,
      durationMs: 42,
    }
    await appendStep(sink, rec)
    const path = join(tmpRoot, 'demo', 'steps.jsonl')
    const lines = await readLines(path)
    expect(lines).toHaveLength(1)
    const obj = JSON.parse(lines[0] ?? '') as Record<string, unknown>
    expect(obj['type']).toBe('launch')
    expect(obj['args']).toEqual({ bundleId: 'com.example' })
    expect(obj['ok']).toBe(true)
    expect(obj['duration_ms']).toBe(42)
    expect(obj['seq']).toBe(1)
    expect(typeof obj['ts']).toBe('number')
    expect('err' in obj).toBe(false)
  })

  it('multiple calls append sequentially with seq 1,2,3', async () => {
    const sink = { traceRoot: tmpRoot, caseSlug: 'multi' }
    setTraceSink(sink)
    await appendStep(sink, { type: 'launch', args: {}, ok: true, durationMs: 1 })
    await appendStep(sink, { type: 'tap', args: { selector: { text: 'OK' } }, ok: true, durationMs: 2 })
    await appendStep(sink, { type: 'screenshot', args: {}, ok: true, durationMs: 3 })
    const path = join(tmpRoot, 'multi', 'steps.jsonl')
    const lines = await readLines(path)
    expect(lines).toHaveLength(3)
    const objs = lines.map((l) => JSON.parse(l) as Record<string, unknown>)
    expect(objs.map((o) => o['seq'])).toEqual([1, 2, 3])
    expect(objs.map((o) => o['type'])).toEqual(['launch', 'tap', 'screenshot'])
  })

  it('sanitizeArgs: RegExp → {__regex, flags}; nested object recurses', async () => {
    const sink = { traceRoot: tmpRoot, caseSlug: 'sanitize' }
    setTraceSink(sink)
    await appendStep(sink, {
      type: 'tap',
      args: { selector: { text: /Sign/i, id: 'abc' } },
      ok: true,
      durationMs: 5,
    })
    const path = join(tmpRoot, 'sanitize', 'steps.jsonl')
    const lines = await readLines(path)
    const obj = JSON.parse(lines[0] ?? '') as {
      args: { selector: { text: { __regex: string; flags: string }; id: string } }
    }
    expect(obj.args.selector.text).toEqual({ __regex: 'Sign', flags: 'i' })
    expect(obj.args.selector.id).toBe('abc')
  })

  it('records err string when ok=false (and only then)', async () => {
    const sink = { traceRoot: tmpRoot, caseSlug: 'err' }
    setTraceSink(sink)
    await appendStep(sink, { type: 'tap', args: {}, ok: true, durationMs: 1 })
    await appendStep(sink, {
      type: 'tap',
      args: {},
      ok: false,
      durationMs: 2,
      err: 'boom',
    })
    const path = join(tmpRoot, 'err', 'steps.jsonl')
    const lines = await readLines(path)
    const okLine = JSON.parse(lines[0] ?? '') as Record<string, unknown>
    const failLine = JSON.parse(lines[1] ?? '') as Record<string, unknown>
    expect('err' in okLine).toBe(false)
    expect(okLine['ok']).toBe(true)
    expect(failLine['ok']).toBe(false)
    expect(failLine['err']).toBe('boom')
  })

  it('appendStep returns the seq it wrote (matches stepSeq state)', async () => {
    const sink = { traceRoot: tmpRoot, caseSlug: 'retseq' }
    setTraceSink(sink)
    const a = await appendStep(sink, { type: 'launch', args: {}, ok: true, durationMs: 1 })
    const b = await appendStep(sink, { type: 'tap', args: {}, ok: true, durationMs: 1 })
    const c = await appendStep(sink, { type: 'screenshot', args: {}, ok: true, durationMs: 1 })
    expect(a).toBe(1)
    expect(b).toBe(2)
    expect(c).toBe(3)
    const path = join(tmpRoot, 'retseq', 'steps.jsonl')
    const lines = await readLines(path)
    const seqs = lines.map((l) => (JSON.parse(l) as { seq: number }).seq)
    expect(seqs).toEqual([1, 2, 3])
  })

  it('setTraceSink resets stepSeq for that slug; distinct slugs are independent', async () => {
    const sinkA = { traceRoot: tmpRoot, caseSlug: 'slugA' }
    setTraceSink(sinkA)
    await appendStep(sinkA, { type: 'launch', args: {}, ok: true, durationMs: 1 })
    await appendStep(sinkA, { type: 'tap', args: {}, ok: true, durationMs: 1 })

    const sinkB = { traceRoot: tmpRoot, caseSlug: 'slugB' }
    setTraceSink(sinkB)
    await appendStep(sinkB, { type: 'launch', args: {}, ok: true, durationMs: 1 })

    // Re-setting slugA must reset its stepSeq back to 0; a fresh appendStep
    // for slugA now starts at seq=1 again.
    setTraceSink(sinkA)
    await appendStep(sinkA, { type: 'screenshot', args: {}, ok: true, durationMs: 1 })

    const linesA = await readLines(join(tmpRoot, 'slugA', 'steps.jsonl'))
    const linesB = await readLines(join(tmpRoot, 'slugB', 'steps.jsonl'))
    expect(linesA).toHaveLength(3)
    expect(linesB).toHaveLength(1)
    const seqsA = linesA.map((l) => (JSON.parse(l) as { seq: number }).seq)
    expect(seqsA).toEqual([1, 2, 1])
    const seqsB = linesB.map((l) => (JSON.parse(l) as { seq: number }).seq)
    expect(seqsB).toEqual([1])
  })

})

describe('trace-sink writeStepPng', () => {
  it('writes png buffer to <traceRoot>/<slug>/step-<NNNN>.png with 4-digit pad', async () => {
    const sink = { traceRoot: tmpRoot, caseSlug: 'demo' }
    setTraceSink(sink)
    const head = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
    const payload = Buffer.alloc(1500, 0x42)
    const buf = Buffer.concat([head, payload])
    const out = await writeStepPng(sink, 5, buf)
    expect(out).toBe(join(tmpRoot, 'demo', 'step-0005.png'))
    const read = await readFile(out)
    expect(read.equals(buf)).toBe(true)
    expect(read.subarray(0, 4).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47]))).toBe(true)
  })

  it('mkdir recursive auto-creates dir', async () => {
    const traceRoot = join(tmpRoot, 'deep', 'nested')
    const sink = { traceRoot, caseSlug: 'a' }
    setTraceSink(sink)
    const buf = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x00, 0x01, 0x02, 0x03])
    await expect(writeStepPng(sink, 1, buf)).resolves.toBe(
      join(traceRoot, 'a', 'step-0001.png'),
    )
    const read = await readFile(join(traceRoot, 'a', 'step-0001.png'))
    expect(read.equals(buf)).toBe(true)
  })

  it('seq pad to 4 digits, > 9999 not truncated', async () => {
    const sink = { traceRoot: tmpRoot, caseSlug: 'pad' }
    setTraceSink(sink)
    const buf = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0xff])
    const big = await writeStepPng(sink, 12345, buf)
    expect(big).toBe(join(tmpRoot, 'pad', 'step-12345.png'))
    const small = await writeStepPng(sink, 1, buf)
    expect(small).toBe(join(tmpRoot, 'pad', 'step-0001.png'))
    const readBig = await readFile(big)
    const readSmall = await readFile(small)
    expect(readBig.equals(buf)).toBe(true)
    expect(readSmall.equals(buf)).toBe(true)
  })
})

describe('trace-sink writeFailureJson', () => {
  it('writes pretty-printed JSON to <traceRoot>/<slug>/failure-<NNNN>.json with 4-digit pad', async () => {
    const sink = { traceRoot: tmpRoot, caseSlug: 'demo' }
    setTraceSink(sink)
    const payload: FailurePayload = {
      ok: false,
      code: 'NOT_VISIBLE',
      message: 'x',
      suggestions: ['a'],
      visibleElements: [],
      selector: { text: 'foo' },
      screenshot_path: 'failure-0005.png',
    }
    const out = await writeFailureJson(sink, 5, payload)
    expect(out).toBe(join(tmpRoot, 'demo', 'failure-0005.json'))
    const content = await readFile(out, 'utf8')
    expect(content).toContain('\n  ')
    const parsed = JSON.parse(content) as Record<string, unknown>
    expect(parsed['ok']).toBe(false)
    expect(parsed['code']).toBe('NOT_VISIBLE')
    expect(parsed['message']).toBe('x')
    expect(parsed['suggestions']).toEqual(['a'])
    expect(parsed['visibleElements']).toEqual([])
    expect(parsed['selector']).toEqual({ text: 'foo' })
    expect(parsed['screenshot_path']).toBe('failure-0005.png')
  })

  it('mkdir recursive auto-creates dir', async () => {
    const traceRoot = join(tmpRoot, 'deep', 'nested')
    const sink = { traceRoot, caseSlug: 'a' }
    setTraceSink(sink)
    const payload: FailurePayload = {
      ok: false,
      code: 'NOT_VISIBLE',
      message: 'm',
      suggestions: [],
      visibleElements: [],
    }
    await expect(writeFailureJson(sink, 1, payload)).resolves.toBe(
      join(traceRoot, 'a', 'failure-0001.json'),
    )
    const parsed = JSON.parse(
      await readFile(join(traceRoot, 'a', 'failure-0001.json'), 'utf8'),
    ) as Record<string, unknown>
    expect(parsed['ok']).toBe(false)
    expect(parsed['code']).toBe('NOT_VISIBLE')
  })

  it('seq pad to 4 digits, > 9999 not truncated', async () => {
    const sink = { traceRoot: tmpRoot, caseSlug: 'pad' }
    setTraceSink(sink)
    const payload: FailurePayload = {
      ok: false,
      code: 'NOT_VISIBLE',
      message: 'm',
      suggestions: [],
      visibleElements: [],
    }
    const big = await writeFailureJson(sink, 12345, payload)
    expect(big).toBe(join(tmpRoot, 'pad', 'failure-12345.json'))
    const small = await writeFailureJson(sink, 1, payload)
    expect(small).toBe(join(tmpRoot, 'pad', 'failure-0001.json'))
    const readBig = JSON.parse(await readFile(big, 'utf8')) as Record<string, unknown>
    const readSmall = JSON.parse(await readFile(small, 'utf8')) as Record<string, unknown>
    expect(readBig['code']).toBe('NOT_VISIBLE')
    expect(readSmall['code']).toBe('NOT_VISIBLE')
  })
})
