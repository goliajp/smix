// Per-app alive cache — the Android parity of the iOS `AppAliveCache`
// (swift-bridge/Sources/SmixRunnerCore/SmixRunnerServer.swift).
//
// A single liveness probe failing does not prove the app is dead — it
// may be mid-relaunch, or the probe raced a transition. But hammering a
// genuinely-dead app with more requests just piles up timeouts. So a
// failed probe marks the app dead for a short window; upstream routes
// short-circuit (`isSuppressed`) during it instead of re-probing, and a
// later successful probe (`markAlive`) clears it early. Same contract as
// iOS; the only platform difference is how death is *detected* (Android
// checks the app process / UiAutomator window rather than
// XCUIApplication.state), which is the caller's job, not this cache's.
//
// Counters exist for the same reason as iOS: a stderr line cannot prove
// a branch ran, and cannot tell "app was alive" from "the log dropped".
// Each mutation advances a counter that flows through the diagnostic
// dump, so "did suppression fire" is answerable after the fact.
//
// Pure logic with an injected clock, mirroring SessionTable.

package dev.smix.runner

class AppAliveCache(
    /** How long a failed probe suppresses re-probing. Mirrors iOS ttl. */
    private val ttlMs: Long = 3000,
    private val nowMs: () -> Long,
) {
    private val deadUntil = mutableMapOf<String, Long>()

    var markDeadTotal = 0L; private set
    var markAliveTotal = 0L; private set
    var suppressHitTotal = 0L; private set
    var suppressMissTotal = 0L; private set

    /** A liveness probe failed: suppress the app for the ttl window. */
    fun markDead(bundleId: String) {
        deadUntil[bundleId] = nowMs() + ttlMs
        markDeadTotal++
    }

    /** A probe observed the app running: clear any suppression early. */
    fun markAlive(bundleId: String) {
        deadUntil.remove(bundleId)
        markAliveTotal++
    }

    /**
     * Is the app currently suppressed (a recent probe failed and the
     * window has not elapsed)? An expired window clears itself and is
     * not a hit — one probe failure never latches death forever.
     */
    fun isSuppressed(bundleId: String): Boolean {
        val until = deadUntil[bundleId]
        if (until != null) {
            if (nowMs() < until) {
                suppressHitTotal++
                return true
            }
            deadUntil.remove(bundleId)
        }
        suppressMissTotal++
        return false
    }
}
