// v7.4 c1 — ExpectationFailure + FailureCode mirrors Swift
// SmixSDK.ExpectationFailure + Rust smix-error.
//
// AI-readable JSON contract: errorJson() emits sorted-keys, ISO-8601
// timestamp single-line JSON matching Swift errorDescription contract
// (per design.md §9 #11). v7.4 c4 / v7.5 extends to byte-identical
// cross-binary T1 evidence.

package dev.smix.sdk

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * Structured failure thrown by SDK resolver paths (App.tap, Locator
 * assertions) when matching fails. Codable so the same JSON shape
 * crosses Rust → FFI → Kotlin.
 */
class ExpectationFailure(
    val code: FailureCode,
    override val message: String,
    val selectorJson: String? = null,
    val visibleElements: List<A11yNode> = emptyList(),
    val suggestions: List<String> = emptyList(),
    val timestamp: Long = System.currentTimeMillis(),
) : Exception(message) {
    /**
     * AI-readable JSON dump — what Claude Code sees when the test fails.
     * Sorted keys, single line. Mirror Swift errorDescription.
     */
    fun errorJson(): String {
        val payload = ErrorJsonPayload(
            code = code,
            message = message,
            selector = selectorJson,
            visibleElements = visibleElements,
            suggestions = suggestions,
            timestamp = timestamp,
        )
        return jsonEncoder.encodeToString(ErrorJsonPayload.serializer(), payload)
    }

    companion object {
        private val jsonEncoder = Json { encodeDefaults = true; prettyPrint = false }
    }
}

@Serializable
private data class ErrorJsonPayload(
    @SerialName("code") val code: FailureCode,
    @SerialName("message") val message: String,
    @SerialName("selector") val selector: String?,
    @SerialName("suggestions") val suggestions: List<String>,
    @SerialName("timestamp") val timestamp: Long,
    @SerialName("visibleElements") val visibleElements: List<A11yNode>,
)

/**
 * Machine-readable failure category. Mirror Rust smix-error
 * FailureCode + Swift SmixSDK.FailureCode.
 */
@Serializable
enum class FailureCode {
    @SerialName("notFound") NOT_FOUND,
    @SerialName("ambiguous") AMBIGUOUS,
    @SerialName("notInteractable") NOT_INTERACTABLE,
    @SerialName("timeout") TIMEOUT,
    @SerialName("wrongState") WRONG_STATE,
    @SerialName("unknown") UNKNOWN,
}
