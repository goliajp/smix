package dev.smix.runner

import org.json.JSONObject

/// Several touches at one point: `times` of them, `intervalMs` apart, each
/// held `holdMs` when given — the keys the iOS runner reads on the same
/// route. A body without them is one ordinary tap.
data class TapBurst(val times: Int, val intervalMs: Long, val holdMs: Long?) {
    /// Touch `times` times: `tap` for an ordinary touch, `hold` for one held
    /// that long, `pause` between them. True when every touch went in.
    fun touch(tap: () -> Boolean, hold: (Long) -> Boolean, pause: (Long) -> Unit): Boolean {
        var ok = true
        for (i in 0 until times) {
            if (i > 0) pause(intervalMs)
            ok = (holdMs?.let(hold) ?: tap()) && ok
        }
        return ok
    }

    companion object {
        /// The gap between touches when the caller names none — the iOS
        /// runner's.
        const val INTERVAL_MS: Long = 80

        fun decode(payload: String): TapBurst {
            val req = JSONObject(payload)
            val times = req.optInt("times", 1)
            require(times >= 1) { "times must be at least 1, got $times" }
            return TapBurst(
                times,
                req.optLong("intervalMs", INTERVAL_MS),
                if (req.has("holdMs")) req.getLong("holdMs") else null,
            )
        }
    }
}
