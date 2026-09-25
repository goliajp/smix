package dev.smix.fixture

import android.os.Bundle
import android.widget.Button
import android.widget.EditText
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView

/// The three ways the probe's tree used to differ from the screen, each
/// with something on it that shows the difference.
///
/// Every one of them was reported by a consumer against their own app
/// (2026-09-22) and none of them could be reproduced here, because the
/// fixture had no `AndroidView` and no list long enough to compose a row
/// it does not show. `two-paths-agree` has asserted the right thing since
/// v10 — that the probe sees everything the accessibility path sees — and
/// passed the whole time, over a screen where the claim could not fail.
///
///  - **hosted View**: a native `ImageButton` inside an `AndroidView`,
///    which is the shape of the player chrome the consumer could not tap.
///    It has an id from `ids.xml`, so both readers can name it and the
///    gate can compare them.
///  - **never placed**: a `Layout` that measures its child and does not
///    place it. That is the state a `LazyColumn` leaves a prefetched row
///    in, reached deliberately instead of by getting the timing right.
///  - **half clipped**: a scrolling column, started part-scrolled, whose
///    first row is cut by the viewport's top edge. Its layout rectangle
///    and the part of it on screen are different numbers, which is the
///    whole of the second defect.
class InteropActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            Column(modifier = Modifier.semantics { testTagsAsResourceId = true }) {
                Text("interop screen", modifier = Modifier.testTag("interop_title"))

                AndroidView(
                    modifier = Modifier.fillMaxWidth().testTag("interop_host"),
                    factory = { ctx ->
                        val row = LinearLayout(ctx).apply {
                            orientation = LinearLayout.HORIZONTAL
                            addView(
                                ImageButton(ctx).apply {
                                    id = R.id.fixture_interop_button
                                    contentDescription = "Full screen"
                                    setImageResource(android.R.drawable.ic_menu_crop)
                                },
                            )
                            addView(
                                TextView(ctx).apply {
                                    id = R.id.fixture_interop_label
                                    text = "hosted label"
                                },
                            )
                        }
                        LinearLayout(ctx).apply {
                            orientation = LinearLayout.VERTICAL
                            addView(row)
                            // A hosted field and a hosted control that is
                            // off: the probe wrote `focused = false` and
                            // `enabled = true` for every View it walked,
                            // and `inputText` after a `tapOn` on a View
                            // field then found nothing focused, every run.
                            addView(
                                EditText(ctx).apply {
                                    id = R.id.fixture_interop_input
                                    hint = "hosted field"
                                },
                            )
                            addView(
                                Button(ctx).apply {
                                    id = R.id.fixture_interop_disabled
                                    text = "hosted off"
                                    isEnabled = false
                                },
                            )
                        }
                    },
                )

                // Measured, never placed. `layout(0, 0) {}` is the whole
                // of it: the child exists, carries semantics, and has no
                // position — so anything reporting where it is, is
                // reporting somewhere it has never been.
                Layout(
                    content = {
                        Text("never placed", modifier = Modifier.testTag("interop_unplaced"))
                    },
                ) { measurables, constraints ->
                    measurables.forEach { it.measure(constraints) }
                    layout(0, 0) {}
                }

                // Started 60 pixels down, so the first row is cut by the
                // top edge and its two rectangles differ by that much.
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(220.dp)
                        .verticalScroll(rememberScrollState(initial = 60)),
                ) {
                    repeat(8) { i ->
                        Text(
                            "clipped row $i",
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(100.dp)
                                .testTag("interop_clipped_row_$i"),
                        )
                    }
                }
            }
        }
    }
}
