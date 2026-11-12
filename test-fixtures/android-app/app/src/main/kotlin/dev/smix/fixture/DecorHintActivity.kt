package dev.smix.fixture

import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId

/// A Compose screen with a hint bar the app hangs on the window's decor view
/// itself, beside `android.R.id.content` rather than in it — the shape of a
/// consumer's connection banner, which the probe was reported not to read.
/// Reached with `am start -n dev.smix.fixture/.DecorHintActivity`.
class DecorHintActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            Column(modifier = Modifier.semantics { testTagsAsResourceId = true }) {
                Text("decor screen", modifier = Modifier.testTag("decor_screen_title"))
            }
        }
        val hint = TextView(this).apply {
            id = R.id.fixture_decor_hint
            text = "Offline, retrying"
            setBackgroundColor(0xFF333333.toInt())
            setTextColor(0xFFFFFFFF.toInt())
            setPadding(32, 24, 32, 24)
        }
        (window.decorView as ViewGroup).addView(
            hint,
            FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
                Gravity.CENTER,
            ),
        )
    }
}
