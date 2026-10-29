package dev.smix.fixture

import android.content.Context
import android.view.KeyEvent
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputConnectionWrapper
import android.widget.EditText

/// A field that turns typed Latin letters into full-width ones, the way a
/// Chinese, Japanese or Korean input mode composes typed keys into its own
/// script. A Samsung keyboard in Chinese mode turned `probe@example.test`
/// into `probe@买票了.`; the emulator's keyboard has no such mode that a
/// gate can switch on.
///
/// Only typing goes through it — key events, and text an input method
/// commits. Text set on the field as a whole (`ACTION_SET_TEXT`) arrives
/// as it was set, as it does past a real keyboard.
class ComposingField(context: Context) : EditText(context) {
    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection? {
        val base = super.onCreateInputConnection(outAttrs) ?: return null
        return object : InputConnectionWrapper(base, true) {
            override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean =
                super.commitText(text?.let(::compose), newCursorPosition)

            override fun setComposingText(text: CharSequence?, newCursorPosition: Int): Boolean =
                super.setComposingText(text?.let(::compose), newCursorPosition)

            override fun sendKeyEvent(event: KeyEvent): Boolean =
                composedKey(event) ?: super.sendKeyEvent(event)
        }
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean =
        composedKey(event) ?: super.dispatchKeyEvent(event)

    /// True when `event` was a letter and was put in composed; null when
    /// it is not this field's to change.
    private fun composedKey(event: KeyEvent): Boolean? {
        val c = event.unicodeChar.toChar()
        if (c !in 'a'..'z') return null
        if (event.action == KeyEvent.ACTION_DOWN) {
            val at = selectionEnd.coerceAtLeast(0)
            editableText.insert(at, compose(c.toString()))
        }
        return true
    }

    private fun compose(text: CharSequence): String =
        text.map { if (it in 'a'..'z') (it - 'a' + 0xFF41).toChar() else it }.joinToString("")
}
