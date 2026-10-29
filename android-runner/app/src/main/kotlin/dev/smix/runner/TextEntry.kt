package dev.smix.runner

import android.os.Bundle
import android.view.accessibility.AccessibilityNodeInfo

/// Text that `input text` cannot type, set on the field instead.
///
/// `input text` types what the virtual keyboard's key map has keys for,
/// and throws on anything past ASCII (measured on API 36:
/// `NullPointerException` in `InputShellCommand.sendText`, exit 255). For
/// such text the focused field is given its whole new content with
/// `ACTION_SET_TEXT`: no key goes through the keyboard, and the app's own
/// text listeners hear the change as they do a typed one.
object TextEntry {
    /// Whether `input text` can type all of `text`.
    fun typable(text: String): Boolean = text.all { it.code < 0x80 }

    /// What the field should hold once `text` is entered after what it
    /// `held`, as `input text` would at a caret at the end; null when that
    /// cannot be told — a masked field that holds text reads as bullets,
    /// and setting bullets would replace what it held.
    fun target(held: String, text: String, masked: Boolean): String? =
        if (masked && held.isNotEmpty()) null else held + text

    fun maskedRefusal(field: String, asked: Boolean): String =
        "input-text: " +
            (if (asked) "the text was to be set" else "the text has characters `input text` cannot type, so it would be set") +
            " on $field as a whole, and $field is masked and already holds text that " +
            "cannot be read back to keep. Nothing was entered; clear the field first."

    /// What the field gained, when typing ASCII left characters past
    /// ASCII in it: no key `input text` sends makes one, so the keyboard
    /// on screen turned the keys into them — a Chinese, Japanese or
    /// Korean input mode composes Latin keys into its own script, and
    /// `probe@example.test` was measured landing as `probe@买票了.`.
    /// Null when that is not what happened.
    fun composedByKeyboard(before: String, held: String, text: String): String? {
        if (!typable(text)) return null
        val gained = if (held.startsWith(before)) held.substring(before.length) else held
        return gained.takeIf { g -> g.any { it.code >= 0x80 } }
    }

    /// The sentence that says so, naming the keyboard when it is known,
    /// and the way in that does not go through it.
    fun composedSentence(gained: String, keyboard: String?): String =
        " The field gained \"$gained\", which has characters no key `input text` sends " +
            "makes: the keyboard on screen" + (keyboard?.let { " ($it)" } ?: "") +
            " composed the typed keys into other text, as a Chinese, Japanese or Korean " +
            "input mode does, or the app rewrote what it was given. Typing again goes " +
            "through the same keyboard. To put the text in without it, set it: " +
            "`inputText: { text: ..., method: setText }` in a flow, `smix fill --set-text` " +
            "from the command line."

    fun setDidNotLand(field: String, took: Boolean, before: String, held: String, masked: Boolean): String =
        "input-text: set the text on $field with ACTION_SET_TEXT, and " +
            (if (took) "the node accepted the action" else "the node refused the action") +
            ", but the field " +
            (if (masked) "went from ${before.length} to ${held.length} characters" else "held \"$before\" and holds \"$held\"") +
            ". An app can rewrite what is set on it (a formatter or a length limit), and a " +
            "node that refuses the action is not one this way can fill."

    /// Give `node` the whole of `value`; whether the node took the action.
    fun set(node: AccessibilityNodeInfo, value: String): Boolean =
        node.performAction(
            AccessibilityNodeInfo.ACTION_SET_TEXT,
            Bundle().apply {
                putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, value)
            },
        )
}
