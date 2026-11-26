package dev.smix.fixture

import android.app.Activity
import android.graphics.Rect
import android.os.Bundle
import android.view.TouchDelegate
import android.widget.CheckBox
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView

/**
 * A checkbox drawn 24 dp across whose row hands it a 72 dp touch area
 * through a `TouchDelegate`, and a second one with no delegate. The tree
 * reports the first one's touch area apart from its bounds, and none for
 * the second, so a reader can tell the two apart. A header keeps the row
 * off the screen's origin, where an area left in the row's own
 * coordinates would read the same as one moved onto the screen.
 */
class DelegateActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val px = { dp: Int -> (dp * resources.displayMetrics.density).toInt() }
        val check = CheckBox(this).apply {
            id = R.id.delegate_check
            contentDescription = "delegated"
        }
        val row = FrameLayout(this).apply {
            id = R.id.delegate_row
            setPadding(px(40), px(40), px(40), px(40))
            addView(check, FrameLayout.LayoutParams(px(24), px(24)))
        }
        val plain = CheckBox(this).apply {
            id = R.id.delegate_plain
            contentDescription = "plain"
        }
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                // so the row is not at the screen's origin and its offset shows
                addView(TextView(this@DelegateActivity).apply { text = "delegate" }, LinearLayout.LayoutParams(px(200), px(100)))
                addView(row)
                addView(plain, LinearLayout.LayoutParams(px(24), px(24)))
            },
        )
        row.post {
            val area = Rect()
            check.getHitRect(area)
            val grow = px(24)
            area.inset(-grow, -grow)
            row.touchDelegate = TouchDelegate(area, check)
        }
    }
}
