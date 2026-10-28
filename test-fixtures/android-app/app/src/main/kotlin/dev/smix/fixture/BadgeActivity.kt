package dev.smix.fixture

import android.graphics.Color as AndroidColor
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp

/**
 * A screen built in Compose with a plain View laid over it — an
 * environment badge the app adds to `android.R.id.content` after the
 * Compose content, the way a consumer marks a staging build.
 */
class BadgeActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            Column(Modifier.fillMaxSize().padding(16.dp).semantics { testTagsAsResourceId = true }) {
                Text("Landing", Modifier.testTag("badge-landing-title"))
                Button(onClick = {}, modifier = Modifier.testTag("badge-landing-button")) { Text("Sign in") }
            }
        }
        val badge = TextView(this).apply {
            text = "IDENTITY.STAGING.EXAMPLE.COM"
            contentDescription = "environment IDENTITY.STAGING.EXAMPLE.COM"
            setBackgroundColor(AndroidColor.argb(200, 200, 40, 40))
            setTextColor(AndroidColor.WHITE)
            setPadding(16, 8, 16, 8)
        }
        val frame = FrameLayout(this).apply {
            addView(badge, FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.BOTTOM or Gravity.END))
        }
        findViewById<ViewGroup>(android.R.id.content).addView(
            frame,
            ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT),
        )
    }
}
