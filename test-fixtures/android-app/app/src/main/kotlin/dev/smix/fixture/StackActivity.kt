package dev.smix.fixture

import android.app.Activity
import android.app.Dialog
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView

/// Pages pushed as dialogs, the way a consumer's app pushes every page.
///
/// Each page is a dialog with the translucent no-title theme and an opaque
/// background painted over the whole screen, so it covers what is under it
/// while its window is not opaque as far as the window manager knows. A
/// second one, a sheet at the bottom of the screen, is opened from the
/// first. Three windows of one app stand on one another: the activity, a
/// page that covers it, and a sheet over part of the page.
class StackActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                // clear of the status bar, which an edge-to-edge window draws over
                setPadding(0, 200, 0, 0)
                addView(TextView(this@StackActivity).apply { text = "under the page" })
                addView(
                    Button(this@StackActivity).apply {
                        id = R.id.fixture_stack_open_page
                        text = "Open page"
                        setOnClickListener { openPage() }
                    },
                )
            },
        )
    }

    private fun openPage() = openStackPage(this)
}

private fun page(activity: Activity): Dialog =
    Dialog(activity, android.R.style.Theme_Translucent_NoTitleBar).apply {
        window?.setBackgroundDrawable(ColorDrawable(Color.WHITE))
    }

/// A page over whatever `activity` shows, with a button that opens a sheet
/// over part of it.
internal fun openStackPage(activity: Activity) {
    val page = page(activity)
    page.setContentView(
        LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(0, 200, 0, 0)
            addView(TextView(activity).apply { text = "the page" })
            addView(
                Button(activity).apply {
                    id = R.id.fixture_stack_open_sheet
                    text = "Open sheet"
                    setOnClickListener { openSheet(activity) }
                },
            )
        },
    )
    page.show()
}

private fun openSheet(activity: Activity) {
    val sheet = Dialog(activity, android.R.style.Theme_Translucent_NoTitleBar)
    val panel = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setBackgroundColor(Color.LTGRAY)
        addView(TextView(activity).apply { text = "Time Range" })
        addView(
            Button(activity).apply {
                id = R.id.fixture_stack_sheet_apply
                text = "Apply"
                setOnClickListener { sheet.dismiss() }
            },
        )
    }
    sheet.setContentView(
        FrameLayout(activity).apply {
            addView(
                panel,
                FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.WRAP_CONTENT,
                    Gravity.BOTTOM,
                ),
            )
        },
    )
    sheet.show()
}
