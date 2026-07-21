// Which spelling found the node, named so it can be observed.
//
// The strict lookup and the manual walk return the same node. That is
// why `com.example.app` — the README's placeholder, in the candidate
// list as a real spelling — survived releases without failing anything:
// every real app fell through to the walk, and the walk answered
// identically. Slower, by another code path, indistinguishable from
// outside.
//
// A fix nobody can observe cannot be guarded. viewIdMatchKind is the
// observation surface, and these are its cases.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Test

class ViewIdMatchTest {

    @Test
    fun theQualifiedSpellingIsNamedWhenTheAppUnderTestMatches() {
        assertEquals(
            "qualified",
            RunnerWire.viewIdMatchKind(
                "com.android.settings:id/search_action_bar",
                "search_action_bar",
                "com.android.settings",
            ),
        )
    }

    @Test
    fun theBareSpellingIsItsOwnAnswer() {
        assertEquals(
            "bare",
            RunnerWire.viewIdMatchKind("search_action_bar", "search_action_bar", "com.android.settings"),
        )
    }

    /// The runner's own package is in the candidate list for the
    /// self-tests. A hit on it says the app under test was NOT what
    /// answered, which is a different fact from "qualified".
    @Test
    fun theRunnersOwnPackageIsNotTheAppUnderTest() {
        assertEquals(
            "runner-test",
            RunnerWire.viewIdMatchKind(
                "dev.smix.runner.test:id/some_fixture",
                "some_fixture",
                "com.android.settings",
            ),
        )
    }

    /// Null means every strict candidate missed and the manual walk
    /// took over. This is the value the placeholder era would have
    /// produced for every real app, had anything been looking.
    @Test
    fun noStrictHitMeansTheWalkAnsweredInstead() {
        assertEquals(
            "walk",
            RunnerWire.viewIdMatchKind(null, "search_action_bar", "com.android.settings"),
        )
    }

    /// A qualified hit on some package other than the app under test is
    /// still not the app under test.
    @Test
    fun aQualifiedHitOnAnotherPackageIsNotQualified() {
        assertEquals(
            "other-package",
            RunnerWire.viewIdMatchKind(
                "com.android.systemui:id/search_action_bar",
                "search_action_bar",
                "com.android.settings",
            ),
        )
    }
}
