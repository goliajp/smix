package dev.smix.probe

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A node reports what it is and what shows, as two rectangles.
 *
 * One rectangle cannot answer both questions, and the host asks both.
 * "How much of this can be seen" is the whole of `scrollUntilVisible`'s
 * stop rule: it divides the part inside the frame by the part that could
 * ever be inside it. Hand it a rectangle already clipped to the frame and
 * the division is one over one — every row that shows a sliver reads as
 * fully visible, and the scroll stops with the tap target off screen.
 *
 * That is the defect a consumer reported as `CentroidOutOfFrame { ny:
 * 1.02 }`, fixed once host-side, and re-opened from the other end when
 * the probe began reporting the clipped rectangle as the only one.
 */
class TwoRectanglesTest {
    private fun row(bounds: Bounds, visible: Bounds) = ProbeNode(
        id = 7,
        testTag = "scroll_row_9",
        resourceId = null,
        text = "row 9",
        hint = null,
        editableText = null,
        inputText = null,
        contentDescription = null,
        role = null,
        className = null,
        bounds = bounds,
        visibleBounds = visible,
        focused = false,
        enabled = true,
        visible = true,
        actions = emptyList(),
        children = emptyList(),
    )

    @Test
    fun a_row_clipped_at_the_bottom_edge_reports_its_whole_height_and_the_part_that_shows() {
        // 275 tall, the last 60 of it on screen: the measurements taken
        // from the fixture's scroll list on emulator-5554.
        val node = row(Bounds(0, 2112, 1080, 2387), Bounds(0, 2112, 1080, 2172))
        val json = listOf(node).toWireJson()

        assertTrue(
            "the wire must carry the node's own rectangle: $json",
            json.contains("\"bounds\":[0,2112,1080,2387]"),
        )
        assertTrue(
            "the wire must carry the part that shows, separately: $json",
            json.contains("\"visibleBounds\":[0,2112,1080,2172]"),
        )
    }

    @Test
    fun a_node_clipped_away_entirely_keeps_the_rectangle_it_occupies() {
        val node = row(Bounds(0, 2387, 1080, 2662), Bounds(0, 2387, 0, 2387))
            .copy(visible = false)
        val json = listOf(node).toWireJson()

        assertTrue(
            "absent is for a node nobody placed; this one is placed: $json",
            json.contains("\"bounds\":[0,2387,1080,2662]"),
        )
        assertTrue("$json", json.contains("\"visibleBounds\":[0,2387,0,2387]"))
        assertTrue("$json", json.contains("\"visible\":false"))
    }

    @Test
    fun the_two_are_the_same_rectangle_when_nothing_clips_it() {
        val whole = Bounds(0, 300, 1080, 575)
        val json = listOf(row(whole, whole)).toWireJson()
        assertEquals(
            "a node wholly on screen says the same thing twice, and that is not a reason to say it once",
            2,
            Regex("\\[0,300,1080,575\\]").findAll(json).count(),
        )
    }
}
