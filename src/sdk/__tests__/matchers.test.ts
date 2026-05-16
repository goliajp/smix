import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mkdtemp, rm, readFile, readdir, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { ElementHandle } from '../element.js'
import { ElementExpectation } from '../matchers.js'
import {
  setTraceSink,
  clearTraceSink,
  getTraceSink,
} from '../trace-sink.js'
import {
  ExpectationFailure,
  type ScreenDescription,
  type A11yNode,
  type Selector,
} from '../../core/index.js'
import type { Driver } from '../../driver/index.js'

// Fixed base64 PNG payload (88-byte 1x1 transparent PNG) — round-tripable
// through Buffer.from(..., 'base64') and starts with the canonical PNG
// magic 0x89 0x50 0x4E 0x47 0x0D 0x0A 0x1A 0x0A.
const FAKE_PNG_BASE64 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNgAAIAAAUAAeIVWUgAAAAASUVORK5CYII='
const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

function makeScreen(
  screenshot: string | undefined = FAKE_PNG_BASE64,
): ScreenDescription {
  return {
    screenshot: screenshot ?? '',
    elements: [],
    frontApp: 'com.example',
    summary: '',
    capturedAt: 1700000000000,
  }
}

function makeDriver(opts: {
  describe: () => Promise<ScreenDescription>
  resolveNode?: A11yNode | null
}): Driver {
  const notImpl = (op: string) => (): Promise<never> =>
    Promise.reject(new Error(`FakeDriver.${op}() not used in this test`))
  return {
    launch: notImpl('launch'),
    terminate: notImpl('terminate'),
    install: notImpl('install'),
    uninstall: notImpl('uninstall'),
    background: notImpl('background'),
    foreground: notImpl('foreground'),
    tap: notImpl('tap'),
    doubleTap: notImpl('doubleTap'),
    longPress: notImpl('longPress'),
    fill: notImpl('fill'),
    clear: notImpl('clear'),
    swipe: notImpl('swipe'),
    scroll: notImpl('scroll'),
    scrollTo: notImpl('scrollTo'),
    pressKey: notImpl('pressKey'),
    hideKeyboard: notImpl('hideKeyboard'),
    openUrl: notImpl('openUrl'),
    pasteboardSet: notImpl('pasteboardSet'),
    pasteboardGet: notImpl('pasteboardGet'),
    grantPermission: notImpl('grantPermission'),
    setAppearance: notImpl('setAppearance'),
    setLocale: notImpl('setLocale'),
    setNetwork: notImpl('setNetwork'),
    waitFor: notImpl('waitFor'),
    screenshot: notImpl('screenshot'),
    tree: notImpl('tree'),
    describe: opts.describe,
    findOne: () => Promise.resolve(opts.resolveNode ?? null),
    findAll: () => Promise.resolve(opts.resolveNode ? [opts.resolveNode] : []),
  }
}

function makeHandle(driver: Driver, selector: Selector = { text: 'NotReal' }): ElementHandle {
  return new ElementHandle(driver, selector)
}

let tmpRoot: string

beforeEach(async () => {
  clearTraceSink()
  tmpRoot = await mkdtemp(join(tmpdir(), 'simx-matchers-'))
})

afterEach(async () => {
  clearTraceSink()
  if (tmpRoot) {
    await rm(tmpRoot, { recursive: true, force: true }).catch(() => undefined)
  }
})

