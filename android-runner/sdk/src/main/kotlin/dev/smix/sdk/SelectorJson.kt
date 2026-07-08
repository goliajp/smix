// v7.4 c2 — Selector wire JSON encoding helpers.
// v7.4 c4 — switched to SelectorSerializer (handles 7 case + Modifiers
// flatten + Pattern). Single entry point replaces per-case branching.

package dev.smix.sdk

import kotlinx.serialization.json.Json

/**
 * Encode a [Selector] to its Rust-compatible untagged JSON wire shape
 * with [Modifiers] / [IndexModifiers] flattened. Powered by
 * [SelectorSerializer] (c4).
 */
internal fun encodeSelectorJson(selector: Selector): String =
    Json.encodeToString(SelectorSerializer, selector)

/**
 * Encode an [A11yNode] tree to JSON for the FFI boundary.
 */
internal fun encodeTreeJson(tree: A11yNode): String =
    Json.encodeToString(A11yNode.serializer(), tree)
