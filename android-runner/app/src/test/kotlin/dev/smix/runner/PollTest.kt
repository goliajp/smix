package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Test

class PollTest {
    /** A clock that only moves when a look or a pause says so. */
    private class Clock(var now: Long = 0) {
        fun advance(ms: Long) { now += ms }
    }

    @Test
    fun aSlowLookBegunInsideTheBudgetIsNotTheLastWord() {
        // The first look begins at 0, takes 2100 ms and sees the old
        // state. Giving up there is the /back defect in general form.
        val clock = Clock()
        var looks = 0
        val seen = Poll.until(
            budgetMs = 2000,
            pauseMs = 50,
            look = {
                looks += 1
                if (looks == 1) { clock.advance(2100); "old" } else "new"
            },
            done = { it == "new" },
            nowMs = { clock.now },
            pause = { clock.advance(it) },
        )
        assertEquals("new", seen)
        assertEquals(2, looks)
    }

    @Test
    fun aLookBegunAfterTheBudgetThatStillSaysNoEndsTheWait() {
        val clock = Clock()
        var looks = 0
        val seen = Poll.until(
            budgetMs = 200,
            pauseMs = 50,
            look = { looks += 1; clock.advance(10); "old" },
            done = { it == "new" },
            nowMs = { clock.now },
            pause = { clock.advance(it) },
        )
        assertEquals("old", seen)
        // Looks begin at 0, 60, 120, 180 and 240; the one at 240 is the
        // first begun past 200, and it ends the wait.
        assertEquals(5, looks)
    }

    @Test
    fun theTimeLeftIsWhatRemainedWhenTheLookBegan() {
        val clock = Clock()
        val lefts = mutableListOf<Long>()
        Poll.until(
            budgetMs = 100,
            pauseMs = 60,
            look = { left -> lefts += left; clock.advance(10); false },
            done = { it },
            nowMs = { clock.now },
            pause = { clock.advance(it) },
        )
        assertEquals(listOf(100L, 30L, 0L), lefts)
    }
}
