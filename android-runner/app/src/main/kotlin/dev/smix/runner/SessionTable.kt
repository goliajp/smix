// In-memory /session/* bookkeeping for the Android runner. Pure JVM
// (no android.* imports) so open/close/list/renew logic runs under
// :app:testDebugUnitTest; the shell side effects (am start /
// am force-stop) stay in SmixHttpServer (androidTest).
//
// No persistence — sessions die with the instrumentation process, which
// matches the iOS runner's in-memory session table (persistence there is
// a supervisor concern, out of scope for the Android runner).

package dev.smix.runner

class SessionTable(private val nowMs: () -> Long = System::currentTimeMillis) {
    data class Entry(
        val sessionId: String,
        val bundleId: String,
        val openedAtMs: Long,
        val lastActivatedAtMs: Long,
    )

    private val entries = LinkedHashMap<String, Entry>()
    private var counter = 0

    @Synchronized
    fun open(bundleId: String, activated: Boolean): Entry {
        counter += 1
        val now = nowMs()
        val entry = Entry(
            sessionId = "sess-android-$counter",
            bundleId = bundleId,
            openedAtMs = now,
            lastActivatedAtMs = if (activated) now else 0L,
        )
        entries[entry.sessionId] = entry
        return entry
    }

    @Synchronized
    fun get(sessionId: String): Entry? = entries[sessionId]

    /// Returns whether the session was known — the wire response is
    /// `ok:true` either way (close is idempotent per the Rust
    /// SessionCloseRequest contract).
    @Synchronized
    fun close(sessionId: String): Boolean = entries.remove(sessionId) != null

    @Synchronized
    fun closeAll(): Int {
        val n = entries.size
        entries.clear()
        return n
    }

    @Synchronized
    fun list(): List<Entry> = entries.values.toList()

    /// Stamps lastActivatedAtMs = now. Null when the session is unknown
    /// (wire: the 404 not_found envelope).
    @Synchronized
    fun renewActivation(sessionId: String): Entry? {
        val existing = entries[sessionId] ?: return null
        val renewed = existing.copy(lastActivatedAtMs = nowMs())
        entries[sessionId] = renewed
        return renewed
    }
}
