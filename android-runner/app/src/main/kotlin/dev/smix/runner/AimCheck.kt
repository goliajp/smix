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
    /// left: that is [covered]'s to answer.
    fun gone(target: Target, reading: HitChain.Reading): String? {
        val read = reading as? HitChain.Reading.Read ?: return null
        if (read.uncertain != null) return null
        if (read.chain.any { same(target, it) } || read.under.any { same(target, it) }) return null
        if (read.chain.isEmpty()) return "nothing"
        return read.chain.take(3).joinToString(" < ") { describe(it) }
    }

    /// What a touch at the point would reach instead of `target`, when the
    /// target is still there under it — a toast, a sheet's scrim drawn over
    /// it — or null when the target takes the touch, has gone (see [gone]),
    /// or the reading cannot say.
    fun covered(target: Target, reading: HitChain.Reading): String? {
        val read = reading as? HitChain.Reading.Read ?: return null
        if (read.uncertain != null || read.chain.any { same(target, it) }) return null
        if (read.under.none { same(target, it) }) return null
        return read.chain.take(3).joinToString(" < ") { describe(it) }.ifEmpty { "nothing" }
    }

    /// Whether a touch read as `reading` reaches `target`.
    fun reaches(target: Target, reading: HitChain.Reading): Boolean {
        val read = reading as? HitChain.Reading.Read ?: return false
        return read.uncertain == null && read.chain.any { same(target, it) }
    }

    /// Points inside `frame`, as much of it as is on a `w` x `h` display, to
    /// try a touch at: the centres of the cells of a 5 x 5 grid over it,
    /// nearest its centre first. At most [MAX_TRIES].
    fun candidates(frame: HitChain.Box, w: Int, h: Int): List<Pair<Int, Int>> {
        val l = maxOf(frame.left, 0)
        val t = maxOf(frame.top, 0)
        val r = minOf(frame.right, w)
        val b = minOf(frame.bottom, h)
        if (r - l < 2 || b - t < 2) return emptyList()
        val cx = (l + r) / 2.0
        val cy = (t + b) / 2.0
        val points = (0 until 5).flatMap { i ->
            (0 until 5).map { j ->
                Pair(l + ((r - l) * (i + 0.5) / 5).toInt(), t + ((b - t) * (j + 0.5) / 5).toInt())
            }
        }
        return points.distinct()
            .sortedBy { (x, y) -> (x - cx) * (x - cx) + (y - cy) * (y - cy) }
            .take(MAX_TRIES)
    }

    /// Points read before a touch gives up on finding a part of a covered
    /// target that nothing is over: each is a reading of what is under it.
    const val MAX_TRIES = 12

    /// Where a touch aimed at `target` goes in, decided the moment before it
    /// is sent: at `(px, py)` when the target takes it there (or nothing was
    /// aimed at, or the reading cannot say); at the first point inside the
    /// target that it takes, when something drawn over it covers the aimed
    /// point; and nowhere — the refusal — when the target has gone, or every
    /// part tried is covered. `read` reads what is under a point.
    fun place(
        target: Target?,
        px: Int,
        py: Int,
        w: Int,
        h: Int,
        read: (Int, Int) -> HitChain.Reading,
    ): Placement {
        val here = read(px, py)
        if (target == null) return Placement.Touch(px, py, here)
        gone(target, here)?.let { return Placement.Refuse(goneBody(it, target)) }
        val cover = covered(target, here) ?: return Placement.Touch(px, py, here)
        for ((x, y) in candidates(target.frame, w, h)) {
            val there = read(x, y)
            if (reaches(target, there)) return Placement.Touch(x, y, there)
        }
        return Placement.Refuse(coveredBody(cover, target))
    }

    sealed class Placement {
        data class Touch(val x: Int, val y: Int, val reading: HitChain.Reading) : Placement()
        data class Refuse(val body: String) : Placement()
    }

    /// The refusal for a target every part of which something drawn over it
    /// takes: what that is, after what was looked for.
    fun coveredBody(saw: String, target: Target): String =
        JSONObject()
            .put("error", "target_covered_before_touch")
            .put("saw", "$saw; it is drawn over ${describe(HitChain.Entry(target.identifier, target.label, target.frame))}")
            .toString()

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
