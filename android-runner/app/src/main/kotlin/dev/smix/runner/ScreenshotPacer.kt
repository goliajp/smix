// Screenshot rate-limit pacer — the Android parity of the iOS
// `screenshot_pacer.rs` (crates/smix-simctl/src/screenshot_pacer.rs).
//
// The iOS pacer exists because `simctl io screenshot` under a tight loop
// crashes SimRenderServer on iOS 26.5. Android's UiAutomator screenshot
// does not crash the emulator the same way, but a tight screenshot loop
// still contends with the device and starves the flow it is trying to
// observe, so the same floor applies: it is cheap insurance and keeps
// the sense/act contract identical across platforms. The pacer only
// gates *when* a screenshot is taken; it never touches the pixels.
//
// Pure logic with an injected clock, mirroring SessionTable — the
// interval decision is deterministic and unit-tested without a device.

package dev.smix.runner

/** Pacer tuning. Defaults mirror the iOS pacer. */
data class ScreenshotPacerConfig(
    /** Fast-path floor between screenshots. */
    val minIntervalMs: Long = 100,
    /** A recent wall at or above this puts the pacer on the slow path. */
    val slowThresholdMs: Long = 1500,
    /** Floor while on the slow path. */
    val slowMinIntervalMs: Long = 1500,
    /** A wall at or above this (or a failure) opens the circuit. */
    val circuitThresholdMs: Long = 1500,
    /** How long the circuit stays open once tripped. */
    val circuitHoldMs: Long = 3000,
    /** How many recent walls the slow-path check looks back over. */
    val rollingWindow: Int = 8,
)

/** What the caller should do before the next screenshot. */
sealed class PacerDecision {
    /** Proceed after waiting [ms] (0 = immediately). */
    data class Wait(val ms: Long) : PacerDecision()

    /** The circuit is open; do not screenshot, retry after [retryAfterMs]. */
    data class Backpressure(val retryAfterMs: Long) : PacerDecision()
}

/**
 * Rate-limits UiAutomator screenshots. Not thread-safe on its own; the
 * runner serialises screenshot calls, and the clock is injected so the
 * interval logic is testable without real time.
 */
class ScreenshotPacer(
    private val config: ScreenshotPacerConfig = ScreenshotPacerConfig(),
    private val nowMs: () -> Long,
) {
    private var lastCallEndMs: Long? = null
    private var circuitOpenUntilMs: Long? = null
    private val recentWalls = ArrayDeque<Long>()

    /** Decide whether — and how long — to wait before the next shot. */
    fun computeWait(): PacerDecision {
        val now = nowMs()
        circuitOpenUntilMs?.let { until ->
            if (now < until) return PacerDecision.Backpressure(until - now)
            circuitOpenUntilMs = null
        }
        val floor = if (inSlowPath()) config.slowMinIntervalMs else config.minIntervalMs
        val wait = lastCallEndMs?.let { last ->
            val elapsed = (now - last).coerceAtLeast(0)
            if (elapsed >= floor) 0L else floor - elapsed
        } ?: 0L
        return PacerDecision.Wait(wait)
    }

    /** Record a completed screenshot's wall time and whether it failed. */
    fun record(wallMs: Long, failed: Boolean) {
        if (failed || wallMs >= config.circuitThresholdMs) {
            circuitOpenUntilMs = nowMs() + config.circuitHoldMs
        }
        while (recentWalls.size >= config.rollingWindow) recentWalls.removeFirst()
        recentWalls.addLast(wallMs)
        lastCallEndMs = nowMs()
    }

    private fun inSlowPath(): Boolean = recentWalls.any { it >= config.slowThresholdMs }
}
