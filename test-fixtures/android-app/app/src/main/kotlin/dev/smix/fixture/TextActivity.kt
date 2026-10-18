package dev.smix.fixture

import android.app.Activity
import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.view.ViewGroup
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView

/// Words that are not ASCII, in the shape a consumer's Wi-Fi list has.
///
/// Each row is a clickable layout whose `contentDescription` is the
/// network's name, with the name again as the text of a TextView inside
/// it. Every request to the runner carries such a name as JSON, and the
/// runner once decoded those bodies as US-ASCII: a row named
/// `カフェ☕ Wi-Fi` was found, and its tap refused because the name the
/// runner was told no longer matched the one on screen. An ASCII row
/// sits beside it, because a judgement only on the other one could not
/// tell a broken name from a broken tap.
///
/// The app's own counters say which row a tap reached, and the echo
/// line says what the field received.
class TextActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
        }
        root.addView(TextView(this).apply { text = "text screen" })
        for ((name, countId) in listOf(
            "カフェ☕ Wi-Fi" to R.id.fixture_text_cafe_taps,
            "Lobby Guest" to R.id.fixture_text_lobby_taps,
        )) {
            val taps = TextView(this).apply {
                id = countId
                text = "$name taps 0"
            }
            var count = 0
            val row = LinearLayout(this).apply {
                contentDescription = name
                isClickable = true
                setPadding(32, 40, 32, 40)
                addView(TextView(this@TextActivity).apply { text = name; textSize = 20f })
                setOnClickListener {
                    count += 1
                    taps.text = "$name taps $count"
                }
            }
            root.addView(row)
            root.addView(taps)
        }
        val echo = TextView(this).apply {
            id = R.id.fixture_text_echo
            text = "field holds nothing"
        }
        val field = EditText(this).apply {
            id = R.id.fixture_text_input
            hint = "type here"
            addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
                override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
                override fun afterTextChanged(s: Editable?) {
                    echo.text = "field holds $s"
                }
            })
        }
        root.addView(field)
        root.addView(echo)
        setContentView(root)
    }
}