describe('ElementExpectation.toBeVisible failure path', () => {
  it('ExpectationFailure.visibleElements carries elements from driver.describe', async () => {
    const node: A11yNode = {
      rawType: 'button',
      role: 'button',
      label: 'Cancel',
      bounds: { x: 0, y: 0, w: 80, h: 44 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
    }
    const screen: ScreenDescription = {
      screenshot: FAKE_PNG_BASE64,
      elements: [
        {
          role: 'button',
          name: 'Cancel',
          bounds: node.bounds,
          enabled: true,
        },
      ],
      frontApp: 'com.example',
      summary: '',
      capturedAt: 1700000000000,
    }
    const driver = makeDriver({
      describe: () => Promise.resolve(screen),
      resolveNode: null,
    })
    const exp = new ElementExpectation(makeHandle(driver))
    let caught: unknown
    try {
      await exp.toBeVisible({ timeout: 0 })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).visibleElements).toEqual(screen.elements)
  })

  it('ExpectationFailure.screenshot carries base64 PNG from driver.describe', async () => {
    const driver = makeDriver({
      describe: () => Promise.resolve(makeScreen(FAKE_PNG_BASE64)),
      resolveNode: null,
    })
    const exp = new ElementExpectation(makeHandle(driver))
    let caught: unknown
    try {
      await exp.toBeVisible({ timeout: 0 })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).screenshot).toBe(FAKE_PNG_BASE64)
  })

  it('trace sink writes failure-0001.png when registered', async () => {
    setTraceSink({ traceRoot: tmpRoot, caseSlug: 'demo' })
    const driver = makeDriver({
      describe: () => Promise.resolve(makeScreen(FAKE_PNG_BASE64)),
    })
    const exp = new ElementExpectation(makeHandle(driver))
    await expect(exp.toBeVisible({ timeout: 0 })).rejects.toBeInstanceOf(
      ExpectationFailure,
    )
    const out = join(tmpRoot, 'demo', 'failure-0001.png')
    const stats = await stat(out)
    expect(stats.size).toBeGreaterThan(0)
    const bytes = await readFile(out)
    const expected = Buffer.from(FAKE_PNG_BASE64, 'base64')
    expect(bytes.equals(expected)).toBe(true)
    // magic bytes preserved through base64 round-trip.
    expect(bytes.subarray(0, 8).equals(PNG_MAGIC)).toBe(true)
  })

  it('trace sink: seq increments on second matcher failure within same case', async () => {
    setTraceSink({ traceRoot: tmpRoot, caseSlug: 'twice' })
    const driver = makeDriver({
      describe: () => Promise.resolve(makeScreen(FAKE_PNG_BASE64)),
    })
    const exp1 = new ElementExpectation(makeHandle(driver))
    await expect(exp1.toBeVisible({ timeout: 0 })).rejects.toBeInstanceOf(
      ExpectationFailure,
    )
    const exp2 = new ElementExpectation(makeHandle(driver))
    await expect(exp2.toBeVisible({ timeout: 0 })).rejects.toBeInstanceOf(
      ExpectationFailure,
    )
    const dir = join(tmpRoot, 'twice')
    const files = (await readdir(dir)).sort()
    // v0.4 C6 widened this case's file set: each matcher failure now lands
    // both failure-<NNNN>.png AND failure-<NNNN>.json sharing the seq. The
    // seq-increment behaviour the case originally locks is still asserted
    // via the .png pair; the .json siblings are covered by the dedicated
    // C6 case below ('JSON file shares seq with PNG within same case').
    expect(files).toEqual([
      'failure-0001.json',
      'failure-0001.png',
      'failure-0002.json',
      'failure-0002.png',
    ])
  })

  it('no trace sink registered: screenshot field still populated, no fs writes', async () => {
    // sink is cleared in beforeEach; assert it's actually undefined here.
    expect(getTraceSink()).toBeUndefined()
    const driver = makeDriver({
      describe: () => Promise.resolve(makeScreen(FAKE_PNG_BASE64)),
    })
    const exp = new ElementExpectation(makeHandle(driver))
    let caught: unknown
    try {
      await exp.toBeVisible({ timeout: 0 })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).screenshot).toBe(FAKE_PNG_BASE64)
    // tmpRoot still empty: 0 PNG materialized.
    const entries = await readdir(tmpRoot)
    expect(entries).toEqual([])
  })

  it('writeFailurePng fs error is silent: matcher still throws ExpectationFailure', async () => {
    // Make recursive mkdir reject deterministically by pointing traceRoot at
    // an existing file (ENOTDIR for any child path). The matcher must still
    // surface the original NOT_VISIBLE failure with the in-memory screenshot.
    const filePath = join(tmpRoot, 'not-a-dir')
    await writeFile(filePath, 'x')
    setTraceSink({ traceRoot: filePath, caseSlug: 'demo' })
    const driver = makeDriver({
      describe: () => Promise.resolve(makeScreen(FAKE_PNG_BASE64)),
    })
    const exp = new ElementExpectation(makeHandle(driver))
    let caught: unknown
    try {
      await exp.toBeVisible({ timeout: 0 })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).code).toBe('NOT_VISIBLE')
    expect((caught as ExpectationFailure).screenshot).toBe(FAKE_PNG_BASE64)
  })
})

