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

    fun maskedRefusal(field: String): String =
        "input-text: the text has characters `input text` cannot type, so it would be " +
            "set on $field as a whole, and $field is masked and already holds text that " +
            "cannot be read back to keep. Nothing was entered; clear the field first."

    /// Give `node` the whole of `value`; whether the node took the action.
    fun set(node: AccessibilityNodeInfo, value: String): Boolean =
        node.performAction(
            AccessibilityNodeInfo.ACTION_SET_TEXT,
            Bundle().apply {
                putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, value)
            },
        )
}
