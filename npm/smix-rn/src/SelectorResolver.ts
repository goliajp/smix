// The seam where selector resolution is supplied: the real one wraps the
// Rust core over the HTTP runner, the mock answers from a table. A bare
// function type keeps that seam as small as it can be.

export type SelectorResolver = (
  treeJson: string,
  selectorJson: string,
) => Promise<readonly string[]>

/**
 * Wraps `resolve_selector_labels`: each match's label, empty string where
 * it has none.
 */
export type LabelsResolver = (
  treeJson: string,
  selectorJson: string,
) => Promise<readonly string[]>

/**
 * Mock resolver for vitest unit tests. Pre-registered selectorJson →
 * id list mappings; falls back to empty list for unknown selectors.
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
 * Mock labels resolver for unit tests. Pre-registered selectorJson →
 * label list mappings.
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
