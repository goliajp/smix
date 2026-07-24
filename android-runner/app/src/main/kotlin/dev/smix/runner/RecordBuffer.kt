// The recording buffer: accumulates captured AccessibilityEvents while a
// recording is active, and maps them to IRAction JSON on drain.
//
// It buffers the RAW CapturedAxEvents, not pre-mapped actions, because the
// mapping coalesces same-field keystrokes into one fill/clear — that needs the
// event sequence, so mapping runs at drain time over the buffered stream.
//
// Thread-safe: the UiAutomation listener appends from its callback thread
// while /record/poll drains from the server thread.

package dev.smix.runner

object RecordBuffer {
    private val lock = Any()
    private val events = mutableListOf<CapturedAxEvent>()
    private var active = false

    /** Begin recording: clear any prior events and accept new ones. */
    fun start() = synchronized(lock) {
        events.clear()
        active = true
    }

    /** Append a captured event — dropped unless a recording is active. */
    fun append(event: CapturedAxEvent) = synchronized(lock) {
        if (active) events.add(event)
    }

    /** Drain: map the buffered events to IRAction JSON and clear the buffer.
     *  A streaming read — polling repeatedly loses nothing. */
    fun poll(): List<String> = synchronized(lock) {
        val mapped = RecordMapper.map(events).actions
        events.clear()
        mapped
    }

    /** Stop recording and drain the remainder. */
    fun stop(): List<String> = synchronized(lock) {
        val mapped = RecordMapper.map(events).actions
        events.clear()
        active = false
        mapped
    }

    /** For tests. */
    fun isActive(): Boolean = synchronized(lock) { active }
}
