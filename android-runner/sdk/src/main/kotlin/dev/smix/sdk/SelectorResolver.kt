// SelectorResolver injection boundary, plus LabelResolver for
// Locator.toHaveCount / toHaveLabel.
//
// App takes a SelectorResolver in its constructor; default impl wraps
// the UniFFI Kotlin binding `uniffi.smix.resolveSelector`. JVM unit
// tests pass an in-memory mock so they don't trigger JNA initialization
// (which can't load `libuniffi_smix.so` on host macOS — that .so is
// Android-only).

package dev.smix.sdk

/**
 * Functional interface wrapping the Rust selector resolver (single-id
 * or all-id list).
 */
fun interface SelectorResolver {
    fun resolve(treeJson: String, selectorJson: String): List<String>
}

/**
 * Functional interface wrapping `resolve_selector_labels` — returns
 * each matched node's `.label` (empty string when label is None).
 * Used by Locator.toHaveCount / toHaveLabel.
 */
fun interface LabelResolver {
    fun resolve(treeJson: String, selectorJson: String): List<String>
}

/**
 * Default resolver — UniFFI-backed. Lazy lambda capture defers
 * `uniffi.smix` class load to .resolve() invocation (JNA-safe).
 */
internal val DefaultFfiResolver: SelectorResolver =
    SelectorResolver { treeJson, selectorJson ->
        uniffi.smix.resolveSelector(treeJson, selectorJson)
    }

/**
 * Default labels resolver — UniFFI-backed via
 * `uniffi.smix.resolveSelectorLabels`.
 */
internal val DefaultFfiLabelsResolver: LabelResolver =
    LabelResolver { treeJson, selectorJson ->
        uniffi.smix.resolveSelectorLabels(treeJson, selectorJson)
    }
