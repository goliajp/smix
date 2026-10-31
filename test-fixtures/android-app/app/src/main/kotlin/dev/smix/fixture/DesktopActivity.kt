package dev.smix.fixture

import android.app.Activity
import android.content.res.Configuration
import android.os.Bundle
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView

/// What a desktop window's pointer and keyboard did, written where a test
/// reads it: hover, a secondary click, a wheel, a mouse drag, the keys
/// pressed and the window's size. A Chromebook build's hover styles were
/// checked by eye because nothing could put a mouse over them, so each
/// effect here is a label rather than a look.
class DesktopActivity : Activity() {
    private lateinit var hoverState: TextView
    private lateinit var window_: TextView
    private lateinit var keys: TextView
    private var hovers = 0
    private var contexts = 0
    private var wheel = 0f
    private var keysSeen = mutableListOf<String>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val dp = resources.displayMetrics.density

        hoverState = label("hover out 0", R.id.desk_hover_state)
        // Spans the window, as a list row does: its hover is the row's.
        val hoverRow = label("hover row", R.id.desk_hover_row).apply {
            setPadding((16 * dp).toInt(), (24 * dp).toInt(), 0, (24 * dp).toInt())
            setOnHoverListener { _, e ->
                when (e.actionMasked) {
                    MotionEvent.ACTION_HOVER_ENTER -> {
                        hovers++
                        hoverState.text = "hover in $hovers"
                    }
                    MotionEvent.ACTION_HOVER_EXIT -> hoverState.text = "hover out $hovers"
                }
                false
            }
        }
        val context = label("context clicks 0", R.id.desk_context).apply {
            setPadding(0, (24 * dp).toInt(), 0, (24 * dp).toInt())
            setOnContextClickListener {
                contexts++
                text = "context clicks $contexts"
                true
            }
        }
        val scroll = label("wheel 0 ctrl no", R.id.desk_scroll).apply {
            setPadding(0, (24 * dp).toInt(), 0, (24 * dp).toInt())
            setOnGenericMotionListener { _, e ->
                if (e.actionMasked != MotionEvent.ACTION_SCROLL) return@setOnGenericMotionListener false
                wheel += e.getAxisValue(MotionEvent.AXIS_VSCROLL)
                val ctrl = if (e.metaState and KeyEvent.META_CTRL_ON != 0) "yes" else "no"
                text = "wheel ${wheel.toInt()} ctrl $ctrl"
                true
            }
        }
        var from = 0f to 0f
        val drag = label("drag none", R.id.desk_drag).apply {
            setPadding(0, (48 * dp).toInt(), 0, (48 * dp).toInt())
            setOnTouchListener { _, e ->
                when (e.actionMasked) {
                    MotionEvent.ACTION_DOWN -> from = e.rawX to e.rawY
                    MotionEvent.ACTION_UP -> {
                        val dx = ((e.rawX - from.first) / dp).toInt()
                        val dy = ((e.rawY - from.second) / dp).toInt()
                        val tool = if (e.getToolType(0) == MotionEvent.TOOL_TYPE_MOUSE) "mouse" else "finger"
                        text = "drag $dx $dy $tool"
                    }
                }
                true
            }
        }
        window_ = label("", R.id.desk_window)
        keys = label("keys none", R.id.desk_keys)
        showWindow()

        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                listOf(hoverRow, hoverState, context, scroll, drag, window_, keys).forEach {
                    addView(it, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
                }
            },
        )
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        showWindow()
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) return super.onKeyDown(keyCode, event)
        val mods = buildList {
            if (event.isCtrlPressed) add("ctrl")
            if (event.isShiftPressed) add("shift")
            if (event.isAltPressed) add("alt")
            if (event.isMetaPressed) add("meta")
        }
        keysSeen += (mods + KeyEvent.keyCodeToString(keyCode).removePrefix("KEYCODE_")).joinToString("+")
        keys.text = "keys " + keysSeen.takeLast(4).joinToString(" ")
        return true
    }

    private fun showWindow() {
        val c = resources.configuration
        window_.text = "window ${c.screenWidthDp} ${c.screenHeightDp}"
    }

    private fun label(text: String, id: Int) = TextView(this).apply {
        this.text = text
        this.id = id
        textSize = 18f
    }
}
