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
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

/**
 * A control that takes itself away and comes back, over a layer that also
 * takes a touch — the shape of a player's "go live" pill over its picture.
 * A touch aimed at the pill that arrives while it is away reaches the
 * picture; the counts say which got it.
 */
class BlinkActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var pill by remember { mutableIntStateOf(0) }
            var under by remember { mutableIntStateOf(0) }
            var shown by remember { mutableStateOf(true) }
            LaunchedEffect(Unit) {
                while (true) {
                    delay(700)
                    shown = !shown
                }
            }
            Column(Modifier.fillMaxSize().padding(16.dp).semantics { testTagsAsResourceId = true }) {
                Text("pill=$pill under=$under", Modifier.testTag("blink-counts"))
                Box(
                    Modifier.fillMaxWidth().height(300.dp)
                        .background(Color(0xFF263238))
                        .clickable { under += 1 }
                        .testTag("blink-under"),
                    contentAlignment = Alignment.Center,
                ) {
                    if (shown) {
                        Text(
                            "GO LIVE",
                            Modifier.background(Color(0xFFE53935))
                                .clickable { pill += 1 }
                                .padding(16.dp)
                                .testTag("blink-pill"),
                            color = Color.White,
                        )
                    }
                }
            }
        }
    }
}
