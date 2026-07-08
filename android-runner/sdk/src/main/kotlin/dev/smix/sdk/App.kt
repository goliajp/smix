// v7.4 c1 — App class skeleton mirrors Swift SmixSDK.App.
//
// v7.4 c1 stubs all act/sense methods to throw SmixError.NotImplemented
// with explicit stage markers; c2 wires App.tap via FFI + SimRuntime,
// c3 wires App.find + Locator. v7.5+ adds full API parity.

package dev.smix.sdk

import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/**
 * A handle to a running app on the emulator. All methods are suspend
 * to interop with Kotlin coroutines.
 *
 * Constructed via [Smix.launchApp]. Holds a reference to the underlying
 * [SmixSimRuntime] which performs the actual UiAutomator / JNI work.
 */
class App internal constructor(
    val bundleId: String,
    internal val runtime: SmixSimRuntime,
    internal val resolver: SelectorResolver = DefaultFfiResolver,
    internal val labelsResolver: LabelResolver = DefaultFfiLabelsResolver,
) {
    // ---- act methods --------------------------------------------------

    /**
     * Tap on the first element matching [selector]. Captures an a11y
     * snapshot, resolves the selector via the Rust FFI core, picks the
     * first match, and synthesizes a tap at its bounds center.
     *
     * Throws [ExpectationFailure] with `code = NOT_FOUND` when the
     * resolver returns zero matches — populates `visibleElements` with
     * the first 20 nodes from the current tree for AI agent diagnosis.
     */
    suspend fun tap(selector: Selector, timeout: Duration = 5.seconds) {
        val tree = runtime.snapshotTree()
        val treeJson = encodeTreeJson(tree)
        val selectorJson = encodeSelectorJson(selector)
        val matches: List<String> = try {
            resolver.resolve(treeJson, selectorJson)
        } catch (e: Exception) {
            throw ExpectationFailure(
                code = FailureCode.UNKNOWN,
                message = "FFI resolveSelector raised: ${e.message}",
                selectorJson = selectorJson,
            )
        }
        val firstId = matches.firstOrNull()
        val node = firstId?.let { tree.findById(it) }
        if (node == null) {
            throw ExpectationFailure(
                code = FailureCode.NOT_FOUND,
                message = "no candidates after spatial + index filters",
                selectorJson = selectorJson,
                visibleElements = tree.flatten().take(20),
                suggestions = listOf(
                    "check `accessibilityIdentifier` is set on the target view",
                    "consider relaxing to text(...) if the label is more stable than the id",
                ),
            )
        }
        runtime.synthesizeTap(x = node.bounds.centerX, y = node.bounds.centerY)
    }

    /**
     * Resolve [selector], tap to focus, then type [text]. Mirrors
     * Swift `App.fill` from v7.1 c3.
     */
    suspend fun fill(selector: Selector, text: String) {
        tap(selector)  // tap → focuses the field
        runtime.sendString(text)
    }

    suspend fun pressKey(key: KeyName) {
        runtime.pressKey(key)
    }

    suspend fun swipe(direction: SwipeDirection) {
        runtime.swipe(direction)
    }

    suspend fun screenshot(): ByteArray = runtime.screenshot()

    /**
     * Tap at a normalized coordinate (0..1) — escape hatch per §9 #3.
     *
     * Both [nx] and [ny] must be in `[0.0, 1.0]`. Out-of-range values
     * throw [ExpectationFailure] with code `WRONG_STATE`.
     */
    suspend fun tapAtCoord(nx: Double, ny: Double) {
        if (nx !in 0.0..1.0 || ny !in 0.0..1.0) {
            throw ExpectationFailure(
                code = FailureCode.WRONG_STATE,
                message = "tapAtCoord arguments out of [0,1] range: nx=$nx, ny=$ny",
                suggestions = listOf(
                    "use normalized 0..1 — for raw points, divide by viewport size",
                ),
            )
        }
        runtime.synthesizeTapAtNormalized(nx, ny)
    }

    suspend fun terminate() {
        runtime.terminate(bundleId)
    }

    suspend fun relaunch() {
        runtime.terminate(bundleId)
        runtime.launch(bundleId)
    }

    /**
     * Launch fresh with state clearing options. Mirror Swift
     * App.launchFresh from v7.2 c2.
     */
    suspend fun launchFresh(
        clearState: Boolean = false,
        clearKeychain: Boolean = false,
        appPath: String? = null,
    ) {
        runtime.launchFresh(bundleId, clearState, clearKeychain, appPath)
    }

    // ---- sense methods ------------------------------------------------

    fun find(selector: Selector): Locator = Locator(this, selector)

    suspend fun tree(): A11yNode = runtime.snapshotTree()

    suspend fun systemPopups(): List<A11yNode> = runtime.systemPopups()

    suspend fun openUrl(url: String) {
        runtime.openUrl(url)
    }
}

/**
 * Hardware-like keys the runner can synthesize.
 */
enum class KeyName {
    RETURN, DELETE, SPACE, TAB, ESCAPE, ENTER,
}

/**
 * Cardinal swipe directions (maestro navigation convention).
 */
enum class SwipeDirection {
    UP, DOWN, LEFT, RIGHT,
}
