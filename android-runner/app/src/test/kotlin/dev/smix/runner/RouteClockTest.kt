package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RouteClockTest {
    private var now = 0L
    private fun clock(limitMs: Long) = RouteClock(limitMs) { now }

    @Test
    fun a_stage_within_the_limit_runs_and_is_timed() {
        val c = clock(1_000)
        val got = c.stage("focus") { now += 300; "node" }
        assertEquals("node", got)
        assertFalse(c.spent)
        assertEquals("focus=300", c.stagesText())
    }

    @Test
    fun once_the_limit_is_spent_no_stage_starts_and_the_first_one_skipped_is_named() {
        val c = clock(1_000)
        c.stage("focus") { now += 1_200 }
        var ran = false
        val idle = c.stage("idle") { ran = true }
        val readback = c.stage("readback") { ran = true }
        assertFalse("a stage started after the limit was spent", ran)
        assertNull(idle)
        assertNull(readback)
        assertTrue(c.spent)
        assertEquals("idle", c.stoppedBefore)
    }

    @Test
    fun a_stage_that_began_in_time_runs_to_its_end() {
        // A stage cannot be interrupted once started; the limit only stops
        // the next one. That is why each stage's own budget is clamped.
        val c = clock(1_000)
        c.stage("readback") { now += 5_000 }
        assertFalse(c.spent)
        assertEquals("readback=5000", c.stagesText())
    }

    @Test
    fun a_budget_is_clamped_to_what_is_left() {
        val c = clock(1_000)
        now += 800
        assertEquals(200, c.budget(2_000))
        assertEquals(100, c.budget(100))
        now += 500
        assertEquals(0, c.budget(2_000))
    }

    @Test
    fun what_a_stopped_route_says_names_the_stage_and_each_time() {
        val c = clock(1_000)
        c.stage("focus") { now += 600 }
        c.stage("delete-keys") { now += 700 }
        c.stage("idle") {}
        assertEquals(
            "stopped before idle, its limit spent; focus=600 delete-keys=700 tookMs=1300 limitMs=1000",
            c.saw(),
        )
    }

    @Test
    fun the_log_line_carries_the_route_its_answer_and_its_stages() {
        val c = clock(19_000)
        c.stage("focus") { now += 12 }
        assertEquals(
            "route=/clear-text status=200 tookMs=40 limitMs=19000 stages=[focus=12]",
            RouteLog.line("/clear-text", 200, 40, c),
        )
        assertEquals("route=/tree status=200 tookMs=84", RouteLog.line("/tree", 200, 84, null))
    }

    @Test
    fun a_route_without_a_stated_limit_is_refused_by_name() {
        // /tree keeps no limit of its own: it does not poll
        val e = runCatching { RouteLimits.of("/tree") }.exceptionOrNull()
        assertTrue(e?.message.orEmpty().contains("/tree"))
    }
}
