import type { Selector, Modifiers } from './selector.js'
import type { A11yNode, Rect } from './screen.js'
import {
  isTextSelector,
  isIdSelector,
  isLabelSelector,
  isRoleSelector,
} from './selector.js'

/**
 * Resolve a Selector against an A11yNode tree.
 *
 * Pure function — no driver / runner / sim calls. Caller (Driver in C6+)
 * is responsible for fetching the tree (via A11yTreeSource.getTree()) and
 * for translating the matched node into a tap target (coord or runner /tap).
 *
 * Pipeline (v0.3 C5):
 *   1. collect-all: DFS pre-order collect every node whose own fields match
 *      the base selector form (text / id / label / role+name).
 *   2. spatial filter: for each relation modifier (near / below / above /
 *      leftOf / rightOf / inside), recursively resolve the anchor selector;
 *      filter candidates by bounds geometry. AND semantics across modifiers.
 *      Anchor selector returning null short-circuits to null overall.
 *   3. index pick: apply nth / first / last on the surviving candidate list.
 *      When several index modifiers are given, last-defined wins
 *      (iteration order: first → last → nth).
 *
 * Without any modifier, candidates[0] equals the C4 DFS first-match.
 *
 * Matching semantics (locked in design.md §Selector 匹配语义):
 * - bounds unit = logical points @1x (A11yNode wire chapter)
 * - near threshold = 100 logical points (centroid Euclidean distance)
 * - below/above/leftOf/rightOf = centroid axis dominance, no alignment
 *   tolerance (strict <, >)
 * - inside = bounds geometric containment (NOT DOM ancestry); self excluded
 *   by reference
 * - nth/first/last index = DFS pre-order (matches base collect order)
 * - anchor null = overall null (no throw)
 */
export function resolveSelector(tree: A11yNode, selector: Selector): A11yNode | null {
  const candidates = dfsCollect(tree, (n) => matchesBase(n, selector))
  const filtered = applySpatialFilters(tree, candidates, selector)
  if (filtered === null) return null
  const picked = applyIndex(filtered, selector)
  return picked.length === 0 ? null : (picked[0] ?? null)
}

/**
 * Resolve all matching nodes (v0.3 C6).
 *
 * Same pipeline as resolveSelector but stops before index pick. Index
 * modifiers (nth / first / last) are silently ignored here — Playwright
 * `locator(...).all()` has the same semantics: "find all" is incompatible
 * with "pick one by position", so the index modifier is meaningless on
 * multi-results and we don't surface that as an error.
 *
 * Anchor null still short-circuits to []. base 0-match returns []. Used
 * by Driver.findAll() in C6.
 */
export function resolveSelectorAll(tree: A11yNode, selector: Selector): A11yNode[] {
  const candidates = dfsCollect(tree, (n) => matchesBase(n, selector))
  const filtered = applySpatialFilters(tree, candidates, selector)
  return filtered ?? []
}

function dfsCollect(node: A11yNode, pred: (n: A11yNode) => boolean): A11yNode[] {
  const out: A11yNode[] = []
  walk(node)
  return out
  function walk(n: A11yNode): void {
    if (pred(n)) out.push(n)
    for (const c of n.children) walk(c)
  }
}

function matchesBase(node: A11yNode, selector: Selector): boolean {
  if (isTextSelector(selector)) {
    return matchText(node, selector.text)
  }
  if (isIdSelector(selector)) {
    if (selector.id === '') return false
    return node.identifier === selector.id
  }
  if (isLabelSelector(selector)) {
    if (selector.label === '') return false
    return node.label === selector.label
  }
  if (isRoleSelector(selector)) {
    if (node.role !== selector.role) return false
    if (selector.name === undefined) return true
    return matchText(node, selector.name)
  }
  return false
}

function matchText(node: A11yNode, pattern: string | RegExp): boolean {
  const candidates: ReadonlyArray<string | undefined> = [node.label, node.value, node.text]
  if (pattern instanceof RegExp) {
    for (const c of candidates) {
      if (c !== undefined && pattern.test(c)) return true
    }
    return false
  }
  if (pattern === '') return false
  for (const c of candidates) {
    if (c === pattern) return true
  }
  return false
}

// ---- spatial filters (C5) -----------------------------------------------

const NEAR_THRESHOLD_PT = 100 // logical points @1x

type SpatialKey = 'near' | 'below' | 'above' | 'leftOf' | 'rightOf' | 'inside'
const SPATIAL_KEYS: ReadonlyArray<SpatialKey> = [
  'near',
  'below',
  'above',
  'leftOf',
  'rightOf',
  'inside',
]

function applySpatialFilters(
  tree: A11yNode,
  candidates: A11yNode[],
  selector: Selector,
): A11yNode[] | null {
  let surviving: A11yNode[] = candidates
  const mod = selector as Modifiers
  for (const key of SPATIAL_KEYS) {
    const anchorSel = mod[key]
    if (anchorSel === undefined) continue
    const anchor = resolveSelector(tree, anchorSel)
    if (anchor === null) return null
    surviving = surviving.filter((c) => satisfies(key, c, anchor))
  }
  return surviving
}

function satisfies(key: SpatialKey, c: A11yNode, a: A11yNode): boolean {
  if (c === a) return false
  const cc = centroid(c.bounds)
  const ac = centroid(a.bounds)
  switch (key) {
    case 'near':
      return dist(cc, ac) <= NEAR_THRESHOLD_PT
    case 'below':
      return cc.y > ac.y
    case 'above':
      return cc.y < ac.y
    case 'leftOf':
      return cc.x < ac.x
    case 'rightOf':
      return cc.x > ac.x
    case 'inside':
      return contains(a.bounds, c.bounds)
  }
}

function centroid(r: Rect): { x: number; y: number } {
  return { x: r.x + r.w / 2, y: r.y + r.h / 2 }
}

function dist(p: { x: number; y: number }, q: { x: number; y: number }): number {
  const dx = p.x - q.x
  const dy = p.y - q.y
  return Math.sqrt(dx * dx + dy * dy)
}

function contains(outer: Rect, inner: Rect): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.w <= outer.x + outer.w &&
    inner.y + inner.h <= outer.y + outer.h
  )
}

// ---- index pick (C5) ----------------------------------------------------

function applyIndex(list: A11yNode[], selector: Selector): A11yNode[] {
  const mod = selector as Modifiers
  const hasFirst = mod.first === true
  const hasLast = mod.last === true
  const hasNth = mod.nth !== undefined
  if (!hasFirst && !hasLast && !hasNth) return list
  let picked: A11yNode | undefined
  if (hasFirst) picked = list[0]
  if (hasLast) picked = list[list.length - 1]
  if (hasNth) picked = list[mod.nth as number]
  return picked === undefined ? [] : [picked]
}
