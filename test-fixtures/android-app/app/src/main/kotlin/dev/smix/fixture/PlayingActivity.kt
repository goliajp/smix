package dev.smix.fixture

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp

/**
 * A picture redrawn every frame across the top half — a live video, as a
 * consumer's home screen has one — and below it a still half with a button
 * that counts its presses in `playing-count`.
 */
class PlayingActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var presses by remember { mutableIntStateOf(0) }
            var frame by remember { mutableLongStateOf(0L) }
            LaunchedEffect(Unit) { while (true) withFrameMillis { frame += 1 } }
            // a different level each frame, far apart, as a video's pixels are
            val level = (30 + (frame * 97) % 211) / 255f
            Column(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }.testTag("playing-screen")) {
                Box(Modifier.fillMaxWidth().weight(0.45f).background(Color(level, level, level)).testTag("playing-picture"))
                Box(Modifier.fillMaxWidth().weight(0.55f), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Box(
                            Modifier.size(160.dp, 64.dp).background(Color.LightGray)
                                .clickable { presses += 1 }.testTag("playing-press"),
                            contentAlignment = Alignment.Center,
                        ) { Text("Press") }
                        Text("pressed $presses", Modifier.padding(top = 24.dp).testTag("playing-count"))
                    }
                }
            }
        }
    }
}
