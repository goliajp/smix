// RecordMapper — AccessibilityEvent -> IRAction, JVM-unit-tested without a
// device. Asserts on parsed fields, not JSON string equality, since org.json
// key order is not guaranteed.

package dev.smix.runner

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RecordMapperTest {
    private fun click(viewId: String?, t: Long = 1) =
        CapturedAxEvent(RecordMapper.TYPE_VIEW_CLICKED, viewId, null, null, t)

    private fun textChange(viewId: String?, text: String?, before: String?, t: Long = 1) =
        CapturedAxEvent(RecordMapper.TYPE_VIEW_TEXT_CHANGED, viewId, text, before, t)

    private fun one(events: List<CapturedAxEvent>): JSONObject {
        val r = RecordMapper.map(events)
        assertEquals("expected exactly one action", 1, r.actions.size)
        return JSONObject(r.actions[0])
    }

    @Test
    fun clickedBecomesTapWithShortId() {
        val a = one(listOf(click("com.x:id/login_btn", 42)))
        assertEquals("tap", a.getString("kind"))
        assertEquals("login_btn", a.getJSONObject("selector").getString("id"))
        assertEquals(42L, a.getLong("timestampMs"))
    }

    @Test
    fun textChangeBecomesFill() {
        val a = one(listOf(textChange("com.x:id/email", "a@b.co", "")))
        assertEquals("fill", a.getString("kind"))
        assertEquals("email", a.getJSONObject("selector").getString("id"))
        assertEquals("a@b.co", a.getString("text"))
    }

    @Test
    fun consecutiveTextChangesCoalesceToFinalText() {
        val a = one(
            listOf(
                textChange("com.x:id/q", "h", ""),
                textChange("com.x:id/q", "he", "h"),
                textChange("com.x:id/q", "hel", "he"),
            ),
        )
        assertEquals("fill", a.getString("kind"))
        assertEquals("hel", a.getString("text"))
    }

    @Test
    fun textChangesInterleavedWithNoiseStillCoalesce() {
        // On a real device each keystroke's TEXT_CHANGED is separated by
        // TYPE_VIEW_TEXT_SELECTION_CHANGED (8192) noise; it must not break the
        // coalesce into one fill per keystroke.
        val a = one(
            listOf(
                textChange("com.x:id/q", "s", ""),
                CapturedAxEvent(8192, "com.x:id/q", null, null, 1),
                textChange("com.x:id/q", "sm", "s"),
                CapturedAxEvent(8192, "com.x:id/q", null, null, 1),
                textChange("com.x:id/q", "smix", "sm"),
            ),
        )
        assertEquals("fill", a.getString("kind"))
        assertEquals("smix", a.getString("text"))
    }

    @Test
    fun emptyTextAfterNonEmptyBecomesClear() {
        val a = one(listOf(textChange("com.x:id/q", "", "hel")))
        assertEquals("clear", a.getString("kind"))
        assertEquals("q", a.getJSONObject("selector").getString("id"))
    }

    @Test
    fun nullViewIdIsDroppedAndCounted() {
        val r = RecordMapper.map(listOf(click(null), click("com.x:id/ok")))
        assertEquals(1, r.actions.size)
        assertEquals(1, r.unmapped)
        assertEquals("ok", JSONObject(r.actions[0]).getJSONObject("selector").getString("id"))
    }

    @Test
    fun scrollAndOtherTypesAreDropped() {
        // TYPE_VIEW_SCROLLED (4096) and friends produce no IRAction.
        val r = RecordMapper.map(listOf(CapturedAxEvent(4096, "com.x:id/list", null, null, 1)))
        assertTrue(r.actions.isEmpty())
        assertEquals(0, r.unmapped) // a gap, not an unmapped-target miss
    }
}
