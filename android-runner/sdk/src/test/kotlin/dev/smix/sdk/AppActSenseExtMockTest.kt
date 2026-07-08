// v7.4 c5 — App act/sense extension mock-based unit tests.
//
// Mirror swift-bridge/Tests/SmixSDKTests/AppSwipeScreenshotMockTests.swift +
// AppSenseExtMockTests.swift + AppTapAtCoordAndAppPathMockTests.swift
// (Swift v7.2 c1+c2+c5).

package dev.smix.sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class AppActSenseExtMockTest {

    // MARK: - App.swipe

    @Test
    fun swipeDelegatesToRuntime() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        app.swipe(SwipeDirection.DOWN)
        app.swipe(SwipeDirection.UP)
        app.swipe(SwipeDirection.LEFT)
        app.swipe(SwipeDirection.RIGHT)
        assertEquals(
            listOf(SwipeDirection.DOWN, SwipeDirection.UP, SwipeDirection.LEFT, SwipeDirection.RIGHT),
            runtime.swipeCalls,
        )
    }

    // MARK: - App.screenshot

    @Test
    fun screenshotReturnsRuntimeBytes() = runBlocking {
        val expected = byteArrayOf(0x89.toByte(), 0x50, 0x4e, 0x47)  // PNG magic
        val runtime = MockSimRuntime().apply { screenshotResult = expected }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        val out = app.screenshot()
        assertArrayEquals(expected, out)
        assertEquals(1, runtime.screenshotCalls)
    }

    // MARK: - App.systemPopups

    @Test
    fun systemPopupsReturnsRuntimeList() = runBlocking {
        val alertNode = A11yNode(
            rawType = "alert",
            identifier = "alert-permission",
            bounds = Rect(0.0, 0.0, 200.0, 100.0),
        )
        val runtime = MockSimRuntime().apply { systemPopupsResult = listOf(alertNode) }
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        val popups = app.systemPopups()
        assertEquals(1, popups.size)
        assertEquals("alert-permission", popups[0].identifier)
    }

    // MARK: - App.openUrl

    @Test
    fun openUrlForwardsToRuntime() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        app.openUrl("smix://deep/link?id=42")
        assertEquals(listOf("smix://deep/link?id=42"), runtime.openUrlCalls)
    }

    // MARK: - App.launchFresh

    @Test
    fun launchFreshClearStateAndKeychain() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        app.launchFresh(clearState = true, clearKeychain = true)
        assertEquals(1, runtime.launchFreshCalls.size)
        val call = runtime.launchFreshCalls[0]
        assertEquals("dev.smix.fixture", call.bundleId)
        assertTrue(call.clearState)
        assertTrue(call.clearKeychain)
        assertNull(call.appPath)
    }

    @Test
    fun launchFreshWithAppPathOverride() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        app.launchFresh(appPath = "/tmp/Reinstall.apk")
        assertEquals("/tmp/Reinstall.apk", runtime.launchFreshCalls[0].appPath)
    }

    @Test
    fun launchFreshDefaultsAreFalse() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        app.launchFresh()
        val call = runtime.launchFreshCalls[0]
        assertFalse(call.clearState)
        assertFalse(call.clearKeychain)
    }

    // MARK: - App.tapAtCoord

    @Test
    fun tapAtCoordValidRange() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        app.tapAtCoord(0.5, 0.5)
        assertEquals(1, runtime.tapAtNormalizedCalls.size)
        assertEquals(0.5, runtime.tapAtNormalizedCalls[0].first, 0.001)
        assertEquals(0.5, runtime.tapAtNormalizedCalls[0].second, 0.001)
    }

    @Test
    fun tapAtCoordEdgeValues() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        app.tapAtCoord(0.0, 0.0)
        app.tapAtCoord(1.0, 1.0)
        assertEquals(2, runtime.tapAtNormalizedCalls.size)
    }

    @Test
    fun tapAtCoordOutOfRangeThrowsWrongState() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        try {
            app.tapAtCoord(1.5, 0.5)
            fail("nx > 1.0 must throw WRONG_STATE")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.WRONG_STATE, e.code)
            assertTrue(e.message.contains("out of [0,1]"))
        }
        assertTrue(
            "no runtime call should occur when validation throws",
            runtime.tapAtNormalizedCalls.isEmpty(),
        )
    }

    @Test
    fun tapAtCoordNegativeThrowsWrongState() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.BundleId("dev.smix.fixture"),
            runtime,
            MockSelectorResolver(),
        )
        try {
            app.tapAtCoord(0.5, -0.1)
            fail("ny < 0.0 must throw WRONG_STATE")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.WRONG_STATE, e.code)
        }
    }

    // MARK: - Smix.launchApp(.AppPath)

    @Test
    fun launchWithAppPathDispatchesToRuntime() = runBlocking {
        val runtime = MockSimRuntime()
        val app = Smix.launchApp(
            AppTarget.AppPath("/tmp/MyApp.apk"),
            runtime,
            MockSelectorResolver(),
        )
        assertEquals(listOf("/tmp/MyApp.apk"), runtime.launchFromPathCalls)
        assertEquals(emptyList<String>(), runtime.launchCalls)
        assertEquals("/tmp/MyApp.apk", app.bundleId)
    }
}
