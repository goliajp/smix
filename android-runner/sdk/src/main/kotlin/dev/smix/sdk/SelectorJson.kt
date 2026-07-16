// Selector wire JSON encoding helpers. SelectorSerializer (7 cases +
// Modifiers flatten + Pattern) is the single entry point.

package dev.smix.sdk

import kotlinx.serialization.json.Json

/**
 * Encode a [Selector] to its Rust-compatible untagged JSON wire shape
 * with [Modifiers] / [IndexModifiers] flattened. Powered by
 * [SelectorSerializer].
 */
internal fun encodeSelectorJson(selector: Selector): String =
    Json.encodeToString(SelectorSerializer, selector)

/**
 * Encode an [A11yNode] tree to JSON for the FFI boundary.
 */
internal fun encodeTreeJson(tree: A11yNode): String =
    Json.encodeToString(A11yNode.serializer(), tree)
