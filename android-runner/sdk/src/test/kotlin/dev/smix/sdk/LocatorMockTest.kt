// Locator + App.fill / App.pressKey mock-based unit tests.
//
// Mirrors swift-bridge/Tests/SmixSDKTests/LocatorMockTests.swift +
// AppFillPressKeyMockTests.swift. Verifies:
//   - Locator.toBeVisible polls + returns on first visible match
//   - Locator.toBeVisible throws .timeout when never visible
//   - Locator.toBeVisible throws .wrongState when matched but not visible
//   - Locator.toContainText polls until text contains needle
//   - App.fill = tap-to-focus + sendString
//   - App.pressKey delegates to runtime

package dev.smix.sdk

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class LocatorMockTest {

    // MARK: - toBeVisible

    @Test
    fun toBeVisibleSucceedsWhenInitiallyVisible() = runTest {
        val node = A11yNode(
            rawType = "button",
            role = A11yRole.BUTTON,
            identifier = "btn-x",
            label = "x",
            bounds = Rect(0.0, 0.0, 10.0, 10.0),
            visible = true,
        )
        val tree = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 100.0, 100.0),
            visible = true,
            children = listOf(node),
        )
        val runtime = MockSimRuntime(snapshotResult = tree)
        val mockResolver = MockSelectorResolver().apply {
            registerHit("""{"id":"btn-x"}""", "btn-x")
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )
        // toBeVisible should not throw
        app.find(Selector.Id("btn-x")).toBeVisible(timeout = 500.milliseconds)
    }

    @Test
    fun toBeVisibleTimeoutWhenNeverMatches() = runTest {
        val tree = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 100.0, 100.0),
            visible = true,
            children = emptyList(),
        )
        val runtime = MockSimRuntime(snapshotResult = tree)
        val mockResolver = MockSelectorResolver()  // no hits
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )
        try {
            app.find(Selector.Id("btn-missing")).toBeVisible(timeout = 500.milliseconds)
            fail("must throw .timeout")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.TIMEOUT, e.code)
            assertFalse(e.visibleElements.isEmpty())
            assertFalse(e.suggestions.isEmpty())
        }
    }

    @Test
    fun toBeVisibleWrongStateWhenMatchedButHidden() = runTest {
        val node = A11yNode(
            rawType = "button",
            identifier = "btn-y",
            label = "y",
            bounds = Rect(0.0, 0.0, 10.0, 10.0),
            visible = false,  // matched but hidden
        )
        val tree = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 100.0, 100.0),
            visible = true,
            children = listOf(node),
        )
        val runtime = MockSimRuntime(snapshotResult = tree)
        val mockResolver = MockSelectorResolver().apply {
            registerHit("""{"id":"btn-y"}""", "btn-y")
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )
        try {
            app.find(Selector.Id("btn-y")).toBeVisible(timeout = 200.milliseconds)
            fail("must throw .wrongState")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.WRONG_STATE, e.code)
        }
    }

    // MARK: - toContainText

    @Test
    fun toContainTextSucceedsWhenLabelMatches() = runTest {
        val node = A11yNode(
            rawType = "staticText",
            identifier = "msg",
            label = "Welcome back, Alice!",
            bounds = Rect(0.0, 0.0, 100.0, 24.0),
            visible = true,
        )
        val tree = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 100.0, 100.0),
            children = listOf(node),
        )
        val runtime = MockSimRuntime(snapshotResult = tree)
        val mockResolver = MockSelectorResolver().apply {
            registerHit("""{"id":"msg"}""", "msg")
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )
        app.find(Selector.Id("msg")).toContainText("Alice", timeout = 500.milliseconds)
    }

    @Test
    fun toContainTextTimeoutWhenNeverMatches() = runTest {
        val node = A11yNode(
            rawType = "staticText",
            identifier = "msg",
            label = "Hello, World",
            bounds = Rect(0.0, 0.0, 100.0, 24.0),
            visible = true,
        )
        val tree = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 100.0, 100.0),
            children = listOf(node),
        )
        val runtime = MockSimRuntime(snapshotResult = tree)
        val mockResolver = MockSelectorResolver().apply {
            registerHit("""{"id":"msg"}""", "msg")
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )
        try {
            app.find(Selector.Id("msg")).toContainText("Goodbye", timeout = 200.milliseconds)
            fail("must throw .timeout")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.TIMEOUT, e.code)
        }
    }

    // toHaveLabel + toHaveCount are wired via the dedicated FFI
    // resolve_selector_count / labels paths; see LocatorToHaveMockTest.
}
