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
        setContentView(root)
    }
}
