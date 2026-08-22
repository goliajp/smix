package dev.smix.fixture

import android.app.Activity
import android.os.Bundle
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent

/// The same three controls as MainActivity, in Compose.
///
/// Not a duplicate for its own sake. A Compose text field and an
/// `android.widget.EditText` present themselves to accessibility
/// differently, and 6.4.0 shipped two predicates that were true of one
/// and false of the other: the field's text reaches the accessibility
/// node asynchronously, and the node does not necessarily report
/// itself as editable. Both read as failures on a fill that worked.
///
/// The gates drove the View fixture only, so neither could be seen
/// here — the subject could not exhibit the defect. A consumer's whole
/// Android suite went red on the release that was supposed to fix it.
///
/// `testTagsAsResourceId` is what makes a testTag arrive as
/// `viewIdResourceName`, which is what smix reads as an identifier. It
/// is how Compose apps are addressed, so it is how this one is.
class ComposeActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var typed by remember { mutableStateOf("") }
            var secret by remember { mutableStateOf("") }
            var wrapped by remember { mutableStateOf("") }
            var submitted by remember { mutableStateOf("nothing submitted") }
            Column(
                modifier = Modifier
                    .padding(16.dp)
                    .semantics { testTagsAsResourceId = true },
            ) {
                Text("smix fixture (compose)")
                TextField(
                    value = typed,
                    onValueChange = { typed = it },
                    modifier = Modifier.testTag("compose_input"),
                )
                // A masked field. Its accessibility node reports one
                // bullet per character and never the characters, so a
                // predicate that compares the node's text with what was
                // typed is asking a question this field cannot answer:
                // ten characters read back as ten dots and the fill is
                // judged to have landed nowhere. That verdict blocked a
                // consumer's entire Android suite, because the first
                // flow of twenty signs in.
                TextField(
                    value = secret,
                    onValueChange = { secret = it },
                    visualTransformation = PasswordVisualTransformation(),
                    modifier = Modifier.testTag("compose_password"),
                )
                // A field named by its wrapper, not by itself. A
                // consumer's Kotlin views carry a contentDescription on
                // the surrounding layout and nothing on the input, so a
                // selector resolves to the wrapper and the tap lands at
                // its centre — which need not be inside the field that
                // then takes focus. 6.7.1 required the focused field to
                // contain the tap point and refused every fill in that
                // app.
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 24.dp)
                        .testTag("compose_wrapper"),
                ) {
                    TextField(
                        value = wrapped,
                        onValueChange = { wrapped = it },
                        modifier = Modifier.testTag("compose_wrapped_input"),
                    )
                }
                Button(
                    onClick = { submitted = typed },
                    modifier = Modifier.testTag("compose_submit"),
                ) { Text("Submit") }
                Text(submitted, modifier = Modifier.testTag("compose_result"))
            }
        }
    }
}
