// v7.4 c1 — Modifier sealed interface (placeholder for shape tests).
//
// Mirror Swift SmixSDK.Modifier (v7.1 c1) — 9 case enum. v7.4 c4 lands
// the proper Modifiers struct + Selector fluent chaining (matching
// Swift v7.2 c3 schema).

package dev.smix.sdk

/**
 * Modifier — placeholder for v7.4 c1 shape tests. v7.4 c4 replaces with
 * full Modifiers struct + fluent chaining methods on Selector.
 *
 * Wire JSON (when wired in c4): flattened into Selector body so
 * `{"id":"btn","nth":0,"below":{"text":"hi"}}` round-trips.
 */
sealed interface Modifier {
    object First : Modifier
    object Last : Modifier
    data class Nth(val index: Int) : Modifier
    data class Above(val anchor: Selector) : Modifier
    data class Below(val anchor: Selector) : Modifier
    data class LeftOf(val anchor: Selector) : Modifier
    data class RightOf(val anchor: Selector) : Modifier
    data class Near(val anchor: Selector, val thresholdPts: Double = 100.0) : Modifier
    data class Inside(val anchor: Selector) : Modifier
}
