package dev.smix.probe

/**
 * One semantics node, flattened to the fields smix asks about.
 *
 * The split is deliberate: pulling this out of Compose needs a device, and
 * turning it into the wire needs nothing. Keeping the second half a pure
 * function is what lets the shape of the wire be judged without an
 * emulator — and the wire's shape is where the defects have been.
 *
 * `editableText` and `inputText` are both here because they differ, and
 * the first draft of this file assumed they did not. On a masked field
 * `editableText` reads back `•••••••••` — Compose applies the visual
 * transformation before it reaches semantics, so it is NOT the untouched
 * value, and a predicate comparing it with what was typed asks a question
 * the field cannot answer. That verdict blocked a consumer's entire
 * Android suite at 6.4.0, and reading semantics instead of accessibility
 * does not by itself fix it. `inputText` is the one that does: measured
 * on the fixture, `s3cret-99` typed in reads back as `s3cret-99` there and
 * as nine bullets in `editableText`.
 */
data class ProbeNode(
    val id: Int,
    val testTag: String?,
    val text: String?,
    val editableText: String?,
    val inputText: String?,
    val contentDescription: String?,
    val role: String?,
    val bounds: Bounds,
    val focused: Boolean,
    val enabled: Boolean,
    val actions: List<String>,
    val children: List<ProbeNode>,
)

data class Bounds(val left: Int, val top: Int, val right: Int, val bottom: Int)

/** The semantics tree as smix's wire spells it.
 *
 * Hand-rolled rather than a serialization library: this module is
 * `debugImplementation` in somebody else's app, and a dependency it brings
 * along is a dependency they did not choose. The shape is small and the
 * test above is what holds it.
 */
fun List<ProbeNode>.toWireJson(): String =
    joinToString(prefix = "[", separator = ",", postfix = "]") { it.toJson() }

private fun ProbeNode.toJson(): String = buildString {
    append("{\"id\":").append(id)
    appendField("testTag", testTag)
    appendField("text", text)
    appendField("editableText", editableText)
    appendField("inputText", inputText)
    appendField("contentDescription", contentDescription)
    appendField("role", role)
    append(",\"bounds\":[")
        .append(bounds.left).append(',').append(bounds.top).append(',')
        .append(bounds.right).append(',').append(bounds.bottom).append(']')
    append(",\"focused\":").append(focused)
    append(",\"enabled\":").append(enabled)
    append(",\"actions\":")
        .append(actions.joinToString(prefix = "[", separator = ",", postfix = "]") { it.quoted() })
    append(",\"children\":").append(children.toWireJson())
    append('}')
}

private fun StringBuilder.appendField(name: String, value: String?) {
    if (value != null) append(",\"").append(name).append("\":").append(value.quoted())
}

/** JSON string escaping. Control characters go out as \uXXXX. */
private fun String.quoted(): String = buildString(length + 2) {
    append('"')
    for (c in this@quoted) when {
        c == '"' -> append("\\\"")
        c == '\\' -> append("\\\\")
        c == '\n' -> append("\\n")
        c == '\r' -> append("\\r")
        c == '\t' -> append("\\t")
        c < ' ' -> append("\\u%04x".format(c.code))
        else -> append(c)
    }
    append('"')
}
