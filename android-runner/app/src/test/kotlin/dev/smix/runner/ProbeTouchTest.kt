package dev.smix.runner

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class ProbeTouchTest {
    private val payload = """{"screen":[1080,2340],"roots":[{"id":1,"bounds":[0,0,1080,2340],"children":[
        {"id":2,"resourceId":"delegate_row","bounds":[0,275,1080,561],"children":[
          {"id":3,"resourceId":"delegate_check","bounds":[110,385,176,451],"children":[]}]},
        {"id":4,"resourceId":"delegate_plain","bounds":[0,561,66,627],"children":[]}]}]}"""

    private fun node(json: String, vararg path: Int): JSONObject {
        var n = JSONObject(json).getJSONArray("roots").getJSONObject(0)
        for (i in path) n = n.getJSONArray("children").getJSONObject(i)
        return n
    }

    @Test
    fun anAreaGoesOnTheNodeWithItsIdAndBounds() {
        val area = ProbeTouch.Area("delegate_check", intArrayOf(110, 385, 176, 451), intArrayOf(44, 319, 242, 517))
        val out = ProbeTouch.fill(payload, listOf(area))
        assertEquals("[44,319,242,517]", node(out, 0, 0).getJSONArray("touchBounds").toString())
        assertFalse(node(out, 0).has("touchBounds"))
        assertFalse(node(out, 1).has("touchBounds"))
    }

    @Test
    fun anAreaWhoseBoundsDifferGoesNowhere() {
        val moved = ProbeTouch.Area("delegate_check", intArrayOf(110, 400, 176, 466), intArrayOf(44, 334, 242, 532))
        assertFalse(node(ProbeTouch.fill(payload, listOf(moved)), 0, 0).has("touchBounds"))
        assertEquals(payload, ProbeTouch.fill(payload, emptyList()))
    }
}
