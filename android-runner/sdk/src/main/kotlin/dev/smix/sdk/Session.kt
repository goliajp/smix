// Driving seam for the Kotlin SDK: `Driver` (sense + session lifecycle)
// and `Session` (act) interfaces that `App` holds, plus their default
// UniFFI-backed implementations.
//
// App takes a Driver + Session in its constructor; the default impls
// wrap the UniFFI Kotlin bindings `uniffi.smix.SmixDriver` /
// `uniffi.smix.SmixSession`. JVM unit tests inject in-memory mocks so
// they don't trigger JNA initialization (which can't load
// `libuniffi_smix.so` on host macOS — that .so is Android-only). This
// mirrors the SelectorResolver seam.
//
// The FFI methods take a trailing `CancelToken?`; the seam hides it and
// the default impls pass `null` (mirror the Swift SDK passing
// `cancel: nil`).

package dev.smix.sdk

/**
 * Sensing + session-lifecycle surface: the accessibility tree, opening
 * a session, and listing the runner's open sessions. Default impl wraps
 * `uniffi.smix.SmixDriver`.
 */
interface Driver {
    /** The accessibility tree as JSON — the exact shape the resolver takes. */
    suspend fun tree(): String

    /** Open a session bound to [bundleId]. */
    suspend fun openSession(bundleId: String): Session

    /** The sessions the runner currently holds open, as JSON. */
    suspend fun listSessions(): String
}

/**
 * Acting surface on an open session. Default impl wraps
 * `uniffi.smix.SmixSession`.
 */
interface Session {
    /** The runner's token for this session. */
    fun id(): String

    /** Launch the session's app. */
    suspend fun launchApp()

    /**
     * Tap the element with this accessibility id. Returns whether the
     * runner found one to tap. This is how the SDK taps: it resolves a
     * selector to an id and passes it here, so no coordinate crosses the
     * boundary.
     */
    suspend fun tapById(id: String): Boolean

    /** Tap at a normalized coordinate, both in `0.0..1.0`. */
    suspend fun tapAtNormCoord(nx: Double, ny: Double)

    /** Type into the focused element. */
    suspend fun inputText(text: String)

    /** Press a hardware-like key. [key] is a runner name ("return", "arrowUp"). */
    suspend fun pressKey(key: String)

    /** Swipe so the named direction of content comes into view. */
    suspend fun swipeOnce(direction: String)

    /** System alerts / permission dialogs currently on screen, as JSON. */
    suspend fun systemPopups(): String

    /** Terminate the session's app. */
    suspend fun terminateApp()

    /** Relaunch the session's app in place. */
    suspend fun relaunchApp()

    /** Renew the session's activation so the app stays foregrounded. */
    suspend fun renewActivation()

    /** Close the session, releasing the runner's cached binding (idempotent). */
    suspend fun close()
}

/**
 * Default [Driver] — UniFFI-backed, pointing at a runner on [port] of
 * this machine. Constructing this loads the UniFFI binding, so it is
 * built only by [Smix.launchApp] on-device; host unit tests inject a
 * mock instead.
 */
internal class FfiDriver(port: UShort) : Driver {
    private val inner = uniffi.smix.SmixDriver(port)

    override suspend fun tree(): String = inner.tree(null)

    override suspend fun openSession(bundleId: String): Session =
        FfiSession(inner.openSession(bundleId, null))

    override suspend fun listSessions(): String = inner.listSessions(null)
}

/** Default [Session] — UniFFI-backed, wrapping an `uniffi.smix.SmixSession`. */
internal class FfiSession(private val inner: uniffi.smix.SmixSession) : Session {
    override fun id(): String = inner.id()

    override suspend fun launchApp() = inner.launchApp(null)

    override suspend fun tapById(id: String): Boolean = inner.tapById(id, null)

    override suspend fun tapAtNormCoord(nx: Double, ny: Double) = inner.tapAtNormCoord(nx, ny, null)

    override suspend fun inputText(text: String) = inner.inputText(text, null)

    override suspend fun pressKey(key: String) = inner.pressKey(key, null)

    override suspend fun swipeOnce(direction: String) = inner.swipeOnce(direction, null)

    override suspend fun systemPopups(): String = inner.systemPopups(null)

    override suspend fun terminateApp() = inner.terminateApp(null)

    override suspend fun relaunchApp() = inner.relaunchApp(null)

    override suspend fun renewActivation() = inner.renewActivation(null)

    override suspend fun close() = inner.close(null)
}
