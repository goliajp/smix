package dev.smix.runner

import dev.smix.runner.PinchGesture.Action
import dev.smix.runner.PinchGesture.Point
import org.json.JSONException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class DragGestureTest {
    private val body =
        """{"fromNx":0.2,"fromNy":0.3,"toNx":0.8,"toNy":0.9,"holdMs":500,"durationMs":100,"releaseMs":300}"""

    @Test
    fun decodeReadsEveryField() {
        assertEquals(DragGesture(0.2, 0.3, 0.8, 0.9, 500, 100, 300), DragGesture.decode(body))
    }

    @Test
    fun aMissingTimingIsAnError_notADefault() {
        assertThrows(JSONException::class.java) {
            DragGesture.decode(body.replace(""""holdMs":500,""", ""))
        }
    }

    @Test
    fun aPointOffTheScreenIsRefused() {
        val e = assertThrows(IllegalArgumentException::class.java) {
            DragGesture.decode(body.replace(""""toNy":0.9""", """"toNy":1.2"""))
        }
        assertEquals("toNy out of range: 1.2", e.message)
    }

    @Test
    fun timingsOutsideTheHostsRangesAreRefused() {
        assertThrows(IllegalArgumentException::class.java) {
            DragGesture.decode(body.replace(""""durationMs":100""", """"durationMs":49"""))
        }
        assertThrows(IllegalArgumentException::class.java) {
            DragGesture.decode(body.replace(""""releaseMs":300""", """"releaseMs":5001"""))
        }
    }

    @Test
    fun theFingerHoldsStill_thenTravels_thenRests_thenLifts() {
        val events = DragGesture.decode(body).timeline(1000, 2000)
        assertEquals(Event(0, Action.DOWN, Point(200f, 600f)), events.first().flat())
        assertEquals(Event(500, Action.MOVE, Point(200f, 600f)), events[1].flat())
        val travel = events.subList(2, events.size - 2)
        assertEquals(10, travel.size)
        assertEquals((510L..600L step 10).toList(), travel.map { it.atMs })
        assertEquals(Event(600, Action.MOVE, Point(800f, 1800f)), travel.last().flat())
        assertEquals(Event(900, Action.MOVE, Point(800f, 1800f)), events[events.size - 2].flat())
        assertEquals(Event(900, Action.UP, Point(800f, 1800f)), events.last().flat())
        events.forEach { assertEquals(1, it.fingers.size) }
    }

    @Test
    fun withoutHoldOrRestTheFingerMovesAtOnceAndLiftsOnArrival() {
        val events = DragGesture(0.0, 0.0, 1.0, 1.0, 0, 50, 0).timeline(100, 100)
        assertEquals(listOf(Action.DOWN) + List(5) { Action.MOVE } + Action.UP, events.map { it.action })
        assertEquals(10L, events[1].atMs)
        assertEquals(50L, events.last().atMs)
    }

    private data class Event(val atMs: Long, val action: Action, val at: Point)

    private fun PinchGesture.Event.flat() = Event(atMs, action, fingers.single())
}
