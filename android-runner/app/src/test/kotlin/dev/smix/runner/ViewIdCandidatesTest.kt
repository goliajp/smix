// The spellings /tap-by-id tries for a short id.
//
// This list used to be a literal in the runner, and one of its three
// entries was `com.example.app` — the placeholder from the README. The
// package-qualified spelling could therefore only match a reader who
// had copied the example verbatim; every real app silently fell through
// to the manual walk. Nothing failed, which is why it lasted.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ViewIdCandidatesTest {

    @Test
    fun barePlainIdIsTriedFirst() {
        // Compose's common case. Ordering is the contract: the qualified
        // spellings cost a framework lookup each.
        val candidates = RunnerWire.viewIdCandidates("submit", "jp.golia.app")
        assertEquals("submit", candidates[0])
    }

    @Test
    fun theTargetPackageQualifiesTheId() {
        val candidates = RunnerWire.viewIdCandidates("submit", "jp.golia.app")
        assertTrue(
            "the app under test must get a qualified spelling: $candidates",
            candidates.contains("jp.golia.app:id/submit"),
        )
    }

    @Test
    fun noPlaceholderPackageIsBakedIn() {
        // The regression this file exists for.
        for (target in listOf("jp.golia.app", "", null)) {
            val candidates = RunnerWire.viewIdCandidates("submit", target)
            assertFalse(
                "a placeholder package is being tried for target=$target: $candidates",
                candidates.any { it.startsWith("com.example.") },
            )
        }
    }

    @Test
    fun anAbsentPackageDropsTheQualifiedSpelling() {
        // A caller that sends no App-Bundle-Id header gets the bare and
        // self-test spellings, not a `null:id/…` that matches nothing.
        for (target in listOf(null, "", "   ")) {
            val candidates = RunnerWire.viewIdCandidates("submit", target)
            assertEquals(
                "target=$target should yield exactly the two package-free spellings",
                listOf("submit", "dev.smix.runner.test:id/submit"),
                candidates,
            )
        }
    }

    @Test
    fun theRunnersOwnTestPackageSurvives() {
        // The self-tests address their fixtures by id.
        val candidates = RunnerWire.viewIdCandidates("submit", "jp.golia.app")
        assertTrue(candidates.contains("dev.smix.runner.test:id/submit"))
    }
}
