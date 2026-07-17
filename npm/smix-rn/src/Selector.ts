// The wire form of `smix-selector`'s untagged `Selector` enum, with
// `Modifiers` flattened onto the Text/Id/Label/Role/LocalizedText bases.
// Focused takes no modifiers; Anchor carries AnchorBox + IndexModifiers.
//
//   sel.id('btn-login')
//     .below(sel.text(literal('Sign In')))
//     .nth(0)
//
// Each fluent method returns a NEW Selector, so a partly-built selector
// can be shared and specialised without the branches affecting each other.

import { type Pattern, patternFromJson, patternToJson } from './Pattern.js'

/** All-optional modifier set stacked onto a base Selector. */
export interface Modifiers {
  near?: Selector | undefined
  below?: Selector | undefined
  above?: Selector | undefined
  leftOf?: Selector | undefined
  rightOf?: Selector | undefined
  inside?: Selector | undefined
  ancestor?: Selector | undefined
  nth?: number | undefined
  first?: boolean | undefined
  last?: boolean | undefined
}

/** Spatial-only anchor box (no index, no base). Used by `anchor()`. */
export interface AnchorBox {
  near?: Selector | undefined
  below?: Selector | undefined
  above?: Selector | undefined
  leftOf?: Selector | undefined
  rightOf?: Selector | undefined
  inside?: Selector | undefined
  ancestor?: Selector | undefined
}

/** Index-only modifier subset used by `anchor()`. */
export interface IndexModifiers {
  nth?: number | undefined
  first?: boolean | undefined
  last?: boolean | undefined
}

export type SelectorKind =
  | 'id' | 'text' | 'label' | 'role'
  | 'focused' | 'anchor' | 'localizedText'

/** Discriminated union (kind tag for narrowing; wire-shape is untagged). */
export type SelectorData =
  | { kind: 'id'; id: string; modifiers: Modifiers }
  | { kind: 'text'; text: Pattern; modifiers: Modifiers }
  | { kind: 'label'; label: string; modifiers: Modifiers }
  | { kind: 'role'; role: string; name?: Pattern | undefined; modifiers: Modifiers }
  | { kind: 'focused' }
  | { kind: 'anchor'; box: AnchorBox; index: IndexModifiers }
  | { kind: 'localizedText'; map: Readonly<Record<string, string>>; modifiers: Modifiers }

/**
 * A11y selector — identifies a node in the tree.
 *
 * Wire JSON shape matches Rust smix-selector `#[serde(untagged)]`:
 * discriminator is which key is present. Modifiers are flattened
 * into the same JSON body. Implemented via [selectorToJson].
 */
export class Selector {
  constructor(public readonly data: SelectorData) {}

  // ---- Constructors -----------------------------------------------------

  static id(value: string): Selector {
    return new Selector({ kind: 'id', id: value, modifiers: {} })
  }
  static text(p: Pattern): Selector {
    return new Selector({ kind: 'text', text: p, modifiers: {} })
  }
  static label(value: string): Selector {
    return new Selector({ kind: 'label', label: value, modifiers: {} })
  }
  static role(value: string, name?: Pattern): Selector {
    return new Selector({
      kind: 'role',
      role: value,
      ...(name !== undefined ? { name } : {}),
      modifiers: {},
    })
  }
  static focused(): Selector {
    return new Selector({ kind: 'focused' })
  }
  static anchor(box: AnchorBox, index: IndexModifiers = {}): Selector {
    return new Selector({ kind: 'anchor', box, index })
  }
  static localizedText(map: Record<string, string>): Selector {
    return new Selector({ kind: 'localizedText', map, modifiers: {} })
  }

  // ---- Fluent chaining --------------------------------------------------

  below(anchor: Selector): Selector { return this.withMods(m => ({ ...m, below: anchor })) }
  above(anchor: Selector): Selector { return this.withMods(m => ({ ...m, above: anchor })) }
  leftOf(anchor: Selector): Selector { return this.withMods(m => ({ ...m, leftOf: anchor })) }
  rightOf(anchor: Selector): Selector { return this.withMods(m => ({ ...m, rightOf: anchor })) }
  near(anchor: Selector): Selector { return this.withMods(m => ({ ...m, near: anchor })) }
  inside(anchor: Selector): Selector { return this.withMods(m => ({ ...m, inside: anchor })) }
  ancestor(anchor: Selector): Selector { return this.withMods(m => ({ ...m, ancestor: anchor })) }
  nth(i: number): Selector { return this.withMods(m => ({ ...m, nth: i })) }
  first(): Selector { return this.withMods(m => ({ ...m, first: true })) }
  last(): Selector { return this.withMods(m => ({ ...m, last: true })) }

