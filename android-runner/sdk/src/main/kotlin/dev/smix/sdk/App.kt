// App class — drives the act/sense pipeline over the FFI driving
// surface. A tap is: driver.tree() → resolveSelector → session.tapById.
//
// App holds both a `Driver` (tree / session list) and a `Session`
// (act), injected through the driving seam (see Session.kt) so host
// unit tests can supply in-memory mocks without loading the
// Android-only `libuniffi_smix.so`.

package dev.smix.sdk

import kotlinx.serialization.json.Json
import kotlinx.serialization.builtins.ListSerializer
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/**
 * A handle to a running app on the emulator. All methods are suspend
 * to interop with Kotlin coroutines.
 *
 * Constructed via [Smix.launchApp]. Holds a [Driver] (accessibility
 * tree, session listing) and a [Session] (acting on the target app),
 * both speaking to the smix runner over the FFI boundary.
 */
class App internal constructor(
    val bundleId: String,
    internal val driver: Driver,
    internal val session: Session,
    internal val resolver: SelectorResolver = DefaultFfiResolver,
    internal val labelsResolver: LabelResolver = DefaultFfiLabelsResolver,
) {
    // ---- act methods --------------------------------------------------

    /**
     * Tap on the first element matching [selector]. Captures an a11y
     * snapshot via the driver, resolves the selector via the Rust FFI
     * core, picks the first match, and taps it by accessibility id.
     *
     * Throws [ExpectationFailure] with `code = ELEMENT_NOT_FOUND` when the
     * resolver returns zero matches — populates `visibleElements` with
     * the first 20 nodes from the current tree for AI agent diagnosis.
     */
    suspend fun tap(selector: Selector, timeout: Duration = 5.seconds) {
        val id = resolveFirstOrThrow(selector)
        session.tapById(id)
    }

    /**
     * Resolve [selector], tap to focus, then type [text]. Mirrors
     * Swift `App.fill`.
     */
    suspend fun fill(selector: Selector, text: String) {
        val id = resolveFirstOrThrow(selector)
        session.tapById(id)
        session.inputText(text)
    }

    suspend fun pressKey(key: KeyName) {
        session.pressKey(key.wireName)
    }

    suspend fun swipe(direction: SwipeDirection) {
        session.swipeOnce(direction.wireName)
    }

    /**
     * Tap at a normalized coordinate (0..1) — the one coordinate
     * escape hatch on the API surface.
     *
     * Both [nx] and [ny] must be in `[0.0, 1.0]`. Out-of-range values
     * throw [ExpectationFailure] with code `ASSERTION_FAILED`.
     */
    suspend fun tapAtCoord(nx: Double, ny: Double) {
        if (nx !in 0.0..1.0 || ny !in 0.0..1.0) {
            throw ExpectationFailure(
                code = FailureCode.ASSERTION_FAILED,
                message = "tapAtCoord arguments out of [0,1] range: nx=$nx, ny=$ny",
                suggestions = listOf(
                    "use normalized 0..1 — for raw points, divide by viewport size",
                ),
            )
        }
        session.tapAtNormCoord(nx, ny)
    }

    suspend fun terminate() {
        session.terminateApp()
    }

    suspend fun relaunch() {
        session.relaunchApp()
    }

    /**
     * Shared snapshot + resolve + first-match-or-throw helper used by
     * [tap] / [fill]. Returns the matched accessibility id.
     *
     * `driver.tree()` returns the tree already as a JSON string in the
     * exact shape the resolver takes, so it is fed straight to the
     * resolver with no re-encode; the tree is only decoded on the miss
     * path, to populate `visibleElements`.
     */
    internal suspend fun resolveFirstOrThrow(selector: Selector): String {
        val treeJson = driver.tree()
        val selectorJson = encodeSelectorJson(selector)
        val matches: List<String> = try {
            resolver.resolve(treeJson, selectorJson)
        } catch (e: Exception) {
            throw ExpectationFailure(
                code = FailureCode.DRIVER_ERROR,
                message = "FFI resolveSelector raised: ${e.message}",
                selectorJson = selectorJson,
            )
        }
        val firstId = matches.firstOrNull()
        if (firstId == null) {
            val tree = decodeTree(treeJson)
            throw ExpectationFailure(
                code = FailureCode.ELEMENT_NOT_FOUND,
                message = "no candidates after spatial + index filters",
                selectorJson = selectorJson,
                visibleElements = tree.flatten().take(20),
                suggestions = listOf(
                    "check `accessibilityIdentifier` is set on the target view",
                    "consider relaxing to text(...) if the label is more stable than the id",
                ),
            )
        }
        return firstId
    }

    /**
     * Decode the driver's tree JSON string into an [A11yNode]. Throws
     * [ExpectationFailure] with `code = DRIVER_ERROR` on decode failure (a
     * wire / SDK bug at the FFI boundary).
     */
    internal fun decodeTree(json: String): A11yNode = try {
        treeJson.decodeFromString(A11yNode.serializer(), json)
    } catch (e: Exception) {
        throw ExpectationFailure(
            code = FailureCode.DRIVER_ERROR,
            message = "tree JSON decode failed: ${e.message}",
        )
    }

    // ---- sense methods ------------------------------------------------

    fun find(selector: Selector): Locator = Locator(this, selector)

    suspend fun tree(): A11yNode = decodeTree(driver.tree())

    /** Internal helper used by [Locator] — snapshot + decode the tree. */
    internal suspend fun runtimeSnapshot(): A11yNode = decodeTree(driver.tree())

    /** Collect system popup windows (permission prompts / system alerts). */
    suspend fun systemPopups(): List<SystemPopup> {
        val json = session.systemPopups()
        return try {
            popupJson.decodeFromString(ListSerializer(SystemPopup.serializer()), json)
        } catch (e: Exception) {
            throw ExpectationFailure(
                code = FailureCode.DRIVER_ERROR,
                message = "system-popups JSON decode failed: ${e.message}",
            )
        }
    }

    private companion object {
        val treeJson = Json { ignoreUnknownKeys = true }
        val popupJson = Json { ignoreUnknownKeys = true }
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

// ---- wire name mappings ---------------------------------------------

/**
 * The name the runner knows this key by. The FFI `parse_wire_enum`
 * takes camelCase names ("return", "delete", …) and there is no "enter"
 * — `ENTER` maps to "return", the same keystroke.
 */
val KeyName.wireName: String
    get() = when (this) {
        KeyName.RETURN -> "return"
        KeyName.DELETE -> "delete"
        KeyName.SPACE -> "space"
        KeyName.TAB -> "tab"
        KeyName.ESCAPE -> "escape"
        KeyName.ENTER -> "return"
    }

val SwipeDirection.wireName: String
    get() = when (this) {
        SwipeDirection.UP -> "up"
        SwipeDirection.DOWN -> "down"
        SwipeDirection.LEFT -> "left"
        SwipeDirection.RIGHT -> "right"
    }
