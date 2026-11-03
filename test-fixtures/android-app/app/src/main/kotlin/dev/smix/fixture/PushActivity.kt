package dev.smix.fixture

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.widget.TextView

/// What the last tapped notification opened, read from the app's side.
class PushActivity : Activity() {
    private lateinit var opened: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        opened = TextView(this).apply { id = R.id.push_opened }
        setContentView(opened)
        show(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        show(intent)
    }

    private fun show(intent: Intent) {
        opened.text = intent.getStringExtra(OPENED) ?: "nothing opened"
    }

    companion object {
        const val OPENED = "opened"
    }
}
