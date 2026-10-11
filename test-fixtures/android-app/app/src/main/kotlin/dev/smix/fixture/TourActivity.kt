package dev.smix.fixture

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView

/**
 * A walkthrough over a screen that never stops laying out, as a
 * consumer's home screen does under its first-run tour.
 *
 * The content below has a bar whose width changes every frame (a live
 * timeline, a player's progress), so the app's main thread is always
 * part-way through a layout. Over it, in the same window, sits the tour:
 * a full-screen clickable scrim and a card anchored to `tour-target`
 * with a clickable "Next" that advances the step. The tour itself holds
 * still once placed. `tour-step` counts the presses the app received.
 */
class TourActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var step by remember { mutableIntStateOf(0) }
            var anchorY by remember { mutableStateOf<Float?>(null) }
            var tick by remember { mutableIntStateOf(0) }
            LaunchedEffect(Unit) {
                while (true) withFrameMillis { tick = (it / 16).toInt() % 300 }
            }
            val density = LocalDensity.current
            BoxWithConstraints(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }) {
                // Rebuilt every frame: a layout pass long enough to be in
                // the middle of when somebody else reads the tree.
                key(tick % 2) {
                    Column(Modifier.padding(16.dp)) {
                        Text("tour-step: $step", Modifier.testTag("tour-step"))
                        Box(Modifier.height(24.dp).width((tick + 1).dp).background(Color.Gray).testTag("tour-live"))
                        AndroidView(
                            factory = { android.view.SurfaceView(it) },
                            modifier = Modifier.fillMaxWidth().height(120.dp).testTag("tour-player"),
                        )
                        repeat(40) { i ->
                            Row(Modifier.fillMaxWidth().height(12.dp)) { Text("row $i"); Text("$tick") }
                        }
                        Text(
                            "the thing the tour points at",
                            Modifier.testTag("tour-target").onGloballyPositioned { anchorY = it.positionInRoot().y },
                        )
                    }
                }
                // Out of the part rebuilt every frame: a press and its
                // release have to reach the same node, for a person too.
                run {
                    val y = anchorY
                    if (step < 3 && y != null) Box(Modifier.fillMaxSize()) {
                        val advance = { step += 1 }
                        Box(
                            Modifier
                                .fillMaxSize()
                                .background(Color.Black.copy(alpha = 0.55f))
                                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = advance)
                                .testTag("tour-scrim"),
                        )
                        Column(
                            Modifier
                                .offset(x = 24.dp, y = with(density) { y.toDp() } + 48.dp)
                                .width(320.dp)
                                .background(Color.White)
                                .padding(16.dp),
                        ) {
                            Text("step ${step + 1} of 3")
                            Text("Next", Modifier.testTag("tour-advance").clickable(onClick = advance).padding(12.dp))
                        }
                    }
                }
            }
        }
    }
}
