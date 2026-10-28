package dev.smix.fixture

import android.annotation.SuppressLint
import android.graphics.Color
import android.os.Bundle
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import androidx.activity.ComponentActivity

/**
 * A zoom driven by the platform's `ScaleGestureDetector`, computed the way
 * a consumer's player does: the zoom at `onScaleBegin` times the current
 * span over the span at `onScaleBegin`. `scale-state` also says the span
 * when the fingers came down and when the detector began, which is what
 * tells two recognizers' readings of one pinch apart.
 */
class ScaleActivity : ComponentActivity() {
    private var zoom = 1f
    private var startZoom = 1f
    private var startSpan = 0f
    private var downSpan = 0f
    private lateinit var state: TextView

    @SuppressLint("ClickableViewAccessibility")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        state = TextView(this).apply { id = R.id.scale_state; setPadding(32, 32, 32, 16) }
        val reset = Button(this).apply {
            id = R.id.scale_reset
            text = "Reset Zoom"
            setOnClickListener { zoom = 1f; show() }
        }
        val detector = ScaleGestureDetector(
            this,
            object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
                override fun onScaleBegin(d: ScaleGestureDetector): Boolean {
                    startZoom = zoom
                    startSpan = d.currentSpan
                    return true
                }

                override fun onScale(d: ScaleGestureDetector): Boolean {
                    if (startSpan <= 0f) return true
                    zoom = (startZoom * (d.currentSpan / startSpan)).coerceIn(1f, 5f)
                    show()
                    return true
                }
            },
        )
        val surface = View(this).apply {
            id = R.id.scale_surface
            setBackgroundColor(Color.DKGRAY)
            setOnTouchListener { _, e ->
                if (e.actionMasked == MotionEvent.ACTION_POINTER_DOWN && e.pointerCount == 2) {
                    downSpan = Math.hypot((e.getX(1) - e.getX(0)).toDouble(), (e.getY(1) - e.getY(0)).toDouble()).toFloat()
                }
                detector.onTouchEvent(e)
                true
            }
        }
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                addView(state)
                addView(reset)
                addView(surface, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f))
            },
        )
        show()
    }

    private fun show() {
        state.text = "scale=%.2f down=%.0f begin=%.0f".format(zoom, downSpan, startSpan)
    }
}
