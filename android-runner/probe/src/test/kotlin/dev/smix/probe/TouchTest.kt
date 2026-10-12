// Whether a touch stops at a node, as the toolkit decides — the part of
// it that needs no device.

package dev.smix.probe

import org.junit.Assert.assertEquals
import org.junit.Test

class TouchTest {
    @Test
    fun a_compose_node_with_pointer_input_takes_the_touch_and_one_without_lets_it_through() {
        // a vertex handle that drags with pointerInput
        assertEquals(Touch.TAKES, Touch.ofCompose(pointerInput = true))
        // a zone's bounding box carrying only semantics { onClick }
        assertEquals(Touch.THROUGH, Touch.ofCompose(pointerInput = false))
    }

    @Test
    fun a_view_is_read_by_what_it_says_and_said_unsure_when_its_own_code_decides() {
        assertEquals(Touch.TAKES, Touch.ofView(clickable = true, scrolls = false, ownCode = true))
        assertEquals(Touch.TAKES, Touch.ofView(clickable = false, scrolls = true, ownCode = false))
        assertEquals(Touch.THROUGH, Touch.ofView(clickable = false, scrolls = false, ownCode = false))
        assertEquals(Touch.MAYBE, Touch.ofView(clickable = false, scrolls = false, ownCode = true))
        // the listener could not be read
        assertEquals(Touch.MAYBE, Touch.ofView(clickable = false, scrolls = false, ownCode = null))
    }

    @Test
    fun the_reading_goes_on_the_wire() {
        val node = ProbeNode(
            id = 1, testTag = null, resourceId = null, text = null, hint = null, editableText = null,
            inputText = null, contentDescription = null, role = null, className = null,
            bounds = Bounds(0, 0, 1, 1), visibleBounds = Bounds(0, 0, 1, 1), focused = false,
            enabled = true, visible = true, actions = emptyList(), children = emptyList(), touch = Touch.THROUGH,
        )
        assertEquals(true, listOf(node).toWireJson().contains("\"touch\":\"through\""))
    }
}