  private withMods(mutate: (m: Modifiers) => Modifiers): Selector {
    const d = this.data
    switch (d.kind) {
      case 'id':
        return new Selector({ ...d, modifiers: mutate(d.modifiers) })
      case 'text':
        return new Selector({ ...d, modifiers: mutate(d.modifiers) })
      case 'label':
        return new Selector({ ...d, modifiers: mutate(d.modifiers) })
      case 'role':
        return new Selector({ ...d, modifiers: mutate(d.modifiers) })
      case 'localizedText':
        return new Selector({ ...d, modifiers: mutate(d.modifiers) })
      case 'focused':
        return this  // no modifiers — no-op
      case 'anchor': {
        const proxy = mutate({})
        const newIdx: IndexModifiers = {
          ...(proxy.nth !== undefined ? { nth: proxy.nth } : d.index.nth !== undefined ? { nth: d.index.nth } : {}),
          ...(proxy.first !== undefined ? { first: proxy.first } : d.index.first !== undefined ? { first: d.index.first } : {}),
          ...(proxy.last !== undefined ? { last: proxy.last } : d.index.last !== undefined ? { last: d.index.last } : {}),
        }
        return new Selector({ ...d, index: newIdx })
      }
    }
  }
}

// ---- JSON encoding (Rust-compatible untagged + flatten) -----------------

function addModifiers(out: Record<string, unknown>, m: Modifiers): void {
  if (m.near !== undefined) out.near = selectorToJsonValue(m.near)
  if (m.below !== undefined) out.below = selectorToJsonValue(m.below)
  if (m.above !== undefined) out.above = selectorToJsonValue(m.above)
  if (m.leftOf !== undefined) out.leftOf = selectorToJsonValue(m.leftOf)
  if (m.rightOf !== undefined) out.rightOf = selectorToJsonValue(m.rightOf)
  if (m.inside !== undefined) out.inside = selectorToJsonValue(m.inside)
  if (m.ancestor !== undefined) out.ancestor = selectorToJsonValue(m.ancestor)
  if (m.nth !== undefined) out.nth = m.nth
  if (m.first !== undefined) out.first = m.first
  if (m.last !== undefined) out.last = m.last
}

function addAnchorBox(out: Record<string, unknown>, b: AnchorBox): void {
  if (b.near !== undefined) out.near = selectorToJsonValue(b.near)
  if (b.below !== undefined) out.below = selectorToJsonValue(b.below)
  if (b.above !== undefined) out.above = selectorToJsonValue(b.above)
  if (b.leftOf !== undefined) out.leftOf = selectorToJsonValue(b.leftOf)
  if (b.rightOf !== undefined) out.rightOf = selectorToJsonValue(b.rightOf)
  if (b.inside !== undefined) out.inside = selectorToJsonValue(b.inside)
  if (b.ancestor !== undefined) out.ancestor = selectorToJsonValue(b.ancestor)
}

function addIndex(out: Record<string, unknown>, i: IndexModifiers): void {
  if (i.nth !== undefined) out.nth = i.nth
  if (i.first !== undefined) out.first = i.first
  if (i.last !== undefined) out.last = i.last
}

/** Encode a Selector to its Rust-compatible untagged JSON value. */
export function selectorToJsonValue(s: Selector): unknown {
  const out: Record<string, unknown> = {}
  const d = s.data
  switch (d.kind) {
    case 'id':
      out.id = d.id
      addModifiers(out, d.modifiers)
      break
    case 'text':
      out.text = patternToJson(d.text)
      addModifiers(out, d.modifiers)
      break
    case 'label':
      out.label = d.label
      addModifiers(out, d.modifiers)
      break
    case 'role':
      out.role = d.role
      if (d.name !== undefined) out.name = patternToJson(d.name)
      addModifiers(out, d.modifiers)
      break
    case 'focused':
      out.focused = true
      break
    case 'anchor': {
      const box: Record<string, unknown> = {}
      addAnchorBox(box, d.box)
      out.anchor = box
      addIndex(out, d.index)
      break
    }
    case 'localizedText':
      out.localizedText = d.map
      addModifiers(out, d.modifiers)
      break
  }
  return out
}

