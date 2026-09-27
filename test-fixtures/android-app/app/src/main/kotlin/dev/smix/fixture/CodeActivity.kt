package dev.smix.fixture

import android.app.Activity
import android.content.Context
import android.os.Bundle
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.InputMethodManager
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView

/// A reset form in the shape a consumer's has: an address field, then a
/// numeric one-time-code field that submits itself once it is full.
///
/// Submitting puts the keyboard away and takes focus off the field, so
/// a read of "the focused field" after the last digit finds nothing —
/// the state a readback that asks for focus got wrong. The second code
/// field goes further and leaves the screen when it is full, so a
/// readback of that node has nothing to read at all.
///
/// The code field asks for a number pad, whose first key sits where a
/// tap aimed by the wrong rule has landed on a consumer's device.
class CodeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val result = TextView(this).apply {
            id = R.id.fixture_code_result
            text = "nothing submitted"
        }
        val email = EditText(this).apply {
            id = R.id.fixture_code_email
            hint = "address"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS
        }
        val code = numericField(R.id.fixture_code, "six digits")
        val leaving = numericField(R.id.fixture_code_leaving, "four digits, then gone")

        code.addTextChangedListener(whenFull(6) { value ->
            result.text = "code $value"
            code.clearFocus()
            hideKeyboard(code)
        })
        leaving.addTextChangedListener(whenFull(4) { value ->
            result.text = "leaving $value"
            hideKeyboard(leaving)
            leaving.visibility = View.GONE
        })

        // Holds the main thread, the way an app does straight after a
        // submit: a key sent meanwhile is delivered, late, and whether it
        // was "injected" depends on how long the injection waited.
        val busy = Button(this).apply {
            id = R.id.fixture_code_busy
            text = "Busy for four seconds"
            setOnClickListener { Thread.sleep(BUSY_MS) }
        }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(this@CodeActivity).apply { text = "reset" })
            addView(email)
            addView(code)
            addView(leaving)
            addView(result)
            addView(busy)
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
        }
        setContentView(root)
    }

    private fun numericField(viewId: Int, hintText: String) = EditText(this).apply {
        id = viewId
        hint = hintText
        inputType = InputType.TYPE_CLASS_NUMBER
    }

    private fun whenFull(length: Int, submit: (String) -> Unit) = object : TextWatcher {
        override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
        override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) = Unit
        override fun afterTextChanged(s: Editable?) {
            val value = s?.toString().orEmpty()
            if (value.length == length) submit(value)
        }
    }

    private fun hideKeyboard(field: View) {
        (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager)
            .hideSoftInputFromWindow(field.windowToken, 0)
    }

    private companion object {
        const val BUSY_MS = 4_000L
    }
}
