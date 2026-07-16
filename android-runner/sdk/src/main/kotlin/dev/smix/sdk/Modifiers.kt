// Modifiers data class + AnchorBox + IndexModifiers mirror the Swift
// SDK + Rust smix-selector struct shape.
//
// Wire JSON shape: Modifiers fields are FLATTENED into the Selector
// body via custom KSerializer (matches Rust `#[serde(flatten)]`).
// AnchorBox is the spatial-only subset for Selector.Anchor.
// IndexModifiers is the index-only subset.

package dev.smix.sdk

import kotlinx.serialization.Serializable

/**
 * All-optional modifier set stacked onto a base Selector.
 * Spatial fields hold sub-selectors; index fields are scalar picks.
 *
 * Serialization: fields are FLATTENED into the Selector JSON body
 * via SelectorSerializer; the standalone @Serializable is used only
 * for nested decoding helpers.
 */
@Serializable
data class Modifiers(
    val near: Selector? = null,
    val below: Selector? = null,
    val above: Selector? = null,
    val leftOf: Selector? = null,
    val rightOf: Selector? = null,
    val inside: Selector? = null,
    val ancestor: Selector? = null,
    val nth: Int? = null,
    val first: Boolean? = null,
    val last: Boolean? = null,
) {
    /** True when no modifier field is set. */
    val isEmpty: Boolean
        get() = near == null && below == null && above == null &&
            leftOf == null && rightOf == null && inside == null &&
            ancestor == null && nth == null && first == null && last == null

    companion object {
        val EMPTY = Modifiers()
    }
}

/**
 * Spatial-only anchor box (no index, no base). Used by
 * [Selector.Anchor] — at least one field must be non-null for the
 * resolver to produce candidates.
 */
@Serializable
data class AnchorBox(
    val near: Selector? = null,
    val below: Selector? = null,
    val above: Selector? = null,
    val leftOf: Selector? = null,
    val rightOf: Selector? = null,
    val inside: Selector? = null,
    val ancestor: Selector? = null,
)

/** Index-only modifier subset used by [Selector.Anchor]. */
@Serializable
data class IndexModifiers(
    val nth: Int? = null,
    val first: Boolean? = null,
    val last: Boolean? = null,
) {
    companion object {
        val EMPTY = IndexModifiers()
    }
}
