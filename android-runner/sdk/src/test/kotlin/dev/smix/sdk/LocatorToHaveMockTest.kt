// Locator.toHaveCount + toHaveLabel unit tests over the driving seam.
// toHaveCount wires through MockLabelsResolver (the FFI
// resolve_selector_count / labels path); toHaveLabel uses the standard
// pollUntil path with the node.label predicate.

package dev.smix.sdk

import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds

class LocatorToHaveMockTest {

    @Test
    fun toHaveCountSuccessWhenMatchesExactly() = runTest {
        val tree = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 100.0, 100.0),
            visible = true,
            children = emptyList(),
        )
        val labelsResolver = MockLabelsResolver().apply {
            registerLabels("""{"label":"Item"}""", listOf("Item", "Item", "Item"))
        }
        val app = mockApp(tree = tree, resolver = MockSelectorResolver(), labelsResolver = labelsResolver)
        // Should not throw — 3 matches expected, 3 present.
        app.find(Selector.Label("Item")).toHaveCount(3, timeout = 500.milliseconds)
    }

    @Test
    fun toHaveCountThrowsWrongStateWhenCountMismatch() = runTest {
        val tree = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 100.0, 100.0),
            visible = true,
        )
        val labelsResolver = MockLabelsResolver().apply {
            registerLabels("""{"label":"Item"}""", listOf("Item"))  // 1 match, expect 3
        }
        val app = mockApp(tree = tree, resolver = MockSelectorResolver(), labelsResolver = labelsResolver)
        try {
            app.find(Selector.Label("Item")).toHaveCount(3, timeout = 200.milliseconds)
            fail("must throw ASSERTION_FAILED")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.ASSERTION_FAILED, e.code)
            assertTrue("message must include actual count", e.message.contains("saw 1"))
        }
    }

    @Test
    fun toHaveLabelSuccessWhenLabelMatches() = runTest {
        val node = A11yNode(
            rawType = "button",
            identifier = "btn-x",
            label = "Submit",
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
        val app = mockApp(tree = tree, resolver = resolver, labelsResolver = MockLabelsResolver())
        app.find(Selector.Id("btn-x")).toHaveLabel("Submit", timeout = 500.milliseconds)
    }

    @Test
    fun toHaveLabelTimeoutWhenLabelMismatch() = runTest {
        val node = A11yNode(
            rawType = "button",
            identifier = "btn-x",
            label = "Cancel",  // wrong label vs expected "Submit"
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
        val app = mockApp(tree = tree, resolver = resolver, labelsResolver = MockLabelsResolver())
        try {
            app.find(Selector.Id("btn-x")).toHaveLabel("Submit", timeout = 200.milliseconds)
            fail("must throw timeout")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.TIMEOUT, e.code)
        }
    }
}
