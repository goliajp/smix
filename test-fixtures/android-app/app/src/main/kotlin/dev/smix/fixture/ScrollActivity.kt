package dev.smix.fixture

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp

/// A long scrolling screen whose rows are ALL composed, so the whole
/// list is in the semantics tree at once and the off-screen rows carry
/// their layout coordinates.
///
/// That is the shape a consumer reported (a `verticalScroll` column of
/// settings rows). "The target is somewhere in the tree" was the old
/// stop condition, so the scroll returned before swiping at all and the
/// tap that followed aimed below the bottom edge of the screen. The
/// `LazyColumn` next door in `ComposeActivity` cannot show this — its
/// far rows are never composed, so they are absent rather than
/// misplaced.
///
/// The result label sits above the scrolling column and stays on
/// screen, so what a row's tap did can be read without scrolling back.
/// Asserting on the row itself would be asserting on the thing under
/// test.
///
/// **The subject is computed, not hoped for.** What C5 measures is one
/// row cut by the bottom edge with its middle below it — the state
/// where the old stop rule returned and the tap that followed missed.
/// With rows at a fixed pitch, whether such a row exists is decided by
/// `(screen bottom − the column's top) mod pitch`: only the first half
/// of that range has one, so roughly two screens in five had none. C5
/// went red for a day on a screen that simply had no subject on it, and
/// it read as a product defect (open-items O1).
///
/// So the column puts a computed spacer above its rows, sized to leave
/// exactly a quarter of a row showing at the bottom edge. Every screen
/// height then has the subject, and the row's middle is half a row
/// below the edge rather than by whatever margin the day allowed.
private val ROW_HEIGHT = 100.dp

class ScrollActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var tapped by remember { mutableStateOf("nothing tapped") }
            var spacerPx by remember { mutableIntStateOf(0) }
            val density = LocalDensity.current
            val screenPx = remember {
                getSystemService(WindowManager::class.java)
                    .currentWindowMetrics.bounds.height()
            }
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(8.dp)
                    .semantics { testTagsAsResourceId = true },
            ) {
                Text(tapped, modifier = Modifier.testTag("scroll_result"))
                // Full width, both the column and its rows: a swipe
                // travels down the middle of the screen, and a column
                // only as wide as its widest button is not under it —
                // the first version of this fixture scrolled for nobody.
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .weight(1f)
                        .onGloballyPositioned { coords ->
                            // The screen's bottom edge, not this
                            // column's: the rows carry on below the
                            // viewport with rectangles of their own,
                            // and the row being asked about is the one
                            // the SCREEN cuts.
                            val top = coords.positionInWindow().y.toInt()
                            val pitch = with(density) { ROW_HEIGHT.toPx() }.toInt()
                            val quarter = pitch / 4
                            val want = (((screenPx - top - quarter) % pitch) + pitch) % pitch
                            // Setting it every pass would relayout for
                            // ever; the column's top does not move, so
                            // one pass settles it.
                            if (want != spacerPx) spacerPx = want
                        }
                        .verticalScroll(rememberScrollState()),
                ) {
                    Spacer(Modifier.height(with(density) { spacerPx.toDp() }))
                    repeat(40) { i ->
                        Button(
                            onClick = { tapped = "tapped $i" },
                            modifier = Modifier
                                .fillMaxWidth()
                                // A fixed pitch, so the arithmetic above
                                // is about one number and not about
                                // whatever height a Button defaults to.
                                .height(ROW_HEIGHT)
                                .testTag("scroll_row_$i"),
                        ) { Text("scroll row $i") }
                    }
                }
            }
        }
    }
}