describe('ElementExpectation.toBeVisible failure path JSON (v0.4 C6)', () => {
  it('trace sink writes failure-0001.json containing toJSON(false) fields + screenshot_path', async () => {
    setTraceSink({ traceRoot: tmpRoot, caseSlug: 'demo' })
    const driver = makeDriver({
      describe: () => Promise.resolve(makeScreen(FAKE_PNG_BASE64)),
    })
    const exp = new ElementExpectation(makeHandle(driver))
    await expect(exp.toBeVisible({ timeout: 0 })).rejects.toBeInstanceOf(
      ExpectationFailure,
    )
    const out = join(tmpRoot, 'demo', 'failure-0001.json')
    const content = await readFile(out, 'utf8')
    expect(content).toContain('\n  ')
    const obj = JSON.parse(content) as Record<string, unknown>
    expect(obj['ok']).toBe(false)
    expect(obj['code']).toBe('NOT_VISIBLE')
    expect(typeof obj['message']).toBe('string')
    expect(obj['suggestions']).toEqual([])
    expect(obj['visibleElements']).toEqual([])
    expect(obj['selector']).toBeDefined()
    expect(obj['screenshot_path']).toBe('failure-0001.png')
    expect('screenshot' in obj).toBe(false)
  })

  it('JSON file shares seq with PNG within same case', async () => {
    setTraceSink({ traceRoot: tmpRoot, caseSlug: 'twice' })
    const driver = makeDriver({
      describe: () => Promise.resolve(makeScreen(FAKE_PNG_BASE64)),
    })
    const exp1 = new ElementExpectation(makeHandle(driver))
    await expect(exp1.toBeVisible({ timeout: 0 })).rejects.toBeInstanceOf(
      ExpectationFailure,
    )
    const exp2 = new ElementExpectation(makeHandle(driver))
    await expect(exp2.toBeVisible({ timeout: 0 })).rejects.toBeInstanceOf(
      ExpectationFailure,
    )
    const files = (await readdir(join(tmpRoot, 'twice'))).sort()
    expect(files).toEqual([
      'failure-0001.json',
      'failure-0001.png',
      'failure-0002.json',
      'failure-0002.png',
    ])
  })

  it('screenshot missing: JSON still written without screenshot_path', async () => {
    setTraceSink({ traceRoot: tmpRoot, caseSlug: 'nopng' })
    const screen: ScreenDescription = {
      screenshot: '',
      elements: [],
      frontApp: 'com.example',
      summary: '',
      capturedAt: 1700000000000,
    }
    const driver = makeDriver({
      describe: () => Promise.resolve(screen),
    })
    const exp = new ElementExpectation(makeHandle(driver))
    await expect(exp.toBeVisible({ timeout: 0 })).rejects.toBeInstanceOf(
      ExpectationFailure,
    )
    const entries = (await readdir(join(tmpRoot, 'nopng'))).sort()
    expect(entries).toEqual(['failure-0001.json'])
    const obj = JSON.parse(
      await readFile(join(tmpRoot, 'nopng', 'failure-0001.json'), 'utf8'),
    ) as Record<string, unknown>
    expect(obj['ok']).toBe(false)
    expect(obj['code']).toBe('NOT_VISIBLE')
    expect(typeof obj['message']).toBe('string')
    expect(obj['suggestions']).toEqual([])
    expect(obj['visibleElements']).toEqual([])
    expect(obj['selector']).toBeDefined()
    expect('screenshot_path' in obj).toBe(false)
    expect('screenshot' in obj).toBe(false)
  })

  it('JSON fs write failure is silent: matcher still throws ExpectationFailure with NOT_VISIBLE', async () => {
    // ENOTDIR on traceRoot child path blocks both PNG and JSON writes; both
    // paths use independent try/catch so neither failure masks the matcher
    // throw. ExpectationFailure.screenshot base64 stays in-memory.
    const filePath = join(tmpRoot, 'not-a-dir')
    await writeFile(filePath, 'x')
    setTraceSink({ traceRoot: filePath, caseSlug: 'demo' })
    const driver = makeDriver({
      describe: () => Promise.resolve(makeScreen(FAKE_PNG_BASE64)),
    })
    const exp = new ElementExpectation(makeHandle(driver))
    let caught: unknown
    try {
      await exp.toBeVisible({ timeout: 0 })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    expect((caught as ExpectationFailure).code).toBe('NOT_VISIBLE')
    expect((caught as ExpectationFailure).screenshot).toBe(FAKE_PNG_BASE64)
  })
})

describe('ElementExpectation.toBeVisible suggestions ranking (v0.4 C3)', () => {
  function makeScreenWithElements(
    elements: ScreenDescription['elements'],
  ): ScreenDescription {
    return {
      screenshot: FAKE_PNG_BASE64,
      elements,
      frontApp: 'com.example',
      summary: '',
      capturedAt: 1700000000000,
    }
  }

  async function runFailureWith(
    elements: ScreenDescription['elements'],
    selector: Selector,
  ): Promise<ExpectationFailure> {
    const driver = makeDriver({
      describe: () => Promise.resolve(makeScreenWithElements(elements)),
      resolveNode: null,
    })
    const exp = new ElementExpectation(makeHandle(driver, selector))
    let caught: unknown
    try {
      await exp.toBeVisible({ timeout: 0 })
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ExpectationFailure)
    return caught as ExpectationFailure
  }

  it('identical name yields high-similarity candidate above threshold', async () => {
    const f = await runFailureWith(
      [{ role: 'cell', name: 'General', bounds: { x: 0, y: 0, w: 10, h: 10 }, enabled: true }],
      { text: 'General' },
    )
    expect(f.suggestions.length).toBe(1)
    expect(f.suggestions[0]).toContain('"General"')
    expect(f.suggestions[0]).toContain('(similarity 1.00')
    expect(f.suggestions[0]).toContain('field name')
  })

  it('single-char typo ranks closest visible first, top-3', async () => {
    const f = await runFailureWith(
      [
        { role: 'cell', name: 'General', bounds: { x: 0, y: 0, w: 10, h: 10 }, enabled: true },
        { role: 'cell', name: 'About', bounds: { x: 0, y: 10, w: 10, h: 10 }, enabled: true },
        { role: 'cell', name: 'Display', bounds: { x: 0, y: 20, w: 10, h: 10 }, enabled: true },
        { role: 'cell', name: 'Privacy', bounds: { x: 0, y: 30, w: 10, h: 10 }, enabled: true },
      ],
      { text: 'Genrl' },
    )
    expect(f.suggestions.length).toBeGreaterThanOrEqual(1)
    expect(f.suggestions.length).toBeLessThanOrEqual(3)
    expect(f.suggestions[0]).toContain('"General"')
    expect(f.suggestions[0]).toMatch(/\(similarity 0\.\d{2}, field name\)/)
  })

  it('matches against text field when name differs', async () => {
    const f = await runFailureWith(
      [
        {
          role: 'staticText',
          name: 'wrapper',
          text: 'Display & Brightness',
          bounds: { x: 0, y: 0, w: 10, h: 10 },
          enabled: true,
        },
      ],
      { text: 'Display & Brightness' },
    )
    expect(f.suggestions.length).toBe(1)
    expect(f.suggestions[0]).toContain('"Display & Brightness"')
    expect(f.suggestions[0]).toContain('field text')
  })

  it('sorts by similarity desc, tiebreak field name > text then DFS index', async () => {
    // 3 visible all distance-1 to target 'Geneal'.
    // Index 0: name='Generl' (dist 1, sim 6/7≈0.857, field name)
    // Index 1: { name='wrapper', text='Genral' } — name distance 6 (low) so text 'Genral' wins
    //   for that element (dist 1, sim 6/7≈0.857, field text)
    // Index 2: name='Genral' (dist 1, sim 6/7≈0.857, field name)
    // Expected order: index 0 (name, idx 0), index 2 (name, idx 2), index 1 (text, idx 1)
    const f = await runFailureWith(
      [
        { role: 'cell', name: 'Generl', bounds: { x: 0, y: 0, w: 10, h: 10 }, enabled: true },
        {
          role: 'cell',
          name: 'wrapper',
          text: 'Genral',
          bounds: { x: 0, y: 10, w: 10, h: 10 },
          enabled: true,
        },
        { role: 'cell', name: 'Genral', bounds: { x: 0, y: 20, w: 10, h: 10 }, enabled: true },
      ],
      { text: 'Geneal' },
    )
    expect(f.suggestions.length).toBe(3)
    expect(f.suggestions[0]).toContain('"Generl"')
    expect(f.suggestions[0]).toContain('field name')
    expect(f.suggestions[1]).toContain('"Genral"')
    expect(f.suggestions[1]).toContain('field name')
    expect(f.suggestions[2]).toContain('"Genral"')
    expect(f.suggestions[2]).toContain('field text')
  })

  it('filters out candidates with similarity <= 0.5', async () => {
    // target 'Sgni' (4) vs 'General' (7) → longer=7, distance=5 → sim 2/7 ≈ 0.286 < 0.5
    const f = await runFailureWith(
      [
        { role: 'cell', name: 'General', bounds: { x: 0, y: 0, w: 10, h: 10 }, enabled: true },
        { role: 'cell', name: 'About', bounds: { x: 0, y: 10, w: 10, h: 10 }, enabled: true },
      ],
      { text: 'Sgni' },
    )
    expect(f.suggestions).toEqual([])
  })

  it('truncates to top 3 when more than 3 strong matches', async () => {
    // 5 candidates all distance-1 from 'General' (sim 6/7 ≈ 0.857).
    const f = await runFailureWith(
      [
        { role: 'cell', name: 'Generl', bounds: { x: 0, y: 0, w: 10, h: 10 }, enabled: true },
        { role: 'cell', name: 'Genral', bounds: { x: 0, y: 10, w: 10, h: 10 }, enabled: true },
        { role: 'cell', name: 'Genera', bounds: { x: 0, y: 20, w: 10, h: 10 }, enabled: true },
        { role: 'cell', name: 'eneral', bounds: { x: 0, y: 30, w: 10, h: 10 }, enabled: true },
        { role: 'cell', name: 'Generl ', bounds: { x: 0, y: 40, w: 10, h: 10 }, enabled: true },
      ],
      { text: 'General' },
    )
    expect(f.suggestions.length).toBe(3)
    // All entries should start with the canonical "Did you mean ..." prefix.
    for (const s of f.suggestions) {
      expect(s).toMatch(/^Did you mean ".+"\? \(similarity 0\.\d{2}, field name\)$/)
    }
  })

  it('id-only selector yields no suggestions (id not fuzzy)', async () => {
    const f = await runFailureWith(
      [{ role: 'cell', name: 'general-cell', bounds: { x: 0, y: 0, w: 10, h: 10 }, enabled: true }],
      { id: 'genral-cell' },
    )
    expect(f.suggestions).toEqual([])
  })

  it('empty visible list yields no suggestions (safeDescribe catch-all path)', async () => {
    const f = await runFailureWith([], { text: 'General' })
    expect(f.suggestions).toEqual([])
  })
})

// ============================================================
// v0.4 C7: matchers.pollFor exponential backoff + invert bug fix
//
// pollFor (matchers.ts ~line 120) is private — these tests drive it
// through the public ElementExpectation surface:
//   - non-invert path via toBeVisible / toHaveCount (5 matchers all share
//     the same pollFor; toBeVisible used as the canonical case)
//   - invert path via toBeHidden (the only invert caller)
// All cases use vi.useFakeTimers so CI < 50ms wall regardless of timeouts;
// pattern matches src/sim/__tests__/simctl.test.ts:449-461.
// ============================================================
describe('pollFor backoff (v0.4 C7)', () => {
  function makeVisibleNode(): A11yNode {
    return {
      rawType: 'button',
      label: 'X',
      bounds: { x: 0, y: 0, w: 80, h: 44 },
      enabled: true,
      selected: false,
      hasFocus: false,
      visible: true,
      children: [],
    }
  }

  it('immediate hit: probe returns non-null on first call, resolves without setTimeout', async () => {
    vi.useFakeTimers()
    try {
      const findOne = vi.fn(async () => makeVisibleNode())
      const driver = makeDriver({
        describe: () => Promise.resolve(makeScreen()),
      })
      // Cast to override the readonly findOne with a spy. The driver factory
      // builds findOne as a fixed-result function; for these timing tests we
      // need a vi.fn() to count probes.
      ;(driver as unknown as { findOne: typeof findOne }).findOne = findOne
      const exp = new ElementExpectation(makeHandle(driver))
      await exp.toBeVisible({ timeout: 5000 })
      expect(findOne).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('exponential backoff: 50 → 75 → 112 → 168 → ... findOne call count grows by formula', async () => {
    vi.useFakeTimers()
    try {
      const findOne = vi.fn(async () => null) // always miss
      const driver = makeDriver({
        describe: () => Promise.resolve(makeScreen()),
      })
      ;(driver as unknown as { findOne: typeof findOne }).findOne = findOne
      const exp = new ElementExpectation(makeHandle(driver))
      const pending = exp.toBeVisible({ timeout: 10_000 }).catch((e: unknown) => e)
      // First probe runs synchronously inside the loop (no sleep before it);
      // microtask flush is enough to count it.
      await vi.advanceTimersByTimeAsync(0)
      expect(findOne).toHaveBeenCalledTimes(1)
      // interval[0] = 50
      await vi.advanceTimersByTimeAsync(50)
      expect(findOne).toHaveBeenCalledTimes(2)
      // interval[1] = 75 (50 * 1.5)
      await vi.advanceTimersByTimeAsync(75)
      expect(findOne).toHaveBeenCalledTimes(3)
      // interval[2] = 112.5 → setTimeout uses 112.5; advance 113 to clear it
      await vi.advanceTimersByTimeAsync(113)
      expect(findOne).toHaveBeenCalledTimes(4)
      // interval[3] = 168.75 → advance 169
      await vi.advanceTimersByTimeAsync(169)
      expect(findOne).toHaveBeenCalledTimes(5)
      // Drain to completion so the dangling promise doesn't leak.
      await vi.advanceTimersByTimeAsync(10_000)
      await pending
    } finally {
      vi.useRealTimers()
    }
  })

  it('deadline truncation: last sleep clipped to remaining budget', async () => {
    vi.useFakeTimers()
    try {
      const findOne = vi.fn(async () => null)
      const driver = makeDriver({
        describe: () => Promise.resolve(makeScreen()),
      })
      ;(driver as unknown as { findOne: typeof findOne }).findOne = findOne
      const exp = new ElementExpectation(makeHandle(driver))
      const pending = exp.toBeVisible({ timeout: 100 }).catch((e: unknown) => e)
      // probe#1 at t=0
      await vi.advanceTimersByTimeAsync(0)
      expect(findOne).toHaveBeenCalledTimes(1)
      // probe#2 at t=50
      await vi.advanceTimersByTimeAsync(50)
      expect(findOne).toHaveBeenCalledTimes(2)
      // remaining = 50ms; next sleep is min(75, 50) = 50; probe#3 at t=100
      await vi.advanceTimersByTimeAsync(50)
      expect(findOne).toHaveBeenCalledTimes(3)
      // deadline now reached → timeout
      const caught = await pending
      expect(caught).toBeInstanceOf(ExpectationFailure)
      expect((caught as ExpectationFailure).code).toBe('NOT_VISIBLE')
    } finally {
      vi.useRealTimers()
    }
  })

  it('timeout throws ExpectationFailure with code NOT_VISIBLE when probe never resolves', async () => {
    vi.useFakeTimers()
    try {
      const findOne = vi.fn(async () => null)
      const driver = makeDriver({
        describe: () => Promise.resolve(makeScreen()),
      })
      ;(driver as unknown as { findOne: typeof findOne }).findOne = findOne
      const exp = new ElementExpectation(makeHandle(driver))
      const pending = exp.toBeVisible({ timeout: 200 }).catch((e: unknown) => e)
      await vi.advanceTimersByTimeAsync(300)
      const caught = await pending
      expect(caught).toBeInstanceOf(ExpectationFailure)
      expect((caught as ExpectationFailure).code).toBe('NOT_VISIBLE')
    } finally {
      vi.useRealTimers()
    }
  })

  it('toBeHidden invert timeout: throws ASSERTION_FAILED when element stays visible throughout', async () => {
    vi.useFakeTimers()
    try {
      const visibleNode = makeVisibleNode()
      const findOne = vi.fn(async () => visibleNode)
      const driver = makeDriver({
        describe: () => Promise.resolve(makeScreen()),
      })
      ;(driver as unknown as { findOne: typeof findOne }).findOne = findOne
      const exp = new ElementExpectation(makeHandle(driver))
      const pending = exp.toBeHidden({ timeout: 100 }).catch((e: unknown) => e)
      await vi.advanceTimersByTimeAsync(200)
      const caught = await pending
      expect(caught).toBeInstanceOf(ExpectationFailure)
      expect((caught as ExpectationFailure).code).toBe('ASSERTION_FAILED')
      expect((caught as ExpectationFailure).message.toLowerCase()).toMatch(/hidden.*visible/)
    } finally {
      vi.useRealTimers()
    }
  })

  it('toBeHidden invert success: resolves silently when element absent on first probe', async () => {
    vi.useFakeTimers()
    try {
      const findOne = vi.fn(async () => null) // already hidden
      const driver = makeDriver({
        describe: () => Promise.resolve(makeScreen()),
      })
      ;(driver as unknown as { findOne: typeof findOne }).findOne = findOne
      const exp = new ElementExpectation(makeHandle(driver))
      await expect(exp.toBeHidden({ timeout: 1000 })).resolves.toBeUndefined()
      expect(findOne).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })
})
