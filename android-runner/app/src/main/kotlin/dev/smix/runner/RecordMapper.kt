// AccessibilityEvent -> IRAction, the Android capture leg (v2.10-C2).
//
// The Android recorder taps UiAutomation's AccessibilityEvent stream (the
// structural equivalent of iOS EventRecorder's AX-notification swizzle) and
// maps the semantic events to the platform-neutral IRAction the generators
// already consume — TYPE_VIEW_CLICKED -> tap, TYPE_VIEW_TEXT_CHANGED ->
// fill/clear. The runner emits IRAction JSON directly (not the iOS-only
// RecordedEvent wire, which feeds capsule reconcile, not the generator).
//
// Only the C1 portable set (Tap/Fill/Clear) is captured; Swipe/PressKey/etc.
// have no first-class AccessibilityEvent and are a recorded gap, not faked.
//
// Pure mapping — no Android dependency here beyond the event-type ints, so it
// is JVM-unit-tested without a device.

package dev.smix.runner

import org.json.JSONObject

/** The fields lifted from an AccessibilityEvent, decoupled from Android. */
data class CapturedAxEvent(
    val type: Int,
    val viewId: String?,
    val text: String?,
    val beforeText: String?,
    val eventTimeMs: Long,
)

/** IRAction JSON strings, plus how many events had no mappable target. */
data class MapResult(val actions: List<String>, val unmapped: Int)

object RecordMapper {
    // android.view.accessibility.AccessibilityEvent type constants.
    const val TYPE_VIEW_CLICKED = 1
    const val TYPE_VIEW_TEXT_CHANGED = 16

    /**
     * Map a captured event stream to IRAction JSON. Consecutive text changes
     * on the same field are one fill/clear (keystrokes debounce to the final
     * text). An event with no accessibility id is dropped and counted, never
     * given a fabricated selector.
     */
    fun map(rawEvents: List<CapturedAxEvent>): MapResult {
        // Keep only the mapping types first. On a real device each keystroke's
        // TYPE_VIEW_TEXT_CHANGED is interleaved with droppable noise
        // (TYPE_VIEW_TEXT_SELECTION_CHANGED etc.); filtering it out makes a
        // field's text-change run contiguous so it coalesces, while a real
        // CLICKED between two runs still separates them.
        val events = rawEvents.filter {
            it.type == TYPE_VIEW_CLICKED || it.type == TYPE_VIEW_TEXT_CHANGED
        }
        val actions = mutableListOf<String>()
        var unmapped = 0
        var i = 0
        while (i < events.size) {
            val e = events[i]
            when (e.type) {
                TYPE_VIEW_CLICKED -> {
                    val id = shortId(e.viewId)
                    if (id == null) unmapped++ else actions.add(tapJson(id, e.eventTimeMs))
                    i++
                }
                TYPE_VIEW_TEXT_CHANGED -> {
                    // Coalesce a maximal run of same-field text changes to its
                    // last event — the final text is what was typed.
                    var j = i
                    while (j + 1 < events.size &&
                        events[j + 1].type == TYPE_VIEW_TEXT_CHANGED &&
                        events[j + 1].viewId == e.viewId
                    ) {
                        j++
                    }
                    val last = events[j]
                    val id = shortId(last.viewId)
                    if (id == null) {
                        unmapped++
                    } else {
                        val text = last.text ?: ""
                        if (text.isEmpty() && !last.beforeText.isNullOrEmpty()) {
                            actions.add(clearJson(id, last.eventTimeMs))
                        } else {
                            actions.add(fillJson(id, text, last.eventTimeMs))
                        }
                    }
                    i = j + 1
                }
                else -> i++ // scroll and the rest are a recorded gap
            }
        }
        return MapResult(actions, unmapped)
    }

    /** `com.x:id/login_btn` -> `login_btn`; null / no `:id/` -> null (dropped). */
    private fun shortId(viewId: String?): String? {
        if (viewId.isNullOrEmpty()) return null
        val marker = ":id/"
        val at = viewId.indexOf(marker)
        val short = if (at >= 0) viewId.substring(at + marker.length) else viewId
        return short.ifEmpty { null }
    }

    private fun selector(id: String): JSONObject = JSONObject().put("id", id)

    private fun tapJson(id: String, ts: Long): String =
        JSONObject().put("kind", "tap").put("selector", selector(id))
            .put("timestampMs", ts).toString()

    private fun fillJson(id: String, text: String, ts: Long): String =
        JSONObject().put("kind", "fill").put("selector", selector(id))
            .put("text", text).put("timestampMs", ts).toString()

    private fun clearJson(id: String, ts: Long): String =
        JSONObject().put("kind", "clear").put("selector", selector(id))
            .put("timestampMs", ts).toString()
}
