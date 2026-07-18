// App.fill / App.pressKey orchestration unit tests over the driving seam.

package dev.smix.sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class AppFillPressKeyMockTest {

    @Test
    fun fillTapsByIdThenInputsText() = runBlocking {
        val field = A11yNode(
            rawType = "textField",
            role = A11yRole.TEXT_FIELD,
            identifier = "input-username",
            label = "Username",
            bounds = Rect(50.0, 200.0, 200.0, 40.0),
            enabled = true,
            visible = true,
        )
        val tree = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            visible = true,
            children = listOf(field),
        )
        val session = MockSession()
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"id":"input-username"}""", "input-username")
        }
        val app = mockApp(tree = tree, session = session, resolver = resolver)

        app.fill(Selector.Id("input-username"), "alice")

        // tap-to-focus by resolved id, then a single inputText
        assertEquals(listOf("input-username"), session.tapByIdCalls)
        assertEquals(listOf("alice"), session.inputTextCalls)
    }

    @Test
    fun pressKeyDelegatesToSessionWithWireName() = runBlocking {
        val session = MockSession()
        val app = mockApp(session = session)

        app.pressKey(KeyName.RETURN)
        app.pressKey(KeyName.DELETE)
        app.pressKey(KeyName.ENTER)

        // wireName: camelCase, and ENTER maps to "return".
        assertEquals(listOf("return", "delete", "return"), session.pressKeyCalls)
    }

    @Test
    fun fillFailureSurfacesAsExpectationFailure() = runBlocking {
        val session = MockSession()
        val app = mockApp(session = session, resolver = MockSelectorResolver())  // returns []
        try {
            app.fill(Selector.Id("missing"), "text")
            fail("fill with missing selector must throw ExpectationFailure.ELEMENT_NOT_FOUND")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.ELEMENT_NOT_FOUND, e.code)
        }
        assertTrue(
            "inputText must NOT fire on resolve failure",
            session.inputTextCalls.isEmpty(),
        )
        assertTrue(
            "tapById must NOT fire on resolve failure",
            session.tapByIdCalls.isEmpty(),
        )
    }
}
