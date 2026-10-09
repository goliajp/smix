package dev.smix.fixture

import android.app.Activity
import android.content.Context
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView

/// Views that have been hidden, in the two shapes a consumer's app has.
///
///  - **hidden after it was shown**: a status line in a vertical
///    `LinearLayout`, set GONE in the same frame that new rows are added
///    below it. A `LinearLayout` does not lay out a GONE child, so the
///    line keeps the size and text it had while it was showing.
///  - **hidden but laid out anyway**: an empty-state layer over a list,
///    GONE, whose parent's `onLayout` sizes it to the whole list every
///    pass regardless. It has no id, it is drawn after the list, and a
///    touch passes through it because it is GONE.
///
/// The app's own counter says where a tap on the row went.
class HiddenActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
        }
        val waiting = TextView(this).apply {
            id = R.id.fixture_hidden_waiting
            text = "Waited 0:05 of up to 2:25"
        }
        var added = 0
        val hide = Button(this).apply {
            id = R.id.fixture_hidden_hide
            text = "Hide"
            setOnClickListener {
                waiting.visibility = View.GONE
                repeat(2) {
                    added += 1
                    root.addView(TextView(this@HiddenActivity).apply { text = "row added $added" })
                }
            }
        }
        val taps = TextView(this).apply {
            id = R.id.fixture_hidden_row_taps
            text = "row taps 0"
        }
        var rowTaps = 0
        val row = TextView(this).apply {
            id = R.id.fixture_hidden_row
            text = "a row under a hidden layer"
            textSize = 20f
            setPadding(0, 48, 0, 48)
            isClickable = true
            setOnClickListener {
                rowTaps += 1
                taps.text = "row taps $rowTaps"
            }
        }
        val list = OverlaidList(this, row)

        root.addView(TextView(this).apply { text = "hidden screen" })
        root.addView(hide)
        root.addView(waiting)
        root.addView(taps)
        root.addView(list, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 600))
        setContentView(root)
    }

    /// A list and an empty-state layer over it, both laid out to the
    /// whole of this view on every pass, the layer whether or not it is
    /// showing.
    private class OverlaidList(context: Context, row: View) : FrameLayout(context) {
        private val content = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            addView(row)
        }
        private val empty = FrameLayout(context).apply {
            visibility = View.GONE
            addView(TextView(context).apply { text = "nothing here" })
        }

        init {
            addView(content)
            addView(empty)
        }

        override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
            super.onLayout(changed, l, t, r, b)
            val w = r - l
            val h = b - t
            for (child in listOf(content, empty)) {
                child.measure(
                    MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY),
                    MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY),
                )
                child.layout(0, 0, w, h)
            }
        }
    }
}
