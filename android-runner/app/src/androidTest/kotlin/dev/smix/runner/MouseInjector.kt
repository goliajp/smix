package dev.smix.runner

import android.app.UiAutomation
import android.os.SystemClock
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent

/// Puts a [MouseGesture]'s events in as a mouse: `SOURCE_MOUSE`, a mouse
/// tool, the held buttons in `buttonState`, and on a press or release the
/// button it is about.
internal object MouseInjector {
    /// Which button a press or release is about is what a view's context
    /// click reads (`getActionButton`), and `MotionEvent.obtain` has no
    /// parameter for it; the setter is hidden, so it is reached by name.
    private val setActionButton by lazy {
        MotionEvent::class.java.getMethod("setActionButton", Int::class.javaPrimitiveType)
    }

    /// Every event in, each at its time after the first; false at the
    /// first one the system refused. A failure after a press releases the
    /// button, so a refused drag does not leave it held.
    fun inject(automation: UiAutomation, events: List<MouseGesture.Event>): Boolean {
        val down = SystemClock.uptimeMillis()
        var held = MouseGesture.Button.NONE
        var last: MouseGesture.Event? = null
        for (e in events) {
            val at = down + e.atMs
            val wait = at - SystemClock.uptimeMillis()
            if (wait > 0) SystemClock.sleep(wait)
            val event = obtain(down, at, e)
            val ok = try {
                automation.injectInputEvent(event, true)
            } finally {
                event.recycle()
            }
            if (!ok) {
                if (held != MouseGesture.Button.NONE && last != null) release(automation, down, last, held)
                return false
            }
            held = e.held
            last = e
        }
        return true
    }

    private fun release(automation: UiAutomation, down: Long, at: MouseGesture.Event, held: MouseGesture.Button) {
        val now = SystemClock.uptimeMillis() - down
        for (e in listOf(
            MouseGesture.Event(now, MouseGesture.Action.BUTTON_RELEASE, at.x, at.y, button = held),
            MouseGesture.Event(now, MouseGesture.Action.UP, at.x, at.y),
        )) {
            val event = obtain(down, down + now, e)
            automation.injectInputEvent(event, true)
            event.recycle()
        }
    }

    private fun obtain(down: Long, at: Long, e: MouseGesture.Event): MotionEvent {
        val action = when (e.action) {
            MouseGesture.Action.HOVER_MOVE -> MotionEvent.ACTION_HOVER_MOVE
            MouseGesture.Action.HOVER_EXIT -> MotionEvent.ACTION_HOVER_EXIT
            MouseGesture.Action.DOWN -> MotionEvent.ACTION_DOWN
            MouseGesture.Action.BUTTON_PRESS -> MotionEvent.ACTION_BUTTON_PRESS
            MouseGesture.Action.MOVE -> MotionEvent.ACTION_MOVE
            MouseGesture.Action.BUTTON_RELEASE -> MotionEvent.ACTION_BUTTON_RELEASE
            MouseGesture.Action.UP -> MotionEvent.ACTION_UP
            MouseGesture.Action.SCROLL -> MotionEvent.ACTION_SCROLL
        }
        val props = arrayOf(
            MotionEvent.PointerProperties().apply {
                id = 0
                toolType = MotionEvent.TOOL_TYPE_MOUSE
            },
        )
        val coords = arrayOf(
            MotionEvent.PointerCoords().apply {
                x = e.x
                y = e.y
                pressure = if (e.held == MouseGesture.Button.NONE) 0f else 1f
                size = 1f
                setAxisValue(MotionEvent.AXIS_VSCROLL, e.wheel)
            },
        )
        val meta = if (e.ctrl) KeyEvent.META_CTRL_ON or KeyEvent.META_CTRL_LEFT_ON else 0
        val event = MotionEvent.obtain(
            down, at, action, 1, props, coords, meta, buttons(e.held),
            1f, 1f, 0, 0, InputDevice.SOURCE_MOUSE, 0,
        )
        if (e.button != MouseGesture.Button.NONE) setActionButton.invoke(event, buttons(e.button))
        return event
    }

    private fun buttons(b: MouseGesture.Button): Int = when (b) {
        MouseGesture.Button.NONE -> 0
        MouseGesture.Button.PRIMARY -> MotionEvent.BUTTON_PRIMARY
        MouseGesture.Button.SECONDARY -> MotionEvent.BUTTON_SECONDARY
    }
}
