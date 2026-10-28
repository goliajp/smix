package dev.smix.runner

import org.json.JSONObject

/// Two fingers side by side on a horizontal line through (cx, cy), moved
/// from `fromSpan` apart to `toSpan` apart over `durationMs`, then, when
/// `panMs > 0`, together by (panDx × width, panDy × height) before lifting.
/// Spans are shares of the display's shorter side — the same body the iOS
/// runner reads on `/pinch-at-norm-coord`.
data class PinchGesture(
    val cx: Double,
    val cy: Double,
    val fromSpan: Double,
    val toSpan: Double,
    val durationMs: Long,
    val panDx: Double = 0.0,
    val panDy: Double = 0.0,
    val panMs: Long = 0,
) {
    data class Point(val x: Float, val y: Float)

    data class Fingers(val first: Point, val second: Point)

    /// `down` is where the fingers touch: `prerollPx` short of `start`.
    data class Plan(val down: Fingers, val start: Fingers, val pinched: Fingers, val end: Fingers)

    enum class Action { DOWN, POINTER_DOWN, MOVE, POINTER_UP, UP }

    /// One motion event: when, what, and where both fingers are. A DOWN or
    /// UP carries only the first finger, because only it is on the screen.
    data class Event(val atMs: Long, val action: Action, val fingers: List<Point>)

    /// A finger leaves the display at some moment of the gesture. Refused
    /// rather than clamped: a clamped finger pinches by a ratio nobody asked
    /// for, and the route would report it as done.
    class OffScreen(val finger: Int, val moment: String, val at: Point) : IllegalArgumentException(
        "finger $finger leaves the screen $moment at (${at.x.toInt()}, ${at.y.toInt()}); " +
            "move the centre or use smaller spans",
    )

    /// `prerollPx` is how far short of the start each finger comes down —
    /// the display's touch slop. A recognizer counts a pinch from where it
    /// begins, and begins only once the fingers have moved that far, so a
    /// pinch sent as asked loses its first slop of movement: 1 → 3 left a
    /// Compose transform at 2.62x. Moving that distance first, slowly,
    /// starts the recognizer before the counted pinch does.
    fun plan(width: Int, height: Int, prerollPx: Float): Plan {
        val short = minOf(width, height).toDouble()
        fun pair(x: Double, y: Double, span: Double): Fingers {
            val half = span * short / 2
            return Fingers(Point((x - half).toFloat(), y.toFloat()), Point((x + half).toFloat(), y.toFloat()))
        }
        val mx = cx * width
        val my = cy * height
        val start = pair(mx, my, fromSpan)
        val pinched = pair(mx, my, toSpan)
        val end = if (panMs > 0) pair(mx + panDx * width, my + panDy * height, toSpan) else pinched
        fun shortOf(from: Point, to: Point): Point {
            val dx = to.x - from.x
            val dy = to.y - from.y
            val len = maxOf(1f, Math.sqrt((dx * dx + dy * dy).toDouble()).toFloat())
            return Point(from.x - dx / len * prerollPx, from.y - dy / len * prerollPx)
        }
        val down = Fingers(shortOf(start.first, pinched.first), shortOf(start.second, pinched.second))
        val moments = listOf(
            "as it comes down" to down,
            "at the start" to start,
            "after the pinch" to pinched,
            "after the pan" to end,
        )
        for ((moment, f) in moments) {
            listOf(f.first, f.second).forEachIndexed { i, p ->
                if (p.x < 0 || p.x > width || p.y < 0 || p.y > height) throw OffScreen(i + 1, moment, p)
            }
        }
        return Plan(down, start, pinched, end)
    }

    /// The motion events for `plan`, a move every [STEP_MS] or so.
    fun timeline(plan: Plan): List<Event> {
        val events = mutableListOf(
            Event(0, Action.DOWN, listOf(plan.down.first)),
            Event(0, Action.POINTER_DOWN, listOf(plan.down.first, plan.down.second)),
        )
        fun moves(from: Fingers, to: Fingers, startMs: Long, lengthMs: Long) {
            val steps = maxOf(1L, lengthMs / STEP_MS).toInt()
            for (i in 1..steps) {
                val t = i.toFloat() / steps
                events += Event(
                    startMs + lengthMs * i / steps,
                    Action.MOVE,
                    listOf(lerp(from.first, to.first, t), lerp(from.second, to.second, t)),
                )
            }
        }
        moves(plan.down, plan.start, 0, PREROLL_MS)
        moves(plan.start, plan.pinched, PREROLL_MS, durationMs)
        if (panMs > 0) moves(plan.pinched, plan.end, PREROLL_MS + durationMs, panMs)
        val liftMs = PREROLL_MS + durationMs + panMs + HOLD_MS
        events += Event(liftMs, Action.POINTER_UP, listOf(plan.end.first, plan.end.second))
        events += Event(liftMs, Action.UP, listOf(plan.end.first))
        return events
    }

    companion object {
        const val STEP_MS: Long = 10
        /// How long the fingers take to reach the start from where they
        /// came down — the iOS runner's figure.
        const val PREROLL_MS: Long = 200
        /// How long the fingers stay still at the end before lifting.
        const val HOLD_MS: Long = 200
        val DURATION_RANGE = 50L..10_000L
        val PAN_RANGE = 0L..10_000L

        private fun lerp(a: Point, b: Point, t: Float) = Point(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)

        fun decode(payload: String): PinchGesture {
            val req = JSONObject(payload)
            fun share(key: String, ok: (Double) -> Boolean): Double {
                val v = req.getDouble(key)
                require(ok(v)) { "$key out of range: $v" }
                return v
            }
            fun ms(key: String, range: LongRange, default: Long?): Long {
                val v = if (default != null && !req.has(key)) default else req.getLong(key)
                require(v in range) { "$key out of range: $v" }
                return v
            }
            fun optShare(key: String): Double {
                if (!req.has(key)) return 0.0
                return share(key) { it in -1.0..1.0 }
            }
            return PinchGesture(
                cx = share("cx") { it in 0.0..1.0 },
                cy = share("cy") { it in 0.0..1.0 },
                fromSpan = share("fromSpan") { it > 0 && it <= 1 },
                toSpan = share("toSpan") { it > 0 && it <= 1 },
                durationMs = ms("durationMs", DURATION_RANGE, null),
                panDx = optShare("panDx"),
                panDy = optShare("panDy"),
                panMs = ms("panMs", PAN_RANGE, 0),
            )
        }
    }
}
