// v7.4 c1 — A11yNode + Rect mirror Rust smix-screen A11yNode + Swift
// SmixSDK.A11yNode. Wire form via `@SerialName` matches Rust
// `#[serde(rename_all = "camelCase")]`.

package dev.smix.sdk

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class A11yNode(
    @SerialName("rawType") val rawType: String,
    @SerialName("role") val role: A11yRole? = null,
    @SerialName("identifier") val identifier: String? = null,
    @SerialName("label") val label: String? = null,
    @SerialName("title") val title: String? = null,
    @SerialName("placeholderValue") val placeholderValue: String? = null,
    @SerialName("value") val value: String? = null,
    @SerialName("text") val text: String? = null,
    @SerialName("bounds") val bounds: Rect,
    @SerialName("enabled") val enabled: Boolean = false,
    @SerialName("selected") val selected: Boolean = false,
    @SerialName("hasFocus") val hasFocus: Boolean = false,
    @SerialName("visible") val visible: Boolean = false,
    @SerialName("children") val children: List<A11yNode> = emptyList(),
) {
    /**
     * Recursively find the first node whose [identifier] matches [id].
     * Returns null when no match anywhere in the subtree.
     */
    fun findById(id: String): A11yNode? {
        if (identifier == id) return this
        for (child in children) {
            child.findById(id)?.let { return it }
        }
        return null
    }

    /**
     * DFS pre-order flatten — self + all descendants in a single list.
     * Used by [ExpectationFailure.visibleElements] to populate the
     * "what was on screen when this failed" payload.
     */
    fun flatten(): List<A11yNode> {
        val out = mutableListOf(this)
        for (child in children) {
            out.addAll(child.flatten())
        }
        return out
    }
}

@Serializable
data class Rect(
    val x: Double,
    val y: Double,
    val w: Double,
    val h: Double,
) {
    val centerX: Double get() = x + w / 2.0
    val centerY: Double get() = y + h / 2.0
}
