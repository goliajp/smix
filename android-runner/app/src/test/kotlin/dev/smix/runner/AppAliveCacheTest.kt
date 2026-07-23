// AppAliveCache — the Android parity of the iOS alive cache, pinned
// with an injected clock. The load-bearing property: one probe failure
// suppresses briefly but never latches death forever.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppAliveCacheTest {

    @Test
    fun anUnknownAppIsNotSuppressed() {
        val c = AppAliveCache(ttlMs = 100, nowMs = { 0L })
        assertFalse(c.isSuppressed("com.a"))
        assertEquals(1L, c.suppressMissTotal)
    }

    @Test
    fun aFailedProbeSuppressesWithinTheWindow() {
        var t = 0L
        val c = AppAliveCache(ttlMs = 100, nowMs = { t })
        c.markDead("com.a") // dead until 100
        t = 50
        assertTrue(c.isSuppressed("com.a"))
        assertEquals(1L, c.suppressHitTotal)
    }

    @Test
    fun theWindowExpiresRatherThanLatchingDeathForever() {
        var t = 0L
        val c = AppAliveCache(ttlMs = 100, nowMs = { t })
        c.markDead("com.a")
        t = 150 // past the ttl
        assertFalse("an expired window must not suppress", c.isSuppressed("com.a"))
        // and the expiry cleared it — a second check is a clean miss.
        assertFalse(c.isSuppressed("com.a"))
    }

    @Test
    fun aSuccessfulReprobeClearsSuppressionEarly() {
        var t = 0L
        val c = AppAliveCache(ttlMs = 100, nowMs = { t })
        c.markDead("com.a") // dead until 100
        t = 20
        c.markAlive("com.a") // app came back before the window elapsed
        assertFalse(c.isSuppressed("com.a"))
        assertEquals(1L, c.markAliveTotal)
    }

    @Test
    fun suppressionIsPerApp() {
        var t = 0L
        val c = AppAliveCache(ttlMs = 100, nowMs = { t })
        c.markDead("com.a")
        t = 10
        assertTrue(c.isSuppressed("com.a"))
        assertFalse("a different app is unaffected", c.isSuppressed("com.b"))
    }

    @Test
    fun countersTallyEachMutation() {
        val c = AppAliveCache(ttlMs = 100, nowMs = { 0L })
        c.markDead("com.a")
        c.markDead("com.b")
        c.markAlive("com.a")
        c.isSuppressed("com.b") // hit
        c.isSuppressed("com.c") // miss
        assertEquals(2L, c.markDeadTotal)
        assertEquals(1L, c.markAliveTotal)
        assertEquals(1L, c.suppressHitTotal)
        assertEquals(1L, c.suppressMissTotal)
    }
}
