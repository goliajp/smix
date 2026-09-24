package dev.smix.fixture

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
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

/**
 * An element that can be made to move by a known amount, and a press that
 * moves nothing — the subject `rememberBounds` / `assertBoundsUnchanged`
 * are judged on. "Moved" and "did not move" both have to be on the
 * screen, or a comparison that always says "unchanged" passes.
 *
 * The spacer is there, zero tall, before the press too, so the column's
 * layout around it is the same in both states and the move is exactly the
 * 8 dp a consumer's tour buttons were built not to cause.
 */
class BoundsActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var shifted by remember { mutableStateOf(false) }
            Column(
                modifier = Modifier
                    .padding(16.dp)
                    .semantics { testTagsAsResourceId = true },
            ) {
                Text("bounds")
                Spacer(modifier = Modifier.height(if (shifted) 8.dp else 0.dp))
                Text("anchor", modifier = Modifier.testTag("bounds_target"))
                Row {
                    Button(onClick = { shifted = true }, modifier = Modifier.testTag("bounds_move")) {
                        Text("Move")
                    }
                    Button(onClick = {}, modifier = Modifier.testTag("bounds_stay")) {
                        Text("Stay")
                    }
                }
            }
        }
    }
}
