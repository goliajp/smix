import { describe, it, expect } from 'vitest'
import { resolveSelector, resolveSelectorAll } from '../resolve-selector.js'
import type { A11yNode } from '../screen.js'

/**
 * Construct an A11yNode with sensible defaults. Each test specifies only the
 * fields it cares about; the rest fall back to canonical zero values.
 *
 * `role` is omitted by default to model real wire output (RunnerClient.getTree
 * drops `role` when rawType ∉ KNOWN_ROLES — see role.ts elementTypeNameToRole
 * + v0.3 C3 decision log).
 */
function buildNode(partial: Partial<A11yNode> = {}): A11yNode {
  return {
    rawType: 'other',
    bounds: { x: 0, y: 0, w: 10, h: 10 },
    enabled: true,
    selected: false,
    hasFocus: false,
    visible: true,
    children: [],
    ...partial,
  }
}

describe('resolveSelector — text selector base', () => {
  it('returns null on a single-node tree when text does not match label', () => {
    const tree = buildNode({ label: 'Bar' })
    expect(resolveSelector(tree, { text: 'Foo' })).toBeNull()
  })

  it('returns root when text selector matches root label', () => {
    const tree = buildNode({ label: 'OK' })
    const result = resolveSelector(tree, { text: 'OK' })
    expect(result).toBe(tree)
  })

  it('returns child when text selector matches via DFS', () => {
    const child = buildNode({ label: 'OK' })
    const tree = buildNode({ label: 'X', children: [child] })
    expect(resolveSelector(tree, { text: 'OK' })).toBe(child)
  })

  it('text=string matches node.value field', () => {
    const tree = buildNode({ label: 'X', value: '42' })
    expect(resolveSelector(tree, { text: '42' })).toBe(tree)
  })

  it('text=string matches node.text field', () => {
    const tree = buildNode({ label: '', value: '', text: 'Foo' })
    expect(resolveSelector(tree, { text: 'Foo' })).toBe(tree)
  })

  it('returns null when label/value/text all undefined', () => {
    const tree = buildNode({ rawType: 'other' })
    expect(resolveSelector(tree, { text: 'Anything' })).toBeNull()
  })

  it('empty string text="" never matches (false-positive guard)', () => {
    const tree = buildNode({ label: '', value: '', text: '' })
    expect(resolveSelector(tree, { text: '' })).toBeNull()
  })

  it('text=RegExp matches partial (no auto-anchoring)', () => {
    const tree = buildNode({ label: 'Hello world' })
    expect(resolveSelector(tree, { text: /^Hel/ })).toBe(tree)
  })

  it('text=RegExp respects case-sensitivity by default', () => {
    const tree = buildNode({ label: 'Hello' })
    expect(resolveSelector(tree, { text: /^hello/ })).toBeNull()
  })

  it('text=RegExp with /i flag matches case-insensitively', () => {
    const tree = buildNode({ label: 'Hello' })
    expect(resolveSelector(tree, { text: /^hello/i })).toBe(tree)
  })

  it('text=string is case-sensitive strict equal (no contains)', () => {
    const tree = buildNode({ label: 'OK Cancel' })
    expect(resolveSelector(tree, { text: 'OK' })).toBeNull()
  })
})

describe('resolveSelector — id selector base', () => {
  it('id matches node.identifier strict equal', () => {
    const tree = buildNode({ identifier: 'okBtn', label: 'OK' })
    expect(resolveSelector(tree, { id: 'okBtn' })).toBe(tree)
  })

  it('id is case-sensitive', () => {
    const tree = buildNode({ identifier: 'OKBTN' })
    expect(resolveSelector(tree, { id: 'okBtn' })).toBeNull()
  })

  it('id="" never matches even when identifier is also empty', () => {
    const tree = buildNode({ identifier: '' })
    expect(resolveSelector(tree, { id: '' })).toBeNull()
  })

  it('id does not match identifier missing on node', () => {
    const tree = buildNode({})
    expect(resolveSelector(tree, { id: 'okBtn' })).toBeNull()
  })
})

