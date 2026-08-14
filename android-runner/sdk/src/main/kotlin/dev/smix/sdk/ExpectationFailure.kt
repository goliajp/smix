// ExpectationFailure + FailureCode mirror Swift
// SmixSDK.ExpectationFailure + Rust smix-error.
//
// AI-readable JSON contract: errorJson() emits sorted-keys, ISO-8601
// timestamp single-line JSON byte-identical to the Swift
// errorDescription contract.

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
 * Machine-readable failure category. The @SerialName values are Rust
 * `smix_error::FailureCode`'s wire strings verbatim —
 * `crates/smix-error/tests/sdk_failure_code_parity.rs` reads this
 * declaration and fails if the two sets ever diverge.
 */
@Serializable
enum class FailureCode {
    @SerialName("ELEMENT_NOT_FOUND") ELEMENT_NOT_FOUND,
    @SerialName("NOT_VISIBLE") NOT_VISIBLE,
    @SerialName("NOT_ENABLED") NOT_ENABLED,
    @SerialName("AMBIGUOUS") AMBIGUOUS,
    @SerialName("TIMEOUT") TIMEOUT,
    @SerialName("ASSERTION_FAILED") ASSERTION_FAILED,
    @SerialName("APP_NOT_RUNNING") APP_NOT_RUNNING,
    @SerialName("SIMULATOR_NOT_BOOTED") SIMULATOR_NOT_BOOTED,
    /** The touch was synthesised, and it did not land inside the element the selector matched. Distinct from element-not-found: not-found means fix the selector, missed means the element was there and the touch went elsewhere. */
    @SerialName("TAP_MISSED") TAP_MISSED,
    /** The screen is described in one coordinate space and the touch would be delivered in another, so no aim can land where the tree says the element is. Distinct from tap-missed: a miss invites another attempt with a better point, and there is no better point here — whatever is passed gets recomputed against the app's frame and then read against the device's. */
    @SerialName("COORDINATE_SPACE_MISMATCH") COORDINATE_SPACE_MISMATCH,
    @SerialName("DRIVER_ERROR") DRIVER_ERROR,
}
