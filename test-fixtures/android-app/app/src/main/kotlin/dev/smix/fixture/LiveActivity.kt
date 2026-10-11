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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

/**
 * A player screen whose tree never holds still, as a consumer's camera page
 * does.
 *
 * A detection box moves every frame, so the semantics tree is never the
 * same twice, in a full-size layer composed over the pill that does
 * nothing with a touch. The screen opens live, goes to playback
 * two seconds in — which shows a "GO LIVE" pill — and the pill takes it
 * back to live and goes away. `live-presses` counts the presses the app
 * received.
 */
class LiveActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var playback by remember { mutableStateOf(false) }
            var presses by remember { mutableIntStateOf(0) }
            var x by remember { mutableIntStateOf(0) }
            LaunchedEffect(Unit) { while (true) withFrameMillis { x = ((it / 8) % 200).toInt() } }
            LaunchedEffect(Unit) {
                delay(2000)
                playback = true
            }
            Column(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }.padding(16.dp)) {
                Text(if (playback) "playback" else "live", Modifier.testTag("live-mode"))
                Text("live-presses: $presses", Modifier.testTag("live-presses"))
                Box(Modifier.fillMaxWidth().height(240.dp).background(Color.DarkGray)) {
                    if (playback) {
                        Text(
                            "GO LIVE",
                            Modifier
                                .offset(x = 16.dp, y = 180.dp)
                                .background(Color.White)
                                .testTag("live-go")
                                .clickable { presses += 1; playback = false }
                                .padding(12.dp),
                        )
                    }
                    // Composed after the pill and the size of the player, as
                    // the consumer's detection layer is: drawn over it, and
                    // nothing in it does anything with a touch.
                    Box(Modifier.fillMaxSize().testTag("live-boxes")) {
                        Box(Modifier.offset(x = x.dp, y = 40.dp).size(60.dp).background(Color.Green).testTag("live-box"))
                    }
                }
            }
        }
    }
}
