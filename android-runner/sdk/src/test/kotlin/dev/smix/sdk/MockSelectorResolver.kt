// v7.4 c2 — JVM test helper: deterministic mock SelectorResolver.
//
// Tests pre-seed `returnMap` keyed by selectorJson; the mock looks up
// the matching key on resolve(). Falls back to empty List when no
// match registered. Throws if `throwOnNext` is set.

package dev.smix.sdk

class MockSelectorResolver : SelectorResolver {
    /** Stub responses, keyed by selectorJson (deterministic). */
    val returnMap = mutableMapOf<String, List<String>>()
    /** Set non-null to make the next resolve() raise this exception. */
    var throwOnNext: RuntimeException? = null
    /** Log of calls received. */
    val calls = mutableListOf<Pair<String, String>>()

    override fun resolve(treeJson: String, selectorJson: String): List<String> {
        calls.add(treeJson to selectorJson)
        throwOnNext?.let { e ->
            throwOnNext = null
            throw e
        }
        return returnMap[selectorJson] ?: emptyList()
    }

    /** Convenience: register that a selector returns a single match. */
    fun registerHit(selectorJson: String, id: String) {
        returnMap[selectorJson] = listOf(id)
    }
}

/**
 * v7.6 c1 — JVM test helper: mock LabelResolver for Locator.toHaveCount /
 * toHaveLabel tests. Pre-seed `returnMap` with selectorJson →
 * matched-labels list; falls back to empty list when no match registered.
 */
class MockLabelsResolver : LabelResolver {
    val returnMap = mutableMapOf<String, List<String>>()
    var throwOnNext: RuntimeException? = null
    val calls = mutableListOf<Pair<String, String>>()

    override fun resolve(treeJson: String, selectorJson: String): List<String> {
        calls.add(treeJson to selectorJson)
        throwOnNext?.let { e ->
            throwOnNext = null
            throw e
        }
        return returnMap[selectorJson] ?: emptyList()
    }

    /** Register the matched-labels list for a selector. */
    fun registerLabels(selectorJson: String, labels: List<String>) {
        returnMap[selectorJson] = labels
    }
}
