package dev.smix.fixture

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.widget.Button
import android.widget.FrameLayout
import android.widget.TextView

/**
 * A button under a toast, as a consumer's home screen had one: the menu
 * button at the top right stays where it is, and a card added after it in
 * the same window lies across its lower part, the card's own close button
 * over the menu button's centre. `cover_count` counts the menu button's
 * presses, `cover_closes` the close button's.
 *
 * And a button nothing of which shows: `cover_hidden`, under a panel along
 * the bottom that takes every touch on it, as a sheet's scrim does.
 * `cover_sheet_taps` counts the touches the panel took.
 */
class CoverActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val px = { dp: Int -> (dp * resources.displayMetrics.density).toInt() }
        var presses = 0
        var closes = 0
        val count = TextView(this).apply { id = R.id.cover_count; text = "pressed 0" }
        val closed = TextView(this).apply { id = R.id.cover_closes; text = "closed 0" }
        val menu = Button(this).apply {
            id = R.id.cover_menu
            text = "Menu"
            // a raised button is drawn, and touched, above a later sibling
            // at rest; the consumer's was flat
            stateListAnimator = null
            elevation = 0f
            setOnClickListener { presses += 1; count.text = "pressed $presses" }
        }
        val close = Button(this).apply {
            id = R.id.cover_close
            text = "X"
            setOnClickListener { closes += 1; closed.text = "closed $closes" }
        }
        val card = FrameLayout(this).apply {
            id = R.id.cover_card
            setBackgroundColor(Color.DKGRAY)
            addView(TextView(this@CoverActivity).apply {
                text = "No network connection"
                setTextColor(Color.WHITE)
            }, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT, Gravity.CENTER_VERTICAL))
            addView(close, FrameLayout.LayoutParams(px(48), px(48), Gravity.END))
        }
        val root = FrameLayout(this)
        root.addView(menu, FrameLayout.LayoutParams(px(48), px(48), Gravity.END).apply {
            topMargin = px(80)
            rightMargin = px(8)
        })
        root.addView(count, FrameLayout.LayoutParams(MATCH_PARENT, px(40), Gravity.CENTER))
        root.addView(closed, FrameLayout.LayoutParams(MATCH_PARENT, px(40), Gravity.CENTER).apply {
            topMargin = px(60)
        })
        // across the menu button's lower part: its centre is under the card's close button
        root.addView(card, FrameLayout.LayoutParams(MATCH_PARENT, px(48), Gravity.TOP).apply {
            topMargin = px(96)
            leftMargin = px(16)
            rightMargin = px(8)
        })
        var hidden = 0
        var sheetTaps = 0
        val hiddenCount = TextView(this).apply { id = R.id.cover_hidden_count; text = "hidden 0" }
        val sheetCount = TextView(this).apply { id = R.id.cover_sheet_taps; text = "sheet 0" }
        val hiddenButton = Button(this).apply {
            id = R.id.cover_hidden
            text = "Hidden"
            stateListAnimator = null
            elevation = 0f
            setOnClickListener { hidden += 1; hiddenCount.text = "hidden $hidden" }
        }
        val sheet = FrameLayout(this).apply {
            id = R.id.cover_sheet
            setBackgroundColor(Color.GRAY)
            setOnClickListener { sheetTaps += 1; sheetCount.text = "sheet $sheetTaps" }
        }
        root.addView(hiddenCount, FrameLayout.LayoutParams(MATCH_PARENT, px(40), Gravity.CENTER).apply {
            topMargin = px(120)
        })
        root.addView(sheetCount, FrameLayout.LayoutParams(MATCH_PARENT, px(40), Gravity.CENTER).apply {
            topMargin = px(180)
        })
        root.addView(hiddenButton, FrameLayout.LayoutParams(px(96), px(48), Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL).apply {
            bottomMargin = px(60)
        })
        // over the whole of the hidden button, and every touch on it is its
        root.addView(sheet, FrameLayout.LayoutParams(MATCH_PARENT, px(160), Gravity.BOTTOM))
        setContentView(root)
    }
}