describe('resolveSelector — label selector base', () => {
  it('label strict equal matches', () => {
    const tree = buildNode({ label: 'OK' })
    expect(resolveSelector(tree, { label: 'OK' })).toBe(tree)
  })

  it('label does not contains-match', () => {
    const tree = buildNode({ label: 'OK Cancel' })
    expect(resolveSelector(tree, { label: 'OK' })).toBeNull()
  })

  it('label="" never matches even when node.label is also empty', () => {
    const tree = buildNode({ label: '' })
    expect(resolveSelector(tree, { label: '' })).toBeNull()
  })
})

describe('resolveSelector — role+name selector base', () => {
  it('role-only selector matches when role equals', () => {
    const tree = buildNode({ role: 'button', label: 'X' })
    expect(resolveSelector(tree, { role: 'button' })).toBe(tree)
  })

  it('role+name (string) matches when role and label match', () => {
    const tree = buildNode({ role: 'button', label: 'OK' })
    expect(resolveSelector(tree, { role: 'button', name: 'OK' })).toBe(tree)
  })

  it('role mismatch returns null even when name matches', () => {
    const tree = buildNode({ role: 'cell', label: 'OK' })
    expect(resolveSelector(tree, { role: 'button', name: 'OK' })).toBeNull()
  })

  it('role+name (RegExp) matches via partial test', () => {
    const tree = buildNode({ role: 'button', label: 'Press OK' })
    expect(resolveSelector(tree, { role: 'button', name: /OK/ })).toBe(tree)
  })

  it('role+name matches via node.value field', () => {
    const tree = buildNode({ role: 'staticText', value: '42' })
    expect(resolveSelector(tree, { role: 'staticText', name: '42' })).toBe(tree)
  })

  it('node with role=undefined does not match a role selector', () => {
    const tree = buildNode({ rawType: 'group', label: 'OK' })
    expect(resolveSelector(tree, { role: 'button', name: 'OK' })).toBeNull()
  })

  it('role+name="" (empty string) never matches', () => {
    const tree = buildNode({ role: 'button', label: '' })
    expect(resolveSelector(tree, { role: 'button', name: '' })).toBeNull()
  })
})

describe('resolveSelector — DFS pre-order + first-match wins', () => {
  it('with multiple matches, returns the first DFS pre-order match', () => {
    const first = buildNode({ label: 'OK' })
    const second = buildNode({ label: 'OK' })
    const tree = buildNode({ label: 'Root', children: [first, second] })
    expect(resolveSelector(tree, { text: 'OK' })).toBe(first)
  })

  it('DFS pre-order descends fully before moving to next sibling', () => {
    // tree: root
    //   A (label='X')
    //     A1 (label='OK')
    //   B (label='OK')
    // Expect A1 (DFS visits A subtree before B)
    const a1 = buildNode({ label: 'OK' })
    const a = buildNode({ label: 'X', children: [a1] })
    const b = buildNode({ label: 'OK' })
    const tree = buildNode({ label: 'Root', children: [a, b] })
    expect(resolveSelector(tree, { text: 'OK' })).toBe(a1)
  })

  it('nested cell > button: returns the button (iOS 26 layout)', () => {
    // Models iOS 26 Settings cell layout: cell.label='' wraps button.label='General'
    const button = buildNode({ rawType: 'button', role: 'button', label: 'General' })
    const cell = buildNode({ rawType: 'cell', role: 'cell', label: '', children: [button] })
    const tree = buildNode({ label: 'Root', children: [cell] })
    expect(resolveSelector(tree, { text: 'General' })).toBe(button)
  })

  it('self matches before descending into children', () => {
    // root self matches; child also matches; should return root
    const child = buildNode({ label: 'OK' })
    const tree = buildNode({ label: 'OK', children: [child] })
    expect(resolveSelector(tree, { text: 'OK' })).toBe(tree)
  })
})

