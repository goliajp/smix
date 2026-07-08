// v7.5 c1 — SelectorResolver injection boundary.
//
// Mirror Kotlin v7.4 c2 R-Kotlin-A: real impl wraps Rust FFI core
// (in TS via HTTP runner or future ffi-napi binding); mock impl for
// vitest. Lambda-style fn interface keeps the abstraction minimal.

export type SelectorResolver = (
  treeJson: string,
  selectorJson: string,
) => Promise<readonly string[]>

/**
 * v7.6 c1 — labels resolver wrapping Rust `resolve_selector_labels`
 * (each match's `.label`; empty string when None). Used by
 * Locator.toHaveCount + Locator.toHaveLabel.
 */
export type LabelsResolver = (
  treeJson: string,
  selectorJson: string,
) => Promise<readonly string[]>

/**
 * Mock resolver for vitest unit tests. Pre-registered selectorJson →
 * id list mappings; falls back to empty list for unknown selectors.
 * Mirrors Kotlin MockSelectorResolver from v7.4 c2.
 */
export class MockSelectorResolver {
  readonly returnMap = new Map<string, readonly string[]>()
  throwOnNext: Error | null = null
  readonly calls: { treeJson: string; selectorJson: string }[] = []

  resolve: SelectorResolver = async (treeJson, selectorJson) => {
    this.calls.push({ treeJson, selectorJson })
    const t = this.throwOnNext
    if (t !== null) {
      this.throwOnNext = null
      throw t
    }
    return this.returnMap.get(selectorJson) ?? []
  }

  /** Register a single-hit response for the given selector JSON key. */
  registerHit(selectorJson: string, id: string): void {
    this.returnMap.set(selectorJson, [id])
  }

  /** Register a multi-hit response. */
  registerHits(selectorJson: string, ids: readonly string[]): void {
    this.returnMap.set(selectorJson, ids)
  }
}

/**
 * v7.6 c1 — Mock labels resolver for vitest tests. Pre-registered
 * selectorJson → label list mappings. Mirrors Kotlin MockLabelsResolver.
 */
export class MockLabelsResolver {
  readonly returnMap = new Map<string, readonly string[]>()
  throwOnNext: Error | null = null
  readonly calls: { treeJson: string; selectorJson: string }[] = []

  resolve: LabelsResolver = async (treeJson, selectorJson) => {
    this.calls.push({ treeJson, selectorJson })
    const t = this.throwOnNext
    if (t !== null) {
      this.throwOnNext = null
      throw t
    }
    return this.returnMap.get(selectorJson) ?? []
  }

  /** Register the matched-labels list for a selector. */
  registerLabels(selectorJson: string, labels: readonly string[]): void {
    this.returnMap.set(selectorJson, labels)
  }
}
