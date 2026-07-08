// v7.5 c1 — Selector full 7-case + Modifiers flatten + Pattern + fluent
// chaining roundtrip + Rust-compatible wire shape tests.
//
// Mirror Kotlin SelectorFullSchemaTest + Swift SelectorFullSchemaTests.

import { describe, expect, test } from 'vitest'
import {
  encodeSelectorJson,
  literal,
  regex,
  Selector,
  selectorFromJsonValue,
  selectorToJsonValue,
  type AnchorBox,
} from '../index.js'

describe('Pattern wire shape', () => {
  test('literal encodes as bare string', () => {
    const sel = Selector.text(literal('hello'))
    const json = encodeSelectorJson(sel)
    expect(json).toContain('"text":"hello"')
  })

  test('regex encodes as object', () => {
    const sel = Selector.text(regex('foo.*', 'i'))
    const json = encodeSelectorJson(sel)
    expect(json).toContain('"regex":"foo.*"')
    expect(json).toContain('"flags":"i"')
  })
})

describe('Selector base wire shape', () => {
  test('id with modifiers flatten', () => {
    const sel = Selector.id('btn-login').below(Selector.text(literal('Sign In'))).nth(0)
    const json = encodeSelectorJson(sel)
    expect(json).toContain('"id":"btn-login"')
    expect(json).toContain('"below":{"text":"Sign In"}')
    expect(json).toContain('"nth":0')
  })

  test('role with name pattern', () => {
    const sel = Selector.role('button', literal('Submit'))
    const json = encodeSelectorJson(sel)
    expect(json).toContain('"role":"button"')
    expect(json).toContain('"name":"Submit"')
  })

  test('focused encodes as boolean', () => {
    const json = encodeSelectorJson(Selector.focused())
    expect(json).toBe('{"focused":true}')
  })

  test('anchor with index', () => {
    const box: AnchorBox = { below: Selector.text(literal('Address')) }
    const sel = Selector.anchor(box, { nth: 2 })
    const json = encodeSelectorJson(sel)
    expect(json).toContain('"anchor":{"below":{"text":"Address"}}')
    expect(json).toContain('"nth":2')
  })

  test('localizedText carries multi-locale map', () => {
    const sel = Selector.localizedText({ en: 'Submit', ja: '送信' })
    const json = encodeSelectorJson(sel)
    expect(json).toContain('"localizedText"')
    expect(json).toContain('Submit')
    expect(json).toContain('送信')
  })
})

describe('Wire decode roundtrip', () => {
  test('id with below and nth', () => {
    const wire = { id: 'btn-login', below: { text: 'Sign In' }, nth: 0 }
    const sel = selectorFromJsonValue(wire)
    expect(sel.data.kind).toBe('id')
    if (sel.data.kind === 'id') {
      expect(sel.data.id).toBe('btn-login')
      expect(sel.data.modifiers.nth).toBe(0)
      const below = sel.data.modifiers.below
      expect(below?.data.kind).toBe('text')
    }
  })

  test('focused', () => {
    const sel = selectorFromJsonValue({ focused: true })
    expect(sel.data.kind).toBe('focused')
  })

  test('anchor', () => {
    const sel = selectorFromJsonValue({ anchor: { below: { text: 'Address' } }, nth: 2 })
    expect(sel.data.kind).toBe('anchor')
    if (sel.data.kind === 'anchor') {
      expect(sel.data.index.nth).toBe(2)
    }
  })

  test('localizedText', () => {
    const sel = selectorFromJsonValue({ localizedText: { en: 'Submit', ja: '送信' } })
    expect(sel.data.kind).toBe('localizedText')
    if (sel.data.kind === 'localizedText') {
      expect(sel.data.map.en).toBe('Submit')
    }
  })

  test('role with regex pattern', () => {
    const sel = selectorFromJsonValue({ role: 'button', name: { regex: '^Sub', flags: 'i' } })
    expect(sel.data.kind).toBe('role')
    if (sel.data.kind === 'role') {
      expect(sel.data.role).toBe('button')
      expect(sel.data.name?.kind).toBe('regex')
    }
  })
})

describe('Fluent chaining', () => {
  test('below sets modifier', () => {
    const sel = Selector.id('btn').below(Selector.text(literal('anchor')))
    if (sel.data.kind === 'id') {
      expect(sel.data.modifiers.below).toBeDefined()
    }
  })

  test('chained multiple', () => {
    const sel = Selector.id('btn')
      .below(Selector.text(literal('address')))
      .above(Selector.text(literal('title')))
      .nth(3)
    if (sel.data.kind === 'id') {
      expect(sel.data.modifiers.below).toBeDefined()
      expect(sel.data.modifiers.above).toBeDefined()
      expect(sel.data.modifiers.nth).toBe(3)
    }
  })

  test('first and last', () => {
    const f = Selector.label('Item').first()
    const l = Selector.label('Item').last()
    if (f.data.kind === 'label') expect(f.data.modifiers.first).toBe(true)
    if (l.data.kind === 'label') expect(l.data.modifiers.last).toBe(true)
  })
})

describe('Roundtrip with modifiers', () => {
  test('id with multiple modifiers byte-identical roundtrip', () => {
    const original = Selector.id('btn-foo')
      .below(Selector.text(literal('Address')))
      .nth(0)
    const json = JSON.stringify(selectorToJsonValue(original))
    const decoded = selectorFromJsonValue(JSON.parse(json))
    const reencoded = JSON.stringify(selectorToJsonValue(decoded))
    expect(reencoded).toBe(json)
  })

  test('role with modifiers and name roundtrip', () => {
    const original = Selector.role('button', literal('Submit'))
      .below(Selector.text(literal('Email')))
    const json = JSON.stringify(selectorToJsonValue(original))
    const decoded = selectorFromJsonValue(JSON.parse(json))
    const reencoded = JSON.stringify(selectorToJsonValue(decoded))
    expect(reencoded).toBe(json)
  })
})
