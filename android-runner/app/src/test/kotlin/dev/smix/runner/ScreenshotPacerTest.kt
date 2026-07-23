// ScreenshotPacer interval logic — the Android parity of the iOS
// pacer's compute_wait, pinned with an injected clock so the decision
// is deterministic and needs no device.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ScreenshotPacerTest {

    private fun cfg() = ScreenshotPacerConfig(
        minIntervalMs = 10,
        slowThresholdMs = 100,
        slowMinIntervalMs = 50,
        circuitThresholdMs = 200,
        circuitHoldMs = 100,
        rollingWindow = 4,
    )

    @Test
    fun firstCallWaitsForNothing() {
        val p = ScreenshotPacer(cfg(), nowMs = { 0L })
        assertEquals(PacerDecision.Wait(0), p.computeWait())
    }

    @Test
    fun withinTheFloorItWaitsTheRemainder() {
        var t = 0L
        val p = ScreenshotPacer(cfg(), nowMs = { t })
        p.record(wallMs = 5, failed = false) // last_end = 0
        t = 4 // 4ms later, floor is 10
        assertEquals(PacerDecision.Wait(6), p.computeWait()) // 10 - 4
    }

    @Test
    fun pastTheFloorItProceeds() {
        var t = 0L
        val p = ScreenshotPacer(cfg(), nowMs = { t })
        p.record(wallMs = 5, failed = false)
        t = 15 // past the 10ms floor
        assertEquals(PacerDecision.Wait(0), p.computeWait())
    }

    @Test
    fun aSlowWallLiftsTheFloor() {
        var t = 0L
        val p = ScreenshotPacer(cfg(), nowMs = { t })
        // wall 120 >= slowThreshold 100 -> slow path, floor becomes 50.
        p.record(wallMs = 120, failed = false)
        t = 20 // 20ms later; fast floor 10 would pass, slow floor 50 does not
        assertEquals(PacerDecision.Wait(30), p.computeWait()) // 50 - 20
    }

    @Test
    fun aFailureOpensTheCircuit() {
        var t = 0L
        val p = ScreenshotPacer(cfg(), nowMs = { t })
        p.record(wallMs = 5, failed = true) // opens circuit until 0 + 100
        t = 40
        val d = p.computeWait()
        assertTrue("circuit should be open: $d", d is PacerDecision.Backpressure)
        assertEquals(60L, (d as PacerDecision.Backpressure).retryAfterMs) // 100 - 40
    }

    @Test
    fun aVerySlowWallOpensTheCircuit() {
        var t = 0L
        val p = ScreenshotPacer(cfg(), nowMs = { t })
        p.record(wallMs = 250, failed = false) // 250 >= circuitThreshold 200
        t = 10
        assertTrue(p.computeWait() is PacerDecision.Backpressure)
    }

    @Test
    fun theCircuitClosesAfterItsHold() {
        var t = 0L
        val p = ScreenshotPacer(cfg(), nowMs = { t })
        p.record(wallMs = 5, failed = true) // circuit until 100
        t = 120 // past the hold
        assertTrue(p.computeWait() is PacerDecision.Wait)
    }

    @Test
    fun theSlowPathIsForgottenOnceRecentWallsAreFast() {
        var t = 0L
        val p = ScreenshotPacer(cfg(), nowMs = { t }) // rollingWindow 4
        p.record(wallMs = 120, failed = false) // slow
        repeat(4) { p.record(wallMs = 5, failed = false) } // push the slow wall out
        t = 1_000_000 // long after, so only the floor matters
        p.record(wallMs = 5, failed = false)
        t += 15 // past the fast floor 10, not the slow floor 50
        assertEquals(PacerDecision.Wait(0), p.computeWait())
    }
}
