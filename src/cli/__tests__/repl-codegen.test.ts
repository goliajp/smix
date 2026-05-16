import { describe, it, expect, afterAll } from 'vitest'
import { promises as fs } from 'node:fs'
import * as path from 'node:path'
import {
  generateTestTs,
  isValidSaveName,
  validateGeneratedTs,
  type HistoryEntryLike,
} from '../commands/repl-codegen.js'
import type { ParsedCommand } from '../commands/repl-parser.js'

function entry(seq: number, raw: string, parsed: ParsedCommand): HistoryEntryLike {
  return { seq, raw, parsed }
}

describe('isValidSaveName (v0.5 C3)', () => {
  it('accepts simple lowercase names', () => {
    expect(isValidSaveName('foo')).toBe(true)
    expect(isValidSaveName('a')).toBe(true)
    expect(isValidSaveName('my-flow-1')).toBe(true)
    expect(isValidSaveName('abc123')).toBe(true)
    expect(isValidSaveName('a'.repeat(64))).toBe(true)
  })

  it('rejects empty / leading dash / uppercase / path-sep / space / overlong', () => {
    expect(isValidSaveName('')).toBe(false)
    expect(isValidSaveName('-foo')).toBe(false)
    expect(isValidSaveName('Foo')).toBe(false)
    expect(isValidSaveName('FOO')).toBe(false)
    expect(isValidSaveName('foo/bar')).toBe(false)
    expect(isValidSaveName('foo\\bar')).toBe(false)
    expect(isValidSaveName('foo.bar')).toBe(false)
    expect(isValidSaveName('foo bar')).toBe(false)
    expect(isValidSaveName('a'.repeat(65))).toBe(false)
    expect(isValidSaveName('..')).toBe(false)
  })
})

describe('generateTestTs (v0.5 C3)', () => {
  it('emits header + empty body when entries is []', () => {
    const out = generateTestTs([], 'empty-case')
    expect(out).toContain("import { test } from '../src/sdk/index.js'")
    expect(out).toContain("test('empty-case', async ({ app }) => {")
    expect(out).toMatch(/\}\)\n$/)
  })

  it('renders a single launch action', () => {
    const out = generateTestTs(
      [entry(1, 'launch com.apple.Preferences', {
        kind: 'verb',
        verb: 'launch',
        arg: 'com.apple.Preferences',
      })],
      'one-launch',
    )
    expect(out).toContain("  await app.launch('com.apple.Preferences')")
  })

  it('preserves entry order across multiple actions', () => {
    const out = generateTestTs(
      [
        entry(1, 'launch X', { kind: 'verb', verb: 'launch', arg: 'X' }),
        entry(2, 'tap text=General', {
          kind: 'verb',
          verb: 'tap',
          selector: { text: 'General' },
        }),
        entry(3, 'tap id=About', {
          kind: 'verb',
          verb: 'tap',
          selector: { id: 'About' },
        }),
      ],
      'multi',
    )
    const idxLaunch = out.indexOf("await app.launch('X')")
    const idxTapGeneral = out.indexOf("await app.tap({ text: 'General' })")
    const idxTapAbout = out.indexOf("await app.tap({ id: 'About' })")
    expect(idxLaunch).toBeGreaterThan(-1)
    expect(idxTapGeneral).toBeGreaterThan(idxLaunch)
    expect(idxTapAbout).toBeGreaterThan(idxTapGeneral)
  })

  it('renders tap role+name as object literal in canonical order', () => {
    const out = generateTestTs(
      [entry(1, 'tap role=button,name=General', {
        kind: 'verb',
        verb: 'tap',
        selector: { role: 'button', name: 'General' },
      })],
      'role-name',
    )
    expect(out).toContain("  await app.tap({ role: 'button', name: 'General' })")
  })

  it('renders tap label as object literal', () => {
    const out = generateTestTs(
      [entry(1, 'tap label=Done', {
        kind: 'verb',
        verb: 'tap',
        selector: { label: 'Done' },
      })],
      'label-only',
    )
    expect(out).toContain("  await app.tap({ label: 'Done' })")
  })

  it('skips describe action (decision 4.e)', () => {
    const out = generateTestTs(
      [
        entry(1, 'launch X', { kind: 'verb', verb: 'launch', arg: 'X' }),
        entry(2, 'describe', { kind: 'verb', verb: 'describe' }),
        entry(3, 'tap text=General', {
          kind: 'verb',
          verb: 'tap',
          selector: { text: 'General' },
        }),
      ],
      'skip-describe',
    )
    const awaitCount = (out.match(/await app\./g) ?? []).length
    expect(awaitCount).toBe(2)
    expect(out).not.toMatch(/app\.describe\(/)
  })

  it('renders screenshot as await app.screenshot()', () => {
    const out = generateTestTs(
      [entry(1, 'screenshot', { kind: 'verb', verb: 'screenshot' })],
      'snap',
    )
    expect(out).toContain('  await app.screenshot()')
  })

  it("escapes single quote in string literals", () => {
    const out = generateTestTs(
      [entry(1, "launch com'x", { kind: 'verb', verb: 'launch', arg: "com'x" })],
      'esc',
    )
    expect(out).toContain("await app.launch('com\\'x')")
  })

  it('uses LF line endings and ends with single newline', () => {
    const out = generateTestTs(
      [entry(1, 'launch X', { kind: 'verb', verb: 'launch', arg: 'X' })],
      'lf-only',
    )
    expect(out).not.toMatch(/\r/)
    expect(out.endsWith('\n')).toBe(true)
    expect(out.endsWith('\n\n')).toBe(false)
  })

  it("first line is import from '../src/sdk/index.js'", () => {
    const out = generateTestTs([], 'header')
    const firstLine = out.split('\n')[0]
    expect(firstLine).toBe("import { test } from '../src/sdk/index.js'")
  })

  it('uses 2-space indent for action lines', () => {
    const out = generateTestTs(
      [entry(1, 'launch X', { kind: 'verb', verb: 'launch', arg: 'X' })],
      'indent',
    )
    expect(out).toMatch(/\n {2}await app\.launch/)
  })
})

describe('validateGeneratedTs (v0.5 C3 — real bun x tsc)', () => {
  const tmpFiles: string[] = []
  afterAll(async () => {
    for (const f of tmpFiles) {
      try {
        await fs.rm(f)
      } catch {
        // ignore
      }
    }
  })

  it('returns ok for a generated TS that compiles', async () => {
    const abs = path.resolve(process.cwd(), 'examples', '_test-tmp-ok.test.ts')
    tmpFiles.push(abs)
    const content = generateTestTs(
      [entry(1, 'launch com.apple.Preferences', {
        kind: 'verb',
        verb: 'launch',
        arg: 'com.apple.Preferences',
      })],
      'tmp-ok',
    )
    await fs.writeFile(abs, content, 'utf8')
    const r = await validateGeneratedTs(abs)
    expect(r.ok).toBe(true)
  }, 30000)

  it('returns fail with `error TS` for a TS file with type error', async () => {
    const abs = path.resolve(process.cwd(), 'examples', '_test-tmp-bad.test.ts')
    tmpFiles.push(abs)
    const bad = [
      "import { test } from '../src/sdk/index.js'",
      '',
      "test('bad', async ({ app }) => {",
      "  const x: number = 'string'",
      '  void x',
      '  void app',
      '})',
      '',
    ].join('\n')
    await fs.writeFile(abs, bad, 'utf8')
    const r = await validateGeneratedTs(abs)
    expect(r.ok).toBe(false)
    if (!r.ok) {
      expect(r.output).toMatch(/error TS\d+/)
    }
  }, 30000)
})
