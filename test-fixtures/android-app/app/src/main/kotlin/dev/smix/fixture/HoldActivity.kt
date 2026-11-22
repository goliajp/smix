package dev.smix.fixture

import android.app.Activity
import android.content.Context
import android.graphics.Canvas
import android.os.Bundle
import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/// A button that counts long presses and taps apart, so a gate can tell
/// a press held long enough from a tap that only went in slowly, and one
/// with no long-press handler, which a press held and released clicks —
/// the way a press on a control that is rebuilt mid-press is tested.
///
/// Started with `--ez busy true`, a view under it redraws every frame
/// and spends most of each frame on the main thread, the way a screen
/// full of live content does: the press has to be delivered to an app
/// that is slow to finish each input event.
class HoldActivity : Activity() {
    private var held = 0
    private var tapped = 0
    private var clicked = 0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val result = TextView(this).apply { id = R.id.hold_result }
        fun show() { result.text = "held $held tapped $tapped clicked $clicked" }
        show()
        val target = Button(this).apply {
            id = R.id.hold_target
            text = "hold me"
            setOnClickListener { tapped++; show() }
            setOnLongClickListener { held++; show(); true }
        }
        // how long the last press on the plain button was down, from the
        // touch's own times: what the app received, not what was asked for
        val last = TextView(this).apply { id = R.id.hold_last; text = "no press" }
        val plain = Button(this).apply {
            id = R.id.hold_plain
            text = "click me"
            setOnClickListener { clicked++; show() }
            setOnTouchListener { _, e ->
                if (e.actionMasked == MotionEvent.ACTION_UP) {
                    last.text = "down ${e.eventTime - e.downTime} ms"
                }
                false
            }
        }
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(result)
            addView(target)
            addView(plain)
            addView(last)
        }
        if (intent.getBooleanExtra("busy", false)) root.addView(Busy(this))
        setContentView(root)
    }

    private class Busy(context: Context) : View(context) {
        init { minimumHeight = 200 }

        override fun onDraw(canvas: Canvas) {
            super.onDraw(canvas)
            SystemClock.sleep(40)
            invalidate()
        }
    }
}
