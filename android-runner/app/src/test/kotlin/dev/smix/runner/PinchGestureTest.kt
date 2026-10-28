package dev.smix.runner

import dev.smix.runner.PinchGesture.Action
import dev.smix.runner.PinchGesture.Point
import org.json.JSONException
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class PinchGestureTest {
    private val zoomIn = PinchGesture(0.5, 0.5, 0.2, 0.6, 500)

    @Test
    fun decodeWithoutPanHasNoPan() {
        val g = PinchGesture.decode("""{"cx":0.5,"cy":0.5,"fromSpan":0.2,"toSpan":0.6,"durationMs":500}""")
        assertEquals(zoomIn, g)
    }

    @Test
    fun decodeReadsThePan() {
        val g = PinchGesture.decode(
            """{"cx":0.5,"cy":0.5,"fromSpan":0.2,"toSpan":0.6,"durationMs":500,"panDx":0.1,"panDy":-0.2,"panMs":300}""",
        )
        assertEquals(0.1, g.panDx, 0.0)
        assertEquals(-0.2, g.panDy, 0.0)
        assertEquals(300L, g.panMs)
    }

    @Test
    fun decodeMissingSpanThrows() {
        assertThrows(JSONException::class.java) {
            PinchGesture.decode("""{"cx":0.5,"cy":0.5,"fromSpan":0.2,"durationMs":500}""")
        }
    }

    @Test
    fun decodeZeroSpanIsRefused() {
        val e = assertThrows(IllegalArgumentException::class.java) {
            PinchGesture.decode("""{"cx":0.5,"cy":0.5,"fromSpan":0,"toSpan":0.6,"durationMs":500}""")
        }
        assertEquals("fromSpan out of range: 0.0", e.message)
    }

    @Test
    fun decodeTooShortDurationIsRefused() {
        val e = assertThrows(IllegalArgumentException::class.java) {
            PinchGesture.decode("""{"cx":0.5,"cy":0.5,"fromSpan":0.2,"toSpan":0.6,"durationMs":10}""")
        }
        assertEquals("durationMs out of range: 10", e.message)
    }

    @Test
    fun spansAreSharesOfTheShorterSide() {
        val p = zoomIn.plan(400, 800, 0f)
        assertEquals(Point(160f, 400f), p.start.first)
        assertEquals(Point(240f, 400f), p.start.second)
        assertEquals(Point(80f, 400f), p.pinched.first)
        assertEquals(Point(320f, 400f), p.pinched.second)
        assertEquals(p.pinched, p.end)
        val landscape = zoomIn.plan(800, 400, 0f)
        assertEquals(240f, landscape.pinched.second.x - landscape.pinched.first.x, 0.01f)
    }

    @Test
    fun panMovesBothFingersTogether() {
        val p = zoomIn.copy(panDy = -0.25, panMs = 300).plan(400, 800, 0f)
        assertEquals(Point(80f, 200f), p.end.first)
        assertEquals(Point(320f, 200f), p.end.second)
    }

    @Test
    fun fingerOffScreenNamesFingerAndMoment() {
        val e = assertThrows(PinchGesture.OffScreen::class.java) {
            PinchGesture(0.1, 0.5, 0.1, 0.6, 500).plan(400, 800, 0f)
        }
        assertEquals(1, e.finger)
        assertEquals("after the pinch", e.moment)
        assertEquals(
            "finger 1 leaves the screen after the pinch at (-80, 400); move the centre or use smaller spans",
            e.message,
        )
    }

    @Test
    fun fingersComeDownShortOfTheStart() {
        val p = zoomIn.plan(400, 800, 5f)
        assertEquals(Point(165f, 400f), p.down.first)
        assertEquals(Point(235f, 400f), p.down.second)
        val e = assertThrows(PinchGesture.OffScreen::class.java) {
            PinchGesture(0.5, 0.5, 1.0, 0.2, 500).plan(400, 800, 5f)
        }
        assertEquals("as it comes down", e.moment)
    }

    @Test
    fun timelinePutsBothFingersDownMovesThemAndLiftsBoth() {
        val g = zoomIn.copy(panDy = -0.25, panMs = 300)
        val plan = g.plan(400, 800, 5f)
        val events = g.timeline(plan)
        assertEquals(
            listOf(Action.DOWN, Action.POINTER_DOWN),
            events.take(2).map { it.action },
        )
        assertEquals(
            listOf(Action.POINTER_UP, Action.UP),
            events.takeLast(2).map { it.action },
        )
        assertEquals(plan.down.first, events.first().fingers[0])
        val moves = events.filter { it.action == Action.MOVE }
        assertEquals(20 + 50 + 30, moves.size)
        assertEquals(plan.start.first, moves[19].fingers[0])
        assertEquals(200L, moves[19].atMs)
        assertEquals(plan.pinched.first, moves[69].fingers[0])
        assertEquals(700L, moves[69].atMs)
        assertEquals(plan.end.second, moves.last().fingers[1])
        assertEquals(1200L, events.last().atMs)
        assertEquals(events.map { it.atMs }.sorted(), events.map { it.atMs })
        assertEquals(1, events.first().fingers.size)
        assertEquals(1, events.last().fingers.size)
    }

    @Test
    fun bodyPutsOkAndFingersOnTheWire() {
        val obj = JSONObject(RunnerWire.pinchBody(true, zoomIn.plan(400, 800, 0f)))
        assertEquals(true, obj.getBoolean("ok"))
        val to = obj.getJSONObject("fingers").getJSONArray("to")
        assertEquals(80, to.getJSONObject(0).getInt("x"))
        assertEquals(320, to.getJSONObject(1).getInt("x"))
    }
}
