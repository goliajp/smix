// v1.0.3 — HttpSmixSimRuntime + Session wiring for Kotlin SDK.
//
// java.net.HttpURLConnection–backed SmixSimRuntime implementation that
// speaks the SmixRunnerCore wire (mirror of TS HttpSimRuntime and
// Swift HttpSmixSimRuntime). Adds session lifecycle: when a session id
// is attached via setSessionId(id), every subsequent request carries
// the Session-Id header so the runner reuses the session's cached
// UiAutomator handle and skips per-request .activate() equivalents.
//
// Consumers who need session lifecycle use the Session class
// (Session.kt) which drives /session/open|close|renew-activation via
// this runtime. Consumers who want the legacy per-request path skip
// Session entirely; every request goes out without Session-Id.

package dev.smix.sdk

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.serializer
import java.net.HttpURLConnection
import java.net.URL
import java.util.Base64
import java.util.concurrent.atomic.AtomicReference

/**
 * HTTP-backed [SmixSimRuntime] targeting a running SmixRunnerCore
 * instance at a client-supplied base URL.
 *
 * Session-aware: when a session id is attached via [setSessionId],
 * every request carries the `Session-Id` header and the runner reuses
 * the session's cached binding.
 *
 * Thread-safe on the session-id field (AtomicReference), so consumers
 * may share a single runtime across coroutine dispatchers.
 */
