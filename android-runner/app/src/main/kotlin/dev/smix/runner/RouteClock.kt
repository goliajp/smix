// A route's own time limit, and what each of its stages spent of it.
//
// The host waits for a route as long as the route says it can take, plus a
// margin. Stages that each keep their own budget can still add up past
// that: a look begun in time finishes late, or a shell command has no bound
// at all. A /clear-text on a loaded emulator ran past the host's 26 s and
// the host reported a step that "may have acted", with nothing to say where
// the time went. So a route carries its limit, starts no stage once it is
// spent, and says where it stopped and what each stage took. The host then
// hears back unless the whole emulator stops.

package dev.smix.runner

class RouteClock(
    val limitMs: Long,
    private val nowMs: () -> Long = { android.os.SystemClock.elapsedRealtime() },
) {
    private val started = nowMs()
    private val stages = mutableListOf<Pair<String, Long>>()

    /** The stage that was not started because the limit was spent. */
    var stoppedBefore: String? = null
        private set

    fun elapsedMs(): Long = nowMs() - started

    fun leftMs(): Long = maxOf(limitMs - elapsedMs(), 0L)

    /** [budgetMs], or what is left of the limit when that is less. */
    fun budget(budgetMs: Long): Long = minOf(budgetMs, leftMs())

    val spent: Boolean get() = stoppedBefore != null

    /**
     * Runs [block] as stage [name], unless the limit is already spent: then
     * nothing runs, [stoppedBefore] names this stage, and the result is null.
     * A null from a stage that ran is told apart by [spent].
     */
    fun <T> stage(name: String, block: () -> T): T? {
        if (spent) return null
        if (leftMs() <= 0L) {
            stoppedBefore = name
            return null
        }
        val began = nowMs()
        try {
            return block()
        } finally {
            stages += name to (nowMs() - began)
        }
    }

    /** "focus=12 set-text=3 idle=401 readback=80" — each stage that ran. */
    fun stagesText(): String = stages.joinToString(" ") { "${it.first}=${it.second}" }

    /** What a route that stopped says about it. */
    fun saw(): String = buildString {
        stoppedBefore?.let { append("stopped before $it, its limit spent; ") }
        append(stagesText().ifEmpty { "no stage ran" })
        append(" tookMs=${elapsedMs()} limitMs=$limitMs")
    }
}

// One line per route in the device log, so a route that took too long can
// be read back after the fact: when it arrived, what it answered, how long
// it took and — for a route with a clock — each stage. The line is short
// on purpose; it is written for every request.
object RouteLog {
    const val TAG = "smix-route"

    val current = ThreadLocal<RouteClock?>()

    fun line(path: String, status: Int, tookMs: Long, clock: RouteClock?): String = buildString {
        append("route=$path status=$status tookMs=$tookMs")
        if (clock != null) {
            append(" limitMs=${clock.limitMs}")
            clock.stoppedBefore?.let { append(" stoppedBefore=$it") }
            val stages = clock.stagesText()
            if (stages.isNotEmpty()) append(" stages=[$stages]")
        }
    }
}
