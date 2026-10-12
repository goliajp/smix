package dev.smix.fixture

import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView

/**
 * Four places a touch goes somewhere a reader of the tree can get wrong,
 * each in the shape a consumer's app had it, each counting what it
 * received — so where a tap went is read from the app:
 *
 * - a vertex handle that drags with `pointerInput` and has no semantics
 *   action, over a clickable zone (`touch-handle` / `touch-zone`)
 * - a clickable View under a full-size View that is not clickable, both
 *   hosted in Compose, as a player's "GO LIVE" sits under its overlays
 *   (`GO LIVE`)
 * - a name label over a hosted View, inside a clickable card: the touch
 *   goes through the label, and the card opens (`touch-card-label`)
 * - a Material bottom sheet over its scrim (`touch-sheet-30`), and a
 *   panel placed below the short container that holds it, over a
 *   clickable scrim — the shape a consumer's sheet had (`touch-panel-30`)
 */
class TouchActivity : ComponentActivity() {
    @OptIn(ExperimentalMaterial3Api::class)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var handle by remember { mutableIntStateOf(0) }
            var zone by remember { mutableIntStateOf(0) }
            var pills by remember { mutableIntStateOf(0) }
            var card by remember { mutableIntStateOf(0) }
            var sheet by remember { mutableIntStateOf(0) }
            var sheetOpen by remember { mutableStateOf(false) }
            var panel by remember { mutableIntStateOf(0) }
            var panelOpen by remember { mutableStateOf(false) }
            Box(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }) {
            Column(Modifier.fillMaxSize().padding(16.dp)) {
                Text(
                    "handle=$handle zone=$zone pill=$pills card=$card sheet=$sheet panel=$panel",
                    Modifier.testTag("touch-counts"),
                )
                Box(Modifier.size(300.dp, 140.dp).testTag("touch-canvas")) {
                    Box(
                        Modifier.offset(60.dp, 40.dp).size(160.dp, 80.dp)
                            .background(Color(0xFF80DEEA))
                            .clickable { zone += 1 }
                            .testTag("touch-zone"),
                    )
                    Box(
                        Modifier.offset(40.dp, 20.dp).size(48.dp)
                            .background(Color(0xFFE57373))
                            .pointerInput(Unit) { detectTapGestures { handle += 1 } }
                            .testTag("touch-handle"),
                    )
                }
                AndroidView(
                    factory = { ctx ->
                        FrameLayout(ctx).apply {
                            addView(
                                // A flat control: a Material Button is raised in Z
                                // and drawn over its later siblings.
                                TextView(ctx).apply {
                                    text = "GO LIVE"
                                    setPadding(48, 32, 48, 32)
                                    setOnClickListener { pills += 1 }
                                },
                                FrameLayout.LayoutParams(
                                    FrameLayout.LayoutParams.WRAP_CONTENT,
                                    FrameLayout.LayoutParams.WRAP_CONTENT,
                                    Gravity.BOTTOM or Gravity.START,
                                ),
                            )
                            // over the button, full size, not clickable
                            addView(View(ctx), FrameLayout.LayoutParams(-1, -1))
                        }
                    },
                    modifier = Modifier.fillMaxWidth().height(160.dp).padding(top = 12.dp),
                )
                Box(
                    Modifier.fillMaxWidth().height(160.dp).padding(top = 12.dp)
                        .clickable { card += 1 }
                        .testTag("touch-card"),
                ) {
                    AndroidView(factory = { View(it) }, modifier = Modifier.fillMaxSize())
                    Text("Kitchen", Modifier.padding(12.dp).testTag("touch-card-label"))
                }
                Text(
                    "open the sheet",
                    Modifier.padding(top = 12.dp).clickable { sheetOpen = true }.padding(12.dp).testTag("touch-open-sheet"),
                )
                Text(
                    "open the panel",
                    Modifier.clickable { panelOpen = true }.padding(12.dp).testTag("touch-open-panel"),
                )
            }
            if (panelOpen) {
                Box(
                    Modifier.fillMaxSize()
                        .background(Color.Black.copy(alpha = 0.4f))
                        .clickable { panelOpen = false }
                        .semantics { contentDescription = "Close panel" },
                )
                // Short, and the panel it holds is placed below it. A node
                // of its own in the semantics tree, as a sheet's container is.
                Box(Modifier.fillMaxWidth().height(80.dp).semantics { isTraversalGroup = true }) {
                    Column(Modifier.offset(y = 420.dp).fillMaxWidth().background(Color.White)) {
                        Text(
                            "30 minutes",
                            Modifier.fillMaxWidth()
                                .clickable { panel += 1; panelOpen = false }
                                .padding(16.dp)
                                .testTag("touch-panel-30"),
                        )
                    }
                }
            }
            }
            if (sheetOpen) {
                ModalBottomSheet(onDismissRequest = { sheetOpen = false }) {
                    Column(Modifier.semantics { testTagsAsResourceId = true }) {
                        Text("Snooze notifications", Modifier.padding(16.dp))
                        for ((tag, words) in listOf("touch-sheet-30" to "30 minutes", "touch-sheet-60" to "1 hour")) {
                            Text(
                                words,
                                Modifier.fillMaxWidth()
                                    .clickable {
                                        if (tag == "touch-sheet-30") sheet += 1
                                        sheetOpen = false
                                    }
                                    .padding(16.dp)
                                    .testTag(tag),
                            )
                        }
                    }
                }
            }
        }
    }
}
