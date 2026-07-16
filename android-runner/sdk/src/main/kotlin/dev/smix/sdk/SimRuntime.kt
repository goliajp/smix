// SmixSimRuntime interface + MockSimRuntime impl. Kept at full API
// parity with the Swift SDK's runtime protocol.

package dev.smix.sdk

/**
 * Low-level emulator runtime — captures + injects events on the
 * running app.
 */
interface SmixSimRuntime {
    suspend fun launch(bundleId: String)
    suspend fun terminate(bundleId: String)
    suspend fun snapshotTree(): A11yNode
    suspend fun synthesizeTap(x: Double, y: Double)
    suspend fun sendString(text: String)
    suspend fun pressKey(key: KeyName)

    /** Cardinal swipe on the app's main view. */
    suspend fun swipe(direction: SwipeDirection)

    /** PNG bytes of the current sim screen. */
    suspend fun screenshot(): ByteArray

    /** Snapshot of system-level windows (alert / sheet / overlay). */
    suspend fun systemPopups(): List<A11yNode>

    /** Open a deep-link URL via the system handler. */
    suspend fun openUrl(url: String)

    /**
     * Launch fresh with state clearing options (mirrors Swift
     * `SmixSimRuntime.launchFresh`):
     *   - [clearState]: wipe sandbox dirs (Documents / Library) before relaunch
     *   - [clearKeychain]: wipe keychain items for this bundle id
     *   - [appPath]: optional override (when reinstalling from a path)
     */
    suspend fun launchFresh(
        bundleId: String,
        clearState: Boolean,
        clearKeychain: Boolean,
        appPath: String?,
    )

    /** Install and launch an app from an absolute APK / .app path. */
    suspend fun launchFromPath(appPath: String)

    /**
     * Synthesize a tap at a normalized coordinate (0..1 over the device's
     * logical viewport). Mirror Swift App.tapAtCoord escape hatch.
     */
    suspend fun synthesizeTapAtNormalized(nx: Double, ny: Double)
}

/** Mock-recorded tap call. */
data class TapCall(val x: Double, val y: Double)

/** Mock-recorded launchFresh call. */
data class LaunchFreshCall(
    val bundleId: String,
    val clearState: Boolean,
    val clearKeychain: Boolean,
    val appPath: String?,
)

/**
 * In-memory mock runtime — used by JVM unit tests for deterministic
 * wire verification without booting an emulator.
 */
class MockSimRuntime(
    var snapshotResult: A11yNode = emptyDefault(),
) : SmixSimRuntime {
    val tapCalls = mutableListOf<TapCall>()
    val launchCalls = mutableListOf<String>()
    val terminateCalls = mutableListOf<String>()
    val sendStringCalls = mutableListOf<String>()
    val pressKeyCalls = mutableListOf<KeyName>()
    val swipeCalls = mutableListOf<SwipeDirection>()
    var screenshotCalls: Int = 0
        private set
    var screenshotResult: ByteArray = ByteArray(0)
    var systemPopupsResult: List<A11yNode> = emptyList()
    val openUrlCalls = mutableListOf<String>()
    val launchFreshCalls = mutableListOf<LaunchFreshCall>()
    val launchFromPathCalls = mutableListOf<String>()
    val tapAtNormalizedCalls = mutableListOf<Pair<Double, Double>>()

    var afterSnapshot: ((Int) -> Unit)? = null
    private var snapshotCallCount = 0

    override suspend fun launch(bundleId: String) {
        launchCalls.add(bundleId)
    }

    override suspend fun terminate(bundleId: String) {
        terminateCalls.add(bundleId)
    }

    override suspend fun snapshotTree(): A11yNode {
        val idx = snapshotCallCount++
        afterSnapshot?.invoke(idx)
        return snapshotResult
    }

    override suspend fun synthesizeTap(x: Double, y: Double) {
        tapCalls.add(TapCall(x, y))
    }

    override suspend fun sendString(text: String) {
        sendStringCalls.add(text)
    }

    override suspend fun pressKey(key: KeyName) {
        pressKeyCalls.add(key)
    }

    override suspend fun swipe(direction: SwipeDirection) {
        swipeCalls.add(direction)
    }

    override suspend fun screenshot(): ByteArray {
        screenshotCalls++
        return screenshotResult
    }

    override suspend fun systemPopups(): List<A11yNode> = systemPopupsResult

    override suspend fun openUrl(url: String) {
        openUrlCalls.add(url)
    }

    override suspend fun launchFresh(
        bundleId: String,
        clearState: Boolean,
        clearKeychain: Boolean,
        appPath: String?,
    ) {
        launchFreshCalls.add(LaunchFreshCall(bundleId, clearState, clearKeychain, appPath))
    }

    override suspend fun launchFromPath(appPath: String) {
        launchFromPathCalls.add(appPath)
    }

    override suspend fun synthesizeTapAtNormalized(nx: Double, ny: Double) {
        tapAtNormalizedCalls.add(nx to ny)
    }

    companion object {
        fun emptyDefault(): A11yNode = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            enabled = true,
            visible = true,
            children = emptyList(),
        )
    }
}
