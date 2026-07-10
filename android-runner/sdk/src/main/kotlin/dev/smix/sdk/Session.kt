// v1.0.3 — Session lifecycle guard for Kotlin SDK.
//
// A Session is opened against a running smix-runner (via
// HttpSmixSimRuntime) and drives POST /session/open|close|
// renew-activation. While open, the runtime carries `Session-Id` on
// every request so the runner short-circuits per-request activation
// and reuses the cached UiAutomator binding.
//
// Consumer flow:
//
//   val runtime = HttpSmixSimRuntime(
//     baseUrl = "http://127.0.0.1:28080",
//     bundleId = "com.example.app",
//   )
//   val session = Session.open(runtime, activate = true)
//   try {
//     val app = Smix.launchApp(AppTarget.BundleId("com.example.app"), runtime)
//     app.tap(Selector.Id("btn-login"))
//   } finally {
//     session.close()
//   }

package dev.smix.sdk

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

/**
 * v1.0.4 §D7 — session-scoped sim health classification observed via
 * the runner's `X-Sim-Health` response header. Consumers subscribe via
 * [Session.stateFlow].
 */
enum class SessionState {
    HEALTHY,
    DEGRADED,
    CYCLING,
    DEAD;

    companion object {
        fun fromHeader(value: String): SessionState? = when (value.trim().lowercase()) {
            "healthy" -> HEALTHY
            "degraded" -> DEGRADED
            "cycling" -> CYCLING
            "dead" -> DEAD
            else -> null
        }
    }
}

/**
 * Runner-side session guard.
 *
 * A Session corresponds to a single POST /session/open on the runner.
 * While it is open, the associated [HttpSmixSimRuntime] carries the
 * `Session-Id` header on every request, and the runner reuses the
 * session's cached binding — no per-request `.activate()` equivalents.
 */
class Session private constructor(
    val sessionId: String,
    val activatedOnce: Boolean,
    val serverTimeMs: Long,
    private val runtime: HttpSmixSimRuntime,
) {
    // Non-atomic close guard is fine: duplicate POST /session/close is
    // idempotent on the runner side.
    private var closed = false

    private val _stateFlow = MutableStateFlow(SessionState.HEALTHY)

    /**
     * v1.0.4 §D7 — current sim-health classification. Updated
     * automatically as the runtime parses `X-Sim-Health` headers.
     */
    val stateFlow: StateFlow<SessionState> get() = _stateFlow.asStateFlow()

    /** v1.0.4 §D7 — current state snapshot. */
    val state: SessionState get() = _stateFlow.value

    internal fun updateState(next: SessionState) {
        _stateFlow.value = next
    }

    /**
     * v1.0.5 §D1 — probe `/session/list` and return `true` iff this
     * session's id is still known to the runner. Consumers wire this
     * after a state transition to `CYCLING`/`DEAD` to decide whether
     * to keep the session (persisted across `runner cycle`) or open
     * a fresh one.
     */
    suspend fun stillValid(): Boolean {
        check(!closed) { "session already closed" }
        val obj = runtime.postJsonObject("/session/list", JsonObject(emptyMap()))
        val arr = obj["sessions"] as? kotlinx.serialization.json.JsonArray ?: return false
        return arr.any { element ->
            val entry = element as? JsonObject ?: return@any false
            entry["sessionId"]?.jsonPrimitive?.content == sessionId
        }
    }

    /**
     * v1.0.4 §D14 — instruct the runner to `terminate()` + `launch()`
     * the session's cached UiAutomator binding IN PLACE, preserving
     * this session id. Returns wall-clock milliseconds the cycle took.
     */
    suspend fun relaunchApp(): Long {
        check(!closed) { "session already closed" }
        val body = JsonObject(mapOf("sessionId" to JsonPrimitive(sessionId)))
        val obj = runtime.postJsonObject("/session/relaunch-app", body)
        return obj["wallMs"]?.jsonPrimitive?.longOrNull ?: 0L
    }

    /**
     * Ask the runner to re-issue `.activate()` on the session's cached
     * binding. Subject to a 2s per-session rate limit; when rate-
     * limited, returns false (no-op, session still healthy).
     */
    suspend fun renewActivation(): Boolean {
        check(!closed) { "session already closed" }
        val body = JsonObject(mapOf("sessionId" to JsonPrimitive(sessionId)))
        val obj = runtime.postJsonObject("/session/renew-activation", body)
        return obj["activated"]?.jsonPrimitive?.booleanOrNull ?: false
    }

    /**
     * Release the session — sends POST /session/close (idempotent),
     * clears the `Session-Id` header from the runtime. Subsequent
     * runtime requests fall through to the legacy per-request rebind
     * path (rate-limited to 1 activate / 5s / bundle-id as of v1.0.2).
     */
    suspend fun close() {
        if (closed) {
            return
        }
        closed = true
        try {
            val body = JsonObject(mapOf("sessionId" to JsonPrimitive(sessionId)))
            runtime.postVoid("/session/close", body)
        } finally {
            // Clear the client-side header regardless of runner outcome.
            runtime.setSessionId(null)
        }
    }

    companion object {
        /**
         * Open a session against the runtime's runner. When
         * [activate] is true, the runner calls `.activate()` on the
         * target once at open time; when false, the runner opens a
         * passive binding suitable when the caller has already ensured
         * foreground state.
         *
         * The returned [sessionId] is stashed on the [runtime]; every
         * subsequent request from that runtime carries the
         * `Session-Id` header.
         */
        suspend fun open(
            runtime: HttpSmixSimRuntime,
            activate: Boolean = false,
        ): Session {
            val body = JsonObject(mapOf(
                "bundleId" to JsonPrimitive(runtime.bundleId),
                "activate" to JsonPrimitive(activate),
            ))
            val obj = runtime.postJsonObject("/session/open", body)
            val sid = obj["sessionId"]?.jsonPrimitive?.content
                ?: error("smix runner /session/open: missing sessionId field")
            val activatedOnce = obj["activatedOnce"]?.jsonPrimitive?.booleanOrNull ?: false
            val serverTimeMs = obj["serverTimeMs"]?.jsonPrimitive?.longOrNull ?: 0L
            runtime.setSessionId(sid)
            val session = Session(sid, activatedOnce, serverTimeMs, runtime)
            // v1.0.4 §D7 — wire the runtime's X-Sim-Health parse into
            // this session's state machine.
            runtime.attachSessionStateSetter { next -> session.updateState(next) }
            return session
        }
    }
}
