import type { A11yNode } from '../core/screen.js'

/**
 * Abstract source of an a11y tree. v0.3 C3 has exactly one impl
 * (RunnerClient via XCUIElementSnapshot route); v0.7+ will add
 * HostAxpTreeSource (AccessibilityPlatformTranslation host-side direct read).
 *
 * Selector resolver (C4+) and SDK (C6+) depend on this interface, not on
 * RunnerClient directly — so swapping tree sources later is a 1-line change.
 *
 * Single-method by design: multi-device / cell-aware variants land in
 * v0.4-v0.6 when Cell allocator is in scope and the Cell holds its own source.
 */
export interface A11yTreeSource {
  getTree(): Promise<A11yNode>
}
