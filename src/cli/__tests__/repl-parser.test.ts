import { describe, it, expect } from 'vitest'
import { parseReplLine } from '../commands/repl-parser.js'

describe('parseReplLine', () => {
  it('treats empty string as noop', () => {
    expect(parseReplLine('')).toEqual({ kind: 'noop' })
  })

  it('treats whitespace-only line as noop', () => {
    expect(parseReplLine('   \t  ')).toEqual({ kind: 'noop' })
  })

  it('parses .exit meta', () => {
    expect(parseReplLine('.exit')).toEqual({ kind: 'meta', verb: '.exit' })
  })

  it('parses .help meta', () => {
    expect(parseReplLine('.help')).toEqual({ kind: 'meta', verb: '.help' })
  })

  it('rejects unknown meta', () => {
    const r = parseReplLine('.foo')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/unknown meta '\.foo'/)
    }
  })

  it('parses launch <bundleId>', () => {
    expect(parseReplLine('launch com.apple.Preferences')).toEqual({
      kind: 'verb',
      verb: 'launch',
      arg: 'com.apple.Preferences',
    })
  })

  it('rejects launch without arg', () => {
    const r = parseReplLine('launch')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/launch requires a bundleId/)
    }
  })

  it('parses terminate <bundleId>', () => {
    expect(parseReplLine('terminate com.apple.Preferences')).toEqual({
      kind: 'verb',
      verb: 'terminate',
      arg: 'com.apple.Preferences',
    })
  })

  it('parses screenshot (no args)', () => {
    expect(parseReplLine('screenshot')).toEqual({
      kind: 'verb',
      verb: 'screenshot',
    })
  })

  it('rejects screenshot with extra args', () => {
    const r = parseReplLine('screenshot extra')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/screenshot takes no arguments/)
    }
  })

  it('parses describe (no args)', () => {
    expect(parseReplLine('describe')).toEqual({
      kind: 'verb',
      verb: 'describe',
    })
  })

  it('parses tap text=<value>', () => {
    expect(parseReplLine('tap text=General')).toEqual({
      kind: 'verb',
      verb: 'tap',
      selector: { text: 'General' },
    })
  })

  it('parses tap id=<value>', () => {
    expect(parseReplLine('tap id=AccountCell')).toEqual({
      kind: 'verb',
      verb: 'tap',
      selector: { id: 'AccountCell' },
    })
  })

  it('parses tap label=<value>', () => {
    expect(parseReplLine('tap label=Done')).toEqual({
      kind: 'verb',
      verb: 'tap',
      selector: { label: 'Done' },
    })
  })

  it('parses tap role=<role>,name=<name>', () => {
    expect(parseReplLine('tap role=button,name=General')).toEqual({
      kind: 'verb',
      verb: 'tap',
      selector: { role: 'button', name: 'General' },
    })
  })

  it('rejects multi-modifier selector with C5 hint', () => {
    const r = parseReplLine('tap text=General,below=text=Settings')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/modifier not supported in C1/)
    }
  })

  it('rejects tap without selector', () => {
    const r = parseReplLine('tap')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/tap requires a selector/)
    }
  })

  it('rejects tap with bad selector key', () => {
    const r = parseReplLine('tap foo=bar')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/bad selector/)
    }
  })

  it('rejects tap with empty value', () => {
    const r = parseReplLine('tap text=')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/text= must have value/)
    }
  })

  it('rejects unknown verb', () => {
    const r = parseReplLine('unknown_verb foo')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/unknown verb 'unknown_verb'/)
    }
  })

  it('parses tap role=<role> without name', () => {
    expect(parseReplLine('tap role=button')).toEqual({
      kind: 'verb',
      verb: 'tap',
      selector: { role: 'button' },
    })
  })
})

describe('meta verbs .history/.undo/.redo (v0.5 C2)', () => {
  it('parses .history meta', () => {
    expect(parseReplLine('.history')).toEqual({ kind: 'meta', verb: '.history' })
  })

  it('parses .undo meta', () => {
    expect(parseReplLine('.undo')).toEqual({ kind: 'meta', verb: '.undo' })
  })

  it('parses .redo meta', () => {
    expect(parseReplLine('.redo')).toEqual({ kind: 'meta', verb: '.redo' })
  })

  it('rejects .history with trailing arg', () => {
    const r = parseReplLine('.history 20')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/\.history takes no arguments/)
    }
  })

  it('rejects .undo with trailing arg', () => {
    const r = parseReplLine('.undo 5')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/\.undo takes no arguments/)
    }
  })

  it('rejects .redo with trailing arg', () => {
    const r = parseReplLine('.redo foo')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/\.redo takes no arguments/)
    }
  })
})

describe('.save as <name> grammar (v0.5 C3)', () => {
  it('parses .save as foo', () => {
    expect(parseReplLine('.save as foo')).toEqual({
      kind: 'meta',
      verb: '.save',
      name: 'foo',
    })
  })

  it('parses .save as my-flow-1 (dash and digits allowed by parser shape)', () => {
    expect(parseReplLine('.save as my-flow-1')).toEqual({
      kind: 'meta',
      verb: '.save',
      name: 'my-flow-1',
    })
  })

  it('accepts bad/path in parser layer (whitelist enforced in dispatch)', () => {
    expect(parseReplLine('.save as bad/path')).toEqual({
      kind: 'meta',
      verb: '.save',
      name: 'bad/path',
    })
  })

  it('rejects bare .save (no arg)', () => {
    const r = parseReplLine('.save')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/\.save requires 'as <name>'/)
    }
  })

  it("rejects .save foo (missing 'as' keyword)", () => {
    const r = parseReplLine('.save foo')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/\.save requires 'as <name>'/)
    }
  })

  it("rejects .save as (empty name after 'as')", () => {
    const r = parseReplLine('.save as')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/\.save requires a name after 'as'/)
    }
  })

  it('rejects .save as foo bar (trailing extra arg)', () => {
    const r = parseReplLine('.save as foo bar')
    expect(r.kind).toBe('error')
    if (r.kind === 'error') {
      expect(r.message).toMatch(/\.save expects exactly one name argument/)
    }
  })
})