/** Encode a Selector to its Rust-compatible untagged JSON string. */
export function encodeSelectorJson(s: Selector): string {
  return JSON.stringify(selectorToJsonValue(s))
}

/** Decode a Rust-compatible untagged JSON value into a Selector. */
export function selectorFromJsonValue(value: unknown): Selector {
  if (typeof value !== 'object' || value === null) {
    throw new Error(`Selector must be a JSON object, got: ${JSON.stringify(value)}`)
  }
  const obj = value as Record<string, unknown>

  // Focused: explicit boolean field, highest-priority discriminator.
  if (obj.focused === true) return Selector.focused()

  // Anchor: nested box discriminator.
  if (obj.anchor !== undefined && typeof obj.anchor === 'object' && obj.anchor !== null) {
    const box = decodeAnchorBox(obj.anchor as Record<string, unknown>)
    return Selector.anchor(box, decodeIndex(obj))
  }

  if (typeof obj.id === 'string') {
    return new Selector({ kind: 'id', id: obj.id, modifiers: decodeModifiers(obj) })
  }
  if (obj.text !== undefined) {
    return new Selector({ kind: 'text', text: patternFromJson(obj.text), modifiers: decodeModifiers(obj) })
  }
  if (typeof obj.label === 'string') {
    return new Selector({ kind: 'label', label: obj.label, modifiers: decodeModifiers(obj) })
  }
  if (typeof obj.role === 'string') {
    const name = obj.name !== undefined ? patternFromJson(obj.name) : undefined
    return new Selector({
      kind: 'role',
      role: obj.role,
      ...(name !== undefined ? { name } : {}),
      modifiers: decodeModifiers(obj),
    })
  }
  if (obj.localizedText !== undefined && typeof obj.localizedText === 'object' && obj.localizedText !== null) {
    return new Selector({
      kind: 'localizedText',
      map: obj.localizedText as Record<string, string>,
      modifiers: decodeModifiers(obj),
    })
  }
  throw new Error(`Selector JSON missing recognized discriminator: ${JSON.stringify(obj)}`)
}

function decodeModifiers(obj: Record<string, unknown>): Modifiers {
  const m: Modifiers = {}
  if (obj.near !== undefined) m.near = selectorFromJsonValue(obj.near)
  if (obj.below !== undefined) m.below = selectorFromJsonValue(obj.below)
  if (obj.above !== undefined) m.above = selectorFromJsonValue(obj.above)
  if (obj.leftOf !== undefined) m.leftOf = selectorFromJsonValue(obj.leftOf)
  if (obj.rightOf !== undefined) m.rightOf = selectorFromJsonValue(obj.rightOf)
  if (obj.inside !== undefined) m.inside = selectorFromJsonValue(obj.inside)
  if (obj.ancestor !== undefined) m.ancestor = selectorFromJsonValue(obj.ancestor)
  if (typeof obj.nth === 'number') m.nth = obj.nth
  if (typeof obj.first === 'boolean') m.first = obj.first
  if (typeof obj.last === 'boolean') m.last = obj.last
  return m
}

function decodeAnchorBox(obj: Record<string, unknown>): AnchorBox {
  const b: AnchorBox = {}
  if (obj.near !== undefined) b.near = selectorFromJsonValue(obj.near)
  if (obj.below !== undefined) b.below = selectorFromJsonValue(obj.below)
  if (obj.above !== undefined) b.above = selectorFromJsonValue(obj.above)
  if (obj.leftOf !== undefined) b.leftOf = selectorFromJsonValue(obj.leftOf)
  if (obj.rightOf !== undefined) b.rightOf = selectorFromJsonValue(obj.rightOf)
  if (obj.inside !== undefined) b.inside = selectorFromJsonValue(obj.inside)
  if (obj.ancestor !== undefined) b.ancestor = selectorFromJsonValue(obj.ancestor)
  return b
}

function decodeIndex(obj: Record<string, unknown>): IndexModifiers {
  const i: IndexModifiers = {}
  if (typeof obj.nth === 'number') i.nth = obj.nth
  if (typeof obj.first === 'boolean') i.first = obj.first
  if (typeof obj.last === 'boolean') i.last = obj.last
  return i
}
