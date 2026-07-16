// App.tap mock-based unit tests.
//
// Mirrors swift-bridge/Tests/SmixSDKTests/AppTapMockTests.swift.
// Verifies wire pipeline (snapshot → SelectorResolver → tap synthesize)
// end-to-end via MockSimRuntime + MockSelectorResolver. JVM-only —
// no JNA init, no libuniffi_smix.so load.

package dev.smix.sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class AppTapMockTest {

    // MARK: - .notFound path

    @Test
    fun tapByIdEmptyTreeThrowsNotFound() = runBlocking {
        val runtime = MockSimRuntime()  // default empty container
        val mockResolver = MockSelectorResolver()  // returns [] for everything
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )

        try {
            app.tap(Selector.Id("btn-missing"))
            fail("empty resolver result must yield NOT_FOUND")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.NOT_FOUND, e.code)
            assertFalse("suggestions must populate for AI agent", e.suggestions.isEmpty())
            assertFalse(e.visibleElements.isEmpty())
        }
    }

    @Test
    fun tapByTextAbsentThrowsNotFound() = runBlocking {
        val runtime = MockSimRuntime()
        val mockResolver = MockSelectorResolver()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )

        try {
            app.tap(Selector.Text(Pattern.Literal("Nope")))
            fail("text miss must yield NOT_FOUND")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.NOT_FOUND, e.code)
        }
    }

    // MARK: - Hit path

    @Test
    fun tapByIdHitSynthesizesAtBoundsCenter() = runBlocking {
        val button = A11yNode(
            rawType = "button",
            role = A11yRole.BUTTON,
            identifier = "btn-login",
            label = "Sign In",
            bounds = Rect(100.0, 200.0, 80.0, 40.0),
            enabled = true,
            visible = true,
        )
        val root = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            enabled = true,
            visible = true,
            children = listOf(button),
        )
        val runtime = MockSimRuntime(snapshotResult = root)
        val mockResolver = MockSelectorResolver().apply {
            registerHit("""{"id":"btn-login"}""", "btn-login")
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )

        app.tap(Selector.Id("btn-login"))

        assertEquals("exactly one tap dispatched", 1, runtime.tapCalls.size)
        assertEquals("center x = 100 + 80/2", 140.0, runtime.tapCalls[0].x, 0.01)
        assertEquals("center y = 200 + 40/2", 220.0, runtime.tapCalls[0].y, 0.01)
    }

    @Test
    fun tapByLabelHitSynthesizes() = runBlocking {
        val button = A11yNode(
            rawType = "button",
            role = A11yRole.BUTTON,
            identifier = "btn-foo",
            label = "Submit",
            bounds = Rect(50.0, 600.0, 100.0, 50.0),
            enabled = true,
            visible = true,
        )
        val root = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            enabled = true,
            visible = true,
            children = listOf(button),
        )
        val runtime = MockSimRuntime(snapshotResult = root)
        val mockResolver = MockSelectorResolver().apply {
            registerHit("""{"label":"Submit"}""", "btn-foo")
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )

        app.tap(Selector.Label("Submit"))
        assertEquals(1, runtime.tapCalls.size)
        assertEquals(100.0, runtime.tapCalls[0].x, 0.01)
    }

    @Test
    fun tapPassesEncodedSelectorJsonToResolver() = runBlocking {
        val root = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            visible = true,
            children = emptyList(),
        )
        val runtime = MockSimRuntime(snapshotResult = root)
        val mockResolver = MockSelectorResolver()  // returns []
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )

        try {
            app.tap(Selector.Id("btn-x"))
        } catch (_: ExpectationFailure) { /* expected */ }

        assertEquals(1, mockResolver.calls.size)
        val (_, sel) = mockResolver.calls[0]
        // selectorJson must equal what encodeSelectorJson produces — the
        // Rust-compatible untagged shape `{"id":"btn-x"}`.
        assertEquals("""{"id":"btn-x"}""", sel)
    }

    @Test
    fun tapResolverErrorYieldsUnknownCode() = runBlocking {
        val root = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            visible = true,
            children = emptyList(),
        )
        val runtime = MockSimRuntime(snapshotResult = root)
        val mockResolver = MockSelectorResolver().apply {
            throwOnNext = RuntimeException("simulated FFI parse error")
        }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            mockResolver,
        )

        try {
            app.tap(Selector.Id("anything"))
            fail("resolver raise must surface as ExpectationFailure")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.UNKNOWN, e.code)
            assertTrue("message must include resolver error", e.message.contains("simulated FFI parse error"))
        }
    }

    // MARK: - Lifecycle

    @Test
    fun launchAppCallsRuntimeLaunch() = runBlocking {
        val runtime = MockSimRuntime()
        Smix.launchApp(
            AppTarget.BundleId("dev.smix.target"),
            runtime,
            MockSelectorResolver(),
        )
        assertEquals(listOf("dev.smix.target"), runtime.launchCalls)
    }

    @Test
    fun relaunchCallsTerminateAndLaunch() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.target"),
            runtime,
            MockSelectorResolver(),
        )
        app.relaunch()
        assertEquals(listOf("dev.smix.target", "dev.smix.target"), runtime.launchCalls)
        assertEquals(listOf("dev.smix.target"), runtime.terminateCalls)
    }

    @Test
    fun terminateCallsRuntimeTerminate() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.target"),
            runtime,
            MockSelectorResolver(),
        )
        app.terminate()
        assertEquals(listOf("dev.smix.target"), runtime.terminateCalls)
    }

    // launchApp(.AppPath) is wired to runtime.launchFromPath; see
    // AppActSenseExtMockTest.launchWithAppPathDispatchesToRuntime.

    // MARK: - tree() sense

    @Test
    fun treeReturnsSnapshotFromRuntime() = runBlocking {
        val snapshot = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 1.0, 1.0),
            children = emptyList(),
        )
        val runtime = MockSimRuntime(snapshotResult = snapshot)
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.target"),
            runtime,
            MockSelectorResolver(),
        )
        val tree = app.tree()
        assertEquals(snapshot, tree)
    }
}
