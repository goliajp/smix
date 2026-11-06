package dev.smix.runner

import org.json.JSONObject

/// One finger down at (fromNx, fromNy), still for `holdMs`, moved to
/// (toNx, toNy) over `durationMs`, still there for `releaseMs`, lifted —
/// the body the iOS runner reads on `/drag-at-norm-coord` too.
///
/// The hold is what lets a launcher widget or icon come loose: a launcher
/// starts its drag on a long press, and `UiDevice.drag` moves at once.
/// The rest at the end is what lets a drop target such as "Remove" see
/// the finger over it before it lifts.
data class DragGesture(
    val fromNx: Double,
    val fromNy: Double,
    val toNx: Double,
    val toNy: Double,
    val holdMs: Long,
    val durationMs: Long,
    val releaseMs: Long,
) {
    /// The motion events on a `width` × `height` display, a move every
    /// [STEP_MS] or so while the finger travels. One finger, so each event
    /// carries one point; [PinchGesture.Event] is the shape the injector
    /// takes.
    fun timeline(width: Int, height: Int): List<PinchGesture.Event> {
        val from = PinchGesture.Point((fromNx * width).toFloat(), (fromNy * height).toFloat())
        val to = PinchGesture.Point((toNx * width).toFloat(), (toNy * height).toFloat())
        val events = mutableListOf(PinchGesture.Event(0, PinchGesture.Action.DOWN, listOf(from)))
        if (holdMs > 0) events += PinchGesture.Event(holdMs, PinchGesture.Action.MOVE, listOf(from))
        val steps = maxOf(1L, durationMs / STEP_MS).toInt()
        for (i in 1..steps) {
            val t = i.toFloat() / steps
            val p = PinchGesture.Point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t)
            events += PinchGesture.Event(holdMs + durationMs * i / steps, PinchGesture.Action.MOVE, listOf(p))
        }
        val liftMs = holdMs + durationMs + releaseMs
        if (releaseMs > 0) events += PinchGesture.Event(liftMs, PinchGesture.Action.MOVE, listOf(to))
        events += PinchGesture.Event(liftMs, PinchGesture.Action.UP, listOf(to))
        return events
    }

    companion object {
        const val STEP_MS: Long = 10
        /// The ranges the host checks too (`smix_input::Drag::check`).
        val HOLD_RANGE = 0L..10_000L
        val DURATION_RANGE = 50L..10_000L
        val RELEASE_RANGE = 0L..5_000L

        /// Every field is required: the host fills in the defaults, so the
        /// two runners never disagree about what an absent one means.
        fun decode(payload: String): DragGesture {
            val req = JSONObject(payload)
            fun share(key: String): Double {
                val v = req.getDouble(key)
                require(v in 0.0..1.0) { "$key out of range: $v" }
                return v
            }
            fun ms(key: String, range: LongRange): Long {
                val v = req.getLong(key)
                require(v in range) { "$key out of range: $v" }
                return v
            }
            return DragGesture(
                fromNx = share("fromNx"),
                fromNy = share("fromNy"),
                toNx = share("toNx"),
                toNy = share("toNy"),
                holdMs = ms("holdMs", HOLD_RANGE),
                durationMs = ms("durationMs", DURATION_RANGE),
                releaseMs = ms("releaseMs", RELEASE_RANGE),
            )
        }
    }
}
