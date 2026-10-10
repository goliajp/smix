package dev.smix.fixture

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp

/// The same pages as [StackActivity], over an activity whose content is
/// Compose — the shape of a consumer's app, whose activity is one Compose
/// screen and whose pages are View dialogs over it.
class ComposeStackActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            Column(modifier = Modifier.padding(top = 80.dp).semantics { testTagsAsResourceId = true }) {
                Text("under the page")
                Button(onClick = { openStackPage(this@ComposeStackActivity) }, modifier = Modifier.testTag("compose_stack_open_page")) {
                    Text("Open page")
                }
            }
        }
    }
}
