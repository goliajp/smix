package dev.smix.runner

import dev.smix.runner.MouseGesture.Action
import dev.smix.runner.MouseGesture.Button
import dev.smix.runner.MouseGesture.Kind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class MouseGestureTest {
    @Test
    fun hoverArrivesFromBesideThePointAndRestsOnIt() {
        val events = MouseGesture(Kind.HOVER, 0.5, 0.25).timeline(1000, 800)
        assertEquals(listOf(Action.HOVER_MOVE, Action.HOVER_MOVE), events.map { it.action })
        assertEquals(500f - MouseGesture.APPROACH_PX, events[0].x)
        assertEquals(500f to 200f, events.last().x to events.last().y)
        assertTrue(events.all { it.held == Button.NONE })
        assertTrue(events[1].atMs > events[0].atMs)
    }

    @Test
    fun awayIsOneExitAndNeedsNoPoint() {
        val g = MouseGesture.decode("""{"kind":"away"}""")
        assertEquals(Kind.AWAY, g.kind)
        assertEquals(listOf(Action.HOVER_EXIT), g.timeline(1000, 800).map { it.action })
    }

    @Test
    fun leavingWhenNothingIsHoveredIsDoneAndAnyOtherRefusalIsNot() {
        assertTrue(MouseGesture(Kind.AWAY, 0.0, 0.0).done(injected = false))
        for (kind in Kind.entries.filter { it != Kind.AWAY }) {
            assertEquals(kind.wire, false, MouseGesture(kind, 0.5, 0.5).done(injected = false))
            assertEquals(kind.wire, true, MouseGesture(kind, 0.5, 0.5).done(injected = true))
        }
    }

    @Test
    fun hoverNeedsItsPoint() {
        val e = assertThrows(IllegalArgumentException::class.java) {
            MouseGesture.decode("""{"kind":"hover","nx":0.5}""")
        }
        assertEquals("ny is required", e.message)
    }

    @Test
    fun aPointOffTheDisplayAndAnUnknownKindAreRefused() {
        val off = assertThrows(IllegalArgumentException::class.java) {
            MouseGesture.decode("""{"kind":"hover","nx":1.5,"ny":0.5}""")
        }
        assertTrue(off.message!!, off.message!!.startsWith("nx must be a share"))
        val kind = assertThrows(IllegalArgumentException::class.java) {
            MouseGesture.decode("""{"kind":"wiggle","nx":0.5,"ny":0.5}""")
        }
        assertTrue(kind.message!!, kind.message!!.contains("not wiggle"))
    }

    // A button press while the pointer still hovers is dropped by the
    // system as a conflicting pointer action.
    @Test
    fun everyPressComesAfterTheHoverEnds() {
        for (kind in listOf(Kind.SECONDARY, Kind.DRAG)) {
            val events = MouseGesture(kind, 0.5, 0.5, 0.6, 0.5).timeline(1000, 800)
            val down = events.indexOfFirst { it.action == Action.DOWN }
            val exit = events.indexOfFirst { it.action == Action.HOVER_EXIT }
            assertTrue("$kind: exit $exit before down $down", exit in 0 until down)
            assertTrue("$kind hovers again at the end", events.last().action == Action.HOVER_MOVE)
        }
    }
}
