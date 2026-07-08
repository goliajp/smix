// v7.6 c1 — Locator.toHaveCount + toHaveLabel mock-based unit tests.
//
// Mirror Swift Locator.toHave* + TS LocatorToHave tests (v7.6 c1).
// Verifies count + label assertions wire through MockLabelsResolver +
// MockSelectorResolver (resolve_selector_count / resolve_selector_labels
// dedicated FFI fns from c1).

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
        val runtime = MockSimRuntime(snapshotResult = tree)
        val resolver = MockSelectorResolver()
        val labelsResolver = MockLabelsResolver().apply {
            registerLabels("""{"label":"Item"}""", listOf("Item", "Item", "Item"))
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            resolver,
            labelsResolver,
        )
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
        val runtime = MockSimRuntime(snapshotResult = tree)
        val resolver = MockSelectorResolver()
        val labelsResolver = MockLabelsResolver().apply {
            registerLabels("""{"label":"Item"}""", listOf("Item"))  // 1 match, expect 3
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            resolver,
            labelsResolver,
        )
        try {
            app.find(Selector.Label("Item")).toHaveCount(3, timeout = 200.milliseconds)
            fail("must throw wrongState")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.WRONG_STATE, e.code)
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
        val runtime = MockSimRuntime(snapshotResult = tree)
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"id":"btn-x"}""", "btn-x")
        }
        val labelsResolver = MockLabelsResolver()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            resolver,
            labelsResolver,
        )
        // Locator.toHaveLabel uses the standard pollUntil path with
        // node.label == label predicate (no FFI labels call); this
        // exercises that pipeline.
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
        val runtime = MockSimRuntime(snapshotResult = tree)
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"id":"btn-x"}""", "btn-x")
        }
        val labelsResolver = MockLabelsResolver()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            resolver,
            labelsResolver,
        )
        try {
            app.find(Selector.Id("btn-x")).toHaveLabel("Submit", timeout = 200.milliseconds)
            fail("must throw timeout")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.TIMEOUT, e.code)
        }
    }
}
