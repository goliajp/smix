// App act/sense extension unit tests over the driving seam:
// swipe, systemPopups, tapAtCoord.

package dev.smix.sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class AppActSenseExtMockTest {

    // MARK: - App.swipe

    @Test
    fun swipeDelegatesToSessionWithWireName() = runBlocking {
        val session = MockSession()
        val app = mockApp(session = session)
        app.swipe(SwipeDirection.DOWN)
        app.swipe(SwipeDirection.UP)
        app.swipe(SwipeDirection.LEFT)
        app.swipe(SwipeDirection.RIGHT)
        assertEquals(
            listOf("down", "up", "left", "right"),
            session.swipeOnceCalls,
        )
    }

    // MARK: - App.systemPopups

    @Test
    fun systemPopupsParsesJsonArray() = runBlocking {
        val json = """
            [
              {
                "id": "alert-permission",
                "type": "alert",
                "source": "com.apple.springboard",
                "title": "Allow Location",
                "body": "This app wants your location",
                "buttons": [
                  {"id":"btn-allow","label":"Allow","role":"default","dangerous":false,"outcomeHint":"grants location"},
                  {"id":"btn-deny","label":"Don't Allow","role":"cancel"}
                ]
              }
            ]
        """.trimIndent()
        val session = MockSession(systemPopupsJson = json)
        val app = mockApp(session = session)

        val popups = app.systemPopups()
        assertEquals(1, popups.size)
        val p = popups[0]
        assertEquals("alert-permission", p.id)
        assertEquals("alert", p.type)
        assertEquals("com.apple.springboard", p.source)
        assertEquals("Allow Location", p.title)
        assertEquals(2, p.buttons.size)
        assertEquals("Allow", p.buttons[0].label)
        assertEquals("grants location", p.buttons[0].outcomeHint)
        assertFalse(p.buttons[0].dangerous)
        // outcomeHint defaults to null when absent
        assertNull(p.buttons[1].outcomeHint)
    }

    @Test
    fun systemPopupsEmptyArray() = runBlocking {
        val app = mockApp(session = MockSession(systemPopupsJson = "[]"))
        assertTrue(app.systemPopups().isEmpty())
    }

    // MARK: - App.tapAtCoord

    @Test
    fun tapAtCoordValidRange() = runBlocking {
        val session = MockSession()
        val app = mockApp(session = session)
        app.tapAtCoord(0.5, 0.5)
        assertEquals(1, session.tapAtNormCoordCalls.size)
        assertEquals(0.5, session.tapAtNormCoordCalls[0].first, 0.001)
        assertEquals(0.5, session.tapAtNormCoordCalls[0].second, 0.001)
    }

    @Test
    fun tapAtCoordEdgeValues() = runBlocking {
        val session = MockSession()
        val app = mockApp(session = session)
        app.tapAtCoord(0.0, 0.0)
        app.tapAtCoord(1.0, 1.0)
        assertEquals(2, session.tapAtNormCoordCalls.size)
    }

    @Test
    fun tapAtCoordOutOfRangeThrowsWrongState() = runBlocking {
        val session = MockSession()
        val app = mockApp(session = session)
        try {
            app.tapAtCoord(1.5, 0.5)
            fail("nx > 1.0 must throw WRONG_STATE")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.WRONG_STATE, e.code)
            assertTrue(e.message.contains("out of [0,1]"))
        }
        assertTrue(
            "no session call should occur when validation throws",
            session.tapAtNormCoordCalls.isEmpty(),
        )
    }

    @Test
    fun tapAtCoordNegativeThrowsWrongState() = runBlocking {
        val session = MockSession()
        val app = mockApp(session = session)
        try {
            app.tapAtCoord(0.5, -0.1)
            fail("ny < 0.0 must throw WRONG_STATE")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.WRONG_STATE, e.code)
        }
    }
}
