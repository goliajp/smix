package dev.smix.runner

import org.json.JSONObject

/// Whether the element a touch is aimed at is still under the point, read
/// the moment before the touch goes in.
///
/// A consumer's player took its "go live" pill away by itself between the
/// host's aim and the touch: the chain read before the touch no longer had
/// it, the touch went in anyway and reached the overlay under it, and the
/// host reported the tap missed after the fact. Told what was aimed at, the
/// runner does not send that touch.
///
/// The same element by the host's own rule, so a touch is held back exactly
/// when the host would have judged it missed: by identifier when both carry
/// one, else by label when both do, else — both unnamed — by frame.
object AimCheck {
    data class Target(val identifier: String, val label: String, val frame: HitChain.Box)

    /// The `aimedAt` a host sent, or null from a host that sent none.
    fun decode(payload: String): Target? {
        val o = try {
            JSONObject(payload).optJSONObject("aimedAt")
        } catch (_: org.json.JSONException) {
            null
        } ?: return null
        val f = o.optJSONObject("frame") ?: return null
        val x = f.optDouble("x")
        val y = f.optDouble("y")
        val w = f.optDouble("w")
        val h = f.optDouble("h")
        if (listOf(x, y, w, h).any { it.isNaN() }) return null
        return Target(
            o.optString("identifier"),
            o.optString("label"),
            HitChain.Box(x.toInt(), y.toInt(), (x + w).toInt(), (y + h).toInt()),
        )
    }

    /// What is under the point instead of `target`, or null when the touch
    /// should go in: the target is there, or the reading cannot say — a
    /// reader that could not be read, or a View whose own code decides.
    ///
    /// A target still at the point under something drawn over it has not
    /// left: the touch goes in and the host judges it missed by what took
    /// it, as it did before the runner was told the aim.
    fun gone(target: Target, reading: HitChain.Reading): String? {
        val read = reading as? HitChain.Reading.Read ?: return null
        if (read.uncertain != null) return null
        if (read.chain.any { same(target, it) } || read.under.any { same(target, it) }) return null
        if (read.chain.isEmpty()) return "nothing"
        return read.chain.take(3).joinToString(" < ") { describe(it) }
    }

    private fun same(t: Target, e: HitChain.Entry): Boolean {
        if (t.identifier.isNotEmpty() && e.id.isNotEmpty()) return t.identifier == e.id
        if (t.label.isNotEmpty() && e.label.isNotEmpty()) return t.label == e.label
        if (t.identifier.isEmpty() && t.label.isEmpty() && e.id.isEmpty() && e.label.isEmpty()) {
            val a = t.frame
            val b = e.bounds
            return close(a.left, b.left) && close(a.top, b.top) && close(a.right, b.right) && close(a.bottom, b.bottom)
        }
        return false
    }

    private fun close(a: Int, b: Int) = kotlin.math.abs(a - b) <= 1

    private fun describe(e: HitChain.Entry): String = when {
        e.id.isNotEmpty() -> "id=${e.id}"
        e.label.isNotEmpty() -> "\"${e.label}\""
        else -> "an unnamed element at (${e.bounds.left},${e.bounds.top})"
    }

    /// The refusal, with what the runner was told to look for after what it
    /// found: when the two read the same, what differs is what reached it.
    fun goneBody(saw: String, target: Target): String =
        JSONObject()
            .put("error", "target_gone_before_touch")
            .put("saw", "$saw; it looked for ${describe(HitChain.Entry(target.identifier, target.label, target.frame))}")
            .toString()
}
