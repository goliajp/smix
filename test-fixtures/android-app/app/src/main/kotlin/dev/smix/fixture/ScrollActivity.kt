package dev.smix.fixture

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
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
class ScrollActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var tapped by remember { mutableStateOf("nothing tapped") }
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
                        .verticalScroll(rememberScrollState()),
                ) {
                    repeat(40) { i ->
                        Button(
                            onClick = { tapped = "tapped $i" },
                            modifier = Modifier
                                .fillMaxWidth()
                                .testTag("scroll_row_$i"),
                        ) { Text("scroll row $i") }
                    }
                }
            }
        }
    }
}
