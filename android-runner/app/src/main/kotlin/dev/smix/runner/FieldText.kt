package dev.smix.runner

// What a text field holds, as opposed to what is written on it. Since
// API 26 an empty EditText reports its hint as the node's text and sets
// isShowingHintText; read as content, a search field's "Search…" was
// seven characters that /clear-text could not delete (AB1, 2026-09-25).
// The hint is not lost: the tree carries it as placeholderValue.
object FieldText {
    fun held(text: CharSequence?, showingHint: Boolean): String =
        if (showingHint) "" else text?.toString() ?: ""
}
