// v7.4 c3 — App.fill / App.pressKey mock-based unit tests.

package dev.smix.sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class AppFillPressKeyMockTest {

    @Test
    fun fillTapsThenSendsString() = runBlocking {
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
        val runtime = MockSimRuntime(snapshotResult = tree)
        val mockResolver = MockSelectorResolver().apply {
            registerHit("""{"id":"input-username"}""", "input-username")
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )

        app.fill(Selector.Id("input-username"), "alice")

        // tap → exactly 1 tap synthesized at field center
        assertEquals(1, runtime.tapCalls.size)
        assertEquals(150.0, runtime.tapCalls[0].x, 0.01)
        assertEquals(220.0, runtime.tapCalls[0].y, 0.01)
        // sendString → exactly 1 string sent
        assertEquals(listOf("alice"), runtime.sendStringCalls)
    }

    @Test
    fun pressKeyDelegatesToRuntime() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )

        app.pressKey(KeyName.RETURN)
        app.pressKey(KeyName.DELETE)

        assertEquals(listOf(KeyName.RETURN, KeyName.DELETE), runtime.pressKeyCalls)
    }

    @Test
    fun fillFailureSurfacesAsExpectationFailure() = runBlocking {
        val runtime = MockSimRuntime()  // empty tree
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),  // returns []
        )
        try {
            app.fill(Selector.Id("missing"), "text")
            fail("fill with missing selector must throw ExpectationFailure.NOT_FOUND")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.NOT_FOUND, e.code)
        }
        assertTrue(
            "sendString must NOT fire on tap failure",
            runtime.sendStringCalls.isEmpty(),
        )
    }
}
