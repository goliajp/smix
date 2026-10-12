package dev.smix.fixture

import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.Canvas
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
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.geometry.Offset
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
/** Whether `p` is inside `poly`, by the ray crossing count. */
private fun inside(poly: List<Offset>, p: Offset): Boolean {
    var hit = false
    var j = poly.lastIndex
    for (i in poly.indices) {
        val a = poly[i]
        val b = poly[j]
        if ((a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x) hit = !hit
        j = i
    }
    return hit
}

class TouchActivity : ComponentActivity() {
    @OptIn(ExperimentalMaterial3Api::class)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var handle by remember { mutableIntStateOf(0) }
            var zone by remember { mutableIntStateOf(0) }
            var pills by remember { mutableIntStateOf(0) }
            var pills2 by remember { mutableIntStateOf(0) }
            var polyA by remember { mutableIntStateOf(0) }
            var polyB by remember { mutableIntStateOf(0) }
            var card by remember { mutableIntStateOf(0) }
            var sheet by remember { mutableIntStateOf(0) }
            var sheetOpen by remember { mutableStateOf(false) }
            var panel by remember { mutableIntStateOf(0) }
            var panelOpen by remember { mutableStateOf(false) }
            Box(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }) {
            Column(Modifier.fillMaxSize().padding(16.dp)) {
                Text(
                    "handle=$handle zone=$zone pill=$pills pill2=$pills2 card=$card sheet=$sheet panel=$panel polyA=$polyA polyB=$polyB",
                    Modifier.testTag("touch-counts"),
                )
                Box(Modifier.size(300.dp, 130.dp).testTag("touch-canvas")) {
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
                    modifier = Modifier.fillMaxWidth().height(120.dp).padding(top = 8.dp),
                )
                // Polygon zones the way a consumer's editor draws them: a
                // gesture layer with no semantics decides by the polygon, and
                // over it each zone is a box on its bounding rectangle with
                // only semantics { onClick }. The boxes overlap; the centre
                // of zone 0's box is inside zone 0's polygon and not zone 1's.
                val polygons = remember {
                    listOf(
                        listOf(Offset(0f, 0f), Offset(160f, 0f), Offset(160f, 60f), Offset(0f, 200f)),
                        listOf(Offset(300f, 0f), Offset(300f, 200f), Offset(60f, 200f), Offset(170f, 40f)),
                    )
                }
                // Polygon units are a 300-wide canvas, drawn at 240dp.
                val u = 0.8f
                Box(Modifier.padding(top = 8.dp).size(240.dp, 160.dp).testTag("touch-polys")) {
                    Canvas(Modifier.fillMaxSize()) {
                        val k = size.width / 300f
                        polygons.forEachIndexed { i, poly ->
                            val path = Path().apply {
                                moveTo(poly[0].x * k, poly[0].y * k)
                                poly.drop(1).forEach { lineTo(it.x * k, it.y * k) }
                                close()
                            }
                            drawPath(path, if (i == 0) Color(0x8081C784) else Color(0x80FFB74D))
                        }
                    }
                    Box(
                        Modifier.fillMaxSize().pointerInput(Unit) {
                            detectTapGestures { at ->
                                val p = Offset(at.x * 300f / size.width, at.y * 300f / size.width)
                                when (polygons.indexOfLast { inside(it, p) }) {
                                    0 -> polyA += 1
                                    1 -> polyB += 1
                                }
                            }
                        },
                    )
                    polygons.forEachIndexed { i, poly ->
                        val left = poly.minOf { it.x }
                        val top = poly.minOf { it.y }
                        Box(
                            Modifier.offset((left * u).dp, (top * u).dp)
                                .size(((poly.maxOf { it.x } - left) * u).dp, ((poly.maxOf { it.y } - top) * u).dp)
                                .testTag("touch-poly-$i")
                                .semantics {
                                    onClick {
                                        if (i == 0) polyA += 1 else polyB += 1
                                        true
                                    }
                                },
                        )
                    }
                }
                // A second hosted View drawn over the player, full size, its
                // View hidden — the shape of a consumer's detection overlay.
                // Compose shares a touch on a hosted View with what is under
                // it, so the control below still gets it.
                Box(Modifier.fillMaxWidth().height(100.dp).padding(top = 8.dp)) {
                    AndroidView(
                        factory = { ctx ->
                            FrameLayout(ctx).apply {
                                addView(
                                    TextView(ctx).apply {
                                        text = "GO LIVE 2"
                                        setPadding(48, 32, 48, 32)
                                        setOnClickListener { pills2 += 1 }
                                    },
                                    FrameLayout.LayoutParams(
                                        FrameLayout.LayoutParams.WRAP_CONTENT,
                                        FrameLayout.LayoutParams.WRAP_CONTENT,
                                        Gravity.BOTTOM or Gravity.START,
                                    ),
                                )
                            }
                        },
                        modifier = Modifier.fillMaxSize(),
                    )
                    AndroidView(
                        factory = { View(it).apply { visibility = View.INVISIBLE } },
                        modifier = Modifier.fillMaxSize(),
                    )
                }
                Box(
                    Modifier.fillMaxWidth().height(110.dp).padding(top = 8.dp)
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
