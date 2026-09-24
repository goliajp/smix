package dev.smix.fixture

import android.os.Bundle
import android.widget.ImageButton
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView

/// Two controls with no words on them, each named only for accessibility.
///
/// The subject of a consumer's report: a player's middle button carries an
/// icon and a `contentDescription`, and nothing else. On Android that
/// name is what smix calls `label`; on iOS the same control's name is its
/// `accessibilityLabel`. A flow that runs on both phones wants to say
/// "this control, under either name" — which is what `fallback` is for,
/// and which the chain could not say until it read `label`.
///
/// One of each kind the consumer has: a Compose control, and a plain
/// `ImageButton` hosted by `AndroidView` (their button lives in a View
/// shell inside Compose). Each counts the presses it received, and the
/// count is the reading a check uses — not smix's report of having
/// tapped.
class IconActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var pauses by remember { mutableIntStateOf(0) }
            var stops by remember { mutableIntStateOf(0) }
            Column(
                modifier = Modifier
                    .padding(16.dp)
                    .semantics { testTagsAsResourceId = true },
            ) {
                Text("icons")
                // No text, no test tag: the only name is the description.
                Box(
                    modifier = Modifier
                        .size(56.dp)
                        .background(Color.DarkGray)
                        .clickable { pauses += 1 }
                        .semantics {
                            contentDescription = "Pause"
                            role = Role.Button
                        },
                )
                Text("pauses $pauses", modifier = Modifier.testTag("icon_pause_count"))
                AndroidView(
                    factory = { ctx ->
                        ImageButton(ctx).apply {
                            setImageResource(android.R.drawable.ic_media_pause)
                            contentDescription = "Stop"
                            setOnClickListener { stops += 1 }
                        }
                    },
                    modifier = Modifier.size(56.dp),
                )
                Text("stops $stops", modifier = Modifier.testTag("icon_stop_count"))
            }
        }
    }
}