describe('resolveSelector — edge cases', () => {
  it('single-node tree with no children: self-match works', () => {
    const tree = buildNode({ label: 'Solo', children: [] })
    expect(resolveSelector(tree, { text: 'Solo' })).toBe(tree)
  })

  it('five-level deep tree: leaf match found via DFS', () => {
    const leaf = buildNode({ label: 'leaf' })
    const l4 = buildNode({ label: 'l4', children: [leaf] })
    const l3 = buildNode({ label: 'l3', children: [l4] })
    const l2 = buildNode({ label: 'l2', children: [l3] })
    const l1 = buildNode({ label: 'l1', children: [l2] })
    expect(resolveSelector(l1, { text: 'leaf' })).toBe(leaf)
  })
})

// ============================================================================
// C5 modifier matrix
// ============================================================================

describe('resolveSelector — near modifier', () => {
  // anchor centroid (25, 110); candidate centroid (145, 190); dist ≈ 144
  // Use simple coordinates so distance arithmetic is obvious.

  it('near hit: centroid distance ≈ 80pt (< 100) returns candidate', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 100, y: 100, w: 50, h: 20 } })
    // anchor centroid = (125, 110); candidate centroid = (145, 190); dist ≈ 82.46
    const candidate = buildNode({ label: 'Target', bounds: { x: 120, y: 180, w: 50, h: 20 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', near: { text: 'Anchor' } })).toBe(candidate)
  })

  it('near miss: centroid distance > 100 returns null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 50, h: 20 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 300, y: 300, w: 50, h: 20 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', near: { text: 'Anchor' } })).toBeNull()
  })

  it('near boundary: distance == 100pt exactly is a hit (≤ 100)', () => {
    // anchor centroid (0,0); candidate centroid (100,0); distance = 100
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 0, h: 0 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 100, y: 0, w: 0, h: 0 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', near: { text: 'Anchor' } })).toBe(candidate)
  })

  it('near boundary: distance just over 100 returns null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 0, h: 0 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 100.01, y: 0, w: 0, h: 0 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', near: { text: 'Anchor' } })).toBeNull()
  })

  it('near anchor null: anchor selector finds nothing returns null silently (no throw)', () => {
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [candidate] })
    expect(resolveSelector(tree, { text: 'Target', near: { text: 'NotInTree' } })).toBeNull()
  })
})

describe('resolveSelector — below modifier', () => {
  it('below hit: candidate.cy > anchor.cy', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } }) // cy=5
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 100, w: 10, h: 10 } }) // cy=105
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', below: { text: 'Anchor' } })).toBe(candidate)
  })

  it('below miss: candidate.cy < anchor.cy returns null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 100, w: 10, h: 10 } }) // cy=105
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 0, w: 10, h: 10 } }) // cy=5
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', below: { text: 'Anchor' } })).toBeNull()
  })

  it('below strict: candidate.cy == anchor.cy returns null (strict >)', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } }) // cy=5
    const candidate = buildNode({ label: 'Target', bounds: { x: 50, y: 0, w: 10, h: 10 } }) // cy=5
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', below: { text: 'Anchor' } })).toBeNull()
  })

  it('below side-axis ignored: candidate.cx differs widely yet still hits', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 200, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 100, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', below: { text: 'Anchor' } })).toBe(candidate)
  })
})

describe('resolveSelector — above modifier', () => {
  it('above hit: candidate.cy < anchor.cy', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 100, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', above: { text: 'Anchor' } })).toBe(candidate)
  })

  it('above miss: candidate.cy > anchor.cy returns null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 100, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', above: { text: 'Anchor' } })).toBeNull()
  })

  it('above strict: equal cy returns null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 50, y: 0, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', above: { text: 'Anchor' } })).toBeNull()
  })
})

describe('resolveSelector — leftOf modifier', () => {
  it('leftOf hit: candidate.cx < anchor.cx', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 200, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', leftOf: { text: 'Anchor' } })).toBe(candidate)
  })

  it('leftOf miss: candidate.cx > anchor.cx returns null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 200, y: 0, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', leftOf: { text: 'Anchor' } })).toBeNull()
  })

  it('leftOf strict: equal cx returns null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 100, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', leftOf: { text: 'Anchor' } })).toBeNull()
  })
})

