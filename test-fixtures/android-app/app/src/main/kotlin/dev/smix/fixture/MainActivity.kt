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

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(this@MainActivity).apply { text = "smix fixture" })
            addView(input)
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
