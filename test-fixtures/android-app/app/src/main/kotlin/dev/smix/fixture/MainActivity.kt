package dev.smix.fixture

import android.app.Activity
import android.os.Bundle
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView

/// The Android counterpart of `test-fixtures/demo-app`: an ordinary
/// app, built by hand rather than scaffolded so there is nothing in it
/// but the three views the gates address.
///
/// The result label stays empty until Submit is pressed, so asserting
/// on it distinguishes what the app received from what smix believes it
/// typed — a read of the field's own value would be measuring the same
/// path under test.
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val input = EditText(this).apply {
            id = R.id.fixture_input
            hint = "type here"
        }
        val result = TextView(this).apply {
            id = R.id.fixture_result
            text = "nothing submitted"
        }
        val submit = Button(this).apply {
            id = R.id.fixture_submit
            text = "Submit"
            setOnClickListener { result.text = input.text.toString() }
        }

        // A field named by the layout around it, with nothing on the
        // field itself. A consumer's hand-written Kotlin views are all
        // this shape: the contentDescription sits on the wrapper, so a
        // selector resolves to the wrapper and the tap lands at its
        // centre — which is not inside the input, because the wrapper
        // is taller. 6.7.1 required the focused field to contain the
        // tap point and refused every fill in that app.
        val wrappedInput = EditText(this).apply { hint = "wrapped" }
        val wrapper = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            contentDescription = "wrapped-field"
            // The field sits at the top and a tall block of help text
            // below it, so the wrapper's centre lands on the text and
            // not on the field. That is the geometry that matters: a
            // wrapper whose centre happens to fall inside its input
            // would pass whatever the rule is.
            addView(wrappedInput)
            addView(
                TextView(this@MainActivity).apply {
                    text = (1..8).joinToString("\n") { "help line $it" }
                },
            )
        }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(this@MainActivity).apply { text = "smix fixture" })
            addView(input)
            addView(wrapper)
            addView(submit)
            addView(result)
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
        }
        setContentView(root)
    }

    // The ids come from res/values/ids.xml so `viewIdResourceName` —
    // what smix reads as an element's identifier — carries a name
    // rather than a generated number.
}
