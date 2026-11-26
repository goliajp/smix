package dev.smix.runner

import org.json.JSONArray
import org.json.JSONObject

/// The probe's tree with each `TouchDelegate` area put on the View it
/// goes to.
///
/// The probe runs inside the app, where the View a delegate's region goes
/// to cannot be read without hidden APIs. The accessibility tree names it,
/// so the runner reads the areas there and puts each one on the probe
/// node with the same resource id and the same bounds. A node the probe
/// placed elsewhere gets none: an area put on the wrong control would be
/// worse than an area left out.
object ProbeTouch {
    /// One delegated area: the View's short resource id, its bounds and
    /// the area, both `[left, top, right, bottom]` in screen pixels.
    class Area(val id: String, val bounds: IntArray, val touch: IntArray)

    fun fill(payload: String, areas: List<Area>): String {
        if (areas.isEmpty()) return payload
        val trimmed = payload.trim()
        if (!trimmed.startsWith("{")) return payload
        val o = JSONObject(trimmed)
        o.optJSONArray("roots")?.let { fillArray(it, areas) }
        return o.toString()
    }

    private fun fillArray(arr: JSONArray, areas: List<Area>) {
        for (i in 0 until arr.length()) arr.optJSONObject(i)?.let { fillNode(it, areas) }
    }

    private fun fillNode(node: JSONObject, areas: List<Area>) {
        val id = node.optString("resourceId", "")
        val b = node.optJSONArray("bounds")
        if (id.isNotEmpty() && b != null && b.length() == 4) {
            val bounds = IntArray(4) { b.optInt(it) }
            areas.firstOrNull { it.id == id && it.bounds.contentEquals(bounds) }?.let { a ->
                node.put("touchBounds", JSONArray(a.touch.toList()))
            }
        }
        node.optJSONArray("children")?.let { fillArray(it, areas) }
    }
}