describe('resolveSelector — rightOf modifier', () => {
  it('rightOf hit: candidate.cx > anchor.cx', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 200, y: 0, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', rightOf: { text: 'Anchor' } })).toBe(candidate)
  })

  it('rightOf miss: candidate.cx < anchor.cx returns null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 200, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', rightOf: { text: 'Anchor' } })).toBeNull()
  })

  it('rightOf strict: equal cx returns null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'Target', bounds: { x: 0, y: 100, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, candidate] })
    expect(resolveSelector(tree, { text: 'Target', rightOf: { text: 'Anchor' } })).toBeNull()
  })
})

describe('resolveSelector — directional modifier self-exclusion', () => {
  // A single node that matches both the candidate selector and the anchor
  // selector must NEVER spatial-self-match (c === a short-circuits to false).
  it('below: candidate === anchor by reference is excluded', () => {
    const tree = buildNode({ label: 'Solo', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    expect(resolveSelector(tree, { text: 'Solo', below: { text: 'Solo' } })).toBeNull()
  })

  it('above: candidate === anchor by reference is excluded', () => {
    const tree = buildNode({ label: 'Solo', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    expect(resolveSelector(tree, { text: 'Solo', above: { text: 'Solo' } })).toBeNull()
  })

  it('leftOf: candidate === anchor by reference is excluded', () => {
    const tree = buildNode({ label: 'Solo', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    expect(resolveSelector(tree, { text: 'Solo', leftOf: { text: 'Solo' } })).toBeNull()
  })

  it('rightOf: candidate === anchor by reference is excluded', () => {
    const tree = buildNode({ label: 'Solo', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    expect(resolveSelector(tree, { text: 'Solo', rightOf: { text: 'Solo' } })).toBeNull()
  })
})

describe('resolveSelector — inside modifier', () => {
  it('inside hit: candidate.bounds fully within anchor.bounds', () => {
    const outer = buildNode({ label: 'outer', bounds: { x: 0, y: 0, w: 200, h: 200 } })
    const inner = buildNode({ label: 'inner', bounds: { x: 20, y: 20, w: 50, h: 50 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [outer, inner] })
    expect(resolveSelector(tree, { text: 'inner', inside: { text: 'outer' } })).toBe(inner)
  })

  it('inside miss: candidate overflows anchor on right edge', () => {
    const outer = buildNode({ label: 'outer', bounds: { x: 0, y: 0, w: 100, h: 100 } })
    const overflow = buildNode({ label: 'overflow', bounds: { x: 50, y: 50, w: 100, h: 100 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [outer, overflow] })
    expect(resolveSelector(tree, { text: 'overflow', inside: { text: 'outer' } })).toBeNull()
  })

  it('inside shared-edge: candidate exactly co-edges with anchor still hits (closed containment)', () => {
    const outer = buildNode({ label: 'outer', bounds: { x: 0, y: 0, w: 100, h: 100 } })
    const edged = buildNode({ label: 'edged', bounds: { x: 0, y: 0, w: 100, h: 100 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [outer, edged] })
    expect(resolveSelector(tree, { text: 'edged', inside: { text: 'outer' } })).toBe(edged)
  })

  it('inside self exclusion: candidate === anchor returns null', () => {
    const tree = buildNode({ label: 'Solo', bounds: { x: 0, y: 0, w: 100, h: 100 } })
    expect(resolveSelector(tree, { text: 'Solo', inside: { text: 'Solo' } })).toBeNull()
  })

  it('inside anchor null: anchor not found returns null', () => {
    const inner = buildNode({ label: 'inner', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [inner] })
    expect(resolveSelector(tree, { text: 'inner', inside: { text: 'NotInTree' } })).toBeNull()
  })
})

describe('resolveSelector — nth / first / last index modifiers', () => {
  function threeXTree(): {
    a: A11yNode
    b: A11yNode
    c: A11yNode
    tree: A11yNode
  } {
    const a = buildNode({ label: 'X', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const b = buildNode({ label: 'X', bounds: { x: 0, y: 20, w: 10, h: 10 } })
    const c = buildNode({ label: 'X', bounds: { x: 0, y: 40, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [a, b, c] })
    return { a, b, c, tree }
  }

  it('nth=0 returns first candidate (DFS pre-order)', () => {
    const { a, tree } = threeXTree()
    expect(resolveSelector(tree, { text: 'X', nth: 0 })).toBe(a)
  })

  it('nth=1 returns second candidate', () => {
    const { b, tree } = threeXTree()
    expect(resolveSelector(tree, { text: 'X', nth: 1 })).toBe(b)
  })

  it('nth=2 returns third candidate', () => {
    const { c, tree } = threeXTree()
    expect(resolveSelector(tree, { text: 'X', nth: 2 })).toBe(c)
  })

  it('nth out of range returns null (no throw)', () => {
    const { tree } = threeXTree()
    expect(resolveSelector(tree, { text: 'X', nth: 10 })).toBeNull()
  })

  it('first=true equivalent to nth=0', () => {
    const { a, tree } = threeXTree()
    expect(resolveSelector(tree, { text: 'X', first: true })).toBe(a)
  })

  it('last=true equivalent to nth=len-1', () => {
    const { c, tree } = threeXTree()
    expect(resolveSelector(tree, { text: 'X', last: true })).toBe(c)
  })

  it('first + last + nth all defined: last-defined-wins (nth overrides)', () => {
    const { c, tree } = threeXTree()
    // iteration order: first → last → nth; nth wins
    expect(resolveSelector(tree, { text: 'X', first: true, last: true, nth: 2 })).toBe(c)
  })

  it('first + last (no nth): last-defined-wins → last', () => {
    const { c, tree } = threeXTree()
    expect(resolveSelector(tree, { text: 'X', first: true, last: true })).toBe(c)
  })
})

describe('resolveSelector — DFS index order on nested tree', () => {
  // tree:
  //   root.label='Y'
  //     A.label='OK'
  //     B.label='Y'
  //       C.label='OK'
  // Base selector {text:'OK'} candidates DFS pre-order = [A, C]
  function nestedOKTree(): { a: A11yNode; c: A11yNode; tree: A11yNode } {
    const a = buildNode({ label: 'OK', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const c = buildNode({ label: 'OK', bounds: { x: 0, y: 100, w: 10, h: 10 } })
    const b = buildNode({ label: 'Y', bounds: { x: 0, y: 50, w: 10, h: 10 }, children: [c] })
    const tree = buildNode({ label: 'Y', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [a, b] })
    return { a, c, tree }
  }

  it('first:true on nested tree returns DFS pre-order [0] (A, not C)', () => {
    const { a, tree } = nestedOKTree()
    expect(resolveSelector(tree, { text: 'OK', first: true })).toBe(a)
  })

  it('last:true on nested tree returns deepest DFS pre-order tail (C)', () => {
    const { c, tree } = nestedOKTree()
    expect(resolveSelector(tree, { text: 'OK', last: true })).toBe(c)
  })

  it('base-only on nested tree: candidates[0] still equals C4 first-match (A)', () => {
    const { a, tree } = nestedOKTree()
    expect(resolveSelector(tree, { text: 'OK' })).toBe(a)
  })
})

describe('resolveSelector — multi-modifier AND combination', () => {
  it('below + nth: surviving list indexed after spatial filter', () => {
    // anchor at top; 1 cell above (filtered out), 2 cells below
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 50, w: 10, h: 10 } }) // cy=55
    const aboveCell = buildNode({ label: 'Cell', bounds: { x: 0, y: 0, w: 10, h: 10 } }) // cy=5 → NOT below
    const below1 = buildNode({ label: 'Cell', bounds: { x: 0, y: 100, w: 10, h: 10 } }) // cy=105 → below
    const below2 = buildNode({ label: 'Cell', bounds: { x: 0, y: 200, w: 10, h: 10 } }) // cy=205 → below
    const tree = buildNode({
      label: 'Root',
      bounds: { x: 0, y: 0, w: 500, h: 500 },
      children: [anchor, aboveCell, below1, below2],
    })
    expect(resolveSelector(tree, { text: 'Cell', below: { text: 'Anchor' }, nth: 0 })).toBe(below1)
    expect(resolveSelector(tree, { text: 'Cell', below: { text: 'Anchor' }, nth: 1 })).toBe(below2)
    expect(resolveSelector(tree, { text: 'Cell', below: { text: 'Anchor' }, nth: 2 })).toBeNull()
  })

  it('inside + first: filter then index', () => {
    const cell = buildNode({ rawType: 'cell', role: 'cell', label: 'Cell1', bounds: { x: 0, y: 0, w: 200, h: 200 } })
    const itemInside = buildNode({ label: 'Item', bounds: { x: 10, y: 10, w: 20, h: 20 } })
    const itemOutside = buildNode({ label: 'Item', bounds: { x: 300, y: 300, w: 20, h: 20 } })
    const tree = buildNode({
      label: 'Root',
      bounds: { x: 0, y: 0, w: 500, h: 500 },
      children: [cell, itemInside, itemOutside],
    })
    expect(resolveSelector(tree, { text: 'Item', inside: { role: 'cell' }, first: true })).toBe(itemInside)
  })

  it('near AND below: both filters apply (intersection)', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } }) // cy=5
    // close + below
    const close = buildNode({ label: 'X', bounds: { x: 0, y: 50, w: 10, h: 10 } }) // cy=55, dist ≈ 50 → near & below
    // far below
    const farBelow = buildNode({ label: 'X', bounds: { x: 0, y: 400, w: 10, h: 10 } }) // dist > 100 → not near
    // close above
    const closeAbove = buildNode({ label: 'X', bounds: { x: 0, y: -50, w: 10, h: 10 } }) // not below
    const tree = buildNode({
      label: 'Root',
      bounds: { x: 0, y: 0, w: 500, h: 500 },
      children: [anchor, close, farBelow, closeAbove],
    })
    expect(resolveSelector(tree, { text: 'X', near: { text: 'Anchor' }, below: { text: 'Anchor' } })).toBe(close)
  })

  it('multi-modifier any anchor null → overall null', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 0, y: 0, w: 10, h: 10 } })
    const candidate = buildNode({ label: 'X', bounds: { x: 0, y: 50, w: 10, h: 10 } })
    const tree = buildNode({
      label: 'Root',
      bounds: { x: 0, y: 0, w: 500, h: 500 },
      children: [anchor, candidate],
    })
    // below anchor exists but near anchor doesn't → overall null
    expect(
      resolveSelector(tree, { text: 'X', below: { text: 'Anchor' }, near: { text: 'NotInTree' } }),
    ).toBeNull()
  })

  it('base-only with 4 hits: candidates[0] equals DFS pre-order first (C4-compat)', () => {
    const n1 = buildNode({ label: 'X' })
    const n2 = buildNode({ label: 'X' })
    const n3 = buildNode({ label: 'X' })
    const n4 = buildNode({ label: 'X' })
    const tree = buildNode({ label: 'Root', children: [n1, n2, n3, n4] })
    expect(resolveSelector(tree, { text: 'X' })).toBe(n1)
    expect(resolveSelector(tree, { text: 'X', nth: 0 })).toBe(n1)
  })
})

describe('resolveSelector — recursive modifier (anchor selector has its own modifier)', () => {
  it('anchor selector itself carries modifier: recursion resolves correctly', () => {
    // tree:
    //   cell (bounds 0..200)
    //     Y (label='Y' bounds at cy=10)   ← Y inside cell
    //   X (label='X' bounds at cy=100)    ← X below the Y-inside-cell anchor
    const yInside = buildNode({ label: 'Y', bounds: { x: 10, y: 0, w: 10, h: 20 } })
    const cell = buildNode({ role: 'cell', label: 'C', bounds: { x: 0, y: 0, w: 200, h: 50 }, children: [yInside] })
    const xBelow = buildNode({ label: 'X', bounds: { x: 0, y: 100, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [cell, xBelow] })
    expect(
      resolveSelector(tree, { text: 'X', below: { text: 'Y', inside: { role: 'cell' } } }),
    ).toBe(xBelow)
  })
})

describe('resolveSelector — bounds zero-size edges', () => {
  it('zero-size bounds (w=0,h=0): spatial filter still uses centroid', () => {
    const anchor = buildNode({ label: 'Anchor', bounds: { x: 50, y: 50, w: 0, h: 0 } }) // cy=50
    const below = buildNode({ label: 'Below', bounds: { x: 50, y: 100, w: 0, h: 0 } }) // cy=100
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [anchor, below] })
    expect(resolveSelector(tree, { text: 'Below', below: { text: 'Anchor' } })).toBe(below)
  })
})

describe('resolveSelectorAll — v0.3 C6 sibling export', () => {
  it('multi-hit base + 0 modifier: returns all DFS pre-order matches', () => {
    const n1 = buildNode({ label: 'X', bounds: { x: 0, y: 10, w: 10, h: 10 } })
    const n2 = buildNode({ label: 'X', bounds: { x: 0, y: 30, w: 10, h: 10 } })
    const n3 = buildNode({ label: 'X', bounds: { x: 0, y: 50, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [n1, n2, n3] })
    const result = resolveSelectorAll(tree, { text: 'X' })
    expect(result).toHaveLength(3)
    expect(result[0]).toBe(n1)
    expect(result[1]).toBe(n2)
    expect(result[2]).toBe(n3)
  })

  it('multi-hit base + spatial filter (below anchor): only returns nodes below', () => {
    const anchor = buildNode({ label: 'Header', bounds: { x: 0, y: 20, w: 100, h: 10 } }) // cy=25
    const above1 = buildNode({ label: 'X', bounds: { x: 0, y: 0, w: 10, h: 10 } })       // cy=5  (NOT below)
    const above2 = buildNode({ label: 'X', bounds: { x: 0, y: 10, w: 10, h: 10 } })      // cy=15 (NOT below)
    const below1 = buildNode({ label: 'X', bounds: { x: 0, y: 40, w: 10, h: 10 } })      // cy=45 (below)
    const below2 = buildNode({ label: 'X', bounds: { x: 0, y: 80, w: 10, h: 10 } })      // cy=85 (below)
    const tree = buildNode({
      label: 'Root',
      bounds: { x: 0, y: 0, w: 500, h: 500 },
      children: [anchor, above1, above2, below1, below2],
    })
    const result = resolveSelectorAll(tree, { text: 'X', below: { text: 'Header' } })
    expect(result).toHaveLength(2)
    expect(result[0]).toBe(below1)
    expect(result[1]).toBe(below2)
  })

  it('0 hit: returns empty array (not null)', () => {
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [
      buildNode({ label: 'A' }),
      buildNode({ label: 'B' }),
    ] })
    const result = resolveSelectorAll(tree, { text: 'Nope' })
    expect(result).toEqual([])
    expect(Array.isArray(result)).toBe(true)
  })

  it('anchor selector null: returns [] (does not throw; mirrors resolveSelector)', () => {
    const c1 = buildNode({ label: 'X', bounds: { x: 0, y: 50, w: 10, h: 10 } })
    const c2 = buildNode({ label: 'X', bounds: { x: 0, y: 60, w: 10, h: 10 } })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [c1, c2] })
    // anchor "NotInTree" doesn't exist → short-circuit to [] (same semantics as resolveSelector → null)
    const result = resolveSelectorAll(tree, { text: 'X', below: { text: 'NotInTree' } })
    expect(result).toEqual([])
  })

  it('index modifier given but silently ignored: still returns all base hits', () => {
    const n1 = buildNode({ label: 'X' })
    const n2 = buildNode({ label: 'X' })
    const n3 = buildNode({ label: 'X' })
    const tree = buildNode({ label: 'Root', bounds: { x: 0, y: 0, w: 500, h: 500 }, children: [n1, n2, n3] })
    // first / nth / last all ignored by resolveSelectorAll (Playwright .all() semantics)
    expect(resolveSelectorAll(tree, { text: 'X', first: true })).toHaveLength(3)
    expect(resolveSelectorAll(tree, { text: 'X', nth: 1 })).toHaveLength(3)
    expect(resolveSelectorAll(tree, { text: 'X', last: true })).toHaveLength(3)
  })
})
