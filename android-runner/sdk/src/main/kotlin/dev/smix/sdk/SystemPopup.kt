// SystemPopup + SystemPopupButton — mirror the smix-runner-wire
// SystemPopup shape returned as a JSON array by
// `SmixSession.systemPopups()` (`#[serde(rename_all = "camelCase")]`,
// `type` renamed).

package dev.smix.sdk

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * One system popup on screen (alert / sheet / banner) — the shape the
 * FFI `SmixSession.systemPopups()` returns.
 */
@Serializable
data class SystemPopup(
    @SerialName("id") val id: String,
    @SerialName("type") val type: String,
    @SerialName("source") val source: String,
    @SerialName("title") val title: String = "",
    @SerialName("body") val body: String = "",
    @SerialName("buttons") val buttons: List<SystemPopupButton> = emptyList(),
)

/** One button on a [SystemPopup]. */
@Serializable
data class SystemPopupButton(
    @SerialName("id") val id: String,
    @SerialName("label") val label: String,
    @SerialName("role") val role: String,
    @SerialName("dangerous") val dangerous: Boolean = false,
    @SerialName("outcomeHint") val outcomeHint: String? = null,
)
