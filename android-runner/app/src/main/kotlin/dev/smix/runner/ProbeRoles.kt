package dev.smix.runner

import org.json.JSONArray
import org.json.JSONObject

/**
 * The role a probe node has, spelled the way the accessibility wire spells it.
 *
 * `role:button` has to mean one thing whichever reader answered, and the
 * table that turns an Android class into a role already exists here
 * (`TreeWire.deriveRole`) because the accessibility path needs it. The
 * probe cannot call it — it is a separately published artifact that must
 * not depend on the runner — and copying the table into it would make two
 * that drift. So the probe reports the FACTS it has, the class and
 * Compose's own role, and the naming happens here, once, on the way past.
 *
 * Measured before this existed: every node the probe reported arrived with
 * `rawType` "Other" and no role at all, so a flow using `role:` matched
 * nothing whenever the app carried a probe — the same shape as the
 * interop hole, one field over.
 */
object ProbeRoles {
    /**
     * Compose's own role names, which are not smix's.
     *
     * `androidx.compose.ui.semantics.Role` prints `Button`, `Checkbox`,
     * `Switch`, `RadioButton`, `Tab`, `Image`, `DropdownList`,
     * `ValuePicker`. Left as data rather than lowercasing: `Checkbox`
     * would come out `checkbox` and the wire spells it `checkBox`.
     */
    private val COMPOSE_ROLES = mapOf(
        "Button" to "button",
        "Checkbox" to "checkBox",
        "Switch" to "switch",
        "RadioButton" to "radio",
        "Tab" to "tab",
        "Image" to "image",
        "DropdownList" to "picker",
        "ValuePicker" to "picker",
    )

    /** What to call this node, or null when neither fact names one. */
    fun roleOf(composeRole: String?, className: String?): String? {
        composeRole?.let { COMPOSE_ROLES[it]?.let { named -> return named } }
        return className?.let(TreeWire::deriveRole)
    }

    /**
     * The probe's tree with every node's role filled in.
     *
     * Returns the payload untouched when it is not the shape expected —
     * an older probe, or a failure the probe already described in its own
     * words. Rewriting is an improvement to the answer, and an improvement
     * that cannot be made is not a reason to lose the answer.
     */
    fun fill(payload: String): String {
        val trimmed = payload.trim()
        return when {
            trimmed.startsWith("[") -> JSONArray(trimmed).also { fillArray(it) }.toString()
            trimmed.startsWith("{") -> JSONObject(trimmed).also { o ->
                o.optJSONArray("roots")?.let { fillArray(it) }
            }.toString()
            else -> payload
        }
    }

    private fun fillArray(arr: JSONArray) {
        for (i in 0 until arr.length()) {
            arr.optJSONObject(i)?.let(::fillNode)
        }
    }

    private fun fillNode(node: JSONObject) {
        val named = roleOf(
            node.optString("role", "").ifEmpty { null },
            node.optString("className", "").ifEmpty { null },
        )
        if (named != null) {
            node.put("role", named)
        } else {
            // A Compose role this build has never heard of would otherwise
            // reach the host in Compose's spelling, where it deserialises
            // to nothing while looking like an answer.
            node.remove("role")
        }
        node.optJSONArray("children")?.let(::fillArray)
    }
}
