package dev.smix.probe

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A View hosted by Compose is part of the screen, and the decisions about
 * what to say about it are made here rather than in the walk.
 *
 * The walk needs a device; these do not. Everything that can be gotten
 * wrong about an interop node — its name, whether it is reported at all,
 * and which rectangle is its — is a function of values, so it is judged
 * without an emulator and the walk is left with nothing to decide.
 *
 * The defect, reported from a consumer's Android round (2026-09-22): a
 * player's chrome inside an `AndroidView` was addressable through the
 * accessibility path and vanished the moment the app carried the probe,
 * because the probe walked semantics children and nothing else.
 */
class InteropNodeTest {
    @Test
    fun `a resource name is reported the way the accessibility path reports it`() {
        // The same rule as RunnerWire.shortResourceId, which is what
        // `smix find id:btn_player_fullscreen` has always matched against.
        // Two one-line copies rather than a dependency: the probe is a
        // separate published artifact and must not pull the runner in.
        assertEquals(
            "btn_player_fullscreen",
            shortResourceName("dev.smix.fixture:id/btn_player_fullscreen"),
        )
    }

    @Test
    fun `a name with no id segment is left alone`() {
        assertEquals("already_short", shortResourceName("already_short"))
    }

    @Test
    fun `an interop view carries the fields the accessibility path carries`() {
        val n = interopNode(
            resourceId = "btn_player_fullscreen",
            contentDescription = "Full screen",
            text = null,
            className = "android.widget.ImageButton",
            layout = Bounds(948, 934, 1039, 1025),
            clip = Bounds(0, 0, 1080, 2340),
            placed = true,
            children = emptyList(),
        )
        assertNotNull("an ordinary interop view was not reported at all", n)
        assertEquals("btn_player_fullscreen", n!!.resourceId)
        assertEquals("Full screen", n.contentDescription)
        assertEquals("android.widget.ImageButton", n.className)
        assertEquals(Bounds(948, 934, 1039, 1025), n.bounds)
        assertTrue("a fully visible view was reported as not visible", n.visible)
    }

    @Test
    fun `a node nobody has placed is not reported, because its position would be invented`() {
        // The consumer's case: a LazyColumn row two days down the list,
        // composed for prefetch and never placed, reported at
        // [0,533,1080,743] — a rectangle that was not anywhere.
        assertNull(
            "an unplaced node was given a rectangle",
            interopNode(
                resourceId = "row_far_away",
                contentDescription = null,
                text = "24 Sep",
                className = "android.widget.TextView",
                layout = Bounds(0, 533, 1080, 743),
                clip = Bounds(0, 0, 1080, 2340),
                placed = false,
                children = emptyList(),
            ),
        )
    }

    @Test
    fun `a node clipped away entirely is reported, and reported as not visible`() {
        // Different from the one above, and the difference is the whole
        // point: there we do not know where it is, here we know exactly
        // where it is and that none of it shows. The accessibility path
        // keeps these too, with visible=false.
        val n = interopNode(
            resourceId = "row_scrolled_past",
            contentDescription = null,
            text = "yesterday",
            className = "android.widget.TextView",
            layout = Bounds(0, 2400, 1080, 2500),
            clip = Bounds(0, 0, 1080, 2340),
            placed = true,
            children = emptyList(),
        )
        assertNotNull("a placed but clipped node was dropped", n)
        assertEquals(
            "the rectangle that shows is not empty",
            0,
            n!!.visibleBounds.right - n.visibleBounds.left,
        )
        assertTrue("a node with nothing showing says it is visible", !n.visible)
    }

    @Test
    fun `a half-clipped node keeps the half that shows`() {
        val n = interopNode(
            resourceId = "row_half",
            contentDescription = null,
            text = "half",
            className = "android.widget.TextView",
            layout = Bounds(0, 100, 1080, 300),
            clip = Bounds(0, 200, 1080, 2340),
            placed = true,
            children = emptyList(),
        )
        assertNotNull("a placed, half-visible node was dropped", n)
        // Both rectangles, because the host asks two questions of them:
        // where to aim (the whole node) and how much of it can be seen
        // (the part inside the clip). Answering both with the clipped one
        // made every partly-visible row read as fully visible.
        assertEquals("the node's own rectangle", Bounds(0, 100, 1080, 300), n!!.bounds)
        assertEquals("the part that shows", Bounds(0, 200, 1080, 300), n.visibleBounds)
        assertTrue("a node half on screen says it is invisible", n.visible)
    }

    @Test
    fun `the wire has sixteen fields`() {
        // A count, not "more than none". Sixteen is a fact about what the
        // host reads off this wire today; a field added or removed here
        // changes what every downstream reader sees, so it should cost one
        // deliberate edit in this file rather than pass unnoticed.
        //
        // Sixteen since `visibleBounds` joined `bounds`: one rectangle was
        // being asked both where the node is and how much of it shows, and
        // the second answer was wrong for every clipped node.
        assertEquals(16, ProbeNode::class.java.declaredFields.count { !it.isSynthetic })
    }
}

/**
 * Which rectangle is a semantics node's.
 *
 * The probe reported `positionOnScreen + size`, which is where the layout
 * put the node whether or not any of it is on screen. A consumer measured
 * the difference: a row in a `LazyColumn` reported at `[0,533,1080,743]`
 * with the screen showing it nowhere, and at `[284,1466,795,1550]` once
 * scrolled to. Compose has the other number — `boundsInWindow` is clipped
 * by every ancestor — and it is in window coordinates, so it has to be
 * moved to the screen's before it can be compared with anything.
 */
class VisibleRectTest {
    @Test
    fun `a node with nothing over it keeps its whole rectangle`() {
        assertEquals(
            Bounds(100, 300, 500, 400),
            visibleScreenRect(
                layoutOnScreen = Bounds(100, 300, 500, 400),
                clippedInWindow = Bounds(100, 236, 500, 336),
                windowLeftOnScreen = 0,
                windowTopOnScreen = 64,
            ),
        )
    }

    @Test
    fun `a node the list has scrolled half out keeps the half that shows`() {
        // The viewport starts at y=300 on screen; the row runs 200..400.
        assertEquals(
            Bounds(0, 300, 1080, 400),
            visibleScreenRect(
                layoutOnScreen = Bounds(0, 200, 1080, 400),
                clippedInWindow = Bounds(0, 236, 1080, 336),
                windowLeftOnScreen = 0,
                windowTopOnScreen = 64,
            ),
        )
    }

    @Test
    fun `a node scrolled out entirely has an empty rectangle`() {
        val r = visibleScreenRect(
            layoutOnScreen = Bounds(0, 2400, 1080, 2500),
            clippedInWindow = Bounds(0, 0, 0, 0),
            windowLeftOnScreen = 0,
            windowTopOnScreen = 64,
        )
        assertEquals("a rectangle with nothing in it still has width", 0, r.right - r.left)
        assertEquals("a rectangle with nothing in it still has height", 0, r.bottom - r.top)
    }
}
