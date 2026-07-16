// Modifier sealed interface — mirrors the Swift SmixSDK.Modifier
// 9-case enum.

package dev.smix.sdk

/**
 * Wire JSON: flattened into the Selector body so
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