class HttpSmixSimRuntime(
    val baseUrl: String,
    val bundleId: String,
    private val connectTimeoutMs: Int = 5_000,
    private val readTimeoutMs: Int = 15_000,
) : SmixSimRuntime {

    private val sessionIdRef = AtomicReference<String?>(null)

    /** JSON codec — lenient so pre-v1.0.3 runners without new fields still decode. */
    val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    /**
     * v1.0.3 — attach or clear the `Session-Id` header on every
     * subsequent request. Typically driven by [Session.open] /
     * [Session.close]; consumers who manage sessions manually may call
     * directly.
     */
    fun setSessionId(id: String?) {
        sessionIdRef.set(id)
    }

    /** Current session id, or null when no session is attached. */
    val currentSessionId: String?
        get() = sessionIdRef.get()

    // ---- SmixSimRuntime protocol ------------------------------------

    override suspend fun launch(bundleId: String) {
        postVoid("/sim/launch", JsonObject(mapOf("bundleId" to JsonPrimitive(bundleId))))
    }

    override suspend fun terminate(bundleId: String) {
        postVoid("/sim/terminate", JsonObject(mapOf("bundleId" to JsonPrimitive(bundleId))))
    }

    override suspend fun snapshotTree(): A11yNode {
        val bytes = postRaw("/a11y/snapshot", JsonObject(emptyMap()))
        val obj = json.parseToJsonElement(bytes.decodeToString()).jsonObject
        val treeElem = obj["tree"] ?: error("smix runner /a11y/snapshot: missing tree field")
        return json.decodeFromJsonElement(serializer(), treeElem)
    }

    override suspend fun synthesizeTap(x: Double, y: Double) {
        postVoid("/input/tap", JsonObject(mapOf(
            "x" to JsonPrimitive(x),
            "y" to JsonPrimitive(y),
        )))
    }

    override suspend fun sendString(text: String) {
        postVoid("/input/send-string", JsonObject(mapOf("text" to JsonPrimitive(text))))
    }

    override suspend fun pressKey(key: KeyName) {
        postVoid("/input/press-key", JsonObject(mapOf("key" to JsonPrimitive(key.wireName))))
    }

    override suspend fun swipe(direction: SwipeDirection) {
        postVoid("/input/swipe", JsonObject(mapOf("direction" to JsonPrimitive(direction.wireName))))
    }

    override suspend fun screenshot(): ByteArray {
        val bytes = postRaw("/sim/screenshot", JsonObject(emptyMap()))
        val obj = json.parseToJsonElement(bytes.decodeToString()).jsonObject
        val b64 = obj["png"]?.jsonPrimitive?.content
            ?: error("smix runner /sim/screenshot: missing png field")
        return Base64.getDecoder().decode(b64)
    }

    override suspend fun systemPopups(): List<A11yNode> {
        val bytes = postRaw("/sim/system-popups", JsonObject(emptyMap()))
        val obj = json.parseToJsonElement(bytes.decodeToString()).jsonObject
        val nodes = obj["nodes"] ?: return emptyList()
        return json.decodeFromJsonElement(serializer(), nodes)
    }

    override suspend fun openUrl(url: String) {
        postVoid("/sim/open-url", JsonObject(mapOf("url" to JsonPrimitive(url))))
    }

    override suspend fun launchFresh(
        bundleId: String,
        clearState: Boolean,
        clearKeychain: Boolean,
        appPath: String?,
    ) {
        val entries = mutableMapOf<String, JsonElement>(
            "bundleId" to JsonPrimitive(bundleId),
            "clearState" to JsonPrimitive(clearState),
            "clearKeychain" to JsonPrimitive(clearKeychain),
        )
        if (appPath != null) {
            entries["appPath"] = JsonPrimitive(appPath)
        }
        postVoid("/sim/launch-fresh", JsonObject(entries))
    }

    override suspend fun launchFromPath(appPath: String) {
        postVoid("/sim/launch-from-path", JsonObject(mapOf("appPath" to JsonPrimitive(appPath))))
    }

    override suspend fun synthesizeTapAtNormalized(nx: Double, ny: Double) {
        postVoid("/input/tap-normalized", JsonObject(mapOf(
            "nx" to JsonPrimitive(nx),
            "ny" to JsonPrimitive(ny),
        )))
    }

    // ---- internal HTTP helpers --------------------------------------

    /**
     * v1.0.3 — used by [Session] to drive the `/session/open`,
     * `/session/close`, and `/session/renew-activation` routes through
     * the same transport.
     */
    internal fun postJsonObject(path: String, body: JsonObject): JsonObject {
        val bytes = postRaw(path, body)
        return json.parseToJsonElement(bytes.decodeToString()).jsonObject
    }

    /** POST expecting no meaningful body. */
    internal fun postVoid(path: String, body: JsonElement) {
        postRaw(path, body)
    }

    /** POST returning raw bytes. Throws on non-2xx. */
    internal fun postRaw(path: String, body: JsonElement): ByteArray {
        val url = URL(baseUrl.trimEnd('/') + path)
        val conn = url.openConnection() as HttpURLConnection
        try {
            conn.connectTimeout = connectTimeoutMs
            conn.readTimeout = readTimeoutMs
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.setRequestProperty("Content-Type", "application/json")
            conn.setRequestProperty("App-Bundle-Id", bundleId)
            val sid = sessionIdRef.get()
            if (sid != null) {
                conn.setRequestProperty("Session-Id", sid)
            }
            val payload = json.encodeToString(JsonElement.serializer(), body).toByteArray()
            conn.outputStream.use { it.write(payload) }
            val status = conn.responseCode
            val stream = if (status in 200..299) conn.inputStream else conn.errorStream
            val bytes = stream?.readBytes() ?: ByteArray(0)
            if (status !in 200..299) {
                throw HttpSmixSimRuntimeException(
                    "smix runner $path: HTTP $status: ${String(bytes)}"
                )
            }
            return bytes
        } finally {
            conn.disconnect()
        }
    }
}

/** Errors thrown by [HttpSmixSimRuntime]. */
class HttpSmixSimRuntimeException(message: String) : RuntimeException(message)

// ---- wire name mappings ---------------------------------------------

val KeyName.wireName: String
    get() = when (this) {
        KeyName.RETURN -> "Return"
        KeyName.DELETE -> "Delete"
        KeyName.SPACE -> "Space"
        KeyName.TAB -> "Tab"
        KeyName.ESCAPE -> "Escape"
        KeyName.ENTER -> "Enter"
    }

val SwipeDirection.wireName: String
    get() = when (this) {
        SwipeDirection.UP -> "up"
        SwipeDirection.DOWN -> "down"
        SwipeDirection.LEFT -> "left"
        SwipeDirection.RIGHT -> "right"
    }

