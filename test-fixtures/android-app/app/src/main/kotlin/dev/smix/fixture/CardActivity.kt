package dev.smix.fixture

import android.graphics.Color
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.PopupWindow
import androidx.activity.ComponentActivity
import androidx.activity.ComponentDialog
import androidx.activity.compose.setContent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp

/// A dialog shown as a card in the middle of the screen, with a dimming
/// scrim behind it — the shape a consumer's app takes at tablet width.
///
/// The card is the dialog's own window, sized and centred; the scrim is a
/// full-screen popup attached to the activity and shown once the dialog is
/// attached, so it is added after the dialog while standing under it (a
/// sub-window stands just over the window it belongs to). Taps in the card
/// were judged to land on the scrim, because the probe stacked windows in
/// the order they were added. What each touch reached is counted here,
/// by the listeners themselves.
class CardActivity : ComponentActivity() {
    private var custom by mutableIntStateOf(0)
    private var applied by mutableIntStateOf(0)
    private var behind by mutableIntStateOf(0)
    private var scrimTaps by mutableIntStateOf(0)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            Column(
                modifier = Modifier
                    .padding(16.dp)
                    .semantics { testTagsAsResourceId = true },
            ) {
                Text("card screen")
                Button(onClick = { openCard() }, modifier = Modifier.testTag("card_open")) { Text("Open card") }
                Button(onClick = { behind += 1 }, modifier = Modifier.testTag("card_behind")) { Text("Behind") }
                Text(
                    "custom $custom applied $applied behind $behind scrim $scrimTaps",
                    modifier = Modifier.testTag("card_counts"),
                )
            }
        }
    }

    private fun openCard() {
        val dialog = ComponentDialog(this)
        val content = ComposeView(this).apply {
            setContent {
                Column(modifier = Modifier.padding(24.dp).semantics { testTagsAsResourceId = true }) {
                    Text("Environment")
                    Text(
                        "Custom",
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { custom += 1 }
                            .padding(vertical = 12.dp)
                            .testTag("card_custom"),
                    )
                    Button(onClick = { applied += 1 }, modifier = Modifier.testTag("card_apply")) { Text("Apply") }
                }
            }
        }
        dialog.setContentView(content)
        val density = resources.displayMetrics.density
        val window = dialog.window ?: return
        window.setLayout((320 * density).toInt(), (280 * density).toInt())
        window.setGravity(Gravity.CENTER)
        window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_DIM_BEHIND)

        val scrim = View(this).apply {
            setBackgroundColor(Color.argb(150, 0, 0, 0))
            setOnClickListener {
                scrimTaps += 1
                dialog.dismiss()
            }
        }
        val mask = PopupWindow(scrim, ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT, false)
        mask.isClippingEnabled = false
        mask.isTouchable = true
        window.decorView.addOnAttachStateChangeListener(
            object : View.OnAttachStateChangeListener {
                override fun onViewAttachedToWindow(v: View) {
                    val app = this@CardActivity.window.decorView
                    mask.showAtLocation(app, Gravity.NO_GRAVITY, 0, 0)
                }

                override fun onViewDetachedFromWindow(v: View) {
                    if (mask.isShowing) mask.dismiss()
                }
            },
        )
        dialog.show()
    }
}
