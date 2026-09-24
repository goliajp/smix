package dev.smix.fixture

import android.os.Bundle
import android.os.SystemClock
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The subjects `neverVisible` and a disappearing control are judged on.
 *
 * `Flash` does what a consumer's alert row does when it opens a
 * recording: the press returns, and a moment later a loading overlay
 * stands over the screen for 400 ms and goes. `Quiet` takes the same
 * time and shows nothing — the control that says a watch which never
 * saw anything was not simply blind. Both end by showing `done`, which
 * is what a flow waits for.
 *
 * `Reveal` shows a button that takes itself away after three seconds,
 * as a player's controls do after the picture is touched. It counts the
 * presses it received and how long after it appeared the press came, on
 * the app's own clock — the reading a check trusts, rather than smix's
 * account of having pressed it.
 */
class WatchActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var overlay by remember { mutableStateOf(false) }
            var done by remember { mutableStateOf(false) }
            var vanishing by remember { mutableStateOf(false) }
            var shownAt by remember { mutableLongStateOf(0L) }
            var presses by remember { mutableIntStateOf(0) }
            var latency by remember { mutableLongStateOf(-1L) }
            val scope = rememberCoroutineScope()
            Box(modifier = Modifier.semantics { testTagsAsResourceId = true }) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text("watch")
                    Button(
                        onClick = {
                            scope.launch {
                                delay(300)
                                overlay = true
                                delay(400)
                                overlay = false
                                done = true
                            }
                        },
                        modifier = Modifier.testTag("watch_flash"),
                    ) { Text("Flash") }
                    Button(
                        onClick = {
                            scope.launch {
                                delay(700)
                                done = true
                            }
                        },
                        modifier = Modifier.testTag("watch_quiet"),
                    ) { Text("Quiet") }
                    if (done) Text("done", modifier = Modifier.testTag("watch_done"))
                    Button(
                        onClick = {
                            vanishing = true
                            shownAt = SystemClock.uptimeMillis()
                            scope.launch {
                                delay(3000)
                                vanishing = false
                            }
                        },
                        modifier = Modifier.testTag("watch_reveal"),
                    ) { Text("Reveal") }
                    if (vanishing) {
                        Button(
                            onClick = {
                                presses += 1
                                latency = SystemClock.uptimeMillis() - shownAt
                            },
                            modifier = Modifier.testTag("watch_vanishing"),
                        ) { Text("Tap me") }
                    }
                    Text("presses $presses", modifier = Modifier.testTag("watch_presses"))
                    Text("latency $latency", modifier = Modifier.testTag("watch_latency"))
                }
                if (overlay) {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(Color(0x99000000)),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text("loading", color = Color.White, modifier = Modifier.testTag("watch_overlay"))
                    }
                }
            }
        }
    }
}
