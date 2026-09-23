package dev.smix.fixture

import android.app.AlertDialog
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp

/// A Compose screen that asks for confirmation through the platform's
/// own `AlertDialog`, and counts the confirmations it actually received.
///
/// The subject of a consumer's report: a flow tapped a dialog's confirm
/// button, smix printed `tapped`, and the app never heard it — the touch
/// had landed below the dialog and dismissed it. From outside, a dialog
/// that was confirmed and a dialog that was dismissed are both gone, so
/// the only reading that tells them apart is the app's own. That is the
/// count shown here, incremented by the button's listener and by nothing
/// else.
///
/// Compose around a native dialog on purpose: the semantics probe only
/// sees Compose roots, so a dialog built from Views was absent from the
/// tree a flow reads — the same app, the same dialog, visible to one of
/// smix's two readers and not the other.
class NativeDialogActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var confirmed by remember { mutableIntStateOf(0) }
            Column(
                modifier = Modifier
                    .padding(16.dp)
                    .semantics { testTagsAsResourceId = true },
            ) {
                Text("native dialog")
                Button(
                    onClick = {
                        AlertDialog.Builder(this@NativeDialogActivity)
                            .setTitle("Delete this arrangement?")
                            .setPositiveButton("Delete") { _, _ -> confirmed += 1 }
                            .setNegativeButton("Cancel", null)
                            .show()
                    },
                    modifier = Modifier.testTag("native_dialog_open"),
                ) { Text("Ask") }
                Text("confirmed $confirmed", modifier = Modifier.testTag("native_dialog_count"))
            }
        }
    }
}
