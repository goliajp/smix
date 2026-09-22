package dev.smix.fixture

import android.app.Activity
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView

/// A screen the back key cannot leave, whose text changes anyway.
///
/// This is the shape that made `/back`'s old answer wrong in both
/// directions at once. `UiDevice.pressBack()` returns whether any
/// window emitted `TYPE_WINDOW_CONTENT_CHANGED` within a second of the
/// key going in — so a ticking label satisfies it while nothing has
/// gone anywhere, and the route reported `ok` for a back that could
/// not possibly have happened.
///
/// The ticking half is also what decides that the runner's screen
/// signature must not include text: a clock, a spinner or a countdown
/// is an ordinary thing for a screen to have.
class BlockedActivity : Activity() {
    private val handler = Handler(Looper.getMainLooper())
    private var ticks = 0
    private lateinit var ticker: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        ticker = TextView(this).apply {
            id = R.id.fixture_blocked_ticker
            text = "tick 0"
        }
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(this@BlockedActivity).apply { text = "back goes nowhere here" })
            addView(ticker)
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
        }
        setContentView(root)

        tick()
    }

    /// Swallow the key. `targetSdk` is 35 here, so the framework still
    /// routes back through this rather than through the predictive-back
    /// dispatcher, and an app consuming its own back key is ordinary.
    @Deprecated("the fixture targets the legacy back path on purpose")
    @Suppress("DEPRECATION", "MissingSuperCall")
    override fun onBackPressed() = Unit

    private fun tick() {
        ticks += 1
        ticker.text = "tick $ticks"
        handler.postDelayed({ tick() }, TICK_MS)
    }

    override fun onDestroy() {
        handler.removeCallbacksAndMessages(null)
        super.onDestroy()
    }

    private companion object {
        /// Faster than the runner's 50ms look would need, so a poll
        /// that included text would see a different screen every time.
        const val TICK_MS = 200L
    }
}
