// Locator poll loop, mirrors Swift SmixSDK.Locator.
//
// 250ms tick over a Duration deadline. On timeout, throws
// ExpectationFailure with the LAST tree's first-20 visible elements
// populated for AI agent diagnosis.

package dev.smix.sdk

import kotlinx.coroutines.delay
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TimeSource

/**
 * A deferred locator. Created by [App.find]. Each `toBe…(...)` call
 * polls the runtime every 250ms until the predicate holds or the
 * Duration deadline expires.
 *
 * On timeout, throws [ExpectationFailure] populating
 * `visibleElements` with the last-seen tree snapshot for AI agent
 * diagnosis.
 */
class Locator internal constructor(
    internal val app: App,
    internal val selector: Selector,
) {
    /**
     * Resolve [selector] until at least one match exists AND that match
     * is `visible=true`. Polls every 250ms until [timeout] expires.
     */
    suspend fun toBeVisible(timeout: Duration = 5.seconds) {
        pollUntil(
            timeout = timeout,
            wrongStatePredicate = { node ->
                // Found a match but it's not visible → NOT_VISIBLE
                node != null && !node.visible
            },
            successPredicate = { node ->
                node != null && node.visible
            },
            timeoutMessage = "element never became visible",
        )
    }

    /**
     * Resolve [selector] until a match exists AND `text.contains(needle)`
     * holds on the resolved node's label/title/text fields.
     */
    suspend fun toContainText(text: String, timeout: Duration = 5.seconds) {
        pollUntil(
            timeout = timeout,
            wrongStatePredicate = { _ -> false },
            successPredicate = { node ->
                node != null && (
                    node.label?.contains(text) == true ||
                        node.title?.contains(text) == true ||
                        node.text?.contains(text) == true
                )
            },
            timeoutMessage = "element text never contained \"$text\"",
        )
    }

    /**
     * Resolve [selector] until at least one match exists AND that
     * match's label equals [label]. Wired via the dedicated FFI
     * `resolve_selector_labels` for clean label-only semantics.
     */
    suspend fun toHaveLabel(label: String, timeout: Duration = 5.seconds) {
        pollUntil(
            timeout = timeout,
            wrongStatePredicate = { _ -> false },
            successPredicate = { node ->
                node != null && node.label == label
            },
            timeoutMessage = "element label never equaled \"$label\"",
        )
    }

    /**
     * Resolve [selector] until the resolver returns exactly [n] matches.
     * Wired via the dedicated FFI `resolve_selector_count` — counted
     * via the labels resolver (skips index modifiers, per Playwright
     * "match all" semantics).
     *
     * Note: this uses [LabelResolver] on the App not the standard
     * resolver, since count semantics need the all-matches path which
     * the standard resolver (with index modifiers) doesn't expose.
     */
    suspend fun toHaveCount(n: Int, timeout: Duration = 5.seconds) {
        val deadline = TimeSource.Monotonic.markNow() + timeout
        var lastCount = 0
        var lastTree: A11yNode? = null
        while (true) {
            val tree = app.runtimeSnapshot()
            lastTree = tree
            val labels = try {
                app.labelsResolver.resolve(encodeTreeJson(tree), encodeSelectorJson(selector))
            } catch (e: Exception) {
                throw ExpectationFailure(
                    code = FailureCode.DRIVER_ERROR,
                    message = "FFI resolveSelectorLabels raised during poll: ${e.message}",
                    selectorJson = encodeSelectorJson(selector),
                )
            }
            lastCount = labels.size
            if (lastCount == n) return
            if (deadline.hasPassedNow()) break
            kotlinx.coroutines.delay(250.milliseconds)
        }
        throw ExpectationFailure(
            code = FailureCode.ASSERTION_FAILED,
            message = "expected $n matches, last poll saw $lastCount",
            selectorJson = encodeSelectorJson(selector),
            visibleElements = (lastTree?.flatten()?.take(20)) ?: emptyList(),
            suggestions = listOf(
                "verify the selector matches the expected count in the current tree",
                "consider extending the timeout if render is slow",
            ),
        )
    }

    /**
     * Generic 250ms poll loop. On each tick, resolve [selector] via
     * the app's runtime + resolver, then check predicates.
     *
     * - [successPredicate] returns true → return immediately.
     * - [wrongStatePredicate] returns true → throw NOT_VISIBLE with
     *   the matched node included in visibleElements.
     * - Otherwise, continue polling.
     *
     * On timeout, throws .timeout with the last tree's
     * first-20 visible elements.
     */
    private suspend fun pollUntil(
        timeout: Duration,
        wrongStatePredicate: (A11yNode?) -> Boolean,
        successPredicate: (A11yNode?) -> Boolean,
        timeoutMessage: String,
    ) {
        val deadline = TimeSource.Monotonic.markNow() + timeout
        var lastTree: A11yNode? = null

        while (true) {
            val tree = app.runtimeSnapshot()
            lastTree = tree
            val selectorJson = encodeSelectorJson(selector)
            val matches: List<String> = try {
                app.resolver.resolve(encodeTreeJson(tree), selectorJson)
            } catch (e: Exception) {
                throw ExpectationFailure(
                    code = FailureCode.DRIVER_ERROR,
                    message = "FFI resolveSelector raised during poll: ${e.message}",
                    selectorJson = selectorJson,
                )
            }
            val node = matches.firstOrNull()?.let { tree.findById(it) }

            if (wrongStatePredicate(node)) {
                throw ExpectationFailure(
                    // toBeVisible is the only caller that supplies a
                    // firing wrongStatePredicate, so this branch always
                    // means "matched, but not visible".
                    code = FailureCode.NOT_VISIBLE,
                    message = "element matched but predicate failed (e.g. not visible)",
                    selectorJson = selectorJson,
                    visibleElements = listOfNotNull(node) + tree.flatten().take(19),
                )
            }
            if (successPredicate(node)) return

            if (deadline.hasPassedNow()) break
            delay(250.milliseconds)
        }

        throw ExpectationFailure(
            code = FailureCode.TIMEOUT,
            message = timeoutMessage,
            selectorJson = encodeSelectorJson(selector),
            visibleElements = (lastTree?.flatten()?.take(20)) ?: emptyList(),
            suggestions = listOf(
                "consider extending the timeout if the UI takes longer to settle",
                "verify the selector matches when manually inspected (use Smix.tree())",
            ),
        )
    }
}
