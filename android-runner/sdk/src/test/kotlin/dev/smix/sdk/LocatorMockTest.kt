// Locator poll-loop unit tests over the driving seam. Verifies:
//   - toBeVisible polls + returns on first visible match
//   - toBeVisible throws .timeout when never visible
//   - toBeVisible throws NOT_VISIBLE when matched but not visible
//   - toContainText polls until text contains needle

package dev.smix.sdk

import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds

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
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"id":"btn-x"}""", "btn-x")
        }
        val app = mockApp(tree = tree, resolver = resolver)
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
        val resolver = MockSelectorResolver()  // no hits
        val app = mockApp(tree = tree, resolver = resolver)
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
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"id":"btn-y"}""", "btn-y")
        }
        val app = mockApp(tree = tree, resolver = resolver)
        try {
            app.find(Selector.Id("btn-y")).toBeVisible(timeout = 200.milliseconds)
            fail("must throw NOT_VISIBLE")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.NOT_VISIBLE, e.code)
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
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"id":"msg"}""", "msg")
        }
        val app = mockApp(tree = tree, resolver = resolver)
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
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"id":"msg"}""", "msg")
        }
        val app = mockApp(tree = tree, resolver = resolver)
        try {
            app.find(Selector.Id("msg")).toContainText("Goodbye", timeout = 200.milliseconds)
            fail("must throw .timeout")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.TIMEOUT, e.code)
        }
    }
}
