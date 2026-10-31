package dev.smix.runner

import org.json.JSONObject

/// A mouse at a point of the display: resting over it, leaving, a
/// secondary click, a wheel turn, or a drag with the primary button. Points
/// are shares of the display (0..1), as `/tap-at-norm-coord` takes them.
///
/// `adb shell input` has no hover and no buttons, so a desktop window's
/// hover styles and right-click menus could not be reached at all; these
/// go in as `SOURCE_MOUSE` events the way a Chromebook's pointer sends them.
data class MouseGesture(
    val kind: Kind,
    val nx: Double,
    val ny: Double,
    val toNx: Double = nx,
    val toNy: Double = ny,
    val wheel: Float = 0f,
    val ctrl: Boolean = false,
) {
    enum class Kind(val wire: String) {
        HOVER("hover"),
        AWAY("away"),
        SECONDARY("secondary"),
        WHEEL("wheel"),
        DRAG("drag"),
        ;

        companion object {
            fun of(wire: String): Kind =
                entries.firstOrNull { it.wire == wire }
                    ?: throw IllegalArgumentException(
                        "kind must be one of ${entries.joinToString { it.wire }}, not $wire",
                    )
        }
    }

    enum class Action { HOVER_MOVE, HOVER_EXIT, DOWN, BUTTON_PRESS, MOVE, BUTTON_RELEASE, UP, SCROLL }

    enum class Button { NONE, PRIMARY, SECONDARY }

    /// One event: when after the first, what, where in display pixels,
    /// which button is held afterwards, which one this press or release is
    /// about, the wheel's turn and whether Ctrl is held.
    data class Event(
        val atMs: Long,
        val action: Action,
        val x: Float,
        val y: Float,
        val held: Button = Button.NONE,
        val button: Button = Button.NONE,
        val wheel: Float = 0f,
        val ctrl: Boolean = false,
    )

    /// The events for a display `width` × `height` pixels.
    fun timeline(width: Int, height: Int): List<Event> {
        val x = (nx * width).toFloat()
        val y = (ny * height).toFloat()
        // A pointer arrives from somewhere: a first move a little away lets
        // the window see it enter before it is over the target.
        val approach = Event(0, Action.HOVER_MOVE, x - APPROACH_PX, y)
        return when (kind) {
            Kind.HOVER -> listOf(approach, Event(STEP_MS, Action.HOVER_MOVE, x, y))
            Kind.AWAY -> listOf(Event(0, Action.HOVER_EXIT, x, y))
            Kind.SECONDARY -> listOf(
                approach,
                Event(STEP_MS, Action.HOVER_MOVE, x, y),
                // A pointer stops hovering as a button goes down; a DOWN
                // while still hovering is dropped as conflicting.
                Event(2 * STEP_MS, Action.HOVER_EXIT, x, y),
                Event(2 * STEP_MS, Action.DOWN, x, y, held = Button.SECONDARY),
                Event(2 * STEP_MS, Action.BUTTON_PRESS, x, y, held = Button.SECONDARY, button = Button.SECONDARY),
                Event(3 * STEP_MS, Action.BUTTON_RELEASE, x, y, button = Button.SECONDARY),
                Event(3 * STEP_MS, Action.UP, x, y),
                Event(4 * STEP_MS, Action.HOVER_MOVE, x, y),
            )
            Kind.WHEEL -> listOf(
                approach,
                Event(STEP_MS, Action.HOVER_MOVE, x, y),
                Event(2 * STEP_MS, Action.SCROLL, x, y, wheel = wheel, ctrl = ctrl),
            )
            Kind.DRAG -> {
                val tx = (toNx * width).toFloat()
                val ty = (toNy * height).toFloat()
                val moves = (1..DRAG_STEPS).map { i ->
                    val f = i.toFloat() / DRAG_STEPS
                    Event((2 + i) * STEP_MS, Action.MOVE, x + (tx - x) * f, y + (ty - y) * f, held = Button.PRIMARY)
                }
                val end = (3 + DRAG_STEPS) * STEP_MS
                listOf(
                    approach,
                    Event(STEP_MS, Action.HOVER_MOVE, x, y),
                    Event(2 * STEP_MS, Action.HOVER_EXIT, x, y),
                    Event(2 * STEP_MS, Action.DOWN, x, y, held = Button.PRIMARY),
                    Event(2 * STEP_MS, Action.BUTTON_PRESS, x, y, held = Button.PRIMARY, button = Button.PRIMARY),
                ) + moves + listOf(
                    Event(end, Action.BUTTON_RELEASE, tx, ty, button = Button.PRIMARY),
                    Event(end, Action.UP, tx, ty),
                    Event(end + STEP_MS, Action.HOVER_MOVE, tx, ty),
                )
            }
        }
    }

    companion object {
        const val STEP_MS = 16L
        const val APPROACH_PX = 8f
        const val DRAG_STEPS = 12

        /// `{"kind":"hover","nx":0.5,"ny":0.5}`, plus `toNx`/`toNy` for a
        /// drag and `wheel`/`ctrl` for a wheel turn.
        fun decode(body: String): MouseGesture {
            val o = try {
                JSONObject(body)
            } catch (e: org.json.JSONException) {
                throw IllegalArgumentException("body is not JSON: ${e.message}")
            }
            fun share(name: String, default: Double? = null): Double {
                val v = if (o.has(name)) o.getDouble(name) else default
                    ?: throw IllegalArgumentException("$name is required")
                require(v in 0.0..1.0) { "$name must be a share of the display, 0..1, not $v" }
                return v
            }
            val kind = Kind.of(o.optString("kind"))
            val nx = share("nx")
            val ny = share("ny")
            return MouseGesture(
                kind = kind,
                nx = nx,
                ny = ny,
                toNx = if (kind == Kind.DRAG) share("toNx") else nx,
                toNy = if (kind == Kind.DRAG) share("toNy") else ny,
                wheel = if (kind == Kind.WHEEL) {
                    require(o.has("wheel")) { "wheel is required" }
                    o.getDouble("wheel").toFloat()
                } else {
                    0f
                },
                ctrl = o.optBoolean("ctrl", false),
            )
        }
    }
}
